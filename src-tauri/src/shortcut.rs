//! Global push-to-talk shortcut: key press starts the dictation, key release ends it.
//! The shortcut comes from the settings and can be changed from the window.

use std::str::FromStr;
use std::sync::{Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{
    Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutEvent, ShortcutState,
};

use crate::dictation;
use crate::settings;
use crate::trace::trace;
use crate::trial;

/// What the UI shows: which shortcut, and what went wrong if anything did.
#[derive(Clone, Default, Serialize)]
pub struct ShortcutStatus {
    /// For people, e.g. "Ctrl+Option+Espace".
    pub label: String,
    pub error: Option<String>,
}

/// The shortcut in use, shared between startup and the window that changes it.
#[derive(Default)]
pub struct CurrentShortcut {
    /// None when no shortcut could be registered.
    pub shortcut: Mutex<Option<Shortcut>>,
    status: Mutex<ShortcutStatus>,
    /// The Escape shortcuts registered right now to cancel a dictation (see `set_cancel_key`).
    cancel_keys: Mutex<Vec<Shortcut>>,
}

pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// Called by the plugin on every press and every release of the shortcut.
fn handle(app: &AppHandle, _shortcut: &Shortcut, event: ShortcutEvent) {
    match event.state() {
        ShortcutState::Pressed => {
            trace!("[shortcut] pressed");
            if !trial::take_shortcut_press(app, true) {
                dictation::on_press(app);
            }
        }
        ShortcutState::Released => {
            trace!("[shortcut] released");
            if !trial::take_shortcut_press(app, false) {
                dictation::on_release(app);
            }
        }
    }
}

/// Registers Escape as a global shortcut while `active`, and frees it otherwise. It is active
/// only while a dictation can be cancelled, so the other apps keep their Escape the rest of the
/// time. The menu bar entry cancels too, so a failure here only costs the keyboard way.
///
/// While recording, the keys of the push-to-talk shortcut are still down, and macOS matches a
/// shortcut with its exact modifiers: Ctrl+Option+Escape is not Escape. So Escape is also
/// registered with the modifiers of the push-to-talk shortcut.
pub fn set_cancel_key(app: &AppHandle, active: bool) {
    // macOS wants shortcuts to be registered on the main thread, and the phase changes come
    // from other threads.
    let main_app = app.clone();
    let result = app.run_on_main_thread(move || {
        let current = main_app.state::<CurrentShortcut>();
        let global = main_app.global_shortcut();
        let mut registered = lock(&current.cancel_keys);
        if active {
            if !registered.is_empty() {
                return;
            }
            let mut keys = vec![Shortcut::new(None, Code::Escape)];
            if let Some(talk) = *lock(&current.shortcut)
                && !talk.mods.is_empty()
            {
                keys.push(Shortcut::new(Some(talk.mods), Code::Escape));
            }
            for key in keys {
                let result = global.on_shortcut(key, |app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        dictation::cancel(app);
                    }
                });
                match result {
                    Ok(()) => {
                        trace!("[shortcut] Escape (cancel) registered: {key}");
                        registered.push(key);
                    }
                    Err(err) => eprintln!("[shortcut] Escape (cancel) not registered: {err}"),
                }
            }
        } else {
            for key in registered.drain(..) {
                trace!("[shortcut] Escape (cancel) freed: {key}");
                if let Err(err) = global.unregister(key) {
                    eprintln!("[shortcut] Escape (cancel) not freed: {err}");
                }
            }
        }
    });
    if let Err(err) = result {
        eprintln!("[shortcut] could not reach the main thread: {err}");
    }
}

/// Reads a shortcut written like "control+alt+Space". Without Ctrl, Option, Shift or Cmd, only
/// F13 to F24 are accepted: a letter alone could no longer be typed in any app, and on a Mac
/// keyboard F1 to F12 alone are the brightness, sound and media keys, which macOS keeps for
/// itself (the shortcut would never fire).
pub fn parse(text: &str) -> Result<Shortcut, String> {
    let shortcut =
        Shortcut::from_str(text).map_err(|_| format!("Combinaison non reconnue : {text}"))?;
    if shortcut.mods.is_empty() {
        match function_key_number(&shortcut) {
            Some(13..=24) => {}
            Some(_) => {
                return Err(
                    "Seules, les touches F1 à F12 règlent la luminosité, le son ou la \
                            musique : macOS ne les transmet pas aux applications. Ajoute une \
                            touche Ctrl, Option, Maj ou Cmd, ou choisis F13 à F24."
                        .into(),
                );
            }
            None => {
                return Err(
                    "Ajoute au moins une touche Ctrl, Option, Maj ou Cmd : seule, cette touche \
                     ne pourrait plus servir à écrire."
                        .into(),
                );
            }
        }
    }
    Ok(shortcut)
}

/// 1 for F1, 24 for F24, None for any other key.
fn function_key_number(shortcut: &Shortcut) -> Option<u32> {
    shortcut.key.to_string().strip_prefix('F')?.parse().ok()
}

/// "Ctrl+Option+Espace", in the order of the Mac menus.
pub fn label(shortcut: &Shortcut) -> String {
    let mut parts = Vec::new();
    for (modifier, name) in [
        (Modifiers::CONTROL, "Ctrl"),
        (Modifiers::ALT, "Option"),
        (Modifiers::SHIFT, "Maj"),
        (Modifiers::SUPER, "Cmd"),
    ] {
        if shortcut.mods.contains(modifier) {
            parts.push(name.to_string());
        }
    }
    let key = shortcut.key.to_string();
    let key = match key.as_str() {
        "Space" => "Espace",
        other => other
            .strip_prefix("Key")
            .or_else(|| other.strip_prefix("Digit"))
            .unwrap_or(other),
    };
    parts.push(key.to_string());
    parts.join("+")
}

/// Puts `new` in place of the shortcut registered in `registered`. If macOS refuses `new`, the
/// old one is put back; `registered` is None afterwards only if even that failed. Must run on the
/// main thread, where macOS wants shortcuts to be registered.
pub fn replace(
    app: &AppHandle,
    registered: &mut Option<Shortcut>,
    new: Shortcut,
    handler: fn(&AppHandle, &Shortcut, ShortcutEvent),
) -> Result<(), String> {
    if *registered == Some(new) {
        return Ok(());
    }
    let global = app.global_shortcut();
    if let Some(old) = *registered {
        global
            .unregister(old)
            .map_err(|err| format!("Impossible de libérer l'ancien raccourci : {err}"))?;
    }
    if let Err(err) = global.on_shortcut(new, handler) {
        // Put the old one back, so that the feature keeps working.
        if let Some(old) = *registered
            && let Err(err) = global.on_shortcut(old, handler)
        {
            *registered = None;
            return Err(format!("Aucun raccourci actif : {err}"));
        }
        return Err(format!(
            "macOS refuse {} : {err}. Le raccourci précédent reste actif.",
            label(&new)
        ));
    }
    *registered = Some(new);
    Ok(())
}

/// Registers the shortcut saved in the settings, at startup.
pub fn register(app: &AppHandle) {
    let (text, settings_error) = match settings::load(app) {
        Ok(settings) => (settings.shortcut, None),
        Err(message) => (settings::DEFAULT_SHORTCUT.to_string(), Some(message)),
    };
    let result = parse(&text).and_then(|shortcut| {
        app.global_shortcut()
            .on_shortcut(shortcut, handle)
            .map_err(|err| {
                format!(
                    "Impossible d'activer le raccourci {} : {err}",
                    label(&shortcut)
                )
            })?;
        Ok(shortcut)
    });

    let current = app.state::<CurrentShortcut>();
    let status = match result {
        Ok(shortcut) => {
            *lock(&current.shortcut) = Some(shortcut);
            ShortcutStatus {
                label: label(&shortcut),
                error: settings_error,
            }
        }
        Err(message) => ShortcutStatus {
            label: text,
            error: Some(message),
        },
    };
    if let Some(message) = &status.error {
        eprintln!("[shortcut] {message}");
    }
    *lock(&current.status) = status;
}

#[tauri::command]
pub fn get_shortcut_status(current: tauri::State<'_, CurrentShortcut>) -> ShortcutStatus {
    lock(&current.status).clone()
}

/// Replaces the shortcut by `text` (e.g. "control+shift+KeyD") and saves it. If macOS refuses
/// the new one, the old one stays active. A sync command: Tauri runs it on the main thread,
/// where macOS wants shortcuts to be registered.
#[tauri::command]
pub fn set_shortcut(
    app: AppHandle,
    current: tauri::State<'_, CurrentShortcut>,
    learn: tauri::State<'_, crate::learn::CurrentLearnShortcut>,
    text: String,
) -> Result<ShortcutStatus, String> {
    let new = parse(&text)?;
    if *lock(&learn.shortcut) == Some(new) {
        return Err(format!(
            "{} sert déjà à apprendre un mot : choisis une autre combinaison.",
            label(&new)
        ));
    }
    let mut registered = lock(&current.shortcut);
    if let Err(message) = replace(&app, &mut registered, new, handle) {
        if registered.is_none() {
            eprintln!("[shortcut] {message}");
            lock(&current.status).error = Some(message.clone());
        }
        return Err(message);
    }

    let mut status = ShortcutStatus {
        label: label(&new),
        error: None,
    };
    let saved = settings::load(&app).and_then(|mut settings| {
        settings.shortcut = new.into_string();
        settings::save(&app, &settings)
    });
    if let Err(message) = saved {
        status.error = Some(format!(
            "Raccourci actif, mais pas enregistré pour le prochain lancement : {message}"
        ));
    }
    eprintln!("[shortcut] now {}", status.label);
    *lock(&current.status) = status.clone();
    Ok(status)
}

/// Releases the two global shortcuts while the window waits for the user to press a new
/// combination. A combination that is registered is taken by macOS before it reaches the
/// window, so pressing the shortcut of the other function would do nothing and say nothing.
/// `resume_shortcuts` registers them again. A sync command, for the main thread.
#[tauri::command]
pub fn pause_shortcuts(
    app: AppHandle,
    current: tauri::State<'_, CurrentShortcut>,
    learn: tauri::State<'_, crate::learn::CurrentLearnShortcut>,
) -> Result<(), String> {
    let global = app.global_shortcut();
    for shortcut in [*lock(&current.shortcut), *lock(&learn.shortcut)]
        .into_iter()
        .flatten()
    {
        if global.is_registered(shortcut) {
            global
                .unregister(shortcut)
                .map_err(|err| format!("Impossible de libérer {} : {err}", label(&shortcut)))?;
        }
    }
    Ok(())
}

/// Registers again the two global shortcuts released by `pause_shortcuts`. Does nothing for one
/// that is already registered, so it can be called twice.
#[tauri::command]
pub fn resume_shortcuts(
    app: AppHandle,
    current: tauri::State<'_, CurrentShortcut>,
    learn: tauri::State<'_, crate::learn::CurrentLearnShortcut>,
) -> Result<(), String> {
    let global = app.global_shortcut();
    let mut failures = Vec::new();
    let talk = *lock(&current.shortcut);
    let learning = *lock(&learn.shortcut);
    if let Some(shortcut) = talk
        && !global.is_registered(shortcut)
        && let Err(err) = global.on_shortcut(shortcut, handle)
    {
        failures.push(format!("{} : {err}", label(&shortcut)));
    }
    if let Some(shortcut) = learning
        && !global.is_registered(shortcut)
        && let Err(err) = global.on_shortcut(shortcut, crate::learn::handle)
    {
        failures.push(format!("{} : {err}", label(&shortcut)));
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Impossible de réactiver les raccourcis ({}).",
            failures.join(", ")
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_default_shortcut_is_valid_and_readable() {
        let shortcut = parse(settings::DEFAULT_SHORTCUT).unwrap();
        assert_eq!(label(&shortcut), "Option+Espace");
    }

    #[test]
    fn browser_key_names_are_understood() {
        // What the window sends: the modifiers, then KeyboardEvent.code.
        let shortcut = parse("Control+Shift+KeyD").unwrap();
        assert_eq!(label(&shortcut), "Ctrl+Maj+D");
        assert_eq!(label(&parse("Alt+Super+Digit1").unwrap()), "Option+Cmd+1");
        // Saved then read again at the next launch.
        assert_eq!(parse(&shortcut.into_string()).unwrap(), shortcut);
    }

    #[test]
    fn only_f13_to_f24_may_be_alone() {
        assert_eq!(label(&parse("F13").unwrap()), "F13");
        assert_eq!(label(&parse("F24").unwrap()), "F24");
        // The brightness key on a Mac keyboard: macOS keeps it.
        assert!(parse("F1").unwrap_err().contains("F1 à F12"));
        assert!(parse("F12").unwrap_err().contains("F1 à F12"));
        assert_eq!(label(&parse("Control+F1").unwrap()), "Ctrl+F1");
        assert!(parse("KeyD").unwrap_err().contains("Ajoute au moins"));
        assert!(parse("Space").unwrap_err().contains("Ajoute au moins"));
    }

    #[test]
    fn nonsense_is_refused_clearly() {
        assert!(
            parse("Control+Nope")
                .unwrap_err()
                .starts_with("Combinaison non reconnue")
        );
    }
}
