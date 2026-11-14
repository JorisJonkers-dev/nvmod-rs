// SPDX-License-Identifier: GPL-3.0-only
//! The installation check: `nv-viewer <data folder> --check` (exit 0 when the game can start, 1 when not). nv-rs
//! has no standalone play-patch program (its play-patch crate is linked into `nv-viewer`), so this check is the
//! one thing nvmod runs after an install. A release's `release-manifest.json` may name the arguments through its
//! `installation_check` entry; without one the default above is used.

use crate::install::InstallError;
use nvmod_runner::{Call, Runner};
use std::path::Path;
use std::time::Duration;

/// The placeholder the manifest uses for the base copy's Data folder.
const DATA_PLACEHOLDER: &str = "<data folder>";
/// The name the manifest's check command must have: the only program nvmod runs is the installed viewer.
const VIEWER: &str = "nv-viewer";
/// How long the check may take. It reads the whole Data folder, so it is generous, but it cannot hang.
const CHECK_TIMEOUT: Duration = Duration::from_mins(30);
/// How much of the check's output is kept, per stream.
const CHECK_OUTPUT_CAP: usize = 16 * 1024 * 1024;

/// The default arguments of the check.
pub(crate) fn default_arguments() -> Vec<String> {
    vec![DATA_PLACEHOLDER.to_owned(), "--check".to_owned()]
}

/// The arguments of the check from a release's `release-manifest.json` text, with the default for a manifest that
/// has no `installation_check`.
///
/// # Errors
/// A manifest that is not JSON, or whose `installation_check` is malformed or names another program.
pub(crate) fn arguments_from_manifest(text: &str) -> Result<Vec<String>, InstallError> {
    let bad = |why: &str| InstallError::Verification(format!("release-manifest.json: {why}"));
    let manifest: serde_json::Value =
        serde_json::from_str(text).map_err(|e| bad(&format!("not JSON ({e})")))?;
    let Some(entry) = manifest.get("installation_check") else {
        return Ok(default_arguments());
    };
    let command = entry.get("command").and_then(serde_json::Value::as_str);
    if command.map(|c| c.trim_end_matches(".exe")) != Some(VIEWER) {
        return Err(bad(&format!(
            "installation_check.command must be {VIEWER}; nvmod runs no other program"
        )));
    }
    entry
        .get("arguments")
        .and_then(serde_json::Value::as_array)
        .and_then(|list| {
            list.iter()
                .map(|a| a.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
        })
        .ok_or_else(|| bad("installation_check.arguments must be a list of strings"))
}

/// What the check did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Check {
    /// The program and its arguments, as run.
    pub command: Vec<String>,
    /// What the check printed (standard output, then standard error).
    pub output: String,
}

/// Run `viewer` with `arguments` against `data` (the placeholder is replaced by it).
pub(crate) fn run(viewer: &Path, arguments: &[String], data: &Path) -> Result<Check, InstallError> {
    let runner = Runner {
        timeout: CHECK_TIMEOUT,
        max_output: CHECK_OUTPUT_CAP,
        max_stderr: CHECK_OUTPUT_CAP,
        ..Runner::default()
    };
    run_with(&runner, viewer, arguments, data)
}

fn run_with(
    runner: &Runner,
    viewer: &Path,
    arguments: &[String],
    data: &Path,
) -> Result<Check, InstallError> {
    let arguments: Vec<String> = arguments
        .iter()
        .map(|a| {
            if a == DATA_PLACEHOLDER {
                data.display().to_string()
            } else {
                a.clone()
            }
        })
        .collect();
    let mut command = vec![viewer.display().to_string()];
    command.extend(arguments.iter().cloned());
    let out = runner
        .run_any(&Call::new(VIEWER, viewer, &arguments))
        .map_err(|e| {
            InstallError::Failed(format!(
                "cannot run the installation check {}: {e}",
                command.join(" ")
            ))
        })?;
    let output = format!("{}{}", String::from_utf8_lossy(&out.stdout), out.stderr);
    if out.exit_code == 0 {
        Ok(Check { command, output })
    } else {
        Err(InstallError::Failed(format!(
            "the installation check {} failed (exit status: {}):\n{}",
            command.join(" "),
            out.exit_code,
            output.trim_end()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    fn run_script(body: &str, runner: &Runner) -> Result<Check, InstallError> {
        let dir = nvmod_testkit::TempDir::new("check-runner");
        let script = nvmod_testkit::write_script(dir.path(), "nv-viewer", body);
        run_with(runner, &script, &default_arguments(), dir.path())
    }

    #[cfg(unix)]
    #[test]
    fn the_check_runs_with_a_scrubbed_environment() {
        let check = run_script("env", &Runner::default()).unwrap();
        assert!(check.output.contains("LC_ALL=C"), "{}", check.output);
        assert!(!check.output.contains("HOME="), "{}", check.output);
    }

    #[cfg(unix)]
    #[test]
    fn the_check_is_stopped_when_it_runs_too_long() {
        let runner = Runner {
            timeout: Duration::from_millis(300),
            ..Runner::default()
        };
        let why = run_script("exec sleep 30", &runner).unwrap_err();
        assert!(why.to_string().contains("longer than"), "{why}");
    }

    #[cfg(unix)]
    #[test]
    fn the_check_is_stopped_when_it_writes_past_the_cap() {
        let runner = Runner {
            max_output: 1000,
            ..Runner::default()
        };
        let why = run_script("exec yes", &runner).unwrap_err();
        assert!(why.to_string().contains("cap of 1000"), "{why}");
    }

    #[cfg(unix)]
    #[test]
    fn a_failing_check_keeps_its_exit_status_and_output() {
        let why = run_script("echo broken >&2; exit 1", &Runner::default()).unwrap_err();
        let text = why.to_string();
        assert!(
            text.contains("failed (exit status: 1)") && text.contains("broken"),
            "{text}"
        );
    }

    #[test]
    fn no_entry_gives_the_default() {
        assert_eq!(arguments_from_manifest("{}").unwrap(), default_arguments());
    }

    #[test]
    fn an_entry_gives_its_arguments() {
        let text = r#"{"installation_check":{"command":"nv-viewer.exe","arguments":["a","b"]}}"#;
        assert_eq!(arguments_from_manifest(text).unwrap(), ["a", "b"]);
    }

    #[test]
    fn bad_entries_are_refused() {
        for text in [
            "[",
            r#"{"installation_check":{"command":"x","arguments":[]}}"#,
            r#"{"installation_check":{"command":"nv-viewer","arguments":[1]}}"#,
            r#"{"installation_check":{"command":"nv-viewer"}}"#,
        ] {
            assert!(arguments_from_manifest(text).is_err(), "{text}");
        }
    }
}
