// SPDX-License-Identifier: GPL-3.0-only
//! One run of a command: its id, its run log, the subscriber and the panic hook.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::{Instant, SystemTime};

use serde_json::json;
use tracing::span::EnteredSpan;
use tracing_subscriber::filter::LevelFilter;
use tracing_subscriber::layer::SubscriberExt;

use crate::layer::{FileLayer, StderrLayer};
use crate::redact::Redactor;
use crate::state::{KEEP_BYTES, KEEP_RUNS, logs_dir, prune, state_dir};

/// The version of the run log's layout, in the header line. The log is not a contract: keys may change with it.
pub const LOG_SCHEMA_VERSION: u32 = 1;

static RUN_ID: OnceLock<String> = OnceLock::new();

/// The id of the current run, once [`Session::start`] has made one. The runner passes it to tools as
/// `NVMOD_RUN_ID`.
#[must_use]
pub fn run_id() -> Option<&'static str> {
    RUN_ID.get().map(String::as_str)
}

/// How a run logs.
#[derive(Debug, Clone)]
pub struct Config {
    /// The command, for the `op` key (`install`, `mods reorder`, ...). Never its arguments.
    pub op: String,
    /// The version of the program, for the header.
    pub version: String,
    /// Where to write; `None` is the default state directory ([`state_dir`]).
    pub state_dir: Option<PathBuf>,
    /// Redaction of the file.
    pub redactor: Redactor,
    /// The level of the text on standard error (default `warn`).
    pub stderr_level: LevelFilter,
    /// Write a run log (false only for `--help` and `--version`).
    pub write_file: bool,
}

/// A started run. Keep it until the command is over and call [`Session::finish`].
pub struct Session {
    run_id: String,
    started: Instant,
    log_path: Option<PathBuf>,
    _span: Option<EnteredSpan>,
}

fn create_log(logs: &std::path::Path, id: &mut String) -> Option<File> {
    fs::create_dir_all(logs).ok()?;
    prune(logs, KEEP_RUNS - 1, KEEP_BYTES);
    let base = id.clone();
    for n in 1..100 {
        let name = if n == 1 {
            base.clone()
        } else {
            format!("{base}-{n:02}")
        };
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(logs.join(format!("{name}.ndjson")))
        {
            Ok(file) => {
                *id = name;
                return Some(file);
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(_) => return None,
        }
    }
    None
}

impl Session {
    /// Start the run: make the run id, open `logs/<run_id>.ndjson` (after removing the oldest runs), write the
    /// header, install the subscriber and the panic hook, and enter the root span. A log that cannot be
    /// written is skipped without a word: the command still runs.
    #[must_use]
    pub fn start(config: Config) -> Session {
        let now = SystemTime::now();
        let mut id = format!("{}-{}", crate::time::compact(now), std::process::id());
        let state = config.state_dir.clone().unwrap_or_else(state_dir);
        let logs = logs_dir(&state);
        let mut file = config
            .write_file
            .then(|| create_log(&logs, &mut id))
            .flatten();
        let log_path = file.is_some().then(|| logs.join(format!("{id}.ndjson")));
        if let Some(f) = file.as_mut() {
            let header = json!({
                "kind": "header", "log_schema": LOG_SCHEMA_VERSION, "run_id": id,
                "ts": crate::time::format_time(now), "nvmod_version": config.version,
                "os": std::env::consts::OS, "arch": std::env::consts::ARCH,
                "note": "not a contract: keys may change with log_schema",
            });
            let _ = writeln!(f, "{header}");
        }
        let _ = RUN_ID.set(id.clone());
        let file_level = if config.stderr_level > LevelFilter::INFO {
            config.stderr_level
        } else {
            LevelFilter::INFO
        };
        let max = if file.is_some() {
            file_level
        } else {
            config.stderr_level
        };
        let subscriber = tracing_subscriber::registry()
            .with(max)
            .with(StderrLayer::new(config.stderr_level))
            .with(file.map(|f| FileLayer::new(f, config.redactor, file_level)));
        let installed = tracing::subscriber::set_global_default(subscriber).is_ok();
        let span =
            installed.then(|| tracing::info_span!("run", run_id = %id, op = %config.op).entered());
        install_panic_hook(log_path.clone());
        Session {
            run_id: id,
            started: Instant::now(),
            log_path,
            _span: span,
        }
    }

    /// The id of this run.
    #[must_use]
    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// The run log, when one is being written.
    #[must_use]
    pub fn log_path(&self) -> Option<&std::path::Path> {
        self.log_path.as_deref()
    }

    /// Record the end of the command: its exit code and how long it took.
    pub fn finish(self, exit_code: u8) {
        let duration_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        tracing::info!(
            exit_code = u64::from(exit_code),
            duration_ms,
            "command finished"
        );
    }
}

/// On a panic: log it, say that it is a bug, where the run log is and how to make a support bundle, then run the
/// hook that was there (the exit code stays the one a panic always had).
fn install_panic_hook(log_path: Option<PathBuf>) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        tracing::error!(panic = %info, "the program panicked");
        let mut err = std::io::stderr().lock();
        let _ = writeln!(err, "nvmod: this is a bug in nvmod.");
        if let Some(path) = &log_path {
            let _ = writeln!(err, "nvmod: the run log is {}", path.display());
        }
        let _ = writeln!(
            err,
            "nvmod: `nvmod support-bundle` collects what a bug report needs; nothing is sent anywhere."
        );
        drop(err);
        previous(info);
    }));
}
