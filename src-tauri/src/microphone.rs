//! The choice of the microphone: the list of what macOS knows, and the one the user picked,
//! kept in the settings. The dictation and the microphone test of the first launch both open
//! the chosen one (the Mac's default if none was chosen or if it is unplugged).

use serde::Serialize;
use tauri::AppHandle;

use crate::audio::{self, InputDevice};
use crate::settings;

/// What the choice screen shows.
#[derive(Serialize)]
pub struct Microphones {
    devices: Vec<InputDevice>,
    /// The id of the microphone that would be opened now, if there is one.
    selected: Option<String>,
}

/// The id kept in the settings, or None. A settings file that cannot be read is not an error
/// here: recording with the default microphone is better than not recording.
pub fn chosen(app: &AppHandle) -> Option<String> {
    settings::load(app).ok().and_then(|s| s.microphone)
}

#[tauri::command]
pub fn list_microphones(app: AppHandle) -> Microphones {
    Microphones {
        devices: audio::input_devices(),
        selected: audio::used_input_id(chosen(&app).as_deref()),
    }
}

/// Remembers the microphone the user picked.
#[tauri::command]
pub fn set_microphone(app: AppHandle, id: String) -> Result<(), String> {
    let mut current = settings::load(&app)?;
    current.microphone = Some(id);
    settings::save(&app, &current)
}
