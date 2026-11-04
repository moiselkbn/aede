//! Opening Aède when the Mac starts, with SMAppService, Apple's API for login items.
//! macOS keeps the setting itself: Aède shows up under its name in System Settings, General,
//! Login Items, where the user can also turn it off. Off by default.

use objc2_service_management::{SMAppService, SMAppServiceStatus};
use serde::Serialize;

/// What the window shows, sent as "enabled", "disabled"...
#[derive(Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LoginItemStatus {
    Enabled,
    Disabled,
    /// Registered, but not allowed in System Settings, Login Items: either macOS waits for the
    /// user's approval, or the user turned it off there. Aède does not open at login.
    RequiresApproval,
}

fn raw_status() -> SMAppServiceStatus {
    // SAFETY: plain calls to SMAppService on the app's own service, no pointers involved.
    unsafe { SMAppService::mainAppService().status() }
}

fn status() -> LoginItemStatus {
    match raw_status() {
        SMAppServiceStatus::Enabled => LoginItemStatus::Enabled,
        SMAppServiceStatus::RequiresApproval => LoginItemStatus::RequiresApproval,
        // "Not found" is also what macOS answers for an app that never registered: it only
        // means off. A real failure shows up when registering (see set_launch_at_login).
        _ => LoginItemStatus::Disabled,
    }
}

#[tauri::command]
pub fn get_launch_at_login() -> LoginItemStatus {
    status()
}

/// Turns the opening at login on or off, then tells the window the new state.
/// Async so that the second macOS takes to answer does not block the app.
#[tauri::command]
pub async fn set_launch_at_login(enabled: bool) -> Result<LoginItemStatus, String> {
    // Only an enabled item needs unregistering. One the user turned off in System Settings
    // is already off, and macOS refuses to unregister it.
    if !enabled && raw_status() != SMAppServiceStatus::Enabled {
        return Ok(status());
    }
    // SAFETY: see status().
    let result = unsafe {
        let service = SMAppService::mainAppService();
        if enabled {
            service.registerAndReturnError()
        } else {
            service.unregisterAndReturnError()
        }
    };
    result.map_err(|err| {
        let action = if enabled { "activer" } else { "désactiver" };
        format!(
            "Impossible d'{action} l'ouverture au démarrage : {}",
            err.localizedDescription()
        )
    })?;
    // Registering said yes but macOS still does not know the app: say it instead of
    // showing a switch that silently stays off.
    if enabled && raw_status() == SMAppServiceStatus::NotFound {
        return Err(
            "macOS n'a pas pu inscrire Aède à l'ouverture au démarrage (app introuvable \
             à cet emplacement)."
                .to_string(),
        );
    }
    Ok(status())
}

/// Opens System Settings, General, Login Items, where the user approves or removes Aède.
#[tauri::command]
pub fn open_login_items_settings() {
    // SAFETY: see status().
    unsafe { SMAppService::openSystemSettingsLoginItems() };
}
