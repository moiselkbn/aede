//! Rewrites the transcription as a clear written request with a local LLM (llama.cpp).
//!
//! Settings come from the prototype 3 measurements (docs/prototype-3.md): model
//! Qwen3-4B-Instruct-2507 (Q4_K_M), prompt v5, greedy decoding.
//!
//! The model is big (2.5 GB), so it is loaded once, in the background, when the app starts.
//! Everything that touches llama.cpp lives on one dedicated thread; the rest of the app sends
//! it texts and waits for the answer. Any error means "paste the raw transcription instead":
//! the caller decides, and shows the message to the user.

mod check;
mod engine;
mod prompt;

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use llama_cpp_2::llama_backend::LlamaBackend;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::models;
use engine::Engine;

/// The prompt (rules and examples), kept next to the tool that measures it.
const PROMPT: &str = include_str!("../../tools/llm-bench/prompts/v5.txt");

/// How long a rewrite may take. The model writes about 80 characters per second on a quiet
/// machine (prototype 3: 21 to 25 tokens per second), so the delay grows with the length of
/// the text: a fixed 3 s would always give up on a long dictation. It allows about 40 characters
/// per second, half that speed, because the model is 3 to 4 times slower when the machine is
/// busy and the first version (12 ms per character) gave up on 4 dictations out of 28 in the
/// week of tests. The MVP budget is 4 s for a 10 s dictation (PROJET.md), which a quiet machine
/// still respects: the delay is only a limit, not a wait.
const MIN_TIMEOUT: Duration = Duration::from_secs(3);
const MAX_TIMEOUT: Duration = Duration::from_secs(15);
const TIMEOUT_BASE: Duration = Duration::from_secs(2);
const TIMEOUT_PER_CHAR: Duration = Duration::from_millis(25);

/// The delay to rewrite `text`: from 3 s for a short text up to 15 s for a very long one.
pub fn timeout_for(text: &str) -> Duration {
    let chars = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
    (TIMEOUT_BASE + TIMEOUT_PER_CHAR.saturating_mul(chars)).clamp(MIN_TIMEOUT, MAX_TIMEOUT)
}

/// The worker checks its own deadline at every token; this is the extra wait for a step that
/// is slow to finish, before giving up on the answer.
const REPLY_GRACE: Duration = Duration::from_secs(2);

/// Name of the event the UI listens to when the model finishes loading.
const STATUS_EVENT: &str = "rewrite-status";

/// What the UI shows about the model.
#[derive(Clone, Serialize)]
pub struct RewriteStatus {
    /// "loading", "ready" or "failed"
    state: &'static str,
    /// Set when the state is "failed".
    error: Option<String>,
}

#[derive(Default)]
enum State {
    #[default]
    Loading,
    Ready,
    Failed(String),
}

struct Request {
    text: String,
    timeout: Duration,
    /// An answer after this moment is useless: the caller has given up.
    deadline: Instant,
    reply: Sender<Result<String, String>>,
}

/// The state of the model and the way to reach the worker thread.
#[derive(Default)]
pub struct Rewriter {
    state: Mutex<State>,
    requests: Mutex<Option<Sender<Request>>>,
    /// The worker thread, so that quitting can wait for it to free the model.
    worker: Mutex<Option<JoinHandle<()>>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl Rewriter {
    pub fn status(&self) -> RewriteStatus {
        match &*lock(&self.state) {
            State::Loading => RewriteStatus {
                state: "loading",
                error: None,
            },
            State::Ready => RewriteStatus {
                state: "ready",
                error: None,
            },
            State::Failed(message) => RewriteStatus {
                state: "failed",
                error: Some(message.clone()),
            },
        }
    }

    /// Rewrites `text`. Blocks until done, so call it off the async threads. Any error means
    /// the text could not be rewritten in time or properly: use the raw transcription.
    pub fn rewrite(&self, text: &str, timeout: Duration) -> Result<String, String> {
        match &*lock(&self.state) {
            State::Loading => {
                return Err("le modèle de reformulation se charge encore".to_string());
            }
            State::Failed(message) => return Err(message.clone()),
            State::Ready => {}
        }
        let sender = lock(&self.requests)
            .clone()
            .ok_or("le moteur de reformulation n'est pas démarré")?;
        let (reply, answer) = mpsc::channel();
        sender
            .send(Request {
                text: text.to_string(),
                timeout,
                deadline: Instant::now() + timeout + REPLY_GRACE,
                reply,
            })
            .map_err(|_| "le moteur de reformulation s'est arrêté".to_string())?;
        answer
            .recv_timeout(timeout + REPLY_GRACE)
            .map_err(|_| "le moteur de reformulation ne répond plus".to_string())?
    }

    fn set_state(&self, app: &AppHandle, state: State) {
        *lock(&self.state) = state;
        if let Err(err) = app.emit(STATUS_EVENT, self.status()) {
            eprintln!("[rewrite] could not notify the UI: {err}");
        }
    }
}

/// Runs the rewriting again on what Whisper heard for a dictation of the history, and returns
/// the new text, with the personal dictionary as it is now (a word learned since then counts).
/// The history is not changed: the window shows the result next to the old text.
#[tauri::command]
pub async fn rewrite_again(app: AppHandle, at_ms: u64) -> Result<String, String> {
    let entry = crate::history::find_entry(&app, at_ms)?
        .ok_or("Cette dictée n'existe plus dans l'historique.")?;
    let timeout = timeout_for(&entry.raw_text);
    let rewrite_app = app.clone();
    let rewritten = tauri::async_runtime::spawn_blocking(move || {
        rewrite_app
            .state::<Rewriter>()
            .rewrite(&entry.raw_text, timeout)
            .map_err(|reason| format!("Reformulation impossible : {reason}."))
    })
    .await
    .unwrap_or_else(|err| Err(format!("La reformulation a planté : {err}")))?;
    Ok(crate::dictation::with_dictionary(&app, rewritten))
}

/// What the UI asks at startup, in case the model finished loading before it was listening.
#[tauri::command]
pub fn get_rewrite_status(rewriter: tauri::State<'_, Rewriter>) -> RewriteStatus {
    rewriter.status()
}

/// Starts the worker thread, which loads the model and then answers the rewrite requests.
/// The app stays usable meanwhile: a dictation started too early pastes the raw transcription.
pub fn start(app: &AppHandle) {
    let rewriter = app.state::<Rewriter>();
    rewriter.set_state(app, State::Loading);
    let (sender, receiver) = mpsc::channel();
    *lock(&rewriter.requests) = Some(sender);
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("rewrite".into())
        .spawn(move || worker(&app, receiver));
    match spawned {
        Ok(handle) => *lock(&rewriter.worker) = Some(handle),
        Err(err) => eprintln!("[rewrite] could not start the thread: {err}"),
    }
}

/// Frees the model before the app quits. llama.cpp aborts the process at exit if the model is
/// still on the GPU, which macOS reports as a crash. Dropping the sender ends the worker's
/// loop; the worker then frees the context, the model and the backend, in that order. Waits for
/// a load or a rewrite in progress to finish first.
pub fn stop(app: &AppHandle) {
    let rewriter = app.state::<Rewriter>();
    lock(&rewriter.requests).take();
    let worker = lock(&rewriter.worker).take();
    if let Some(worker) = worker
        && worker.join().is_err()
    {
        eprintln!("[rewrite] the worker thread panicked");
    }
}

/// Starts the worker again if it failed, typically because the model was not downloaded yet.
pub fn restart_if_failed(app: &AppHandle) {
    let failed = matches!(*lock(&app.state::<Rewriter>().state), State::Failed(_));
    if failed {
        start(app);
    }
}

fn worker(app: &AppHandle, requests: Receiver<Request>) {
    let rewriter = app.state::<Rewriter>();
    let fail = |message: String| {
        eprintln!("[rewrite] {message}");
        rewriter.set_state(app, State::Failed(message));
    };

    let messages = match prompt::parse(PROMPT) {
        Ok(messages) => messages,
        Err(message) => return fail(message),
    };
    let path = match models::path(app, &models::LLM) {
        Ok(path) => path,
        Err(message) => return fail(message),
    };
    // Checked before starting llama.cpp, so that a missing file (first launch) leaves nothing
    // half started: the worker starts again cleanly once the file is downloaded.
    if !path.is_file() {
        return fail(format!(
            "Modèle de reformulation introuvable : {}",
            path.display()
        ));
    }
    eprintln!("[rewrite] loading {}", path.display());
    let start = Instant::now();
    let mut backend = match LlamaBackend::init() {
        Ok(backend) => backend,
        Err(err) => return fail(format!("llama.cpp ne démarre pas : {err}")),
    };
    // llama.cpp is chatty: keep its logs out of the terminal.
    backend.void_logs();
    let model = match engine::load_model(&backend, &path) {
        Ok(model) => model,
        Err(message) => return fail(message),
    };
    let mut engine = match Engine::new(&backend, &model, &messages) {
        Ok(engine) => engine,
        Err(message) => return fail(message),
    };
    let loaded = start.elapsed();

    // The first run also prepares the GPU, so do it now with a short text.
    let warm_start = Instant::now();
    let warm_up = engine.rewrite(
        "Fais un merge de la branche courante sur main.",
        Duration::from_secs(30),
    );
    eprintln!(
        "[rewrite] model loaded in {:.1} s, warm-up {:.1} s{}",
        loaded.as_secs_f64(),
        warm_start.elapsed().as_secs_f64(),
        warm_up.err().map(|e| format!(" ({e})")).unwrap_or_default()
    );
    rewriter.set_state(app, State::Ready);

    for request in requests {
        let answer = if Instant::now() > request.deadline {
            // The caller stopped waiting while an earlier text was being rewritten.
            Err("délai dépassé".to_string())
        } else {
            engine.rewrite(&request.text, request.timeout)
        };
        // The caller may have given up already: nobody to tell then.
        let _ = request.reply.send(answer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewriting_before_the_model_is_loaded_says_so() {
        let message = Rewriter::default()
            .rewrite("Fais un merge", MIN_TIMEOUT)
            .unwrap_err();
        assert!(message.contains("se charge encore"));
    }

    #[test]
    fn a_failed_model_keeps_its_error() {
        let rewriter = Rewriter {
            state: Mutex::new(State::Failed("boom".to_string())),
            ..Default::default()
        };
        assert_eq!(
            rewriter.rewrite("Fais un merge", MIN_TIMEOUT).unwrap_err(),
            "boom"
        );
        assert_eq!(rewriter.status().state, "failed");
    }

    #[test]
    fn the_delay_grows_with_the_text_between_3_and_15_seconds() {
        assert_eq!(timeout_for("Fais un merge"), Duration::from_secs(3));
        assert_eq!(timeout_for(""), Duration::from_secs(3));
        // 278 and 374 characters: the two dictations of the week of tests that gave up with
        // 4.3 s and 5.5 s.
        assert_eq!(timeout_for(&"a".repeat(278)), Duration::from_millis(8950));
        assert_eq!(timeout_for(&"a".repeat(374)), Duration::from_millis(11350));
        assert!(timeout_for(&"a".repeat(374)) > timeout_for(&"a".repeat(278)));
        // About 560 characters and more reach the limit.
        assert_eq!(timeout_for(&"a".repeat(560)), Duration::from_secs(15));
        assert_eq!(timeout_for(&"a".repeat(5000)), Duration::from_secs(15));
    }

    #[test]
    fn a_missing_model_gives_a_clear_error() {
        let backend = LlamaBackend::init();
        // The backend can only start once per process: another test may hold it.
        if let Ok(backend) = backend {
            let message = engine::load_model(&backend, std::path::Path::new("/nonexistent/m.gguf"))
                .err()
                .unwrap();
            assert!(message.contains("introuvable"));
            assert!(message.contains("/nonexistent/m.gguf"));
        }
    }

    /// Reads the "Entrée" and "Sortie" lines of a llm-bench report.
    fn read_report(path: &std::path::Path) -> Vec<(String, String, String)> {
        let report = std::fs::read_to_string(path).unwrap();
        let mut rows = Vec::new();
        let (mut id, mut input) = (String::new(), String::new());
        for line in report.lines() {
            if let Some(rest) = line.strip_prefix("### ") {
                id = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("Entrée : ") {
                input = rest.to_string();
            } else if line.starts_with("Sortie (") {
                let output = line.split_once(") : ").unwrap().1;
                rows.push((id.clone(), input.clone(), output.to_string()));
            }
        }
        rows
    }

    /// Needs the model file (2.5 GB). Cancelling must stop the rewriting within a token or two.
    /// Run it alone: cargo test --release -- --ignored --nocapture cancelling_stops
    #[test]
    #[ignore]
    fn cancelling_stops_the_rewriting_early() {
        let messages = prompt::parse(PROMPT).unwrap();
        let backend = LlamaBackend::init().unwrap();
        let model = engine::load_model(&backend, &models::path_for_tests(&models::LLM)).unwrap();
        let mut engine = Engine::new(&backend, &model, &messages).unwrap();
        let long = "Le communisme est une idéologie politique qui repose sur l'idée de la mutualisation des ressources. Il n'y a plus de patrons, il n'y a plus de travailleurs. Tous sont travailleurs et tous jouissent des ressources obtenues par le biais du travail.";

        crate::cancel::reset();
        let canceller = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(500));
            crate::cancel::request();
        });
        let start = Instant::now();
        let answer = engine.rewrite(long, Duration::from_secs(30));
        let elapsed = start.elapsed().as_millis();
        canceller.join().unwrap();
        crate::cancel::reset();
        eprintln!("cancelled after 500 ms, returned after {elapsed} ms: {answer:?}");
        assert_eq!(answer, Err("annulé".to_string()));
        assert!(elapsed < 1000, "the rewriting took {elapsed} ms to stop");
    }

    /// Needs the model file (2.5 GB), so it is skipped by default. It checks that the app
    /// gives exactly the texts recorded by llm-bench for prompt v5 on the validation set 3
    /// (docs/prototype-3), and that a tiny delay and a huge text give clear errors.
    /// Run it with: cargo test -- --ignored --nocapture rewrites_like_the_bench
    #[test]
    #[ignore]
    fn rewrites_like_the_bench() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let rows = read_report(&root.join("docs/prototype-3/validation-3-qwen3-4b-instruct-v5.md"));
        assert_eq!(rows.len(), 22);

        let messages = prompt::parse(PROMPT).unwrap();
        let backend = LlamaBackend::init().unwrap();
        let model = engine::load_model(&backend, &models::path_for_tests(&models::LLM)).unwrap();
        let mut engine = Engine::new(&backend, &model, &messages).unwrap();
        // Warm-up, as the app does.
        engine
            .rewrite(
                "Fais un merge de la branche courante sur main.",
                Duration::from_secs(30),
            )
            .unwrap();

        let mut different = Vec::new();
        let mut durations = Vec::new();
        for (id, input, bench_output) in &rows {
            let start = Instant::now();
            let answer = engine.rewrite(input, Duration::from_secs(30));
            durations.push(start.elapsed().as_millis());
            // The report writes lists on one line.
            let text = answer
                .as_ref()
                .map(|t| t.lines().collect::<Vec<_>>().join(" / "));
            if text.as_deref() != Ok(bench_output.as_str()) {
                different.push(format!("{id}:\n  app:   {text:?}\n  bench: {bench_output}"));
            }
        }
        durations.sort_unstable();
        eprintln!(
            "median {} ms, max {} ms",
            durations[durations.len() / 2],
            durations.last().unwrap()
        );
        assert!(
            different.is_empty(),
            "{} phrase(s) differ from the bench:\n{}",
            different.len(),
            different.join("\n")
        );

        // A long dictation (about 560 characters) that gave up with a fixed 3 s delay.
        let long = "Le communisme est une idéologie politique qui repose sur l'idée de la mutualisation des ressources. Il n'y a plus de patrons, il n'y a plus de travailleurs. Tous sont travailleurs et tous jouissent des ressources obtenues par le biais du travail. Chaque individu possède donc l'entreprise entre guillemets et chacun est conseiller administratif de l'entreprise. Ainsi, chacun a un pouvoir décisionnaire et peut donc avoir une incidence sur l'entreprise.";
        let start = Instant::now();
        let answer = engine.rewrite(long, timeout_for(long));
        eprintln!(
            "long text: {} characters, {} ms, delay {:?}",
            long.chars().count(),
            start.elapsed().as_millis(),
            timeout_for(long)
        );
        assert!(
            answer.is_ok(),
            "the long text should be rewritten: {answer:?}"
        );

        let late = engine.rewrite(
            "Fais un merge de la branche courante sur main",
            Duration::ZERO,
        );
        assert!(late.unwrap_err().contains("délai dépassé"));
        let huge = "Fais un test de la page. ".repeat(400);
        assert!(
            engine
                .rewrite(&huge, Duration::from_secs(30))
                .unwrap_err()
                .contains("trop long")
        );
    }

    /// Needs both models (1.6 GB and 2.5 GB), so it is skipped by default. It runs what a
    /// dictation does after the recording, on the 32 recordings of the test set: Whisper, then
    /// the rewriting with the delay of the app. It prints one line per recording and the
    /// summary: the numbers of docs/assemblage.md.
    /// Run it with: cargo test --release -- --ignored --nocapture pipeline_on_the_test_set
    #[test]
    #[ignore]
    fn pipeline_on_the_test_set() {
        use crate::transcription;

        /// The paste takes about 0.3 s (measured in the app).
        const PASTE_MS: u128 = 300;

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let test_set = std::fs::read_to_string(root.join("tests/jargon-fr.md")).unwrap();
        let audios: Vec<&str> = test_set
            .lines()
            .filter_map(|line| line.strip_prefix("Audio :"))
            .map(str::trim)
            .collect();
        assert!(audios.len() >= 30);
        // The English terms listed under each recording ("Termes :" line), in the same order.
        let mut terms: Vec<Vec<String>> = Vec::new();
        for line in test_set.lines() {
            if line.starts_with("Audio :") {
                terms.push(Vec::new());
            } else if let Some(list) = line.strip_prefix("Termes :")
                && let Some(last) = terms.last_mut()
            {
                *last = list
                    .split(',')
                    .map(|term| term.trim().to_string())
                    .filter(|term| !term.is_empty())
                    .collect();
            }
        }
        assert_eq!(terms.len(), audios.len());

        // Same rule as tools/whisper-bench/src/score.rs: lowercase whole words, an apostrophe
        // closes a word, a run of single letters ("a p i") is glued into one word.
        fn tokens(text: &str) -> Vec<String> {
            let mut raw: Vec<String> = Vec::new();
            let mut word = String::new();
            for c in text.to_lowercase().chars() {
                if c.is_alphanumeric() {
                    word.push(c);
                } else if c == '\'' || c == '\u{2019}' {
                    word.push('\'');
                    raw.push(std::mem::take(&mut word));
                } else if !word.is_empty() {
                    raw.push(std::mem::take(&mut word));
                }
            }
            if !word.is_empty() {
                raw.push(word);
            }
            let is_letter = |t: &str| t.chars().count() == 1 && t.chars().all(char::is_alphabetic);
            let mut out: Vec<String> = Vec::new();
            let mut i = 0;
            while i < raw.len() {
                let mut j = i;
                while j < raw.len() && is_letter(&raw[j]) {
                    j += 1;
                }
                if j - i >= 2 {
                    out.push(raw[i..j].concat());
                    i = j;
                } else {
                    out.push(raw[i].clone());
                    i += 1;
                }
            }
            out
        }
        fn term_found(term: &str, words: &[String]) -> bool {
            let wanted = tokens(term);
            wanted.is_empty() || words.windows(wanted.len()).any(|w| w == wanted.as_slice())
        }
        // A sentence with 3 of these words is probably English: it must be read by a person.
        const ENGLISH: [&str; 14] = [
            "the", "and", "is", "are", "this", "that", "with", "for", "you", "please", "of", "to",
            "it", "in",
        ];

        let mut whisper = transcription::load(&models::path_for_tests(&models::WHISPER)).unwrap();
        transcription::run(&mut whisper, &[0.0; 16_000]).unwrap();

        let messages = prompt::parse(PROMPT).unwrap();
        let backend = LlamaBackend::init().unwrap();
        let model = engine::load_model(&backend, &models::path_for_tests(&models::LLM)).unwrap();
        let mut engine = Engine::new(&backend, &model, &messages).unwrap();
        engine
            .rewrite(
                "Fais un merge de la branche courante sur main.",
                Duration::from_secs(30),
            )
            .unwrap();

        let mut totals = Vec::new();
        let mut fallbacks = 0;
        let (mut terms_total, mut terms_in_raw, mut terms_in_final) = (0, 0, 0);
        for (index, audio) in audios.iter().enumerate() {
            let samples: Vec<f32> = hound::WavReader::open(root.join(audio))
                .unwrap()
                .samples::<i16>()
                .map(|s| f32::from(s.unwrap()) / 32768.0)
                .collect();
            let seconds = samples.len() as f32 / 16_000.0;

            let start = Instant::now();
            let raw = transcription::run(&mut whisper, &samples).unwrap();
            let transcribe_ms = start.elapsed().as_millis();
            let start = Instant::now();
            let rewritten = engine.rewrite(&raw, timeout_for(&raw));
            let rewrite_ms = start.elapsed().as_millis();

            let total = transcribe_ms + rewrite_ms + PASTE_MS;
            totals.push((seconds, total));
            let outcome = match &rewritten {
                Ok(_) => "reformulé".to_string(),
                Err(reason) => {
                    fallbacks += 1;
                    format!("REPLI ({reason})")
                }
            };
            eprintln!(
                "{audio}: {seconds:>5.1} s d'audio, transcription {transcribe_ms:>5} ms, reformulation {rewrite_ms:>5} ms, total {total:>5} ms, {outcome}"
            );

            // What the app pastes: the rewritten text, or the raw one after a fallback.
            let pasted: &str = match &rewritten {
                Ok(text) => text,
                Err(_) => &raw,
            };
            let raw_words = tokens(&raw);
            let final_words = tokens(pasted);
            let wanted = &terms[index];
            let in_raw = wanted
                .iter()
                .filter(|term| term_found(term, &raw_words))
                .count();
            let missing: Vec<&String> = wanted
                .iter()
                .filter(|term| !term_found(term, &final_words))
                .collect();
            terms_total += wanted.len();
            terms_in_raw += in_raw;
            terms_in_final += wanted.len() - missing.len();
            let english = final_words
                .iter()
                .filter(|word| ENGLISH.contains(&word.as_str()))
                .count();
            eprintln!(
                "  TERMES whisper {in_raw}/{n}, final {}/{n}, manquants {missing:?}{}",
                wanted.len() - missing.len(),
                if english >= 3 { "  ANGLAIS ?" } else { "" },
                n = wanted.len()
            );
            eprintln!("  BRUT  : {raw}");
            eprintln!("  FINAL : {pasted}");
        }
        eprintln!(
            "TERMES ANGLAIS : Whisper seul {terms_in_raw}/{terms_total}, texte final {terms_in_final}/{terms_total}"
        );

        let mut sorted: Vec<u128> = totals.iter().map(|(_, total)| *total).collect();
        sorted.sort_unstable();
        eprintln!(
            "{} enregistrements, {} repli(s). Total (collage compté 0,3 s) : médiane {} ms, maximum {} ms, {} au-dessus de 4 s",
            totals.len(),
            fallbacks,
            sorted[sorted.len() / 2],
            sorted.last().unwrap(),
            sorted.iter().filter(|&&t| t > 4000).count()
        );
        let around_10s: Vec<u128> = totals
            .iter()
            .filter(|(seconds, _)| (7.0..=13.0).contains(seconds))
            .map(|(_, total)| *total)
            .collect();
        eprintln!(
            "pour 7 à 13 s d'audio : {} enregistrements, totaux {:?} ms",
            around_10s.len(),
            around_10s
        );
        let pid = std::process::id().to_string();
        if let Ok(out) = std::process::Command::new("ps")
            .args(["-o", "rss=", "-p", &pid])
            .output()
        {
            let kb: u64 = String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse()
                .unwrap_or(0);
            eprintln!("mémoire résidente du test : {} Mo", kb / 1024);
        }
        eprintln!("charge de la machine : {}", {
            let out = std::process::Command::new("uptime").output().unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        });
    }
}
