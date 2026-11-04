//! The "stop now" flag of a dictation.
//!
//! One dictation runs at a time, so one flag is enough. The dictation sets it when the user
//! cancels; Whisper and the rewriting read it while they work and stop early. Reading a flag
//! avoids passing a cancel handle through every function that computes.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

static REQUESTED: AtomicBool = AtomicBool::new(false);

/// When the request was made, only to log how long the dictation took to notice it.
static REQUESTED_AT: Mutex<Option<Instant>> = Mutex::new(None);

/// The user asked to cancel the dictation in progress.
pub fn request() {
    *REQUESTED_AT.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    REQUESTED.store(true, Ordering::SeqCst);
}

/// Time since the request, if there is one.
pub fn since_request() -> Option<Duration> {
    let at = *REQUESTED_AT.lock().unwrap_or_else(|e| e.into_inner());
    at.map(|at| at.elapsed())
}

/// Forgets a cancel request. Called when a new dictation starts.
pub fn reset() {
    REQUESTED.store(false, Ordering::SeqCst);
}

/// True if the dictation in progress must stop.
pub fn is_requested() -> bool {
    REQUESTED.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_stays_until_it_is_reset() {
        reset();
        assert!(!is_requested());
        request();
        assert!(is_requested());
        reset();
        assert!(!is_requested());
    }
}
