//! The microphone test of the first launch: opens the microphone, sends its loudness to the
//! page about 30 times per second (the wave of the test screen), and stops when the screen is
//! left. The audio is only kept in memory while the test runs, then thrown away: nothing is
//! written to the disk and nothing is sent anywhere.

use std::sync::{Mutex, MutexGuard};

use tauri::{AppHandle, Emitter, Manager};

use crate::audio::{self, Recording};
use crate::microphone;

/// The event the page listens to: the volume of the microphone, 0.0 to 1.0 (RMS).
const LEVEL_EVENT: &str = "mic-test-level";

/// The test in progress, if any.
#[derive(Default)]
pub struct MicTest {
    recording: Mutex<Option<Recording>>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

impl MicTest {
    /// Keeps `recording` as the test in progress. A test already running is kept: the new one
    /// is stopped, so that two tests never run at once.
    fn keep(&self, recording: Recording) {
        let mut slot = lock(&self.recording);
        if slot.is_some() {
            drop(slot);
            let _ = recording.stop();
        } else {
            *slot = Some(recording);
        }
    }

    fn is_running(&self) -> bool {
        lock(&self.recording).is_some()
    }

    /// Stops the test and throws the audio away. Does nothing if no test runs.
    fn stop(&self) {
        let recording = lock(&self.recording).take();
        if let Some(recording) = recording {
            // The audio is dropped here, on purpose.
            let _ = recording.stop();
        }
    }
}

/// Opens the microphone and starts sending its level. Opening can take a moment (macOS may ask
/// for the permission), so it runs off the main thread.
#[tauri::command]
pub async fn start_mic_test(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        if app.state::<MicTest>().is_running() {
            return Ok(());
        }
        let level_app = app.clone();
        let recording = audio::start(microphone::chosen(&app), move |level| {
            if let Err(err) = level_app.emit(LEVEL_EVENT, level) {
                eprintln!("[mic-test] could not send the level: {err}");
            }
        })?;
        app.state::<MicTest>().keep(recording);
        Ok(())
    })
    .await
    .map_err(|err| format!("Le test du micro a planté : {err}"))?
}

#[tauri::command]
pub fn stop_mic_test(test: tauri::State<'_, MicTest>) {
    test.stop();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stopping_without_a_test_does_nothing() {
        let test = MicTest::default();
        test.stop();
        assert!(!test.is_running());
    }
}
