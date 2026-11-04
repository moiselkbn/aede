//! The audio of every dictation, kept in the recordings folder of the app data folder, one WAV
//! per dictation named after the time it ended (`1761000000000.wav`, the same number as the
//! `at_ms` of its history entry). They let the author listen again to what was said and compare
//! it with what Whisper heard. The folder only stays on this Mac and only the user can read it.
//! The WAVs are mono 16 kHz, like the files of `tests/audio/`.

use std::fs::{DirBuilder, OpenOptions};
use std::io::BufWriter;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use tauri::ipc::Response;
use tauri::{AppHandle, Manager};

use crate::audio;
use crate::settings;

const DIR_NAME: &str = "recordings";

fn dir(app: &AppHandle) -> Result<PathBuf, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("Dossier de données de l'app introuvable : {err}"))?;
    Ok(data.join(DIR_NAME))
}

/// The file name of the dictation that ended at `at_ms`.
fn file_name(at_ms: u64) -> String {
    format!("{at_ms}.wav")
}

/// Only names made by `file_name` are accepted from the page: digits and ".wav", so that a
/// name can never point outside the recordings folder.
fn is_valid_name(name: &str) -> bool {
    name.strip_suffix(".wav")
        .is_some_and(|stem| !stem.is_empty() && stem.chars().all(|c| c.is_ascii_digit()))
}

/// Saves the audio of the dictation that ended at `at_ms` and returns the name of the file.
pub fn save(app: &AppHandle, at_ms: u64, samples: &[f32]) -> Result<String, String> {
    save_in(&dir(app)?, at_ms, samples)
}

fn save_in(dir: &Path, at_ms: u64, samples: &[f32]) -> Result<String, String> {
    // 0o700 and 0o600: only the user can read the recordings. Applies when they are created.
    DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)
        .map_err(|err| format!("Impossible de créer le dossier {} : {err}", dir.display()))?;
    let name = file_name(at_ms);
    let path = dir.join(&name);
    let failed = |err: String| {
        format!(
            "Impossible d'enregistrer l'audio de la dictée ({}) : {err}",
            path.display()
        )
    };
    let file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&path)
        .map_err(|err| failed(err.to_string()))?;
    audio::write_wav(BufWriter::new(file), samples).map_err(failed)?;
    Ok(name)
}

/// The length in seconds of a saved recording. None if it is gone or cannot be read.
pub fn duration_seconds(app: &AppHandle, name: &str) -> Option<f64> {
    duration_seconds_in(&dir(app).ok()?, name)
}

fn duration_seconds_in(dir: &Path, name: &str) -> Option<f64> {
    if !is_valid_name(name) {
        return None;
    }
    let reader = hound::WavReader::open(dir.join(name)).ok()?;
    Some(f64::from(reader.duration()) / f64::from(reader.spec().sample_rate))
}

/// Deletes the folder: no audio of the past dictations is left on disk.
pub fn clear(app: &AppHandle) -> Result<(), String> {
    clear_at(&dir(app)?)
}

fn clear_at(dir: &Path) -> Result<(), String> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "Impossible d'effacer les enregistrements ({}) : {err}",
            dir.display()
        )),
    }
}

/// The bytes of a WAV, for the player of the window.
#[tauri::command]
pub fn get_recording(app: AppHandle, file: String) -> Result<Response, String> {
    if !is_valid_name(&file) {
        return Err(format!("Nom d'enregistrement invalide : {file}"));
    }
    let path = dir(&app)?.join(&file);
    match std::fs::read(&path) {
        Ok(bytes) => Ok(Response::new(bytes)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            Err("Cet enregistrement n'existe plus sur ce Mac.".to_string())
        }
        Err(err) => Err(format!(
            "Enregistrement illisible ({}) : {err}",
            path.display()
        )),
    }
}

/// Whether the audio of each dictation is kept (the setting of the window, off by default).
#[tauri::command]
pub fn get_keep_recordings(app: AppHandle) -> Result<bool, String> {
    Ok(settings::load(&app)?.keep_recordings)
}

/// Turns the keeping of the audio on or off. Turning it off keeps the recordings already saved.
#[tauri::command]
pub fn set_keep_recordings(app: AppHandle, keep: bool) -> Result<(), String> {
    let mut current = settings::load(&app)?;
    current.keep_recordings = keep;
    settings::save(&app, &current)
}

/// Deletes the recording of one dictation. Already gone is not an error.
pub fn remove(app: &AppHandle, name: &str) -> Result<(), String> {
    remove_in(&dir(app)?, name)
}

fn remove_in(dir: &Path, name: &str) -> Result<(), String> {
    if !is_valid_name(name) {
        return Err(format!("Nom d'enregistrement refusé : {name}"));
    }
    let path = dir.join(name);
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(format!(
            "Impossible d'effacer l'enregistrement ({}) : {err}",
            path.display()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("aede-recordings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join(DIR_NAME)
    }

    fn cleanup(dir: &Path) {
        std::fs::remove_dir_all(dir.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_recording_is_named_after_the_end_of_the_dictation() {
        assert_eq!(file_name(1_761_000_000_000), "1761000000000.wav");
    }

    #[test]
    fn only_names_made_by_the_app_are_accepted() {
        assert!(is_valid_name("1761000000000.wav"));
        assert!(!is_valid_name(""));
        assert!(!is_valid_name(".wav"));
        assert!(!is_valid_name("../history.jsonl"));
        assert!(!is_valid_name("../1761000000000.wav"));
        assert!(!is_valid_name("1761000000000.wav/.."));
        assert!(!is_valid_name("abc.wav"));
        assert!(!is_valid_name("1761000000000"));
    }

    #[test]
    fn a_saved_recording_can_be_read_back() {
        let dir = temp_dir("roundtrip");
        let samples: Vec<f32> = (0..1600).map(|i| (i as f32 / 50.0).sin() * 0.5).collect();
        let name = save_in(&dir, 42, &samples).unwrap();
        assert_eq!(name, "42.wav");

        let mut reader = hound::WavReader::open(dir.join(&name)).unwrap();
        let spec = reader.spec();
        assert_eq!(
            (spec.channels, spec.sample_rate, spec.bits_per_sample),
            (1, 16_000, 16)
        );
        assert_eq!(reader.samples::<i16>().count(), samples.len());
        cleanup(&dir);
    }

    #[test]
    fn the_length_of_a_recording_is_read_from_its_file() {
        let dir = temp_dir("duration");
        // Two seconds at 16 kHz.
        let name = save_in(&dir, 5, &[0.1; 32_000]).unwrap();
        assert_eq!(duration_seconds_in(&dir, &name), Some(2.0));
        assert_eq!(duration_seconds_in(&dir, "6.wav"), None);
        assert_eq!(duration_seconds_in(&dir, "../5.wav"), None);
        cleanup(&dir);
    }

    #[test]
    fn only_the_user_can_read_the_recordings() {
        let dir = temp_dir("mode");
        let name = save_in(&dir, 7, &[0.1; 160]).unwrap();
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&dir.join(name)), 0o600);
        cleanup(&dir);
    }

    #[test]
    fn clearing_removes_every_recording() {
        let dir = temp_dir("clear");
        save_in(&dir, 1, &[0.1; 160]).unwrap();
        save_in(&dir, 2, &[0.1; 160]).unwrap();
        clear_at(&dir).unwrap();
        assert!(!dir.exists());
        // Clearing when there is nothing is not an error.
        clear_at(&dir).unwrap();
        cleanup(&dir);
    }

    #[test]
    fn removing_one_recording_keeps_the_others() {
        let dir = temp_dir("remove");
        save_in(&dir, 1, &[0.1; 160]).unwrap();
        save_in(&dir, 2, &[0.1; 160]).unwrap();
        remove_in(&dir, "1.wav").unwrap();
        assert!(!dir.join("1.wav").exists());
        assert!(dir.join("2.wav").exists());
        // Already gone is not an error, and a name outside the folder is refused.
        remove_in(&dir, "1.wav").unwrap();
        assert!(remove_in(&dir, "../history.jsonl").is_err());
        cleanup(&dir);
    }
}
