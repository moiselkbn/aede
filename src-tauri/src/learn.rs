//! Learning a word: the global shortcut that adds to the dictionary what the user corrected in
//! the last dictation.
//!
//! First press: the last dictation is compared with the text now in the field (see
//! corrections.rs), and the corrections found are proposed. Second press within ten seconds:
//! they are added to the dictionary. Escape, or waiting, cancels: nothing is ever added without
//! the second press.
//!
//! After each paste, the few words around the dictation are remembered, in memory only and for
//! a few minutes: they tell where the dictation is in the field, which a correction at its very
//! start or end could not tell otherwise.
//!
//! While the indicator still says that words were added (a couple of seconds), pressing again
//! for those very words changes nothing: "already in the dictionary" is only said once the
//! indicator has left that state.
//!
//! The answers are shown by the indicator under the notch (see indicator.rs), which never takes
//! the focus, so the text of the user stays where it is. If the indicator cannot be reached, a
//! macOS notification says the same thing.

use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread::sleep;
use std::time::{Duration, Instant};

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Shortcut, ShortcutEvent, ShortcutState,
};

use crate::corrections::{self, Correction, Surroundings};
use crate::dictionary;
use crate::field::{self, Reading};
use crate::history;
use crate::indicator;
use crate::paste;
use crate::settings;
use crate::shortcut::{self as talk_shortcut, ShortcutStatus};
use crate::trace::trace;
use crate::trial;

/// How long a proposal waits for the second press.
const PROPOSAL_LIFETIME: Duration = Duration::from_secs(10);

/// How long the indicator says that words were added (`LEARN_DONE_MS` in src/Indicator.tsx: keep
/// the two equal).
const ADDED_STAYS: Duration = Duration::from_secs(2);

/// How long the words around a pasted dictation are remembered.
const CONTEXT_LIFETIME: Duration = Duration::from_secs(15 * 60);

/// Pause between the paste and the reading of the field: the app must have inserted the text.
/// To tune by testing.
const AFTER_PASTE: Duration = Duration::from_millis(250);

/// The words around the last pasted dictation. Kept in memory only, never written to disk.
struct Remembered {
    /// The pasted text, to tell which dictation these words belong to.
    text: String,
    surroundings: Surroundings,
    at: Instant,
}

/// Corrections shown to the user, waiting for the second press.
struct Proposal {
    id: u64,
    corrections: Vec<Correction>,
    until: Instant,
}

/// Something that went wrong or was refused: a few words for the indicator, and the full
/// sentence for the window and the log.
struct Problem {
    short: &'static str,
    full: String,
    /// False for what is normal and needs no alert in the window (nothing to correct, known).
    report: bool,
}

impl From<String> for Problem {
    fn from(full: String) -> Problem {
        // The sentences come from corrections.rs, history.rs and paste.rs: the start of each
        // tells which one it is.
        let (short, report) = if full.starts_with("Aucune dictée") {
            ("Aucune dictée à corriger", false)
        } else if full.starts_with("Déjà dans") {
            ("Déjà dans le dictionnaire", false)
        } else if full.starts_with("Aucun mot") {
            ("Aucun mot changé", false)
        } else if full.contains("corrections d'un coup") {
            ("Trop de corrections", true)
        } else if full.starts_with("Je ne sais pas si") {
            ("Correction ambiguë", true)
        } else if full.starts_with("Plusieurs mots") {
            ("Plusieurs mots changés", true)
        } else if full.starts_with("Je ne retrouve pas") {
            ("Dictée introuvable", true)
        } else if full == paste::NO_ACCESSIBILITY {
            ("Accès au texte refusé", true)
        } else {
            ("Apprentissage impossible", true)
        };
        Problem {
            short,
            full,
            report,
        }
    }
}

impl From<&str> for Problem {
    fn from(full: &str) -> Problem {
        Problem::from(full.to_string())
    }
}

/// The learning shortcut in use, shared between startup and the window that changes it.
#[derive(Default)]
pub struct CurrentLearnShortcut {
    /// None when no shortcut could be registered.
    pub shortcut: Mutex<Option<Shortcut>>,
    status: Mutex<ShortcutStatus>,
}

static REMEMBERED: Mutex<Option<Remembered>> = Mutex::new(None);
static PROPOSAL: Mutex<Option<Proposal>> = Mutex::new(None);
/// The corrections added by the last confirmation, and when.
static JUST_ADDED: Mutex<Option<(Vec<Correction>, Instant)>> = Mutex::new(None);
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// True while a press is being handled: a second press during the reading is ignored.
static BUSY: AtomicBool = AtomicBool::new(false);

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Called by the plugin on every press and every release of the shortcut.
pub(crate) fn handle(app: &AppHandle, _shortcut: &Shortcut, event: ShortcutEvent) {
    // During the first launch the window shows the learning itself (see trial.rs): the real
    // field of another application is not read and the real notch is not used.
    if trial::setup_open(app) {
        trial::take_learn_press(app, event.state() == ShortcutState::Pressed);
        return;
    }
    if event.state() == ShortcutState::Pressed {
        let app = app.clone();
        tauri::async_runtime::spawn(async move { press(app).await });
    }
}

/// The shortcut for people, e.g. "Option+Maj+Espace".
fn label_of(app: &AppHandle) -> String {
    lock(&app.state::<CurrentLearnShortcut>().status)
        .label
        .clone()
}

/// Registers the shortcut saved in the settings, at startup. Must run after the push-to-talk
/// one, which it must not be equal to.
pub fn register(app: &AppHandle) {
    let (text, settings_error) = match settings::load(app) {
        Ok(settings) => (settings.learn_shortcut, None),
        Err(message) => (settings::DEFAULT_LEARN_SHORTCUT.to_string(), Some(message)),
    };
    let talk = app.state::<talk_shortcut::CurrentShortcut>();
    let result = talk_shortcut::parse(&text).and_then(|shortcut| {
        if *talk_shortcut::lock(&talk.shortcut) == Some(shortcut) {
            return Err(format!(
                "{} sert déjà à dicter : choisis une autre combinaison pour apprendre un mot.",
                talk_shortcut::label(&shortcut)
            ));
        }
        app.global_shortcut()
            .on_shortcut(shortcut, handle)
            .map_err(|err| {
                format!(
                    "Impossible d'activer le raccourci {} pour apprendre un mot : {err}",
                    talk_shortcut::label(&shortcut)
                )
            })?;
        Ok(shortcut)
    });

    let current = app.state::<CurrentLearnShortcut>();
    let status = match result {
        Ok(shortcut) => {
            *lock(&current.shortcut) = Some(shortcut);
            ShortcutStatus {
                label: talk_shortcut::label(&shortcut),
                error: settings_error,
            }
        }
        Err(message) => ShortcutStatus {
            label: text,
            error: Some(message),
        },
    };
    if let Some(message) = &status.error {
        eprintln!("[learn] {message}");
        history::report_error(app, message.clone());
    }
    *lock(&current.status) = status;
}

#[tauri::command]
pub fn get_learn_shortcut_status(
    current: tauri::State<'_, CurrentLearnShortcut>,
) -> ShortcutStatus {
    lock(&current.status).clone()
}

/// Replaces the learning shortcut by `text` (e.g. "control+shift+KeyD") and saves it. If macOS
/// refuses the new one, the old one stays active. A sync command: Tauri runs it on the main
/// thread, where macOS wants shortcuts to be registered.
#[tauri::command]
pub fn set_learn_shortcut(
    app: AppHandle,
    current: tauri::State<'_, CurrentLearnShortcut>,
    talk: tauri::State<'_, talk_shortcut::CurrentShortcut>,
    text: String,
) -> Result<ShortcutStatus, String> {
    let new = talk_shortcut::parse(&text)?;
    if *talk_shortcut::lock(&talk.shortcut) == Some(new) {
        return Err(format!(
            "{} sert déjà à dicter : choisis une autre combinaison.",
            talk_shortcut::label(&new)
        ));
    }
    let mut registered = lock(&current.shortcut);
    if let Err(message) = talk_shortcut::replace(&app, &mut registered, new, handle) {
        if registered.is_none() {
            eprintln!("[learn] {message}");
            lock(&current.status).error = Some(message.clone());
        }
        return Err(message);
    }

    let mut status = ShortcutStatus {
        label: talk_shortcut::label(&new),
        error: None,
    };
    let saved = settings::load(&app).and_then(|mut settings| {
        settings.learn_shortcut = new.into_string();
        settings::save(&app, &settings)
    });
    if let Err(message) = saved {
        status.error = Some(format!(
            "Raccourci actif, mais pas enregistré pour le prochain lancement : {message}"
        ));
    }
    eprintln!("[learn] shortcut now {}", status.label);
    *lock(&current.status) = status.clone();
    Ok(status)
}

async fn press(app: AppHandle) {
    if BUSY.swap(true, Ordering::SeqCst) {
        return;
    }
    let proposal = lock(&PROPOSAL)
        .take()
        .filter(|proposal| proposal.until > Instant::now());
    let outcome = match proposal {
        Some(proposal) => {
            set_escape(&app, false);
            confirm(&app, proposal).map(Some)
        }
        None => propose(&app).await.map(|()| None),
    };
    BUSY.store(false, Ordering::SeqCst);
    match outcome {
        // The proposal is already on screen, or there is nothing to say.
        Ok(None) => {}
        Ok(Some(done)) => say(&app, true, "Ajouté au dictionnaire", &done),
        Err(problem) => {
            if problem.report {
                history::report_error(&app, problem.full.clone());
            }
            say(&app, false, problem.short, &problem.full);
        }
    }
}

/// Tells the user how an answer ended: in the indicator, or in a notification if the indicator
/// cannot be reached. `full` goes to the log and to the notification.
fn say(app: &AppHandle, ok: bool, short: &str, full: &str) {
    let outcome = indicator::Outcome {
        ok,
        text: short.to_string(),
    };
    if indicator::show_outcome(app, &outcome) {
        eprintln!("[learn] {full}");
    } else {
        notify(full);
    }
}

fn describe(corrections: &[Correction]) -> String {
    let list: Vec<String> = corrections
        .iter()
        .map(|(wrong, right)| format!("{wrong} → {right}"))
        .collect();
    list.join(" ; ")
}

fn confirm(app: &AppHandle, proposal: Proposal) -> Result<String, Problem> {
    for (wrong, right) in &proposal.corrections {
        dictionary::add(app, wrong, right)?;
    }
    *lock(&JUST_ADDED) = Some((proposal.corrections.clone(), Instant::now()));
    // Tells the window, if it is open, to refresh its list.
    let _ = app.emit(dictionary::CHANGED_EVENT, ());
    Ok(format!(
        "Ajouté au dictionnaire : {}",
        describe(&proposal.corrections)
    ))
}

/// The example of the learning trial of the first launch (src/setup/useLearnDemo.ts shows the
/// same two words): what Whisper "heard" and what the user selected.
const TRIAL_WRONG: &str = "super base";
const TRIAL_RIGHT: &str = "Supabase";

/// The user confirmed, in the window of the first launch, the example pair of the learning trial
/// (see trial.rs). Only this pair can be added this way, and only while the first launch is
/// open: no field of another application is read. If the dictionary already has it (an earlier
/// first launch), it stays as it is: the trial still says "added".
#[tauri::command]
pub fn add_trial_word(app: AppHandle) -> Result<(), String> {
    if !trial::setup_open(&app) {
        return Err("L'essai n'est possible que pendant le premier lancement.".to_string());
    }
    let known = dictionary::load(&app)?;
    let already = known
        .iter()
        .any(|pair| pair.wrong.to_lowercase() == TRIAL_WRONG && pair.right == TRIAL_RIGHT);
    if !already {
        dictionary::add(&app, TRIAL_WRONG, TRIAL_RIGHT)?;
        let _ = app.emit(dictionary::CHANGED_EVENT, ());
    }
    Ok(())
}

/// Reads the field, finds the corrections and shows them. Nothing is saved yet.
async fn propose(app: &AppHandle) -> Result<(), Problem> {
    if !tauri_plugin_macos_permissions::check_accessibility_permission().await {
        return Err(paste::NO_ACCESSIBILITY.into());
    }
    let last =
        history::last_entry(app)?.ok_or("Aucune dictée à corriger : l'historique est vide.")?;
    let read_app = app.clone();
    let read = tauri::async_runtime::spawn_blocking(move || field::read_or_copy(&read_app))
        .await
        .map_err(|err| format!("La lecture du texte a planté : {err}"))??;
    // Words remembered at the paste only describe a text read as a whole.
    let around = remembered_for(&last.text).filter(|_| read.reading == Reading::Accessibility);
    let found = corrections::find_corrections(
        &last.text,
        &read.text,
        around.as_ref(),
        read.selection.as_deref(),
    )?;
    // The indicator still says these words were added: nothing to show, nothing to change.
    if is_just_added(&found) {
        eprintln!("[learn] pressed again while the words just added are shown: nothing to do");
        return Ok(());
    }

    let known = dictionary::load(app)?;
    let new: Vec<Correction> = found
        .iter()
        .filter(|(wrong, right)| {
            !known.iter().any(|pair| {
                pair.wrong.to_lowercase() == wrong.to_lowercase() && pair.right == *right
            })
        })
        .cloned()
        .collect();
    if new.is_empty() {
        return Err(format!("Déjà dans le dictionnaire : {}", describe(&found)).into());
    }

    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    let shortcut = label_of(app);
    let message = format!(
        "Ajouter {} ? Refais {shortcut} pour valider, Échap pour annuler.",
        describe(&new)
    );
    let view = indicator::Proposal {
        pairs: new.clone(),
        shortcut,
        seconds: PROPOSAL_LIFETIME.as_secs(),
    };
    *lock(&PROPOSAL) = Some(Proposal {
        id,
        corrections: new,
        until: Instant::now() + PROPOSAL_LIFETIME,
    });
    set_escape(app, true);
    let timer_app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        sleep(PROPOSAL_LIFETIME);
        expire(&timer_app, id);
    });
    if indicator::show_proposal(app, &view) {
        eprintln!("[learn] {message}");
    } else {
        notify(&message);
    }
    Ok(())
}

/// The proposal `id` was not answered in time.
fn expire(app: &AppHandle, id: u64) {
    let expired = {
        let mut proposal = lock(&PROPOSAL);
        if proposal.as_ref().is_some_and(|p| p.id == id) {
            *proposal = None;
            true
        } else {
            false
        }
    };
    if expired {
        set_escape(app, false);
        if !indicator::clear_proposal(app) {
            notify("Proposition expirée : rien n'a été ajouté.");
        }
    }
}

/// Escape was pressed while a proposal was waiting.
fn cancel(app: &AppHandle) {
    if lock(&PROPOSAL).take().is_some() {
        set_escape(app, false);
        if !indicator::clear_proposal(app) {
            notify("Proposition annulée : rien n'a été ajouté.");
        }
    }
}

/// Drops the proposal in silence. Called when a dictation starts: it needs Escape for itself.
pub fn dismiss(app: &AppHandle) {
    if lock(&PROPOSAL).take().is_some() {
        set_escape(app, false);
        indicator::clear_proposal(app);
    }
}

/// Escape is taken from the other apps only while a proposal waits.
fn set_escape(app: &AppHandle, active: bool) {
    // macOS wants shortcuts to be registered on the main thread.
    let main_app = app.clone();
    let result = app.run_on_main_thread(move || {
        let global = main_app.global_shortcut();
        let key = Shortcut::new(None, Code::Escape);
        trace!("[learn] Escape {}", if active { "taken" } else { "freed" });
        if active {
            let registered = global.on_shortcut(key, |app, _shortcut, event| {
                if event.state() == ShortcutState::Pressed {
                    cancel(app);
                }
            });
            if let Err(err) = registered {
                eprintln!("[learn] Escape not registered: {err}");
            }
        } else if let Err(err) = global.unregister(key) {
            eprintln!("[learn] Escape not freed: {err}");
        }
    });
    if let Err(err) = result {
        eprintln!("[learn] could not reach the main thread: {err}");
    }
}

/// True when `found` is made of the corrections added by the last confirmation (same wrong word
/// whatever the case, same right word) and only of them.
fn all_in(found: &[Correction], added: &[Correction]) -> bool {
    !found.is_empty()
        && found.iter().all(|(wrong, right)| {
            added.iter().any(|(added_wrong, added_right)| {
                added_wrong.to_lowercase() == wrong.to_lowercase() && added_right == right
            })
        })
}

/// True while the indicator still says that exactly these corrections were added.
fn is_just_added(found: &[Correction]) -> bool {
    lock(&JUST_ADDED)
        .as_ref()
        .is_some_and(|(added, at)| at.elapsed() < ADDED_STAYS && all_in(found, added))
}

fn remembered_for(text: &str) -> Option<Surroundings> {
    lock(&REMEMBERED)
        .as_ref()
        .filter(|r| r.text == text && r.at.elapsed() < CONTEXT_LIFETIME)
        .map(|r| r.surroundings.clone())
}

/// Called after a dictation was pasted: reads the field in the background and remembers a few
/// words before and after the pasted text. Never blocks the dictation, and never fails it: with
/// nothing remembered, the shortcut still works for most corrections.
pub fn remember_context(text: String) {
    tauri::async_runtime::spawn_blocking(move || {
        sleep(AFTER_PASTE);
        let surroundings = field::read_accessible()
            .and_then(|field_text| corrections::surroundings_of(&text, &field_text));
        *lock(&REMEMBERED) = match surroundings {
            Ok(surroundings) => Some(Remembered {
                text,
                surroundings,
                at: Instant::now(),
            }),
            Err(why) => {
                eprintln!("[learn] words around the dictation not remembered: {why}");
                None
            }
        };
    });
}

/// A macOS notification through osascript. The text goes in as an argument, never inside the
/// script, so words with quotes cannot change the script.
fn notify(text: &str) {
    eprintln!("[learn] {text}");
    let result = Command::new("/usr/bin/osascript")
        .args([
            "-e",
            "on run argv",
            "-e",
            "display notification (item 1 of argv) with title \"Aède\"",
            "-e",
            "end run",
            "--",
            text,
        ])
        .output();
    if let Err(err) = result {
        eprintln!("[learn] notification failed: {err}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pair(wrong: &str, right: &str) -> Correction {
        (wrong.to_string(), right.to_string())
    }

    #[test]
    fn the_same_words_are_just_added() {
        let added = vec![pair("test", "testeur")];
        assert!(all_in(&[pair("test", "testeur")], &added));
        assert!(all_in(&[pair("Test", "testeur")], &added));
    }

    #[test]
    fn a_new_word_is_not_just_added() {
        let added = vec![pair("test", "testeur")];
        assert!(!all_in(
            &[pair("test", "testeur"), pair("un", "une")],
            &added
        ));
        assert!(!all_in(&[pair("un", "une")], &added));
        // Same wrong word, another right word: a new correction.
        assert!(!all_in(&[pair("test", "testeurs")], &added));
    }

    #[test]
    fn nothing_found_is_not_just_added() {
        assert!(!all_in(&[], &[pair("test", "testeur")]));
        assert!(!all_in(&[pair("test", "testeur")], &[]));
    }
}
