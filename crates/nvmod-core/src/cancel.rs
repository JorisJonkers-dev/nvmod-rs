// SPDX-License-Identifier: GPL-3.0-only
//! Cooperative cancellation. The `nvmod` binary turns SIGTERM, SIGINT (and Ctrl+C on Windows) into
//! [`request`]; every writing step polls [`requested`] (through [`step_point`]) between its sub-steps and
//! fails with [`INTERRUPTED`], so the journal machinery that already undoes a failed step undoes this one.
//! A flag polled between steps keeps this crate free of `unsafe` and of any signal API.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

/// The reason a step reports when it stopped because the run was interrupted.
pub const INTERRUPTED: &str = "interrupted by a termination request";

/// How long [`step_point`] waits at a step named by `NVMOD_PAUSE_AT` before it gives up and goes on.
const PAUSE_LIMIT: Duration = Duration::from_secs(60);

static REQUESTED: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// The stop flag of the run this thread is working for, when it was given one ([`enter`]).
    static SCOPE: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// While it lives, this thread's [`requested`] and [`step_point`] answer for the run's own flag and not for the
/// process-wide request. See [`enter`].
#[derive(Debug)]
#[must_use = "the scope ends when the guard is dropped"]
pub struct Scope {
    previous: Option<Arc<AtomicBool>>,
}

impl Drop for Scope {
    fn drop(&mut self) {
        let previous = self.previous.take();
        SCOPE.with(|s| *s.borrow_mut() = previous);
    }
}

/// Make this thread's safe points ([`step_point`], [`requested`]) obey `flag` only, until the guard is dropped, so
/// one run of a library caller stops with its own token and a process-wide request (left set by a signal handler or
/// by another run) does not stop it. A thread that was given no scope keeps reading the process-wide request, which
/// is what the command line sets from SIGINT and SIGTERM. Scopes nest: the guard puts the outer one back.
pub fn enter(flag: Arc<AtomicBool>) -> Scope {
    let previous = SCOPE.with(|s| s.borrow_mut().replace(flag));
    Scope { previous }
}
/// Ask the running command to stop at its next step.
pub fn request() {
    REQUESTED.store(true, Ordering::SeqCst);
}

/// Whether a stop was requested: by the run's own flag when this thread is inside a [`enter`] scope, by the
/// process-wide request otherwise.
#[must_use]
pub fn requested() -> bool {
    SCOPE.with(|s| {
        s.borrow().as_ref().map_or_else(
            || REQUESTED.load(Ordering::SeqCst),
            |f| f.load(Ordering::SeqCst),
        )
    })
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
        a_scope_obeys_its_own_flag_and_ignores_the_process_wide_request();
        assert!(!requested());
    }

    // Called by the test above, so one test owns the process-wide flag.
    fn a_scope_obeys_its_own_flag_and_ignores_the_process_wide_request() {
        std::thread::spawn(|| {
            let own = Arc::new(AtomicBool::new(false));
            {
                let _scope = enter(Arc::clone(&own));
                // A process-wide request does not stop a run that has its own flag.
                REQUESTED.store(true, Ordering::SeqCst);
                assert!(!requested());
                assert!(step_point("copy").is_ok());
                own.store(true, Ordering::SeqCst);
                assert!(requested());
                assert!(step_point("copy").unwrap_err().contains(INTERRUPTED));
                // A nested scope restores the outer one.
                let inner = Arc::new(AtomicBool::new(false));
                {
                    let _inner = enter(inner);
                    assert!(!requested());
                }
                assert!(requested());
            }
            // Outside the scope the process-wide request is read again.
            assert!(requested());
            REQUESTED.store(false, Ordering::SeqCst);
        })
        .join()
        .unwrap();
    }
}
