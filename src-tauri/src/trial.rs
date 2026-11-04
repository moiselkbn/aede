//! The trials of the first launch. While the window is on one of them, the shortcuts do not
//! behave as usual:
//! - shortcut test: a press only tells the window (event `shortcut-test`), nothing is recorded;
//! - learning shortcut test: the same for the learning shortcut (event `learn-test`), on its
//!   screen and on the learning trial that follows it, where the window reacts to the presses;
//! - dictation trial: the dictation goes all the way, but leaves no trace (no history entry, no
//!   statistics, no kept audio), so the first entry of the history is a real one.
//!
//! The notch stays off for the whole first launch (`setup_open`): the window draws its own
//! (see `emit_ui` in `dictation.rs`), and the learning shortcut never reads another application
//! during it (see `learn::handle`).
//!
//! Both are ignored unless the window is in front: a window hidden in the middle of the first
//! launch must not change how the shortcut works everywhere else.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::{AppHandle, Emitter, Manager};

/// Name of the event sent on every press (`true`) and release (`false`) during the shortcut test.
const SHORTCUT_EVENT: &str = "shortcut-test";

/// Same for the learning shortcut.
const LEARN_EVENT: &str = "learn-test";

#[derive(Default)]
pub struct Trial {
    /// The first launch screens are on: the real notch is not used until they are finished.
    setup_open: AtomicBool,
    shortcut_test: AtomicBool,
    learn_test: AtomicBool,
    dictation: AtomicBool,
    /// The press that the shortcut test took: its release is taken too, whatever the window does
    /// in between, or the dictation would be left recording.
    press_taken: AtomicBool,
}

fn window_in_front(app: &AppHandle) -> bool {
    app.get_webview_window("main")
        .and_then(|window| window.is_focused().ok())
        .unwrap_or(false)
}

fn enabled(app: &AppHandle, flag: impl Fn(&Trial) -> &AtomicBool) -> bool {
    flag(&app.state::<Trial>()).load(Ordering::SeqCst) && window_in_front(app)
}

/// Tells the window about a press or a release and returns true if the shortcut test took it.
pub fn take_shortcut_press(app: &AppHandle, pressed: bool) -> bool {
    let trial = app.state::<Trial>();
    let taken = if pressed {
        let taken = enabled(app, |trial| &trial.shortcut_test);
        trial.press_taken.store(taken, Ordering::SeqCst);
        taken
    } else {
        trial.press_taken.swap(false, Ordering::SeqCst)
    };
    if !taken {
        return false;
    }
    if let Err(err) = app.emit(SHORTCUT_EVENT, pressed) {
        eprintln!("[trial] could not notify the window: {err}");
    }
    true
}

/// Tells the window about a press or a release of the learning shortcut and returns true if its
/// test is on (the learning is then not done). No release to follow: the learning only acts on
/// presses.
pub fn take_learn_press(app: &AppHandle, pressed: bool) -> bool {
    if !enabled(app, |trial| &trial.learn_test) {
        return false;
    }
    if let Err(err) = app.emit(LEARN_EVENT, pressed) {
        eprintln!("[trial] could not notify the window: {err}");
    }
    true
}

/// True if the dictation that ends now is the trial one: it must leave no trace.
pub fn is_dictation_trial(app: &AppHandle) -> bool {
    enabled(app, |trial| &trial.dictation)
}

/// True while the window shows the dictation trial: it draws the wave of the voice itself, so
/// the levels go to it as well as to the indicator. Only the flag is read, not the focus: this
/// is asked about thirty times per second, from the capture thread.
pub fn wants_levels(app: &AppHandle) -> bool {
    app.state::<Trial>().dictation.load(Ordering::SeqCst)
}

/// True while the first launch is going on in a window the user can see. A window closed in the
/// middle of it gives the notch back, or a dictation made elsewhere would have no feedback.
pub fn setup_open(app: &AppHandle) -> bool {
    app.state::<Trial>().setup_open.load(Ordering::SeqCst)
        && app
            .get_webview_window("main")
            .and_then(|window| window.is_visible().ok())
            .unwrap_or(false)
}

#[tauri::command]
pub fn set_setup_open(trial: tauri::State<'_, Trial>, on: bool) {
    trial.setup_open.store(on, Ordering::SeqCst);
}

#[tauri::command]
pub fn set_shortcut_test(trial: tauri::State<'_, Trial>, on: bool) {
    trial.shortcut_test.store(on, Ordering::SeqCst);
}

#[tauri::command]
pub fn set_learn_test(trial: tauri::State<'_, Trial>, on: bool) {
    trial.learn_test.store(on, Ordering::SeqCst);
}

#[tauri::command]
pub fn set_dictation_trial(trial: tauri::State<'_, Trial>, on: bool) {
    trial.dictation.store(on, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_trials_are_off_by_default() {
        let trial = Trial::default();
        assert!(!trial.setup_open.load(Ordering::SeqCst));
        assert!(!trial.shortcut_test.load(Ordering::SeqCst));
        assert!(!trial.learn_test.load(Ordering::SeqCst));
        assert!(!trial.dictation.load(Ordering::SeqCst));
        assert!(!trial.press_taken.load(Ordering::SeqCst));
    }
}
