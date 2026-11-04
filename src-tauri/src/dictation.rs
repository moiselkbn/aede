//! Push-to-talk flow:
//! key down -> start recording, key up -> stop, transcribe, rewrite, paste the text -> back to
//! idle. If the rewriting fails or takes too long, the raw transcription is pasted instead.
//! Until the paste starts, the user can cancel (Escape or the menu bar): nothing is pasted then.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::audio::{self, Recording};
use crate::cancel;
use crate::dictionary;
use crate::history::{self, Entry};
use crate::indicator;
use crate::learn;
use crate::microphone;
use crate::paste;
use crate::permissions;
use crate::recordings;
use crate::rewrite::{self, Rewriter};
use crate::settings;
use crate::shortcut;
use crate::stats;
use crate::trace::trace;
use crate::transcription::Transcriber;
use crate::tray::{self, TrayState};
use crate::trial;

/// Name of the event the UI listens to for phase changes.
const PHASE_EVENT: &str = "dictation-phase";

/// Name of the event sent once a dictation went all the way to the paste. The window reads the
/// history instead, but the notch indicator needs it to show how the dictation ended.
const RESULT_EVENT: &str = "dictation-result";

/// Name of the event sent as soon as Whisper is done, with what it heard. The first launch shows
/// it while the rewriting is still going; the final text comes with `dictation-result`.
const RAW_EVENT: &str = "dictation-raw";

/// A recording shorter than this is a tap on the shortcut, not speech.
const MIN_SECONDS: f32 = 0.3;

/// Below this `speech_level`, nobody is talking: Whisper is not asked, because it invents text
/// on silence ("Merci.", "Sous-titrage Société Radio-Canada"). Measured on the built-in
/// microphone: silence 0.001 to 0.002, weakest speech of the test set 0.015. The threshold
/// sits between the two. To check again with another microphone, see the level in the log.
const MIN_SPEECH_LEVEL: f32 = 0.005;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Phase {
    #[default]
    Idle,
    Recording,
    Transcribing,
    Rewriting,
    Pasting,
}

/// Sent to the UI every time the phase changes.
#[derive(Clone, Serialize)]
struct PhaseChange {
    phase: Phase,
    /// Set when the dictation that just ended failed, so the UI can show it.
    error: Option<String>,
    /// What kind of failure it was, for the short text of the indicator.
    failure: Option<FailureKind>,
}

/// The ways a dictation can fail. Sent to the page as "too_short", "no_sound"...: the texts
/// shown by the indicator are keyed by these codes in src/wave.ts.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
enum FailureKind {
    TooShort,
    TooQuiet,
    NoSound,
    NothingHeard,
    NoMicrophone,
    MicrophoneFailed,
    ModelLoading,
    ModelMissing,
    TranscriptionFailed,
    NoAccessibility,
    PasteFailed,
    /// Not a failure: the user cancelled.
    Cancelled,
    Other,
}

/// Why a dictation failed: its kind for the indicator, a full sentence for the window.
struct Failure {
    kind: FailureKind,
    message: String,
}

impl Failure {
    fn new(kind: FailureKind, message: impl Into<String>) -> Self {
        Failure {
            kind,
            message: message.into(),
        }
    }

    /// `at` names the checkpoint that saw the cancel request, for the log: it shows how long
    /// the dictation took to notice it.
    fn cancelled(at: &str) -> Self {
        match cancel::since_request() {
            Some(waited) => trace!(
                "[dictation] cancel seen {at}, {} ms after the request",
                waited.as_millis()
            ),
            None => trace!("[dictation] cancel seen {at}, without a request"),
        }
        Failure::new(FailureKind::Cancelled, "Dictée annulée.")
    }

    /// The microphone could not record: there is none, or it does not work.
    fn microphone(message: String) -> Self {
        let kind = if message == audio::NO_MICROPHONE {
            FailureKind::NoMicrophone
        } else {
            FailureKind::MicrophoneFailed
        };
        Failure::new(kind, message)
    }
}

/// What the terminal and the notch indicator get after a successful dictation (the window
/// reads the history). Durations are in milliseconds.
#[derive(Clone, Serialize)]
struct DictationResult {
    /// The text that was pasted: the rewritten text, or the raw transcription on a fallback.
    text: String,
    /// What Whisper heard, before the rewriting.
    raw_text: String,
    /// Set when the rewriting was skipped and `text` is the raw transcription: why.
    fallback_reason: Option<String>,
    /// Length of the recorded audio.
    audio_ms: u64,
    /// Key released -> audio ready (stopping the microphone, converting to 16 kHz).
    stop_ms: u64,
    transcribe_ms: u64,
    /// Time spent rewriting, including a rewriting that was given up.
    rewrite_ms: u64,
    /// Includes the pauses of the paste: the text shows up about 200 ms before the end.
    paste_ms: u64,
    /// Key released -> paste finished.
    total_ms: u64,
    /// True if the recording hit the maximum length and its end was dropped.
    truncated: bool,
}

/// The current phase and the recording in progress, shared between the shortcut handler
/// and the dictation task.
#[derive(Default)]
pub struct Dictation {
    phase: Mutex<Phase>,
    /// Filled by `on_press` with the thread that opens the microphone, emptied by `on_release`.
    recording: Mutex<Option<PendingRecording>>,
}

/// The microphone being opened on its own thread: joining it gives the recording, or why the
/// microphone could not be opened.
type PendingRecording = std::thread::JoinHandle<Result<Recording, String>>;

/// Sends an event to the windows. During the first launch the window draws its own notch, so the
/// real one gets nothing: it would show the same thing twice.
fn emit_ui<S: Serialize + Clone>(
    app: &AppHandle,
    event: &str,
    payload: S,
    notch: bool,
) -> tauri::Result<()> {
    if notch {
        app.emit(event, payload)
    } else {
        app.emit_to("main", event, payload)
    }
}

fn emit_phase(app: &AppHandle, phase: Phase, failure: Option<Failure>) {
    let tray_state = match phase {
        Phase::Idle => TrayState::Idle,
        Phase::Recording => TrayState::Recording,
        Phase::Transcribing | Phase::Rewriting | Phase::Pasting => TrayState::Working,
    };
    trace!(
        "[dictation] phase -> {phase:?} (failure: {:?})",
        failure.as_ref().map(|f| f.kind)
    );
    tray::show_state(app, tray_state);
    // Escape is taken from the other apps only while there is something to cancel.
    shortcut::set_cancel_key(app, can_cancel(phase));
    tray::set_cancel_enabled(app, can_cancel(phase));
    if phase == Phase::Recording {
        indicator::follow_screens(app);
    }
    // A plain return to idle still goes to the notch, so that it can never stay on a phase.
    let notch = !trial::setup_open(app) || (phase == Phase::Idle && failure.is_none());
    let (error, failure) = match failure {
        Some(Failure { kind, message }) => (Some(message), Some(kind)),
        None => (None, None),
    };
    let change = PhaseChange {
        phase,
        error,
        failure,
    };
    if let Err(err) = emit_ui(app, PHASE_EVENT, change, notch) {
        eprintln!("[dictation] could not notify the UI: {err}");
    }
}

/// Cancelling is possible while recording (the user changed their mind), and while the text is
/// transcribed or rewritten. Not during the paste, which takes a fraction of a second.
fn can_cancel(phase: Phase) -> bool {
    matches!(
        phase,
        Phase::Recording | Phase::Transcribing | Phase::Rewriting
    )
}

/// The user asked to cancel (Escape, or the menu bar). Does nothing if there is nothing to
/// cancel.
///
/// While recording, the recording is thrown away and we go back to idle at once; the release
/// of the shortcut that follows is ignored, since we are no longer recording. While working,
/// the flag is set while the phase is locked, so a dictation that has just started pasting is
/// never cancelled halfway: either it sees the flag before pasting, or we see the phase and
/// refuse.
pub fn cancel(app: &AppHandle) {
    let state = app.state::<Dictation>();
    let phase = *state.phase.lock().unwrap_or_else(|e| e.into_inner());
    trace!("[dictation] cancel asked in phase {phase:?}");
    if phase == Phase::Recording {
        cancel_recording(app);
        return;
    }
    let phase = state.phase.lock().unwrap_or_else(|e| e.into_inner());
    if can_cancel(*phase) {
        trace!("[dictation] cancel requested");
        cancel::request();
    } else {
        trace!("[dictation] cancel ignored in phase {:?}", *phase);
    }
}

fn cancel_recording(app: &AppHandle) {
    let state = app.state::<Dictation>();
    let mut slot = state.recording.lock().unwrap_or_else(|e| e.into_inner());
    {
        let mut phase = state.phase.lock().unwrap_or_else(|e| e.into_inner());
        if *phase != Phase::Recording {
            // The key was released in the meantime: the dictation goes on.
            return;
        }
        *phase = Phase::Idle;
    }
    trace!("[dictation] recording cancelled");
    let pending = slot.take();
    drop(slot);
    // The microphone may still be opening, and this runs on the main thread: wait for it on
    // another thread. Dropping the recording tells the capture thread to stop; its audio is
    // thrown away.
    if let Some(pending) = pending {
        std::thread::spawn(move || drop(pending.join()));
    }
    emit_phase(
        app,
        Phase::Idle,
        Some(Failure::cancelled("while recording")),
    );
}

/// Moves from `from` to `to` and returns true, or returns false if we are not in `from`.
fn change_phase(app: &AppHandle, from: Phase, to: Phase, failure: Option<Failure>) -> bool {
    let state = app.state::<Dictation>();
    {
        let mut phase = state.phase.lock().unwrap_or_else(|e| e.into_inner());
        if *phase != from {
            return false;
        }
        *phase = to;
    }
    emit_phase(app, to, failure);
    true
}

/// Goes back to idle from whatever phase we are in. Used when a dictation ends.
fn back_to_idle(app: &AppHandle, failure: Option<Failure>) {
    let state = app.state::<Dictation>();
    *state.phase.lock().unwrap_or_else(|e| e.into_inner()) = Phase::Idle;
    emit_phase(app, Phase::Idle, failure);
}

fn millis(duration: Duration) -> u64 {
    duration.as_millis() as u64
}

/// Shortcut key went down. Ignored unless we are idle (so a press during a transcription
/// or a paste does nothing).
pub fn on_press(app: &AppHandle) {
    // A proposal to learn a word waiting for its answer would keep Escape from this dictation.
    learn::dismiss(app);
    if !change_phase(app, Phase::Idle, Phase::Recording, None) {
        return;
    }
    // The previous dictation is over (we were idle): forget a cancel that came too late for it.
    cancel::reset();
    // This runs on the main thread, which also draws the pill of the notch: opening the
    // microphone takes a few hundred milliseconds, so it is done on another thread. The release
    // (or a cancel) later waits for the result by joining the thread, so a very quick release
    // never looks for a recording that is not there yet.
    let level_app = app.clone();
    let failure_app = app.clone();
    let pending = std::thread::spawn(move || {
        let microphone = microphone::chosen(&level_app);
        let started = audio::start(microphone, move |level| {
            indicator::show_level(&level_app, level)
        });
        if let Err(message) = &started {
            eprintln!("[dictation] could not start recording: {message}");
            // Show the failure right away instead of when the key is released. If the key was
            // already released, the phase has moved on and `finish` reports the failure instead.
            change_phase(
                &failure_app,
                Phase::Recording,
                Phase::Idle,
                Some(Failure::microphone(message.clone())),
            );
        }
        started
    });
    let state = app.state::<Dictation>();
    *state.recording.lock().unwrap_or_else(|e| e.into_inner()) = Some(pending);
}

/// Shortcut key went up. Ignored unless a recording was started by a matching press.
pub fn on_release(app: &AppHandle) {
    let released_at = Instant::now();
    if !change_phase(app, Phase::Recording, Phase::Transcribing, None) {
        return;
    }
    let recording = {
        let state = app.state::<Dictation>();
        let mut slot = state.recording.lock().unwrap_or_else(|e| e.into_inner());
        slot.take()
    };
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        match finish(&app, recording, released_at).await {
            Ok(result) => {
                eprintln!(
                    "[dictation] {:.1} s of audio -> stop {} ms, transcription {} ms, rewrite {} ms, paste {} ms, total {} ms",
                    result.audio_ms as f32 / 1000.0,
                    result.stop_ms,
                    result.transcribe_ms,
                    result.rewrite_ms,
                    result.paste_ms,
                    result.total_ms
                );
                if let Some(reason) = &result.fallback_reason {
                    eprintln!("[dictation] raw transcription pasted, rewriting skipped: {reason}");
                }
                if result.truncated {
                    eprintln!("[dictation] the recording was cut at the maximum length");
                }
                // Lets the author collect real Whisper mistakes and see what the rewriting did.
                // The window shows the same texts from the history.
                eprintln!("[dictation] raw:    {}", one_line(&result.raw_text));
                eprintln!("[dictation] pasted: {}", one_line(&result.text));
                // Before going back to idle, or the indicator folds without the outcome.
                let notch = !trial::setup_open(&app);
                if let Err(err) = emit_ui(&app, RESULT_EVENT, result, notch) {
                    eprintln!("[dictation] could not notify the UI: {err}");
                }
                back_to_idle(&app, None);
            }
            Err(failure) => {
                trace!("[dictation] failed: {}", failure.message);
                back_to_idle(&app, Some(failure));
            }
        }
    });
}

/// A text on one line for the terminal (a list becomes "a / b / c").
fn one_line(text: &str) -> String {
    text.lines().collect::<Vec<_>>().join(" / ")
}

/// Puts the wanted words of the personal dictionary in the text about to be pasted. A dictionary
/// that cannot be read never stops the dictation: the text is pasted as it is, and the window
/// shows the error.
pub fn with_dictionary(app: &AppHandle, text: String) -> String {
    match dictionary::load(app) {
        Ok(pairs) => dictionary::apply(&pairs, &text),
        Err(message) => {
            history::report_error(app, message);
            text
        }
    }
}

/// The text to paste: the rewritten one, or the raw transcription (and why) if the rewriting
/// failed. The rewriting must never prevent pasting text.
fn choose_text(raw: &str, rewritten: Result<String, String>) -> (String, Option<String>) {
    match rewritten {
        Ok(text) => (text, None),
        Err(reason) => (raw.to_string(), Some(reason)),
    }
}

/// Whether the audio of a dictation is kept on this Mac. Settings that cannot be read keep
/// nothing, and the reason is shown rather than hidden.
fn keep_audio(app: &AppHandle) -> bool {
    match settings::load(app) {
        Ok(settings) => settings.keep_recordings,
        Err(message) => {
            history::report_error(app, message);
            false
        }
    }
}

/// Stops the recording, transcribes it, rewrites the text and pastes it.
async fn finish(
    app: &AppHandle,
    recording: Option<PendingRecording>,
    released_at: Instant,
) -> Result<DictationResult, Failure> {
    let pending = recording
        .ok_or_else(|| Failure::new(FailureKind::Other, "Aucun enregistrement en cours."))?;
    // Waiting for the microphone, stopping the capture thread and converting the audio all
    // block, so keep them off the async threads.
    let captured = tauri::async_runtime::spawn_blocking(move || {
        pending
            .join()
            .map_err(|_| "L'ouverture du micro a planté.".to_string())??
            .stop()
    })
    .await
    .map_err(|e| {
        Failure::new(
            FailureKind::MicrophoneFailed,
            format!("L'arrêt de l'enregistrement a planté : {e}"),
        )
    })?
    .map_err(Failure::microphone)?;
    let stop_ms = millis(released_at.elapsed());
    if cancel::is_requested() {
        return Err(Failure::cancelled("after the microphone stopped"));
    }

    let audio_seconds = captured.seconds();
    eprintln!(
        "[audio] captured {:.1} s, peak {:.3}, speech level {:.4}{}",
        audio_seconds,
        captured.peak(),
        captured.speech_level(),
        if captured.truncated {
            ", cut at the maximum length"
        } else {
            ""
        }
    );
    audio::save_debug_copy(&captured);
    permissions::note_microphone_signal(captured.peak() > 0.0);

    if audio_seconds < MIN_SECONDS {
        return Err(Failure::new(
            FailureKind::TooShort,
            format!(
                "Enregistrement trop court ({audio_seconds:.1} s). Maintiens le raccourci pendant que tu parles."
            ),
        ));
    }
    if captured.peak() == 0.0 {
        return Err(Failure::new(
            FailureKind::NoSound,
            "Aucun son capté. Vérifie la permission micro et le micro choisi dans les réglages de macOS.",
        ));
    }

    if captured.speech_level() < MIN_SPEECH_LEVEL {
        return Err(Failure::new(
            FailureKind::TooQuiet,
            "Aucune parole détectée (son trop faible). Parle plus près du micro.",
        ));
    }

    // Read before transcribing: why the model cannot be used, if it cannot.
    let model_failure = match app.state::<Transcriber>().status().state {
        "loading" => FailureKind::ModelLoading,
        "failed" => FailureKind::ModelMissing,
        _ => FailureKind::TranscriptionFailed,
    };
    let transcribe_start = Instant::now();
    let truncated = captured.truncated;
    let samples = captured.samples;
    // Kept for the recordings folder: the transcription takes the originals.
    let kept_samples = samples.clone();
    let model_app = app.clone();
    let transcribed = tauri::async_runtime::spawn_blocking(move || {
        model_app.state::<Transcriber>().transcribe(&samples)
    })
    .await;
    // Before reading the result: a cancelled Whisper returns an error that is no failure.
    if cancel::is_requested() {
        return Err(Failure::cancelled("after the transcription"));
    }
    let text = transcribed
        .map_err(|e| {
            Failure::new(
                FailureKind::TranscriptionFailed,
                format!("La transcription a planté : {e}"),
            )
        })?
        .map_err(|message| Failure::new(model_failure, message))?;
    let transcribe_ms = millis(transcribe_start.elapsed());
    if text.is_empty() {
        return Err(Failure::new(
            FailureKind::NothingHeard,
            "Aucune parole reconnue dans l'enregistrement.",
        ));
    }

    change_phase(app, Phase::Transcribing, Phase::Rewriting, None);
    if let Err(err) = app.emit(RAW_EVENT, &text) {
        eprintln!("[dictation] could not notify the UI: {err}");
    }
    let rewrite_start = Instant::now();
    let rewrite_app = app.clone();
    let to_rewrite = text.clone();
    let rewrite_timeout = rewrite::timeout_for(&text);
    let rewritten = tauri::async_runtime::spawn_blocking(move || {
        rewrite_app
            .state::<Rewriter>()
            .rewrite(&to_rewrite, rewrite_timeout)
    })
    .await
    .unwrap_or_else(|e| Err(format!("la reformulation a planté : {e}")));
    // Cancelled, not a rewriting failure: the raw text must not be pasted either.
    if cancel::is_requested() {
        return Err(Failure::cancelled("after the rewriting"));
    }
    let (pasted_text, fallback_reason) = choose_text(&text, rewritten);
    // Also applied to the raw transcription pasted after a failed rewriting.
    let pasted_text = with_dictionary(app, pasted_text);
    let rewrite_ms = millis(rewrite_start.elapsed());

    change_phase(app, Phase::Rewriting, Phase::Pasting, None);
    // A cancel that came just before this point (see `cancel`): the last moment to honour it.
    if cancel::is_requested() {
        return Err(Failure::cancelled("before the paste"));
    }
    let paste_start = Instant::now();
    let pasted = paste::paste_text(app.clone(), pasted_text.clone()).await;
    // The trial of the first launch leaves nothing behind: no context, no audio, no history.
    if !trial::is_dictation_trial(app) {
        if pasted.is_ok() {
            learn::remember_context(pasted_text.clone());
        }
        // Saved whatever the paste did: a text that could not be pasted can be copied from the
        // window. The audio goes with it, to listen again and compare with the texts, if the
        // user chose to keep it (off by default).
        let at_ms = history::now_ms();
        let audio_file = if keep_audio(app) {
            match recordings::save(app, at_ms, &kept_samples) {
                Ok(name) => Some(name),
                Err(message) => {
                    history::report_error(app, message);
                    None
                }
            }
        } else {
            None
        };
        // Before the history gets the dictation: a first start of the statistics reads the history.
        stats::record(app, at_ms, &pasted_text, f64::from(audio_seconds));
        history::record(
            app,
            Entry {
                at_ms,
                text: pasted_text.clone(),
                raw_text: text.clone(),
                fallback_reason: fallback_reason.clone(),
                paste_error: pasted.as_ref().err().cloned(),
                audio_file,
            },
        );
    }
    pasted.map_err(|message| {
        let kind = if message == paste::NO_ACCESSIBILITY {
            FailureKind::NoAccessibility
        } else {
            FailureKind::PasteFailed
        };
        Failure::new(kind, message)
    })?;

    Ok(DictationResult {
        text: pasted_text,
        raw_text: text,
        fallback_reason,
        audio_ms: (audio_seconds * 1000.0) as u64,
        stop_ms,
        transcribe_ms,
        rewrite_ms,
        paste_ms: millis(paste_start.elapsed()),
        total_ms: millis(released_at.elapsed()),
        truncated,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::Captured;
    use std::path::{Path, PathBuf};

    #[test]
    fn failure_kinds_are_sent_with_the_codes_of_the_page() {
        let code = |kind| serde_json::to_string(&kind).unwrap();
        assert_eq!(code(FailureKind::TooShort), "\"too_short\"");
        assert_eq!(code(FailureKind::NoAccessibility), "\"no_accessibility\"");
        assert_eq!(code(FailureKind::Other), "\"other\"");
    }

    #[test]
    fn a_missing_microphone_is_told_apart_from_a_broken_one() {
        let missing = Failure::microphone(audio::NO_MICROPHONE.to_string());
        assert_eq!(missing.kind, FailureKind::NoMicrophone);
        let broken = Failure::microphone("Erreur du micro : stream lost".to_string());
        assert_eq!(broken.kind, FailureKind::MicrophoneFailed);
    }

    fn level_of(path: &Path) -> f32 {
        let samples = hound::WavReader::open(path)
            .unwrap()
            .samples::<i16>()
            .map(|s| f32::from(s.unwrap()) / 32768.0)
            .collect();
        Captured {
            samples,
            truncated: false,
        }
        .speech_level()
    }

    fn wav_files(folder: &str) -> Vec<PathBuf> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join(folder);
        let mut files = Vec::new();
        for entry in std::fs::read_dir(root).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                files.extend(wav_files(&format!(
                    "{folder}/{}",
                    path.file_name().unwrap().to_string_lossy()
                )));
            } else if path.extension().is_some_and(|e| e == "wav") {
                files.push(path);
            }
        }
        files
    }

    #[test]
    fn every_recording_of_the_test_set_is_loud_enough() {
        let files = wav_files("tests/audio");
        assert!(files.len() >= 30);
        for file in files {
            let level = level_of(&file);
            assert!(
                level >= MIN_SPEECH_LEVEL,
                "{}: level {level}",
                file.display()
            );
        }
    }

    #[test]
    fn silent_rooms_are_below_the_threshold() {
        let files = wav_files("tests/silence");
        assert!(files.len() >= 2);
        for file in files {
            let level = level_of(&file);
            assert!(
                level < MIN_SPEECH_LEVEL,
                "{}: level {level}",
                file.display()
            );
        }
    }

    #[test]
    fn the_rewritten_text_is_pasted_when_the_rewriting_works() {
        let (text, reason) = choose_text("euh fais un merge", Ok("Fais un merge".to_string()));
        assert_eq!(text, "Fais un merge");
        assert_eq!(reason, None);
    }

    #[test]
    fn the_raw_transcription_is_pasted_when_the_rewriting_fails() {
        let (text, reason) = choose_text("euh fais un merge", Err("délai dépassé".to_string()));
        assert_eq!(text, "euh fais un merge");
        assert_eq!(reason.as_deref(), Some("délai dépassé"));
    }

    #[test]
    fn a_list_is_shown_on_one_line_in_the_terminal() {
        assert_eq!(one_line("Liste :\n- un\n- deux"), "Liste : / - un / - deux");
        assert_eq!(one_line("Une ligne"), "Une ligne");
    }
}
