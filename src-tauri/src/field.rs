//! Reads the text of the field that has the focus in the active app, only when asked.
//!
//! First choice: the accessibility API of macOS, which gives the text without touching the
//! selection or the clipboard. Browsers and Electron apps only build their accessibility tree
//! on request, so it is requested once if the first read gives nothing. Second choice, for the
//! apps that give no text at all (VS Code): a simulated Cmd+C, which copies the selection, or
//! the whole current line in VS Code when nothing is selected. The previous clipboard text is
//! put back.
//!
//! Every function here sleeps: call them from a worker thread, never from the main one.

use std::ffi::c_void;
use std::ptr::null_mut;
use std::thread::sleep;
use std::time::Duration;

use objc2_foundation::NSString;
use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;

use crate::paste;

#[link(name = "ApplicationServices", kind = "framework")]
unsafe extern "C" {
    fn AXUIElementCreateSystemWide() -> *mut c_void;
    fn AXUIElementCopyAttributeValue(
        element: *const c_void,
        attribute: *const c_void,
        value: *mut *mut c_void,
    ) -> i32;
    fn AXUIElementSetAttributeValue(
        element: *const c_void,
        attribute: *const c_void,
        value: *const c_void,
    ) -> i32;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(cf: *const c_void);
    fn CFGetTypeID(cf: *const c_void) -> usize;
    fn CFStringGetTypeID() -> usize;
    static kCFBooleanTrue: *const c_void;
}

/// Pause after asking an app to build its accessibility tree. To tune by testing.
const AFTER_ENABLE: Duration = Duration::from_millis(400);

/// Put in the clipboard before copying: if it is still there afterwards, nothing was copied.
const SENTINEL: &str = "\u{1}aede-no-selection\u{1}";

/// Pause between Cmd+C and reading the clipboard. To tune by testing.
const AFTER_COPY: Duration = Duration::from_millis(150);

/// How the text was read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Reading {
    /// The whole text of the field, through the accessibility API.
    Accessibility,
    /// The selection, or the current line in VS Code, through a copy.
    Copy,
}

/// A Core Foundation object we own: released when dropped.
struct Cf(*mut c_void);

impl Drop for Cf {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}

fn attribute(element: &Cf, name: &str) -> Result<Cf, i32> {
    let key = NSString::from_str(name);
    let mut out: *mut c_void = null_mut();
    let error = unsafe {
        AXUIElementCopyAttributeValue(
            element.0,
            &*key as *const NSString as *const c_void,
            &mut out,
        )
    };
    if error != 0 || out.is_null() {
        return Err(error);
    }
    Ok(Cf(out))
}

fn string_of(value: &Cf) -> Option<String> {
    unsafe {
        if CFGetTypeID(value.0) != CFStringGetTypeID() {
            return None;
        }
        // A CFString is an NSString (toll-free bridging).
        Some((*(value.0 as *const NSString)).to_string())
    }
}

/// The text of the focused field. The error says which step failed.
fn read_focused() -> Result<String, String> {
    let system = Cf(unsafe { AXUIElementCreateSystemWide() });
    let focused = attribute(&system, "AXFocusedUIElement")
        .map_err(|code| format!("pas de champ actif (code {code})"))?;
    let value = attribute(&focused, "AXValue")
        .map_err(|code| format!("champ sans texte lisible (code {code})"))?;
    string_of(&value).ok_or_else(|| "le contenu du champ n'est pas du texte".to_string())
}

/// Asks the active app to expose its accessibility tree.
fn enable_accessibility_tree() {
    let system = Cf(unsafe { AXUIElementCreateSystemWide() });
    if let Ok(app) = attribute(&system, "AXFocusedApplication") {
        let key = NSString::from_str("AXManualAccessibility");
        unsafe {
            AXUIElementSetAttributeValue(
                app.0,
                &*key as *const NSString as *const c_void,
                kCFBooleanTrue,
            );
        }
    }
}

/// The text of the focused field through the accessibility API. The text may be empty: some
/// apps answer without giving their content.
pub fn read_accessible() -> Result<String, String> {
    match read_focused() {
        Ok(text) if !text.trim().is_empty() => Ok(text),
        first => {
            enable_accessibility_tree();
            sleep(AFTER_ENABLE);
            read_focused().or(first)
        }
    }
}

/// What was read from the active app.
pub struct FieldRead {
    pub text: String,
    pub reading: Reading,
    /// What the user has selected in the field. Only known with the accessibility reading: a
    /// copy gives the selection as the text itself.
    pub selection: Option<String>,
}

/// The text the user has selected in the focused field, if any.
fn read_selected() -> Option<String> {
    let system = Cf(unsafe { AXUIElementCreateSystemWide() });
    let focused = attribute(&system, "AXFocusedUIElement").ok()?;
    let value = attribute(&focused, "AXSelectedText").ok()?;
    let text = string_of(&value)?.trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// The text of the focused field, or if the app gives none, what a Cmd+C copies.
pub fn read_or_copy(app: &AppHandle) -> Result<FieldRead, String> {
    match read_accessible() {
        Ok(text) if !text.trim().is_empty() => Ok(FieldRead {
            text,
            reading: Reading::Accessibility,
            selection: read_selected(),
        }),
        _ => copy_selection(app).map(|text| FieldRead {
            text,
            reading: Reading::Copy,
            selection: None,
        }),
    }
}

/// Copies the selection of the active app (the current line in VS Code when there is none) and
/// puts the old clipboard text back.
fn copy_selection(app: &AppHandle) -> Result<String, String> {
    let clipboard = app.clipboard();
    let previous = clipboard.read_text().ok();
    clipboard
        .write_text(SENTINEL)
        .map_err(|err| format!("Impossible d'écrire dans le presse-papiers : {err}"))?;

    let pressed = paste::press_cmd_c();
    sleep(AFTER_COPY);
    let copied = clipboard.read_text().ok();

    if let Some(previous) = previous {
        clipboard
            .write_text(previous)
            .map_err(|err| format!("Impossible de restaurer le presse-papiers : {err}"))?;
    }
    pressed?;
    match copied {
        Some(text) if text != SENTINEL && !text.trim().is_empty() => Ok(text.trim().to_string()),
        _ => {
            Err("Aède ne peut pas lire le texte de cette application : rien n'a été copié.".into())
        }
    }
}
