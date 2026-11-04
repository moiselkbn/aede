//! Microphone capture. `start()` opens the default input device and records until
//! `Recording::stop()`, which returns the audio the way Whisper wants it: mono, 16 kHz.
//!
//! The cpal stream cannot be moved between threads on macOS, so it lives on its own thread
//! for its whole life. The rest of the app only holds a `Recording` (a stop signal + a handle).

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Error, ErrorKind, FromSample, Sample, SampleFormat, Stream};

/// Whisper only accepts 16 kHz audio.
const WHISPER_RATE: u32 = 16_000;

/// A recording stops growing after this many seconds (the rest is dropped).
const MAX_SECONDS: u32 = 120;

/// Length of the slices used to measure loudness: 30 ms at 16 kHz.
const LEVEL_SLICE: usize = 480;

/// How often the loudness of the last samples is reported while recording (about 30 times
/// per second), and how much audio it covers.
const LEVEL_INTERVAL: Duration = Duration::from_millis(33);

/// Error when macOS has no input device. Public so the dictation can tell it apart.
pub const NO_MICROPHONE: &str = "Aucun micro trouvé.";

/// How long `start()` waits for the microphone to open.
const OPEN_TIMEOUT: Duration = Duration::from_secs(3);

/// If set, the last recording is also written to this WAV file, to listen to it
/// or to replay it in tools/whisper-bench.
const DEBUG_WAV_ENV: &str = "AEDE_SAVE_WAV";

/// A microphone macOS knows about.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct InputDevice {
    /// Stable between launches (cpal's device id, written as text): what the settings keep.
    pub id: String,
    /// What macOS calls it, shown to the user.
    pub name: String,
}

fn describe(device: &cpal::Device) -> Option<InputDevice> {
    Some(InputDevice {
        id: device.id().ok()?.to_string(),
        name: device.description().ok()?.name().to_string(),
    })
}

/// Every microphone macOS knows about, the default one first.
pub fn input_devices() -> Vec<InputDevice> {
    let host = cpal::default_host();
    let default = host.default_input_device().and_then(|d| describe(&d));
    let mut devices: Vec<InputDevice> = host
        .input_devices()
        .map(|devices| devices.filter_map(|d| describe(&d)).collect())
        .unwrap_or_default();
    // The default one goes first, so that a list always starts with what macOS would use.
    if let Some(default) = default {
        devices.retain(|d| d.id != default.id);
        devices.insert(0, default);
    }
    devices
}

/// The microphone `preferred` names (an id of `input_devices`), or the default one when it is
/// not given or not plugged in any more.
fn find_device(preferred: Option<&str>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    preferred
        .and_then(|id| id.parse::<cpal::DeviceId>().ok())
        .and_then(|id| host.device_by_id(&id))
        .or_else(|| host.default_input_device())
}

/// The id of the microphone that `start(preferred, ..)` would open.
pub fn used_input_id(preferred: Option<&str>) -> Option<String> {
    describe(&find_device(preferred)?).map(|d| d.id)
}

/// What `stop()` returns.
pub struct Captured {
    /// Mono, 16 kHz, between -1.0 and 1.0.
    pub samples: Vec<f32>,
    /// True if the recording reached the maximum length and the rest was dropped.
    pub truncated: bool,
}

impl Captured {
    pub fn seconds(&self) -> f32 {
        self.samples.len() as f32 / WHISPER_RATE as f32
    }

    /// How loud the speech is: the volume (RMS) of the loudest 10 % of the 30 ms slices.
    /// Speech has loud slices, a room with nobody talking does not, whatever its steady noise.
    /// Measured on the built-in microphone: 0.015 or more for speech, 0.001 to 0.002 for silence.
    pub fn speech_level(&self) -> f32 {
        let (slices, _incomplete_end) = self.samples.as_chunks::<LEVEL_SLICE>();
        let mut volumes: Vec<f32> = slices.iter().map(|slice| volume(slice)).collect();
        if volumes.is_empty() {
            return 0.0;
        }
        volumes.sort_by(f32::total_cmp);
        volumes[((volumes.len() - 1) as f32 * 0.9) as usize]
    }

    /// Largest absolute sample. Exactly 0.0 means the microphone gave pure silence,
    /// which is what macOS delivers when the microphone permission is refused.
    pub fn peak(&self) -> f32 {
        self.samples.iter().fold(0.0, |peak, s| peak.max(s.abs()))
    }
}

/// Volume (RMS) of some samples: 0.0 for silence or no samples, up to 1.0.
fn volume(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

/// Volume of the last `count` samples (or of all of them if there are fewer).
fn recent_volume(samples: &[f32], count: usize) -> f32 {
    volume(&samples[samples.len().saturating_sub(count)..])
}

/// A recording in progress.
pub struct Recording {
    stop: Sender<()>,
    thread: JoinHandle<Result<Captured, String>>,
}

/// Opens the microphone and starts recording. Returns once the audio is flowing, or why it failed.
/// `preferred` is the id of the microphone the user chose (the default one if None or unplugged).
/// While recording, `on_level` gets the volume of the last samples about 30 times per second.
pub fn start(
    preferred: Option<String>,
    on_level: impl Fn(f32) + Send + 'static,
) -> Result<Recording, String> {
    let (stop_tx, stop_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let thread = std::thread::spawn(move || capture(preferred, ready_tx, stop_rx, on_level));

    match ready_rx.recv_timeout(OPEN_TIMEOUT) {
        Ok(Ok(())) => Ok(Recording {
            stop: stop_tx,
            thread,
        }),
        // The thread has already given up and ends by itself.
        Ok(Err(message)) => Err(message),
        // Dropping `stop_tx` tells the thread to stop if it ever gets going.
        Err(_) => Err(
            "Le micro ne répond pas. Si macOS demande l'accès au micro, accepte puis recommence."
                .to_string(),
        ),
    }
}

impl Recording {
    /// Stops recording and returns the audio converted to mono 16 kHz.
    pub fn stop(self) -> Result<Captured, String> {
        // If the thread is already gone, join() below reports why.
        let _ = self.stop.send(());
        self.thread
            .join()
            .map_err(|_| "Le fil de capture audio a planté.".to_string())?
    }
}

/// Shared between the audio callback (which fills it) and the capture thread.
#[derive(Default)]
struct Shared {
    /// Mono samples at the microphone's own rate.
    samples: Mutex<Vec<f32>>,
    truncated: AtomicBool,
    /// First real stream error, if any.
    error: Mutex<Option<String>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Body of the capture thread: open the stream, report readiness, report the level until the
/// stop signal.
fn capture(
    preferred: Option<String>,
    ready: Sender<Result<(), String>>,
    stop: Receiver<()>,
    on_level: impl Fn(f32),
) -> Result<Captured, String> {
    let shared = Arc::new(Shared::default());
    let (stream, rate) = match open_stream(&shared, preferred.as_deref()) {
        Ok(opened) => opened,
        Err(message) => {
            let _ = ready.send(Err(message.clone()));
            return Err(message);
        }
    };
    let _ = ready.send(Ok(()));

    // Stops when stop() is called, or when the Recording was dropped.
    let level_samples = (rate as u128 * LEVEL_INTERVAL.as_millis() / 1000) as usize;
    while let Err(RecvTimeoutError::Timeout) = stop.recv_timeout(LEVEL_INTERVAL) {
        let level = recent_volume(&lock(&shared.samples), level_samples);
        on_level(level);
    }
    drop(stream);

    if let Some(message) = lock(&shared.error).take() {
        return Err(format!("Erreur du micro : {message}"));
    }
    let raw = std::mem::take(&mut *lock(&shared.samples));
    Ok(Captured {
        samples: resample(&raw, rate, WHISPER_RATE),
        truncated: shared.truncated.load(Ordering::Relaxed),
    })
}

/// Builds and starts the input stream. Also returns the microphone's sample rate.
fn open_stream(shared: &Arc<Shared>, preferred: Option<&str>) -> Result<(Stream, u32), String> {
    let device = find_device(preferred).ok_or(NO_MICROPHONE)?;
    let config = device
        .default_input_config()
        .map_err(|e| format!("Configuration du micro impossible : {e}"))?;
    let rate = config.sample_rate();
    let channels = usize::from(config.channels());
    let max_frames = (rate * MAX_SECONDS) as usize;
    let format = config.sample_format();

    let on_error = {
        let shared = Arc::clone(shared);
        move |err: Error| match err.kind() {
            // Hiccups that do not break the recording.
            ErrorKind::Xrun | ErrorKind::RealtimeDenied => eprintln!("[audio] {err}"),
            _ => {
                eprintln!("[audio] stream error: {err}");
                lock(&shared.error).get_or_insert(err.to_string());
            }
        }
    };

    // The callback is generic over the sample type, so each format needs its own copy.
    macro_rules! input_stream {
        ($t:ty) => {{
            let shared = Arc::clone(shared);
            device.build_input_stream(
                config.into(),
                move |data: &[$t], _: &_| {
                    let mut samples = lock(&shared.samples);
                    if push_mono(&mut samples, data, channels, max_frames) {
                        shared.truncated.store(true, Ordering::Relaxed);
                    }
                },
                on_error,
                None,
            )
        }};
    }
    let stream = match format {
        SampleFormat::F32 => input_stream!(f32),
        SampleFormat::I16 => input_stream!(i16),
        SampleFormat::I32 => input_stream!(i32),
        other => return Err(format!("Format audio non géré : {other}")),
    }
    .map_err(|e| format!("Ouverture du micro impossible : {e}"))?;

    stream
        .play()
        .map_err(|e| format!("Démarrage du micro impossible : {e}"))?;
    eprintln!("[audio] recording from {channels} channel(s) at {rate} Hz ({format})");
    Ok((stream, rate))
}

/// Appends `data` (interleaved channels) to `out` as mono: the average of the channels.
/// Stops at `max_frames` and returns true if some audio was dropped.
fn push_mono<T>(out: &mut Vec<f32>, data: &[T], channels: usize, max_frames: usize) -> bool
where
    T: Sample,
    f32: FromSample<T>,
{
    for frame in data.chunks(channels) {
        if out.len() >= max_frames {
            return true;
        }
        let sum: f32 = frame.iter().map(|&s| f32::from_sample(s)).sum();
        out.push(sum / frame.len() as f32);
    }
    false
}

/// Converts mono audio from `from_rate` to `to_rate` by averaging: each output sample is the
/// mean of the input over the time slice it covers. The averaging also removes the high
/// frequencies that would otherwise fold back into the voice when lowering the rate.
fn resample(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate {
        return input.to_vec();
    }
    // How many input samples one output sample covers (3.0 for 48 kHz -> 16 kHz).
    let ratio = f64::from(from_rate) / f64::from(to_rate);
    let out_len = (input.len() as f64 / ratio) as usize;
    (0..out_len)
        .map(|i| {
            let start = i as f64 * ratio;
            let end = start + ratio;
            let first = start as usize;
            let last = (end.ceil() as usize).min(input.len());
            let covered = input.get(first..last).unwrap_or(&[]);
            let sum: f64 = covered
                .iter()
                .enumerate()
                .map(|(offset, &sample)| {
                    let k = first + offset;
                    // Share of input sample k that lies inside [start, end).
                    let overlap = end.min((k + 1) as f64) - start.max(k as f64);
                    f64::from(sample) * overlap
                })
                .sum();
            (sum / ratio) as f32
        })
        .collect()
}

/// Writes mono 16 kHz audio to a 16-bit WAV file.
pub fn save_wav(path: &Path, samples: &[f32]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    write_wav(std::io::BufWriter::new(file), samples)
}

/// Writes mono 16 kHz audio as a 16-bit WAV to any destination (a file the caller opened with
/// the permissions it wants, for instance).
pub fn write_wav(
    destination: impl std::io::Write + std::io::Seek,
    samples: &[f32],
) -> Result<(), String> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: WHISPER_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::new(destination, spec).map_err(|e| e.to_string())?;
    for &sample in samples {
        let value = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        writer.write_sample(value).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())
}

/// Debug helper: writes the recording to the file named by AEDE_SAVE_WAV, if that is set.
pub fn save_debug_copy(captured: &Captured) {
    let Some(path) = std::env::var_os(DEBUG_WAV_ENV) else {
        return;
    };
    match save_wav(Path::new(&path), &captured.samples) {
        Ok(()) => eprintln!(
            "[audio] debug copy written to {}",
            Path::new(&path).display()
        ),
        Err(message) => eprintln!("[audio] could not write the debug copy: {message}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A microphone that is not plugged in (or an id that means nothing) falls back on the default one.
    #[test]
    fn an_unknown_microphone_falls_back_on_the_default() {
        assert_eq!(used_input_id(Some("not-a-device")), used_input_id(None));
    }

    #[test]
    fn the_device_list_has_unique_ids_and_starts_with_the_default() {
        let devices = input_devices();
        for device in &devices {
            eprintln!("[test] {} -> {}", device.name, device.id);
        }
        let mut ids: Vec<_> = devices.iter().map(|d| &d.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), devices.len());
        if let Some(default) = used_input_id(None) {
            assert_eq!(devices[0].id, default);
        }
    }

    fn sine(freq: f32, rate: u32, seconds: f32) -> Vec<f32> {
        (0..(rate as f32 * seconds) as usize)
            .map(|n| (2.0 * std::f32::consts::PI * freq * n as f32 / rate as f32).sin())
            .collect()
    }

    /// Counts how many times the signal goes from negative to positive.
    fn rising_crossings(samples: &[f32]) -> usize {
        samples
            .windows(2)
            .filter(|w| w[0] < 0.0 && w[1] >= 0.0)
            .count()
    }

    #[test]
    fn same_rate_is_left_alone() {
        let input = vec![0.1, -0.2, 0.3];
        assert_eq!(resample(&input, 16_000, 16_000), input);
    }

    #[test]
    fn exact_ratio_averages_blocks() {
        // 48 kHz -> 16 kHz: every 3 input samples become their mean.
        let out = resample(&[0.0, 0.3, 0.6, 1.0, 1.0, 1.0], 48_000, 16_000);
        assert_eq!(out.len(), 2);
        assert!((out[0] - 0.3).abs() < 1e-6);
        assert!((out[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn constant_signal_stays_constant_at_44_1_khz() {
        let out = resample(&vec![0.5; 44_100], 44_100, 16_000);
        assert_eq!(out.len(), 16_000);
        assert!(out.iter().all(|s| (s - 0.5).abs() < 1e-5));
    }

    #[test]
    fn a_voice_frequency_survives_resampling() {
        // 440 Hz for 1 s must still be 440 Hz, whatever the microphone rate.
        for rate in [48_000, 44_100] {
            let out = resample(&sine(440.0, rate, 1.0), rate, 16_000);
            assert_eq!(out.len(), 16_000);
            let crossings = rising_crossings(&out);
            assert!((439..=441).contains(&crossings), "{rate} Hz: {crossings}");
            let peak = out.iter().fold(0.0_f32, |p, s| p.max(s.abs()));
            assert!(peak > 0.9, "{rate} Hz: peak {peak}");
        }
    }

    #[test]
    fn stereo_is_averaged_to_mono() {
        let mut out = Vec::new();
        let truncated = push_mono(&mut out, &[1.0_f32, 0.0, 0.5, 0.5], 2, 100);
        assert!(!truncated);
        assert_eq!(out, vec![0.5, 0.5]);
    }

    #[test]
    fn integer_samples_are_scaled_to_floats() {
        let mut out = Vec::new();
        push_mono(&mut out, &[i16::MAX, i16::MIN], 1, 100);
        assert!((out[0] - 1.0).abs() < 1e-3);
        assert!((out[1] + 1.0).abs() < 1e-3);
    }

    #[test]
    fn recording_stops_growing_at_the_maximum() {
        let mut out = Vec::new();
        assert!(push_mono(&mut out, &[0.1_f32; 10], 1, 4));
        assert_eq!(out.len(), 4);
        // Once full, more audio is dropped and still reported.
        assert!(push_mono(&mut out, &[0.1_f32; 10], 1, 4));
        assert_eq!(out.len(), 4);
    }

    fn captured(samples: Vec<f32>) -> Captured {
        Captured {
            samples,
            truncated: false,
        }
    }

    #[test]
    fn speech_level_is_zero_for_silence_and_for_nothing() {
        assert_eq!(captured(vec![0.0; 16_000]).speech_level(), 0.0);
        assert_eq!(captured(Vec::new()).speech_level(), 0.0);
        // Shorter than one 30 ms slice.
        assert_eq!(captured(vec![0.5; 100]).speech_level(), 0.0);
    }

    #[test]
    fn recent_volume_only_looks_at_the_last_samples() {
        // Loud first, silent at the end: only the silent end counts.
        let mut samples = vec![0.5; 1000];
        samples.extend(vec![0.0; 100]);
        assert_eq!(recent_volume(&samples, 100), 0.0);
        // The RMS of a constant signal is its absolute value.
        assert!((recent_volume(&[-0.25; 50], 100) - 0.25).abs() < 1e-6);
        assert_eq!(recent_volume(&[], 100), 0.0);
    }

    #[test]
    fn speech_level_follows_the_loud_parts_not_the_average() {
        // 1 s of faint noise with 0.3 s of loud tone in the middle.
        let mut samples = vec![0.001; 16_000];
        for (n, sample) in samples[5_000..9_800].iter_mut().enumerate() {
            *sample = 0.2 * (n as f32 * 0.3).sin();
        }
        let level = captured(samples).speech_level();
        assert!((0.1..0.2).contains(&level), "level {level}");
        // The same noise alone stays quiet.
        let quiet = captured(vec![0.001; 16_000]).speech_level();
        assert!((quiet - 0.001).abs() < 1e-4, "level {quiet}");
    }

    #[test]
    fn peak_of_silence_is_zero() {
        let captured = Captured {
            samples: vec![0.0; 100],
            truncated: false,
        };
        assert_eq!(captured.peak(), 0.0);
    }

    #[test]
    fn wav_file_can_be_read_back() {
        let path = std::env::temp_dir().join("aede-audio-test.wav");
        let samples = sine(440.0, 16_000, 0.5);
        save_wav(&path, &samples).unwrap();

        let mut reader = hound::WavReader::open(&path).unwrap();
        let spec = reader.spec();
        assert_eq!(
            (spec.channels, spec.sample_rate, spec.bits_per_sample),
            (1, 16_000, 16)
        );
        let read: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        assert_eq!(read.len(), samples.len());
        std::fs::remove_file(path).ok();
    }

    /// Needs a microphone and its permission, so it is skipped by default.
    /// Records AEDE_TEST_SECONDS seconds (1 by default) and writes them to AEDE_SAVE_WAV if set.
    /// Run it with: cargo test -- --ignored --nocapture records_from_the_real_microphone
    #[test]
    #[ignore]
    fn records_from_the_real_microphone() {
        let seconds: u64 = std::env::var("AEDE_TEST_SECONDS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        let levels = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&levels);
        let recording =
            start(None, move |level| lock(&sink).push(level)).expect("microphone should open");
        std::thread::sleep(Duration::from_secs(seconds));
        let captured = recording.stop().expect("recording should stop cleanly");
        let levels = lock(&levels);
        eprintln!(
            "captured {:.2} s, peak {:.4}, rms {:.5}, {} levels reported, loudest {:.4}",
            captured.seconds(),
            captured.peak(),
            volume(&captured.samples),
            levels.len(),
            levels.iter().fold(0.0_f32, |a, &b| a.max(b))
        );
        // About 30 levels per second.
        assert!(levels.len() as f32 > seconds as f32 * 20.0);
        save_debug_copy(&captured);
        assert!((captured.seconds() - seconds as f32).abs() < 0.3);
        assert!(!captured.truncated);
    }
}
