//! The notes the author writes during the test week ("Signaler un problème"), saved in
//! feedback.jsonl in the app data folder: one note per line, added at the end. A note about a
//! dictation carries a copy of that dictation (texts and name of the audio file), so the file
//! can be read on its own later, without opening the history. The file only stays on this Mac
//! and only the user can read it.

use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::history::{self, Entry};

const FILE_NAME: &str = "feedback.jsonl";

/// Only one write at a time: a note must not be added while "clear all" removes the file.
static FILE_LOCK: Mutex<()> = Mutex::new(());

/// One note written by the user.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Note {
    /// When the note was written, in milliseconds since 1970.
    pub at_ms: u64,
    /// What the user wrote: what went wrong, or an opinion.
    pub comment: String,
    /// The dictation the note is about, as it was in the history. Empty for a general opinion.
    pub dictation: Option<Entry>,
}

/// What the window gets when it opens.
#[derive(Debug, PartialEq, Serialize)]
pub struct Notes {
    /// Oldest first, in the order of the file.
    notes: Vec<Note>,
    /// Lines that could not be read (a damaged file). Shown to the user, never hidden.
    unreadable_lines: usize,
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

fn append_to(path: &Path, note: &Note) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|err| format!("Impossible de créer le dossier {} : {err}", dir.display()))?;
    }
    let mut line = serde_json::to_string(note)
        .map_err(|err| format!("Impossible d'enregistrer la note : {err}"))?;
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
                "Impossible d'enregistrer la note ({}) : {err}",
                path.display()
            )
        })
}

fn load_from(path: &Path) -> Result<Notes, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(format!("Notes illisibles ({}) : {err}", path.display())),
    };
    let mut notes = Notes {
        notes: Vec::new(),
        unreadable_lines: 0,
    };
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        match serde_json::from_str(line) {
            Ok(note) => notes.notes.push(note),
            Err(_) => notes.unreadable_lines += 1,
        }
    }
    Ok(notes)
}

fn clear_at(path: &Path) -> Result<(), String> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "Impossible d'effacer les notes ({}) : {err}",
            path.display()
        )),
    }
}

/// Deletes the file. Called by "clear all" of the history.
pub fn clear(app: &AppHandle) -> Result<(), String> {
    let path = path(app)?;
    let _guard = lock();
    clear_at(&path)
}

/// Removes the notes written about the dictation that ended at `at_ms`: they carry a copy of
/// its text, which must not outlive it. The other lines stay as they are, damaged ones included.
fn remove_about_in(path: &Path, at_ms: u64) -> Result<(), String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(format!("Notes illisibles ({}) : {err}", path.display())),
    };
    let about_it = |line: &str| {
        serde_json::from_str::<Note>(line)
            .is_ok_and(|note| note.dictation.is_some_and(|entry| entry.at_ms == at_ms))
    };
    let kept: Vec<&str> = text.lines().filter(|line| !about_it(line)).collect();
    if kept.len() == text.lines().count() {
        return Ok(());
    }
    let mut content = kept.join("\n");
    if !content.is_empty() {
        content.push('\n');
    }
    history::replace_file(path, &content)
}

/// Called when a dictation is deleted from the history.
pub fn remove_about(app: &AppHandle, at_ms: u64) -> Result<(), String> {
    let path = path(app)?;
    let _guard = lock();
    remove_about_in(&path, at_ms)
}

/// The note to save, or why the comment cannot be saved.
fn make_note(comment: &str, dictation: Option<Entry>, at_ms: u64) -> Result<Note, String> {
    let comment = comment.trim();
    if comment.is_empty() {
        return Err("Écris quelques mots avant d'enregistrer la note.".to_string());
    }
    Ok(Note {
        at_ms,
        comment: comment.to_string(),
        dictation,
    })
}

#[tauri::command]
pub fn get_feedback(app: AppHandle) -> Result<Notes, String> {
    let path = path(&app)?;
    let _guard = lock();
    load_from(&path)
}

/// Saves a note. `entry_at_ms` is the dictation it is about (its `at_ms` in the history), or
/// nothing for a general opinion.
#[tauri::command]
pub fn add_feedback(
    app: AppHandle,
    entry_at_ms: Option<u64>,
    comment: String,
) -> Result<Note, String> {
    let dictation = match entry_at_ms {
        Some(at_ms) => Some(
            history::find_entry(&app, at_ms)?
                .ok_or("Cette dictée n'est plus dans l'historique.")?,
        ),
        None => None,
    };
    let note = make_note(&comment, dictation, history::now_ms())?;
    let path = path(&app)?;
    let _guard = lock();
    append_to(&path, &note)?;
    Ok(note)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aede-feedback-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join(FILE_NAME)
    }

    fn dictation() -> Entry {
        Entry {
            at_ms: 1_761_000_000_000,
            text: "Fais le deploy.".to_string(),
            raw_text: "euh fais le déploie".to_string(),
            fallback_reason: None,
            paste_error: None,
            audio_file: Some("1761000000000.wav".to_string()),
        }
    }

    #[test]
    fn a_note_carries_the_dictation_it_is_about() {
        let path = temp_file("dictation");
        let note = make_note("  « deploy » mal compris  ", Some(dictation()), 5).unwrap();
        assert_eq!(note.comment, "« deploy » mal compris");
        append_to(&path, &note).unwrap();
        let notes = load_from(&path).unwrap();
        assert_eq!(notes.notes, vec![note]);
        assert_eq!(
            notes.notes[0].dictation.as_ref().unwrap().audio_file,
            Some("1761000000000.wav".to_string())
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_general_opinion_has_no_dictation() {
        let note = make_note("Le raccourci est pratique.", None, 5).unwrap();
        assert_eq!(note.dictation, None);
    }

    #[test]
    fn an_empty_comment_is_refused() {
        assert!(make_note("", None, 5).is_err());
        assert!(make_note("  \n ", Some(dictation()), 5).is_err());
    }

    #[test]
    fn notes_are_read_back_in_order() {
        let path = temp_file("order");
        let first = make_note("Un.", None, 1).unwrap();
        let second = make_note("Deux.", Some(dictation()), 2).unwrap();
        append_to(&path, &first).unwrap();
        append_to(&path, &second).unwrap();
        assert_eq!(load_from(&path).unwrap().notes, vec![first, second]);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_damaged_line_is_counted_and_the_others_kept() {
        let path = temp_file("damaged");
        let before = make_note("Avant.", None, 1).unwrap();
        let after = make_note("Après.", None, 2).unwrap();
        append_to(&path, &before).unwrap();
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(b"{\"at_ms\": 12, \"co\n").unwrap();
        append_to(&path, &after).unwrap();
        let notes = load_from(&path).unwrap();
        assert_eq!(notes.notes, vec![before, after]);
        assert_eq!(notes.unreadable_lines, 1);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn clearing_removes_the_file() {
        let path = temp_file("clear");
        append_to(&path, &make_note("À effacer.", None, 1).unwrap()).unwrap();
        clear_at(&path).unwrap();
        assert!(!path.exists());
        // Clearing when there is nothing is not an error.
        clear_at(&path).unwrap();
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn deleting_a_dictation_removes_its_notes_only() {
        let path = temp_file("remove-about");
        let about_it = make_note("Mal compris.", Some(dictation()), 5).unwrap();
        let general = make_note("Une idée.", None, 6).unwrap();
        let mut other = dictation();
        other.at_ms = 42;
        let about_other = make_note("Autre.", Some(other), 7).unwrap();
        append_to(&path, &about_it).unwrap();
        append_to(&path, &general).unwrap();
        append_to(&path, &about_other).unwrap();

        remove_about_in(&path, 1_761_000_000_000).unwrap();
        assert_eq!(
            load_from(&path).unwrap().notes,
            vec![general.clone(), about_other.clone()]
        );
        // Nothing to remove: the file is left alone. No file at all is fine too.
        remove_about_in(&path, 1_761_000_000_000).unwrap();
        remove_about_in(&temp_file("none"), 1).unwrap();
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
