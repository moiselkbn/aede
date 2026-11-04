//! Inserts text in the active application: put it in the clipboard,
//! simulate Cmd+V, then put the previous clipboard text back.

use std::{thread::sleep, time::Duration};

use core_graphics::{
    event::{CGEvent, CGEventFlags, CGEventTapLocation, CGKeyCode},
    event_source::{CGEventSource, CGEventSourceStateID},
};
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

/// Physical key code of "V" on a Mac keyboard (kVK_ANSI_V). It is the same on QWERTY and AZERTY.
const KEY_V: CGKeyCode = 9;

/// Physical key code of "C" (kVK_ANSI_C).
const KEY_C: CGKeyCode = 8;

/// Pause between writing the clipboard and pressing Cmd+V. To tune by testing.
const BEFORE_PASTE: Duration = Duration::from_millis(50);

/// Pause between pressing Cmd+V and restoring the old clipboard.
/// The target app reads the clipboard after the key press, so we must not restore too early.
/// To tune by testing.
const BEFORE_RESTORE: Duration = Duration::from_millis(200);

/// Error when the accessibility permission is missing. Public so the dictation can tell it apart.
pub const NO_ACCESSIBILITY: &str =
    "Permission accessibilité manquante : impossible de simuler Cmd+V.";

/// Pastes `text` in the app that currently has the focus.
/// Fails with a readable message if the accessibility permission is missing.
pub async fn paste_text(app: AppHandle, text: String) -> Result<(), String> {
    if !tauri_plugin_macos_permissions::check_accessibility_permission().await {
        return Err(NO_ACCESSIBILITY.to_string());
    }
    // Sleeping and clipboard access would block the main thread, so use a worker thread.
    tauri::async_runtime::spawn_blocking(move || paste_blocking(&app, &text))
        .await
        .map_err(|err| format!("Le collage a planté : {err}"))?
}

fn paste_blocking(app: &AppHandle, text: &str) -> Result<(), String> {
    let clipboard = app.clipboard();

    // Only text can be saved for now. If the clipboard held an image or was empty,
    // there is nothing to put back afterwards.
    let previous = clipboard.read_text().ok();

    clipboard
        .write_text(text)
        .map_err(|err| format!("Impossible d'écrire dans le presse-papiers : {err}"))?;
    sleep(BEFORE_PASTE);

    let pressed = press_cmd(KEY_V);
    sleep(BEFORE_RESTORE);

    if let Some(previous) = previous {
        clipboard
            .write_text(previous)
            .map_err(|err| format!("Impossible de restaurer le presse-papiers : {err}"))?;
    }
    pressed
}

/// Sends Cmd+C down then up to the system: copies the selection of the active app.
pub fn press_cmd_c() -> Result<(), String> {
    press_cmd(KEY_C)
}

/// Sends Cmd+`key` down then up to the system.
fn press_cmd(key: CGKeyCode) -> Result<(), String> {
    // A private event source ignores the modifiers physically held right now (Ctrl, Option
    // from the shortcut), so the app only sees Cmd+V.
    let source = CGEventSource::new(CGEventSourceStateID::Private)
        .map_err(|()| "Impossible de créer la source d'événements clavier.".to_string())?;

    for key_down in [true, false] {
        let event = CGEvent::new_keyboard_event(source.clone(), key, key_down)
            .map_err(|()| "Impossible de créer l'événement clavier.".to_string())?;
        event.set_flags(CGEventFlags::CGEventFlagCommand);
        event.post(CGEventTapLocation::HID);
    }
    Ok(())
}
