//! The first launch: the window opens by itself only when something is missing
//! (a permission or a model). Otherwise the app starts quietly in the menu bar.
//! A missing model shows the download screen; a missing permission is asked in a dialog
//! of the dashboard, so it does not need that screen.

use tauri::AppHandle;

use crate::{models, permissions, tray};

/// True when a model file is not on disk.
fn models_missing(app: &AppHandle) -> Result<bool, String> {
    for model in models::ALL {
        if !models::path(app, model)?.is_file() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Development only: `AEDE_FORCE_SETUP=1` opens the first launch screens even when everything is
/// ready, to work on them. Nothing is revoked or downloaded: the screens show the real state.
fn forced() -> bool {
    std::env::var_os("AEDE_FORCE_SETUP").is_some_and(|value| !value.is_empty() && value != "0")
}

/// Lets the UI choose between the first launch screen and the usual one.
#[tauri::command]
pub fn needs_setup(app: AppHandle) -> Result<bool, String> {
    Ok(forced() || models_missing(&app)?)
}

/// Called at startup: the window starts hidden (tauri.conf.json) and is shown only if needed.
pub fn show_window_if_needed(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        // A missing permission also shows the window, so that its dialog can be seen.
        let permissions_missing = !permissions::status().await.all_granted();
        match models_missing(&app) {
            Ok(false) if !permissions_missing && !forced() => {}
            Ok(_) => {
                eprintln!("[first-launch] a permission or a model is missing: showing the window");
                tray::show_window(&app);
            }
            // Show the window anyway: it displays the same error in clear.
            Err(message) => {
                eprintln!("[first-launch] {message}");
                tray::show_window(&app);
            }
        }
    });
}
