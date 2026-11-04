//! The user settings, saved in settings.json in the app data folder.
//! The two global shortcuts (push-to-talk, and the one that learns a corrected word) and the
//! microphone, and the look (`appearance.rs`).

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::appearance::Appearance;

const FILE_NAME: &str = "settings.json";

/// The shortcut until the user picks another one.
pub const DEFAULT_SHORTCUT: &str = "alt+Space";

/// The shortcut that adds a corrected word to the dictionary, until the user picks another one.
pub const DEFAULT_LEARN_SHORTCUT: &str = "shift+alt+Space";

#[derive(Debug, PartialEq, Serialize, Deserialize)]
// A setting missing from the file (added in a later version) takes its default value.
#[serde(default)]
pub struct Settings {
    /// Written the way the global-shortcut plugin reads it, e.g. "control+alt+Space".
    pub shortcut: String,
    /// The shortcut that learns a corrected word, written the same way.
    pub learn_shortcut: String,
    /// The microphone the user chose (an id of `audio::input_devices`). None: the Mac's default.
    pub microphone: Option<String>,
    /// The theme of the window and the style of the notch pill.
    pub appearance: Appearance,
    /// Whether the audio of each dictation is kept on this Mac (test week). Off by default.
    pub keep_recordings: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            shortcut: DEFAULT_SHORTCUT.to_string(),
            learn_shortcut: DEFAULT_LEARN_SHORTCUT.to_string(),
            microphone: None,
            appearance: Appearance::default(),
            keep_recordings: false,
        }
    }
}

fn path(app: &AppHandle) -> Result<PathBuf, String> {
    let data = app
        .path()
        .app_data_dir()
        .map_err(|err| format!("Dossier de données de l'app introuvable : {err}"))?;
    Ok(data.join(FILE_NAME))
}

/// Reads the settings. No file yet means the defaults. A file that cannot be read is an error,
/// so that the user learns why the shortcut is not the one they chose.
pub fn load(app: &AppHandle) -> Result<Settings, String> {
    load_from(&path(app)?)
}

pub fn save(app: &AppHandle, settings: &Settings) -> Result<(), String> {
    save_to(&path(app)?, settings)
}

fn load_from(path: &Path) -> Result<Settings, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Settings::default()),
        Err(err) => return Err(format!("Réglages illisibles ({}) : {err}", path.display())),
    };
    serde_json::from_str(&text)
        .map_err(|err| format!("Réglages illisibles ({}) : {err}", path.display()))
}

fn save_to(path: &Path, settings: &Settings) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)
            .map_err(|err| format!("Impossible de créer le dossier {} : {err}", dir.display()))?;
    }
    let text = serde_json::to_string_pretty(settings)
        .map_err(|err| format!("Impossible d'écrire les réglages : {err}"))?;
    std::fs::write(path, text).map_err(|err| {
        format!(
            "Impossible d'enregistrer les réglages ({}) : {err}",
            path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::appearance::{PillStyle, Theme};

    fn temp_file(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("aede-settings-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join(FILE_NAME)
    }

    #[test]
    fn no_file_gives_the_default_shortcut() {
        let path = temp_file("none");
        assert_eq!(load_from(&path), Ok(Settings::default()));
    }

    #[test]
    fn saved_settings_are_read_back() {
        let path = temp_file("saved");
        let settings = Settings {
            shortcut: "super+shift+KeyD".to_string(),
            learn_shortcut: "super+shift+KeyL".to_string(),
            microphone: Some("mic-1".to_string()),
            appearance: Appearance {
                theme: Theme::Dark,
                pill_style: PillStyle::Black,
            },
            keep_recordings: true,
        };
        save_to(&path, &settings).unwrap();
        assert_eq!(load_from(&path), Ok(settings));
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn the_audio_of_dictations_is_not_kept_until_the_user_says_so() {
        let path = temp_file("keep-audio");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"shortcut": "super+KeyA"}"#).unwrap();
        assert!(!load_from(&path).unwrap().keep_recordings);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_missing_setting_takes_its_default() {
        let path = temp_file("missing");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{}").unwrap();
        assert_eq!(load_from(&path), Ok(Settings::default()));
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_file_from_before_the_dictionary_keeps_its_shortcut() {
        let path = temp_file("old");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"shortcut": "super+KeyA"}"#).unwrap();
        let settings = load_from(&path).unwrap();
        assert_eq!(settings.shortcut, "super+KeyA");
        assert_eq!(settings.learn_shortcut, DEFAULT_LEARN_SHORTCUT);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_broken_file_is_an_error() {
        let path = temp_file("broken");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "not json").unwrap();
        assert!(
            load_from(&path)
                .unwrap_err()
                .starts_with("Réglages illisibles")
        );
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
