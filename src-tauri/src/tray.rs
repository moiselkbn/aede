//! The icon in the menu bar, and the window that goes away and comes back.
//!
//! Closing the window must not stop the app: the push-to-talk shortcut only works while the
//! app runs. So the close button hides the window, and the app is quit from the menu bar icon.
//! The app has a Dock icon only while its window is open: this icon is the way to reach it.
//!
//! The icon is the logo, made by tools/icons/make-icons.swift. It is a template image: macOS
//! paints it in the colour of the menu bar. It stays the same whatever the app is doing (the
//! indicator at the notch shows that); only its tooltip tells the state.

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Window, WindowEvent, Wry};

use crate::dictation;

const TRAY_ID: &str = "main";
const CANCEL_ID: &str = "cancel";
const SHOW_ID: &str = "show";
const QUIT_ID: &str = "quit";

/// Sent to the interface each time the window is hidden or shown again (payload: is it visible).
/// The first launch screens use it to stop what they listen to (the microphone, the shortcuts)
/// while nobody can see them.
pub const VISIBLE_EVENT: &str = "main-window-visible";

fn announce_visibility(app: &AppHandle, visible: bool) {
    if let Err(err) = app.emit(VISIBLE_EVENT, visible) {
        eprintln!("[tray] could not announce the window: {err}");
    }
}

/// What the tooltip of the icon tells.
#[derive(Clone, Copy)]
pub enum TrayState {
    Idle,
    Recording,
    /// Transcribing, rewriting or pasting.
    Working,
}

impl TrayState {
    /// `app_name` is productName from tauri.conf.json, so the name is written in one place.
    fn tooltip(self, app_name: &str) -> String {
        match self {
            TrayState::Idle => app_name.to_string(),
            TrayState::Recording => format!("{app_name} : enregistrement"),
            TrayState::Working => format!("{app_name} : traitement"),
        }
    }
}

/// The "cancel" entry of the menu, kept to grey it out when there is nothing to cancel.
struct CancelItem(MenuItem<Wry>);

/// Puts the icon and its menu (cancel, show the window, quit) in the menu bar.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    // Greyed out until a dictation can be cancelled (see `set_cancel_enabled`).
    let cancel = MenuItem::with_id(
        app,
        CANCEL_ID,
        "Annuler la dictée en cours",
        false,
        None::<&str>,
    )?;
    let show = MenuItem::with_id(app, SHOW_ID, "Afficher la fenêtre", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, QUIT_ID, "Quitter", true, None::<&str>)?;
    let separator = PredefinedMenuItem::separator(app)?;
    let menu = Menu::with_items(app, &[&cancel, &separator, &show, &quit])?;
    app.manage(CancelItem(cancel));

    TrayIconBuilder::with_id(TRAY_ID)
        .icon(tauri::include_image!("icons/tray.png"))
        .icon_as_template(true)
        .tooltip(TrayState::Idle.tooltip(&app.package_info().name))
        .menu(&menu)
        .on_menu_event(|app, event| match event.id.as_ref() {
            CANCEL_ID => dictation::cancel(app),
            SHOW_ID => show_window(app),
            QUIT_ID => app.exit(0),
            _ => {}
        })
        .build(app)?;
    Ok(())
}

/// Changes the tooltip to tell what the app is doing.
pub fn show_state(app: &AppHandle, state: TrayState) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        eprintln!("[tray] no menu bar icon to change");
        return;
    };
    if let Err(err) = tray.set_tooltip(Some(state.tooltip(&app.package_info().name))) {
        eprintln!("[tray] could not change the tooltip: {err}");
    }
}

/// Greys out the "cancel" entry, or makes it clickable.
pub fn set_cancel_enabled(app: &AppHandle, enabled: bool) {
    if let Some(item) = app.try_state::<CancelItem>()
        && let Err(err) = item.0.set_enabled(enabled)
    {
        eprintln!("[tray] could not change the cancel entry: {err}");
    }
}

/// Brings the main window back (from the menu bar icon, or by opening the app again).
pub fn show_window(app: &AppHandle) {
    show_in_dock(app, true);
    if let Some(window) = app.get_webview_window("main") {
        match window.show().and_then(|()| window.set_focus()) {
            Ok(()) => announce_visibility(app, true),
            Err(err) => eprintln!("[tray] could not show the window: {err}"),
        }
    }
}

/// The close button hides the window instead of closing it.
pub fn hide_instead_of_closing(window: &Window, event: &WindowEvent) {
    if let WindowEvent::CloseRequested { api, .. } = event {
        api.prevent_close();
        match window.hide() {
            Ok(()) => announce_visibility(window.app_handle(), false),
            Err(err) => eprintln!("[tray] could not hide the window: {err}"),
        }
        show_in_dock(window.app_handle(), false);
    }
}

/// Puts the app in the Dock and in Cmd+Tab while its window is open, and takes it out when
/// the window is hidden: the rest of the time, it lives in the menu bar only.
fn show_in_dock(app: &AppHandle, visible: bool) {
    let policy = if visible {
        tauri::ActivationPolicy::Regular
    } else {
        tauri::ActivationPolicy::Accessory
    };
    if let Err(err) = app.set_activation_policy(policy) {
        eprintln!("[tray] could not change the Dock icon: {err}");
    }
}
