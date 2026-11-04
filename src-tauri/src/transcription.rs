//! Speech to text with whisper.cpp (through whisper-rs).
//!
//! The model is big (1.6 GB), so it is loaded once, in the background, when the app starts.
//! Settings come from the prototype 2 measurements (docs/prototype-2.md):
//! model large-v3-turbo, French forced, no initial prompt, greedy decoding.

use std::path::Path;
use std::sync::{Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::Instant;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use whisper_rs::{
    FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters, WhisperState,
};

use crate::cancel;
use crate::models;

/// Where the model is: loading, usable, or failed to load.
#[derive(Default)]
enum Model {
    #[default]
    Loading,
    Ready(WhisperState),
    Failed(String),
}

/// Name of the event the UI listens to when the model finishes loading.
const STATUS_EVENT: &str = "model-status";

/// What the UI shows about the model.
#[derive(Clone, Serialize)]
pub struct ModelStatus {
    /// "loading", "ready" or "failed". Also read by the dictation to name a failure.
    pub(crate) state: &'static str,
    /// Set when the state is "failed".
    error: Option<String>,
}

/// Holds the model, shared between the loading thread and the dictation task.
#[derive(Default)]
pub struct Transcriber {
    model: Mutex<Model>,
    /// The thread loading the model, so that quitting can wait for it.
    loader: Mutex<Option<JoinHandle<()>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Reads the model file and prepares it on the GPU.
pub(crate) fn load(path: &Path) -> Result<WhisperState, String> {
    if !path.is_file() {
        return Err(format!(
            "Modèle de transcription introuvable : {}",
            path.display()
        ));
    }
    let path_str = path
        .to_str()
        .ok_or("Le chemin du modèle contient des caractères illisibles.")?;
    let context = WhisperContext::new_with_params(path_str, WhisperContextParameters::default())
        .map_err(|e| format!("Chargement du modèle impossible : {e}"))?;
    context
        .create_state()
        .map_err(|e| format!("Préparation du modèle impossible : {e}"))
}

/// Runs Whisper on mono 16 kHz audio and returns the text on one line.
pub(crate) fn run(state: &mut WhisperState, samples: &[f32]) -> Result<String, String> {
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_language(Some("fr"));
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    // Lets the user cancel: whisper.cpp asks this during the work and stops when it is true.
    params.set_abort_callback_safe(cancel::is_requested);

    state
        .full(params, samples)
        .map_err(|e| format!("Transcription impossible : {e}"))?;

    let text: String = state.as_iter().map(|segment| segment.to_string()).collect();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    // On noise Whisper sometimes writes only punctuation ("..."). That is no speech.
    Ok(if has_words(&text) {
        text
    } else {
        String::new()
    })
}

/// True if the text contains at least one letter or digit.
fn has_words(text: &str) -> bool {
    text.chars().any(char::is_alphanumeric)
}

impl Transcriber {
    pub fn status(&self) -> ModelStatus {
        match &*lock(&self.model) {
            Model::Loading => ModelStatus {
                state: "loading",
                error: None,
            },
            Model::Ready(_) => ModelStatus {
                state: "ready",
                error: None,
            },
            Model::Failed(message) => ModelStatus {
                state: "failed",
                error: Some(message.clone()),
            },
        }
    }

    /// Transcribes mono 16 kHz audio. Blocks until done, so call it off the async threads.
    /// An empty string means Whisper heard no speech.
    pub fn transcribe(&self, samples: &[f32]) -> Result<String, String> {
        match &mut *lock(&self.model) {
            Model::Loading => {
                Err("Le modèle de transcription se charge encore, réessaie dans un instant.".into())
            }
            Model::Failed(message) => Err(message.clone()),
            Model::Ready(state) => run(state, samples),
        }
    }
}

/// What the UI asks at startup, in case the model finished loading before it was listening.
#[tauri::command]
pub fn get_model_status(transcriber: tauri::State<'_, Transcriber>) -> ModelStatus {
    transcriber.status()
}

/// Stores the new state of the model and tells the UI.
fn set_model(app: &AppHandle, model: Model) {
    let transcriber = app.state::<Transcriber>();
    *lock(&transcriber.model) = model;
    if let Err(err) = app.emit(STATUS_EVENT, transcriber.status()) {
        eprintln!("[transcription] could not notify the UI: {err}");
    }
}

/// Starts loading the model on its own thread. The app stays usable meanwhile:
/// a dictation started too early gets a clear message instead of a freeze.
pub fn load_in_background(app: &AppHandle) {
    set_model(app, Model::Loading);
    let thread_app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("model-loader".into())
        .spawn(move || {
            let app = thread_app;
            let result = models::path(&app, &models::WHISPER).and_then(|path| {
                eprintln!("[transcription] loading {}", path.display());
                let start = Instant::now();
                let mut state = load(&path)?;
                let loaded = start.elapsed();
                // The first run also prepares the GPU, so do it now with one second of silence.
                let warm_start = Instant::now();
                run(&mut state, &[0.0; 16_000])?;
                eprintln!(
                    "[transcription] model loaded in {:.1} s, warm-up {:.1} s",
                    loaded.as_secs_f64(),
                    warm_start.elapsed().as_secs_f64()
                );
                Ok(state)
            });
            let model = match result {
                Ok(state) => Model::Ready(state),
                Err(message) => {
                    eprintln!("[transcription] {message}");
                    Model::Failed(message)
                }
            };
            set_model(&app, model);
        });
    match spawned {
        Ok(handle) => *lock(&app.state::<Transcriber>().loader) = Some(handle),
        Err(err) => eprintln!("[transcription] could not start the loading thread: {err}"),
    }
}

/// Frees the model before the app quits. whisper.cpp aborts the process at exit if the model
/// is still on the GPU, which macOS reports as a crash. Waits for a load in progress first,
/// and for a transcription in progress (it holds the model).
pub fn unload(app: &AppHandle) {
    let transcriber = app.state::<Transcriber>();
    let loader = lock(&transcriber.loader).take();
    if let Some(loader) = loader
        && loader.join().is_err()
    {
        eprintln!("[transcription] the loading thread panicked");
    }
    // Replacing the model drops it, which frees its GPU memory.
    *lock(&transcriber.model) = Model::Failed("Aède se ferme.".to_string());
}

/// Loads the model again if it failed, typically because its file was not downloaded yet.
pub fn reload_if_failed(app: &AppHandle) {
    let failed = matches!(*lock(&app.state::<Transcriber>().model), Model::Failed(_));
    if failed {
        load_in_background(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_model_gives_a_clear_error() {
        let message = load(Path::new("/nonexistent/ggml-nothing.bin"))
            .err()
            .unwrap();
        assert!(message.contains("introuvable"));
        assert!(message.contains("/nonexistent/ggml-nothing.bin"));
    }

    #[test]
    fn transcribing_before_the_model_is_loaded_says_so() {
        let transcriber = Transcriber::default();
        let message = transcriber.transcribe(&[0.0; 16_000]).unwrap_err();
        assert!(message.contains("se charge encore"));
    }

    #[test]
    fn text_with_only_punctuation_is_not_words() {
        assert!(!has_words(""));
        assert!(!has_words("..."));
        assert!(!has_words(" . - ? "));
        assert!(has_words("Merci."));
        assert!(has_words("42"));
    }

    #[test]
    fn a_failed_model_keeps_its_error() {
        let transcriber = Transcriber {
            model: Mutex::new(Model::Failed("boom".to_string())),
            ..Default::default()
        };
        assert_eq!(transcriber.transcribe(&[]).unwrap_err(), "boom");
    }

    /// Reads a 16 kHz mono WAV from the test set.
    fn read_wav(path: &Path) -> Vec<f32> {
        hound::WavReader::open(path)
            .unwrap()
            .samples::<i16>()
            .map(|s| f32::from(s.unwrap()) / 32768.0)
            .collect()
    }

    /// Needs the model file (1.6 GB). Cancelling must stop Whisper in the middle of a long
    /// audio, not at the end. Run it alone: cargo test -- --ignored --nocapture cancelling_stops
    #[test]
    #[ignore]
    fn cancelling_stops_whisper_early() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/audio");
        let mut samples = Vec::new();
        let mut files: Vec<_> = std::fs::read_dir(root.join("dev"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e == "wav"))
            .collect();
        files.sort();
        for file in files.iter().cycle().take(12) {
            samples.extend(read_wav(file));
        }
        let mut state = load(&models::path_for_tests(&models::WHISPER)).unwrap();

        cancel::reset();
        let start = Instant::now();
        let full = run(&mut state, &samples);
        let full_ms = start.elapsed().as_millis();
        eprintln!(
            "{:.0} s of audio, full run {full_ms} ms",
            samples.len() as f32 / 16000.0
        );
        assert!(full.is_ok());

        let canceller = std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(300));
            cancel::request();
        });
        let start = Instant::now();
        let cancelled = run(&mut state, &samples);
        let cancelled_ms = start.elapsed().as_millis();
        canceller.join().unwrap();
        cancel::reset();
        eprintln!("cancelled after 300 ms, returned after {cancelled_ms} ms: {cancelled:?}");
        assert!(cancelled.is_err());
        assert!(
            cancelled_ms < 1500,
            "Whisper took {cancelled_ms} ms to stop"
        );
    }

    /// Needs the model file (1.6 GB), so it is skipped by default. It checks that the app
    /// gives exactly the texts recorded in the prototype 2 report for large-v3-turbo.
    /// Run it with: cargo test -- --ignored --nocapture transcribes_the_test_set
    #[test]
    #[ignore]
    fn transcribes_the_test_set_like_the_bench() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");

        // phrase id -> audio file, from the test set.
        let test_set = std::fs::read_to_string(root.join("tests/jargon-fr.md")).unwrap();
        let mut audio_of = std::collections::HashMap::new();
        let mut id = String::new();
        for line in test_set.lines() {
            if let Some(rest) = line.strip_prefix("#### ") {
                id = rest.trim().to_string();
            } else if let Some(path) = line.strip_prefix("Audio :") {
                audio_of.insert(id.clone(), path.trim().to_string());
            }
        }

        // phrase id -> text written by the bench in français forcé, sans vocabulaire.
        let report =
            std::fs::read_to_string(root.join("docs/prototype-2/whisper-large-v3-turbo.md"))
                .unwrap();
        let mut expected = Vec::new();
        let mut id = String::new();
        for line in report.lines() {
            if let Some(rest) = line.strip_prefix("### ") {
                id = rest.split_whitespace().next().unwrap().to_string();
            } else if let Some(rest) = line.strip_prefix("- `fr-none` (") {
                let text = rest.split_once(") : ").unwrap().1;
                expected.push((id.clone(), text.to_string()));
            }
        }
        assert_eq!(expected.len(), 32);

        let start = Instant::now();
        let mut state = load(&models::path_for_tests(&models::WHISPER)).unwrap();
        eprintln!("model loaded in {:.1} s", start.elapsed().as_secs_f64());

        let mut different = Vec::new();
        for (id, bench_text) in &expected {
            let samples = read_wav(&root.join(&audio_of[id]));
            let text = run(&mut state, &samples).unwrap();
            if &text != bench_text {
                different.push(format!("{id}:\n  app:   {text}\n  bench: {bench_text}"));
            }
        }
        assert!(
            different.is_empty(),
            "{} phrase(s) differ from the bench:\n{}",
            different.len(),
            different.join("\n")
        );
    }
}
