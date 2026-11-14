// SPDX-License-Identifier: GPL-3.0-only
//! Cooperative cancellation. The `nvmod` binary turns SIGTERM, SIGINT (and Ctrl+C on Windows) into
//! [`request`]; every writing step polls [`requested`] (through [`step_point`]) between its sub-steps and
//! fails with [`INTERRUPTED`], so the journal machinery that already undoes a failed step undoes this one.
//! A flag polled between steps keeps this crate free of `unsafe` and of any signal API.

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The reason a step reports when it stopped because the run was interrupted.
pub const INTERRUPTED: &str = "interrupted by a termination request";

/// How long [`step_point`] waits at a step named by `NVMOD_PAUSE_AT` before it gives up and goes on.
const PAUSE_LIMIT: Duration = Duration::from_secs(60);

static REQUESTED: AtomicBool = AtomicBool::new(false);

/// Ask the running command to stop at its next step.
pub fn request() {
    REQUESTED.store(true, Ordering::SeqCst);
}

/// Whether a stop was requested.
#[must_use]
pub fn requested() -> bool {
    REQUESTED.load(Ordering::SeqCst)
}

/// Clear the request (tests only; a process never un-requests).
pub fn reset() {
    REQUESTED.store(false, Ordering::SeqCst);
}

/// The place between two sub-steps of a writing step. It fails when the test hook `NVMOD_FAIL_AT=<step>` names
/// this step, or when a stop was requested. `NVMOD_PAUSE_AT=<step>` is the test hook for a slow step: it creates
/// the file named by `NVMOD_PAUSE_FILE` (so a test knows the step is running) and waits up to a minute for a
/// stop request.
///
/// # Errors
/// The reason the step must stop: the injected failure or [`INTERRUPTED`].
pub fn step_point(step: &str) -> Result<(), String> {
    if std::env::var("NVMOD_FAIL_AT").is_ok_and(|v| v == step) {
        return Err(format!("injected failure at step `{step}` (NVMOD_FAIL_AT)"));
    }
    if std::env::var("NVMOD_PAUSE_AT").is_ok_and(|v| v == step) {
        if let Ok(marker) = std::env::var("NVMOD_PAUSE_FILE") {
            let _ = std::fs::write(marker, step);
        }
        let start = Instant::now();
        while !requested() && start.elapsed() < PAUSE_LIMIT {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    if requested() {
        return Err(format!("{INTERRUPTED} at step `{step}`"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_makes_the_next_step_point_fail_and_reset_clears_it() {
        // One test owns the flag: the tests of this module run in one process.
        reset();
        assert!(step_point("anything").is_ok());
        request();
        assert!(requested());
        let why = step_point("lists").unwrap_err();
        assert!(why.contains(INTERRUPTED) && why.contains("lists"));
        reset();
        assert!(step_point("lists").is_ok());
    }
}
