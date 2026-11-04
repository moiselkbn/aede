//! macOS permissions the app needs: accessibility (to simulate Cmd+V) and microphone.
//! The checks themselves come from tauri-plugin-macos-permissions,
//! used here as a plain library (the plugin is not registered, so the UI cannot call it directly).

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set when the last recording was pure silence. macOS delivers exactly that when the microphone
/// is refused, while the permission check below can keep saying "granted" until Aède restarts.
static MICROPHONE_SILENT: AtomicBool = AtomicBool::new(false);

/// Called after each recording with whether any sound came in.
pub fn note_microphone_signal(has_sound: bool) {
    MICROPHONE_SILENT.store(!has_sound, Ordering::Relaxed);
}

#[derive(Clone, Serialize)]
pub struct PermissionsStatus {
    accessibility: bool,
    /// False both when the user has not been asked yet and when they refused, or when the last
    /// recording was pure silence.
    microphone: bool,
}

impl PermissionsStatus {
    pub fn all_granted(&self) -> bool {
        self.accessibility && self.microphone
    }
}

pub async fn status() -> PermissionsStatus {
    PermissionsStatus {
        accessibility: tauri_plugin_macos_permissions::check_accessibility_permission().await,
        microphone: tauri_plugin_macos_permissions::check_microphone_permission().await
            && !MICROPHONE_SILENT.load(Ordering::Relaxed),
    }
}

#[tauri::command]
pub async fn get_permissions_status() -> PermissionsStatus {
    status().await
}

/// Where the microphone stands for macOS, which `status()` cannot say: "not asked yet" and
/// "refused" are both false there. The first one still gets the macOS dialog, the second one
/// never will and only System Settings can change it.
#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MicrophoneAuthorization {
    NotAsked,
    Refused,
    Granted,
}

#[tauri::command]
pub fn microphone_authorization() -> MicrophoneAuthorization {
    use objc2::{class, msg_send};
    use objc2_foundation::NSString;

    // AVAuthorizationStatus: 0 not determined, 1 restricted, 2 denied, 3 authorized.
    let media_type = NSString::from_str("soun");
    let raw: isize = unsafe {
        msg_send![class!(AVCaptureDevice), authorizationStatusForMediaType: &*media_type]
    };
    match raw {
        0 => MicrophoneAuthorization::NotAsked,
        3 => MicrophoneAuthorization::Granted,
        _ => MicrophoneAuthorization::Refused,
    }
}

/// Shows the macOS dialog that sends the user to System Settings.
/// Does nothing visible if the permission is already granted.
#[tauri::command]
pub async fn request_accessibility() {
    tauri_plugin_macos_permissions::request_accessibility_permission().await;
}

/// Shows the macOS microphone dialog. macOS shows it only once: after a refusal,
/// the permission can only be changed in System Settings.
#[tauri::command]
pub async fn request_microphone() -> Result<(), String> {
    tauri_plugin_macos_permissions::request_microphone_permission().await
}

/// Opens the microphone page of System Settings: after a refusal, it is the only place where
/// the permission can be given back.
#[tauri::command]
pub fn open_microphone_settings() -> Result<(), String> {
    let status = std::process::Command::new("/usr/bin/open")
        .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone")
        .status()
        .map_err(|err| format!("Impossible d'ouvrir les Réglages Système : {err}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("Impossible d'ouvrir les Réglages Système.".into())
    }
}
