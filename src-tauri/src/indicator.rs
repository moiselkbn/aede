//! The indicator on the notch: a small window over the notch, at the top of the screen, that
//! shows what the dictation is doing. The page drawn in it is `indicator.html`.
//!
//! macOS has no API for the notch, so the window is placed where the notch is, above the menu
//! bar, with AppKit calls that Tauri does not offer. Without a screen with a notch (external
//! screen, lid closed), it goes to the top middle of the main screen, over the menu bar.
//!
//! The window must never take the focus: the text is pasted in the app that has it. So the
//! window cannot become the active window, lets the clicks go through, and is put on screen
//! with `orderFrontRegardless`, never with Tauri's `show()`, which makes it the active window.
//! It stays on screen all the time, transparent while the app is idle: nothing is shown or
//! hidden during a dictation, only the page changes.

use objc2::MainThreadMarker;
use objc2_app_kit::{NSScreen, NSStatusWindowLevel, NSWindow, NSWindowCollectionBehavior};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::trial;

const LABEL: &str = "indicator";

/// Name of the event that tells the page the size of the notch when the screen changed.
const SPOT_EVENT: &str = "indicator-spot";

/// Name of the event that gives the page the loudness of the microphone while recording.
const LEVEL_EVENT: &str = "audio-level";

/// Names of the events that carry the answers of the learning shortcut (see learn.rs): a
/// proposal waiting for the second press, the result of an answer, and the end of a proposal.
const PROPOSAL_EVENT: &str = "learn-proposal";
const RESULT_EVENT: &str = "learn-result";
const CLEAR_EVENT: &str = "learn-clear";

/// Name of the event that shows a short text for a given time (the end of the first launch).
const NOTICE_EVENT: &str = "pill-notice";

/// Room left around the notch for the page to draw in, in points: on each side, and below. A
/// proposal with three lines and long words is the biggest thing the page shows.
const SIDE_ROOM: f64 = 200.0;
const BELOW_ROOM: f64 = 170.0;

/// Height used when the menu bar is hidden, on a screen without a notch.
const DEFAULT_BAR_HEIGHT: f64 = 24.0;

/// Where the indicator goes, in AppKit screen coordinates (in points, y going up from the
/// bottom of the main screen): the notch, or a notch of width 0 in the menu bar.
struct Spot {
    left: f64,
    top: f64,
    width: f64,
    height: f64,
}

/// What the page needs to draw the pill.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PageSpot {
    width: f64,
    height: f64,
    has_notch: bool,
}

impl Spot {
    fn for_page(&self) -> PageSpot {
        PageSpot {
            width: self.width,
            height: self.height,
            has_notch: self.width > 0.0,
        }
    }
}

/// A correction proposed to the user, with what the page needs to draw it.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    /// (misheard, wanted) pairs, one line each.
    pub pairs: Vec<(String, String)>,
    /// The shortcut to press again to confirm, e.g. "Option+Maj+Espace".
    pub shortcut: String,
    /// How long the proposal stays, for the bar that empties.
    pub seconds: u64,
}

/// The end of an answer: a short text with a green dot (`ok`) or a red one.
#[derive(Clone, Serialize)]
pub struct Outcome {
    pub ok: bool,
    pub text: String,
}

/// A short text with a green dot that stays `ms` milliseconds, then the pill goes back in.
#[derive(Clone, Serialize)]
struct Notice {
    text: String,
    ms: u64,
}

/// Lets the window say something in the pill, e.g. that the app is active once the first launch
/// is over. Nothing happens if the page cannot be reached: the text is only a courtesy.
#[tauri::command]
pub fn show_notice(app: AppHandle, text: String, ms: u64) {
    emit_to_page(&app, NOTICE_EVENT, Notice { text, ms });
}

/// The notch of the first screen that has one (the built-in screen), if any.
fn find_notch(mtm: MainThreadMarker) -> Option<Spot> {
    NSScreen::screens(mtm).iter().find_map(|screen| {
        // The part of the screen hidden by the camera housing: 0 on a screen without a notch.
        let height = screen.safeAreaInsets().top;
        if height <= 0.0 {
            return None;
        }
        let frame = screen.frame();
        // The menu bar parts on the left and on the right of the notch.
        let left_area = screen.auxiliaryTopLeftArea().size.width;
        let right_area = screen.auxiliaryTopRightArea().size.width;
        Some(Spot {
            left: frame.origin.x + left_area,
            top: frame.origin.y + frame.size.height,
            width: frame.size.width - left_area - right_area,
            height,
        })
    })
}

/// The top middle of the main screen (the one the user works on), as high as its menu bar.
fn find_menu_bar(mtm: MainThreadMarker) -> Option<Spot> {
    let screen = NSScreen::mainScreen(mtm)?;
    let frame = screen.frame();
    let top = frame.origin.y + frame.size.height;
    // The visible part of the screen stops under the menu bar.
    let visible = screen.visibleFrame();
    let bar = top - (visible.origin.y + visible.size.height);
    Some(Spot {
        left: frame.origin.x + frame.size.width / 2.0,
        top,
        width: 0.0,
        height: if bar > 0.0 { bar } else { DEFAULT_BAR_HEIGHT },
    })
}

fn find_spot(mtm: MainThreadMarker) -> Option<Spot> {
    find_notch(mtm).or_else(|| find_menu_bar(mtm))
}

/// The NSWindow behind the indicator window.
fn ns_window(window: &WebviewWindow) -> Result<&NSWindow, String> {
    let ptr = window
        .ns_window()
        .map_err(|e| format!("could not reach the macOS window: {e}"))?;
    // SAFETY: Tauri gives the NSWindow behind `window`, which lives as long as the app (the
    // indicator is never closed). Callers are on the main thread.
    Ok(unsafe { &*(ptr as *const NSWindow) })
}

/// Moves the window so that the spot is at the top, in its exact middle.
fn move_window(ns_window: &NSWindow, spot: &Spot) {
    // macOS rounds the window position to whole points, but the notch may start on a half
    // point (185 pt wide on a 1512 pt screen). Start on a whole point and widen the window by
    // the difference, so that the notch stays in its exact middle.
    let left = (spot.left - SIDE_ROOM).floor();
    let width = 2.0 * (spot.left + spot.width / 2.0 - left);
    let height = spot.height + BELOW_ROOM;
    let frame = NSRect::new(
        NSPoint::new(left, spot.top - height),
        NSSize::new(width, height),
    );
    ns_window.setFrame_display(frame, true);
}

/// Creates the indicator window over the notch, or in the menu bar without a notch.
/// Must run on the main thread (AppKit only accepts calls from there).
pub fn create(app: &AppHandle) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("not on the main thread")?;
    let spot = find_spot(mtm).ok_or("no screen found")?;
    log_spot(&spot);

    let page_spot = serde_json::to_string(&spot.for_page()).map_err(|e| e.to_string())?;
    let window = WebviewWindowBuilder::new(app, LABEL, WebviewUrl::App("indicator.html".into()))
        .title(&app.package_info().name)
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .resizable(false)
        .focusable(false)
        .focused(false)
        .skip_taskbar(true)
        .visible(false)
        // The page needs the size of the notch to draw around it, before it gets any event.
        .initialization_script(format!("window.__NOTCH__ = {page_spot};"))
        .build()
        .map_err(|e| format!("could not create the window: {e}"))?;

    let ns_window = ns_window(&window)?;
    // Above the menu bar and its icons, but below the menus that open from them.
    ns_window.setLevel(NSStatusWindowLevel + 1);
    // On every desktop, over apps in full screen, and out of Mission Control and Cmd+`.
    ns_window.setCollectionBehavior(
        NSWindowCollectionBehavior::CanJoinAllSpaces
            | NSWindowCollectionBehavior::Stationary
            | NSWindowCollectionBehavior::FullScreenAuxiliary
            | NSWindowCollectionBehavior::IgnoresCycle,
    );
    // Clicks go to what is under the window (the menu bar icons next to the notch).
    ns_window.setIgnoresMouseEvents(true);
    move_window(ns_window, &spot);
    ns_window.orderFrontRegardless();
    Ok(())
}

/// Moves the indicator to the notch or the main screen again: screens may have been plugged
/// in or out since the last dictation. Called when a dictation starts, from any thread.
pub fn follow_screens(app: &AppHandle) {
    let handle = app.clone();
    let result = app.run_on_main_thread(move || {
        if let Err(err) = place(&handle) {
            eprintln!("[indicator] {err}");
        }
    });
    if let Err(err) = result {
        eprintln!("[indicator] could not reach the main thread: {err}");
    }
}

/// Gives the page the loudness of the microphone (RMS, 0.0 to 1.0), for the wave drawn while
/// recording. Called about 30 times per second by the capture thread.
pub fn show_level(app: &AppHandle, level: f32) {
    if !trial::setup_open(app)
        && let Err(err) = app.emit_to(LABEL, LEVEL_EVENT, level)
    {
        eprintln!("[indicator] could not send the microphone level: {err}");
    }
    // The window of the first launch draws the same wave in its own card.
    if trial::wants_levels(app) {
        let _ = app.emit_to("main", LEVEL_EVENT, level);
    }
}

/// Shows a proposal under the notch. False if the page could not be reached: the caller then
/// falls back on a notification, so that the answer is never lost.
pub fn show_proposal(app: &AppHandle, proposal: &Proposal) -> bool {
    emit_to_page(app, PROPOSAL_EVENT, proposal)
}

/// Shows the result of an answer (added, or why nothing was proposed). False like above.
pub fn show_outcome(app: &AppHandle, outcome: &Outcome) -> bool {
    emit_to_page(app, RESULT_EVENT, outcome)
}

/// Takes the proposal down without a result (cancelled, expired, or a dictation started).
/// False if the page could not be reached.
pub fn clear_proposal(app: &AppHandle) -> bool {
    emit_to_page(app, CLEAR_EVENT, ())
}

fn emit_to_page<S: Serialize + Clone>(app: &AppHandle, event: &str, payload: S) -> bool {
    // No window if it could not be created at startup.
    if app.get_webview_window(LABEL).is_none() {
        return false;
    }
    match app.emit_to(LABEL, event, payload) {
        Ok(()) => true,
        Err(err) => {
            eprintln!("[indicator] could not send {event}: {err}");
            false
        }
    }
}

fn place(app: &AppHandle) -> Result<(), String> {
    // No window if it could not be created at startup: nothing to move.
    let Some(window) = app.get_webview_window(LABEL) else {
        return Ok(());
    };
    let mtm = MainThreadMarker::new().ok_or("not on the main thread")?;
    let spot = find_spot(mtm).ok_or("no screen found")?;
    move_window(ns_window(&window)?, &spot);
    window
        .emit_to(LABEL, SPOT_EVENT, spot.for_page())
        .map_err(|e| format!("could not notify the page: {e}"))
}

fn log_spot(spot: &Spot) {
    if spot.width > 0.0 {
        eprintln!(
            "[indicator] notch {} x {} pt, from {} pt",
            spot.width, spot.height, spot.left
        );
    } else {
        eprintln!(
            "[indicator] no notch: top middle of the main screen, menu bar {} pt",
            spot.height
        );
    }
}
