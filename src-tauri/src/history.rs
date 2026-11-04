//! The dictation history, saved in history.jsonl in the app data folder: one dictation per
//! line, added at the end. Every text is kept, even when the paste failed, so the window can
//! copy it later. The file only stays on this Mac and only the user can read it. The audio of
//! each dictation is next to it (see recordings.rs).

use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::{dictionary, feedback, recordings, trace};

const FILE_NAME: &str = "history.jsonl";

/// Sent to the window with each new entry.
const ADDED_EVENT: &str = "history-added";

/// Sent to the window when an entry could not be saved: the text is then only in the terminal.
const ERROR_EVENT: &str = "history-error";

/// Only one write at a time: a dictation must not append while "clear all" removes the file.
static FILE_LOCK: Mutex<()> = Mutex::new(());

/// One dictation that produced a text.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// When the dictation ended, in milliseconds since 1970. The window formats it.
    pub at_ms: u64,
    /// The final text: the rewritten one, or the raw transcription on a fallback.
    pub text: String,
    /// What Whisper heard, before the rewriting.
    pub raw_text: String,
    /// Set when the raw transcription was used instead of the rewritten text: why.
    pub fallback_reason: Option<String>,
    /// Set when the paste failed: why. The text is still here to be copied.
    pub paste_error: Option<String>,
    /// Name of the WAV in the recordings folder. Not set for the dictations saved before the audio
    /// was kept, and when the audio could not be saved.
    #[serde(default)]
    pub audio_file: Option<String>,
}

/// What the window gets when it opens.
#[derive(Debug, PartialEq, Serialize)]
pub struct History {
    /// Oldest first, in the order of the file.
    entries: Vec<Entry>,
    /// Lines that could not be read (a damaged file). Shown to the user, never hidden.
    unreadable_lines: usize,
}

pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("Dossier de données de l'app introuvable : {err}"))?;
    Ok(data.join(FILE_NAME))
}

fn lock() -> std::sync::MutexGuard<'static, ()> {
    FILE_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Saves a dictation and tells the window. Never fails: the history must not stop the
/// dictation, so an error is shown in the window instead.
pub fn record(app: &AppHandle, entry: Entry) {
    let saved = path(app).and_then(|path| {
        let _guard = lock();
        append_to(&path, &entry)
    });
    match saved {
        Ok(()) => {
            if let Err(err) = app.emit(ADDED_EVENT, entry) {
                eprintln!("[history] could not notify the UI: {err}");
            }
        }
        Err(message) => report_error(app, message),
    }
}

/// Shows an error in the history of the window (the terminal gets it too).
pub fn report_error(app: &AppHandle, message: String) {
    eprintln!("[history] {message}");
    if let Err(err) = app.emit(ERROR_EVENT, message) {
        eprintln!("[history] could not notify the UI: {err}");
    }
}

fn append_to(path: &Path, entry: &Entry) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|err| format!("Impossible de créer le dossier {} : {err}", dir.display()))?;
    }
    let mut line = serde_json::to_string(entry)
        .map_err(|err| format!("Impossible d'enregistrer la dictée dans l'historique : {err}"))?;
    line.push('\n');
    // 0o600: only the user can read the file. Applies when the file is created.
    OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(path)
        .and_then(|mut file| file.write_all(line.as_bytes()))
        .map_err(|err| {
            format!(
                "Impossible d'enregistrer la dictée dans l'historique ({}) : {err}",
                path.display()
            )
        })
}

fn load_from(path: &Path) -> Result<History, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(format!("Historique illisible ({}) : {err}", path.display())),
    };
    let mut history = History {
        entries: Vec::new(),
        unreadable_lines: 0,
    };
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        match serde_json::from_str(line) {
            Ok(entry) => history.entries.push(entry),
            Err(_) => history.unreadable_lines += 1,
        }
    }
    Ok(history)
}

fn clear_at(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "Impossible d'effacer l'historique ({}) : {err}",
            path.display()
        )),
    }
}

#[tauri::command]
pub fn get_history(app: AppHandle) -> Result<History, String> {
    let path = path(&app)?;
    let _guard = lock();
    load_from(&path)
}

/// Every dictation of the history, oldest first. The statistics are built from it once.
pub fn all_entries(app: &AppHandle) -> Result<Vec<Entry>, String> {
    let path = path(app)?;
    let _guard = lock();
    Ok(load_from(&path)?.entries)
}

/// Replaces a file with `content` without ever leaving it half written: the text goes to a
/// temporary file first (0o600, like the file itself), which then takes the place of the old one.
pub fn replace_file(path: &Path, content: &str) -> Result<(), String> {
    let temp = path.with_extension("jsonl.tmp");
    let failed = |err: std::io::Error| {
        format!(
            "Impossible de réécrire le fichier {} : {err}",
            path.display()
        )
    };
    OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .mode(0o600)
        .open(&temp)
        .and_then(|mut file| file.write_all(content.as_bytes()))
        .and_then(|()| std::fs::rename(&temp, path))
        .map_err(failed)
}

/// Removes the line of the dictation that ended at `at_ms` and returns that dictation. The other
/// lines are kept exactly as they are, including the ones that cannot be read. `None` when
/// the dictation is not in the file.
fn remove_from(path: &Path, at_ms: u64) -> Result<Option<Entry>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(format!("Historique illisible ({}) : {err}", path.display())),
    };
    let mut removed = None;
    let mut kept = String::new();
    for line in text.lines() {
        match serde_json::from_str::<Entry>(line) {
            Ok(entry) if entry.at_ms == at_ms && removed.is_none() => removed = Some(entry),
            _ => {
                kept.push_str(line);
                kept.push('\n');
            }
        }
    }
    if removed.is_some() {
        replace_file(path, &kept)?;
    }
    Ok(removed)
}

/// Deletes one dictation: its line, its audio and the notes written about it. Everything is
/// tried even if one part fails, and the failures are reported together.
#[tauri::command]
pub fn delete_entry(app: AppHandle, at_ms: u64) -> Result<(), String> {
    let removed = path(&app).and_then(|path| {
        let _guard = lock();
        remove_from(&path, at_ms)
    })?;
    let Some(entry) = removed else {
        return Err("Cette dictée n'existe plus dans l'historique.".to_string());
    };
    let audio = match &entry.audio_file {
        Some(file) => recordings::remove(&app, file),
        None => Ok(()),
    };
    let failures: Vec<String> = [audio, feedback::remove_about(&app, at_ms)]
        .into_iter()
        .filter_map(Result::err)
        .collect();
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join(" "))
    }
}

/// The most recent dictation, if there is one.
pub fn last_entry(app: &AppHandle) -> Result<Option<Entry>, String> {
    let path = path(app)?;
    let _guard = lock();
    Ok(load_from(&path)?.entries.pop())
}

/// The dictation that ended at `at_ms`, if it is still in the history.
pub fn find_entry(app: &AppHandle, at_ms: u64) -> Result<Option<Entry>, String> {
    let path = path(app)?;
    let _guard = lock();
    Ok(load_from(&path)?
        .entries
        .into_iter()
        .find(|entry| entry.at_ms == at_ms))
}

/// Deletes the file, the audio, the notes and the personal dictionary: nothing of the user is
/// left on disk. Everything is tried even if one part fails, and the failures are reported together.
#[tauri::command]
pub fn clear_history(app: AppHandle) -> Result<(), String> {
    let history = path(&app).and_then(|path| {
        let _guard = lock();
        clear_at(&path)
    });
    let failures: Vec<String> = [
        history,
        recordings::clear(&app),
        feedback::clear(&app),
        trace::clear(&app),
        dictionary::clear(&app),
    ]
    .into_iter()
    .filter_map(Result::err)
    .collect();
    // The dictionary list, if the window shows it, must follow.
    let _ = app.emit(dictionary::CHANGED_EVENT, ());
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join(" "))
    }
}

/// Puts a text of the history in the clipboard.
#[tauri::command]
pub fn copy_text(app: AppHandle, text: String) -> Result<(), String> {
    app.clipboard()
        .write_text(text)
        .map_err(|err| format!("Impossible d'écrire dans le presse-papiers : {err}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aede-history-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join(FILE_NAME)
    }

    fn entry(text: &str) -> Entry {
        Entry {
            at_ms: 1_761_000_000_000,
            text: text.to_string(),
            raw_text: format!("euh {text}"),
            fallback_reason: None,
            paste_error: Some("Permission accessibilité manquante.".to_string()),
            audio_file: Some("1761000000000.wav".to_string()),
        }
    }

    #[test]
    fn no_file_gives_an_empty_history() {
        let history = load_from(&temp_file("none")).unwrap();
        assert!(history.entries.is_empty());
        assert_eq!(history.unreadable_lines, 0);
    }

    #[test]
    fn entries_are_read_back_in_order() {
        let path = temp_file("order");
        append_to(&path, &entry("Fais le deploy.")).unwrap();
        append_to(&path, &entry("Ouvre une pull request.")).unwrap();
        let history = load_from(&path).unwrap();
        assert_eq!(
            history.entries,
            vec![entry("Fais le deploy."), entry("Ouvre une pull request.")]
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn only_the_user_can_read_the_file() {
        let path = temp_file("mode");
        append_to(&path, &entry("Texte privé.")).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_damaged_line_is_counted_and_the_others_kept() {
        let path = temp_file("damaged");
        append_to(&path, &entry("Avant.")).unwrap();
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"at_ms\": 12, \"te\n").unwrap();
        append_to(&path, &entry("Après.")).unwrap();
        let history = load_from(&path).unwrap();
        assert_eq!(history.entries, vec![entry("Avant."), entry("Après.")]);
        assert_eq!(history.unreadable_lines, 1);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_line_saved_before_the_audio_was_kept_is_still_read() {
        let path = temp_file("old");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            &path,
            "{\"at_ms\":1,\"text\":\"Fais le deploy.\",\"raw_text\":\"euh fais le deploy\",\"fallback_reason\":null,\"paste_error\":null}\n",
        )
        .unwrap();
        let history = load_from(&path).unwrap();
        assert_eq!(history.unreadable_lines, 0);
        assert_eq!(history.entries[0].audio_file, None);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn clearing_removes_everything() {
        let path = temp_file("clear");
        append_to(&path, &entry("À effacer.")).unwrap();
        clear_at(&path).unwrap();
        assert!(!path.exists());
        assert!(load_from(&path).unwrap().entries.is_empty());
        // Clearing an empty history is not an error.
        clear_at(&path).unwrap();
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn deleting_one_dictation_keeps_the_others_and_the_damaged_lines() {
        let path = temp_file("delete");
        let mut first = entry("Première.");
        first.at_ms = 1;
        let mut second = entry("Deuxième.");
        second.at_ms = 2;
        append_to(&path, &first).unwrap();
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"at_ms\": 12, \"te\n").unwrap();
        append_to(&path, &second).unwrap();

        assert_eq!(remove_from(&path, 1).unwrap(), Some(first));
        let history = load_from(&path).unwrap();
        assert_eq!(history.entries, vec![second]);
        assert_eq!(history.unreadable_lines, 1);
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn deleting_a_dictation_that_is_not_there_changes_nothing() {
        let path = temp_file("delete-missing");
        // No file at all.
        assert_eq!(remove_from(&path, 1).unwrap(), None);
        append_to(&path, &entry("Reste.")).unwrap();
        let before = std::fs::read_to_string(&path).unwrap();
        assert_eq!(remove_from(&path, 99).unwrap(), None);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), before);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
