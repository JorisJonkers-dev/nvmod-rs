// SPDX-License-Identifier: GPL-3.0-only
//! The local run log of nvmod (ADR-0012 of nv-linux). Library crates use `tracing` macros and install nothing;
//! a binary calls [`Session::start`], which installs the subscriber: human text on standard error (default
//! `warn`) and `logs/<run_id>.ndjson` in the state directory ([`state_dir`]), redacted at the writer
//! ([`Redactor`]) and rotated to the last [`KEEP_RUNS`] runs or [`KEEP_BYTES`]. Nothing is sent anywhere.
//!
//! Keys of a log line: `ts level run_id op step tool tool_version exit_code duration_ms msg`. The log is not a
//! contract and its first line says its schema version.

mod layer;
mod redact;
mod secret;
mod session;
mod state;
mod time;

pub use layer::{FileLayer, StderrLayer};
pub use redact::{KEY_MASK, PATH_MASK, Redactor};
pub use secret::{REDACTED, Secret};
pub use session::{Config, LOG_SCHEMA_VERSION, Session, run_id};
pub use state::{KEEP_BYTES, KEEP_RUNS, logs_dir, prune, state_dir};

/// The most bytes of a tool's standard error written to the run log per call.
pub const STDERR_CAP: usize = 64 * 1024;

/// The tool's standard error cut to [`STDERR_CAP`] bytes (at a character boundary), for one log event.
#[must_use]
pub fn capped_stderr(stderr: &str) -> &str {
    let mut end = stderr.len().min(STDERR_CAP);
    while !stderr.is_char_boundary(end) {
        end -= 1;
    }
    &stderr[..end]
}
