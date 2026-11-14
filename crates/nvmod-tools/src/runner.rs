// SPDX-License-Identifier: GPL-3.0-only
//! The one subprocess runner every tool call goes through. It takes an argument vector (never a shell line), a
//! timeout, an output size cap and a scrubbed environment, re-verifies the tool's SHA-256 before each run when the
//! caller pins one, and maps what happened onto the exit code table: a tool that failed is 4, a tool that cannot
//! be verified is 3, a tool whose contract version is newer than nvmod reads is refused (2).

use crate::{
    Contract, ContractError, EXIT_REFUSED, EXIT_TOOL_FAILED, EXIT_VERIFICATION, sha256_hex,
};
use serde_json::Value;
use std::fmt;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

/// How much of the tool's standard error is kept for a report.
const STDERR_KEEP: usize = 64 * 1024;
/// How long to wait for a pipe to finish after the tool has ended (a child it left behind may hold it open).
const PIPE_GRACE: Duration = Duration::from_secs(2);

/// Why a tool call did not give a usable answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// The tool could not be verified: its checksum is not the pinned one (exit code 3).
    Unverifiable {
        /// The tool's name.
        tool: String,
        /// What was found.
        reason: String,
    },
    /// The tool could not be run, ran too long, wrote too much, or exited with a failure (exit code 4).
    Failed {
        /// The tool's name.
        tool: String,
        /// What happened.
        reason: String,
    },
    /// The tool's output is not a valid document of its contract (exit code 4).
    Malformed {
        /// The tool's name.
        tool: String,
        /// The first validation error.
        reason: String,
    },
    /// The tool speaks a contract version this nvmod does not read (exit code 2).
    Refused {
        /// The tool's name.
        tool: String,
        /// Why, with both versions.
        reason: String,
    },
}

impl ToolError {
    /// The exit code of the command that hit this error.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            ToolError::Unverifiable { .. } => EXIT_VERIFICATION,
            ToolError::Failed { .. } | ToolError::Malformed { .. } => EXIT_TOOL_FAILED,
            ToolError::Refused { .. } => EXIT_REFUSED,
        }
    }
}

impl fmt::Display for ToolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ToolError::Unverifiable { tool, reason } => {
                write!(f, "{tool} could not be verified: {reason}")
            }
            ToolError::Failed { tool, reason } => write!(f, "{tool} failed: {reason}"),
            ToolError::Malformed { tool, reason } => {
                write!(f, "{tool} gave a malformed response: {reason}")
            }
            ToolError::Refused { tool, reason } => write!(f, "{tool} is refused: {reason}"),
        }
    }
}

impl std::error::Error for ToolError {}

/// One call of a tool.
#[derive(Debug, Clone)]
pub struct Call<'a> {
    /// The tool's name, for reports.
    pub tool: &'a str,
    /// The program to run.
    pub program: &'a Path,
    /// Its arguments, one word each. There is no shell.
    pub args: &'a [String],
    /// Bytes written to the tool's standard input, then it is closed.
    pub stdin: Option<&'a [u8]>,
    /// The SHA-256 the program must have, checked right before it is started; `None` runs it unchecked.
    pub expected_sha256: Option<&'a str>,
    /// The exit codes that count as an answer (default: 0). A tool such as loot-rs exits 1 with a valid report.
    pub ok_exit_codes: &'a [i32],
}

impl<'a> Call<'a> {
    /// A call of `program` with `args`, expecting exit code 0, with no pinned checksum and nothing on stdin.
    #[must_use]
    pub fn new(tool: &'a str, program: &'a Path, args: &'a [String]) -> Call<'a> {
        Call {
            tool,
            program,
            args,
            stdin: None,
            expected_sha256: None,
            ok_exit_codes: &[0],
        }
    }
}

/// What a finished tool left.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// Everything on standard output (at most the cap).
    pub stdout: Vec<u8>,
    /// The start of standard error, as text.
    pub stderr: String,
    /// The exit code.
    pub exit_code: i32,
}

/// The limits and the environment of a tool call.
#[derive(Debug, Clone)]
pub struct Runner {
    /// How long a call may run before the tool is killed.
    pub timeout: Duration,
    /// The most bytes of standard output accepted; more kills the tool.
    pub max_output: usize,
    /// The `PATH` the tool sees (by default nvmod's own, which only names folders and is needed on systems such as NixOS
    /// where `/usr/bin` holds almost nothing). Nothing else of nvmod's environment is passed on.
    pub path: String,
    /// Further variables to pass, by name and value.
    pub env: Vec<(String, String)>,
}

impl Default for Runner {
    fn default() -> Runner {
        Runner {
            timeout: Duration::from_secs(300),
            max_output: 64 * 1024 * 1024,
            path: std::env::var("PATH").unwrap_or_default(),
            env: Vec::new(),
        }
    }
}

fn failed(call: &Call, reason: impl Into<String>) -> ToolError {
    ToolError::Failed {
        tool: call.tool.to_owned(),
        reason: reason.into(),
    }
}

/// Read `pipe` into memory, at most `cap` bytes, and send what was kept when it ends. With `over`, reading stops
/// and the flag is set as soon as more than `cap` bytes are offered; without it the rest is read and dropped, so
/// the tool never blocks on a full pipe.
fn drain(
    mut pipe: impl Read + Send + 'static,
    cap: usize,
    over: Option<Arc<AtomicBool>>,
) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut chunk = [0u8; 8192];
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let room = cap.saturating_sub(kept.len());
                    kept.extend_from_slice(&chunk[..n.min(room)]);
                    if n > room
                        && let Some(flag) = &over
                    {
                        flag.store(true, Ordering::SeqCst);
                        break;
                    }
                }
            }
        }
        let _ = tx.send(kept);
    });
    rx
}

/// Start the command. A program that was written an instant ago can still be "busy" on Linux while another thread's
/// fork holds the file open for writing; that clears in milliseconds, so it is tried again a few times.
fn spawn(command: &mut Command) -> std::io::Result<std::process::Child> {
    let mut tries = 0;
    loop {
        match command.spawn() {
            Err(e) if e.raw_os_error() == Some(26) && tries < 10 => {
                tries += 1;
                std::thread::sleep(Duration::from_millis(20));
            }
            other => return other,
        }
    }
}

impl Runner {
    /// Check the pinned checksum of the program, if any.
    fn verify(call: &Call) -> Result<(), ToolError> {
        let Some(expected) = call.expected_sha256 else {
            return Ok(());
        };
        let unverifiable = |reason: String| ToolError::Unverifiable {
            tool: call.tool.to_owned(),
            reason,
        };
        let bytes = std::fs::read(call.program)
            .map_err(|e| unverifiable(format!("cannot read {}: {e}", call.program.display())))?;
        let actual = sha256_hex(&bytes);
        if actual.eq_ignore_ascii_case(expected) {
            Ok(())
        } else {
            Err(unverifiable(format!(
                "{} has SHA-256 {actual}, the pinned one is {expected}; it was not run",
                call.program.display()
            )))
        }
    }

    /// Run `call` and return what the tool left.
    ///
    /// # Errors
    /// `Unverifiable` when the pinned checksum does not match (the tool is not started); `Failed` when the tool
    /// cannot be started, runs past the timeout or the output cap (it is killed), or exits with a code that is
    /// not in `ok_exit_codes`.
    pub fn run(&self, call: &Call) -> Result<Output, ToolError> {
        Self::verify(call)?;
        let mut command = Command::new(call.program);
        command
            .args(call.args)
            .env_clear()
            .env("PATH", &self.path)
            .env("LC_ALL", "C")
            .stdin(if call.stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if cfg!(windows)
            && let Some(root) = std::env::var_os("SystemRoot")
        {
            command.env("SystemRoot", root);
        }
        command.envs(self.env.iter().map(|(k, v)| (k, v)));
        let mut child = spawn(&mut command).map_err(|e| {
            failed(
                call,
                format!("cannot start {}: {e}", call.program.display()),
            )
        })?;
        if let (Some(bytes), Some(mut pipe)) = (call.stdin, child.stdin.take()) {
            let bytes = bytes.to_vec();
            std::thread::spawn(move || {
                let _ = pipe.write_all(&bytes);
            });
        }
        let over = Arc::new(AtomicBool::new(false));
        let (Some(stdout_pipe), Some(stderr_pipe)) = (child.stdout.take(), child.stderr.take())
        else {
            let _ = child.kill();
            return Err(failed(call, "its output pipes could not be opened"));
        };
        let out = drain(stdout_pipe, self.max_output, Some(Arc::clone(&over)));
        let err = drain(stderr_pipe, STDERR_KEEP, None);
        let deadline = Instant::now() + self.timeout;
        let mut stop = None;
        let status = loop {
            if over.load(Ordering::SeqCst) {
                stop = Some(format!(
                    "it wrote more than the cap of {} bytes",
                    self.max_output
                ));
            } else if nvmod_core::cancel::requested() {
                stop = Some(nvmod_core::cancel::INTERRUPTED.to_owned());
            } else if Instant::now() >= deadline {
                stop = Some(format!("it ran longer than {:?}", self.timeout));
            } else {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(e) => return Err(failed(call, format!("cannot wait for the tool: {e}"))),
                }
            }
            let _ = child.kill();
            break child
                .wait()
                .map_err(|e| failed(call, format!("cannot wait for the tool: {e}")))?;
        };
        let stdout = out.recv_timeout(PIPE_GRACE).unwrap_or_default();
        let stderr =
            String::from_utf8_lossy(&err.recv_timeout(PIPE_GRACE).unwrap_or_default()).into_owned();
        let first_line = stderr.lines().next().unwrap_or("").trim();
        if let Some(reason) = stop {
            return Err(failed(
                call,
                format!("{reason}; it was stopped. {first_line}"),
            ));
        }
        let code = status.code();
        match code {
            Some(c) if call.ok_exit_codes.contains(&c) => Ok(Output {
                stdout,
                stderr,
                exit_code: c,
            }),
            Some(c) => Err(failed(call, format!("exit code {c}. {first_line}"))),
            None => Err(failed(call, "it was ended by a signal".to_owned())),
        }
    }
}

/// Run `call`, parse its standard output as JSON and check it against `contract`. Returns the contract version the
/// document speaks and the document.
///
/// # Errors
/// Everything [`Runner::run`] reports, plus `Malformed` when the output is not JSON or does not match the schema
/// of its version (the report names the tool and the first error), and `Refused` when the tool speaks a newer
/// version than nvmod reads.
pub fn call_contract(
    runner: &Runner,
    call: &Call,
    contract: &Contract,
) -> Result<(u32, Value), ToolError> {
    let output = runner.run(call)?;
    let malformed = |reason: String| ToolError::Malformed {
        tool: call.tool.to_owned(),
        reason,
    };
    let document: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| malformed(format!("the output is not JSON: {e}")))?;
    match contract.check(&document) {
        Ok(version) => Ok((version, document)),
        Err(e @ (ContractError::Newer { .. } | ContractError::TooOld { .. })) => {
            Err(ToolError::Refused {
                tool: call.tool.to_owned(),
                reason: e.to_string(),
            })
        }
        Err(e) => Err(malformed(e.to_string())),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use nvmod_testkit::TempDir;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn script(dir: &TempDir, name: &str, body: &str) -> PathBuf {
        let path = dir.path().join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    fn args(words: &[&str]) -> Vec<String> {
        words.iter().map(|w| (*w).to_owned()).collect()
    }

    fn fast() -> Runner {
        Runner {
            timeout: Duration::from_secs(10),
            ..Runner::default()
        }
    }

    #[test]
    fn arguments_are_passed_as_words_with_no_shell() {
        let dir = TempDir::new("run-args");
        let p = script(&dir, "t", r#"printf '%s|' "$@""#);
        let a = args(&["a b", "c;touch pwned", "$HOME"]);
        let out = fast().run(&Call::new("t", &p, &a)).unwrap();
        assert_eq!(out.stdout, b"a b|c;touch pwned|$HOME|");
        assert!(!dir.path().join("pwned").exists());
    }

    #[test]
    fn the_environment_is_scrubbed_and_extras_are_passed() {
        let dir = TempDir::new("run-env");
        let p = script(&dir, "t", "env");
        // Set in the parent: must not reach the tool.
        let mut runner = fast();
        runner.env.push(("NVMOD_EXTRA".into(), "1".into()));
        let out = runner.run(&Call::new("t", &p, &[])).unwrap();
        let text = String::from_utf8(out.stdout).unwrap();
        let names: Vec<&str> = text.lines().filter_map(|l| l.split('=').next()).collect();
        assert!(
            names.contains(&"PATH") && names.contains(&"NVMOD_EXTRA"),
            "{text}"
        );
        assert!(
            !names.contains(&"HOME") && !names.contains(&"USER"),
            "{text}"
        );
        assert!(text.contains("LC_ALL=C"), "{text}");
    }

    #[test]
    fn standard_input_is_passed_and_closed() {
        let dir = TempDir::new("run-stdin");
        let p = script(&dir, "t", "cat");
        let mut call = Call::new("t", &p, &[]);
        call.stdin = Some(b"{\"request\":1}");
        assert_eq!(fast().run(&call).unwrap().stdout, b"{\"request\":1}");
    }

    #[test]
    fn a_tool_that_runs_too_long_is_killed_and_reported_as_failed() {
        let dir = TempDir::new("run-timeout");
        let p = script(&dir, "t", "exec sleep 30");
        let runner = Runner {
            timeout: Duration::from_millis(300),
            ..Runner::default()
        };
        let started = Instant::now();
        let why = runner.run(&Call::new("slow", &p, &[])).unwrap_err();
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(
            matches!(&why, ToolError::Failed { tool, reason } if tool == "slow" && reason.contains("longer")),
            "{why}"
        );
        assert_eq!(why.exit_code(), 4);
    }

    #[test]
    fn a_tool_that_writes_past_the_cap_is_killed() {
        let dir = TempDir::new("run-cap");
        let p = script(&dir, "t", "exec yes");
        let runner = Runner {
            max_output: 1000,
            ..fast()
        };
        let why = runner.run(&Call::new("loud", &p, &[])).unwrap_err();
        assert!(
            matches!(&why, ToolError::Failed { reason, .. } if reason.contains("cap of 1000")),
            "{why}"
        );
    }

    #[test]
    fn output_exactly_at_the_cap_is_accepted() {
        let dir = TempDir::new("run-cap-exact");
        let p = script(&dir, "t", "printf 12345");
        let runner = Runner {
            max_output: 5,
            ..fast()
        };
        assert_eq!(
            runner.run(&Call::new("t", &p, &[])).unwrap().stdout,
            b"12345"
        );
    }

    #[test]
    fn a_failing_exit_code_is_a_tool_failure_with_the_first_line_of_its_error() {
        let dir = TempDir::new("run-exit");
        let p = script(&dir, "t", "echo 'it broke' >&2; echo second >&2; exit 7");
        let why = fast().run(&Call::new("t", &p, &[])).unwrap_err();
        let text = why.to_string();
        assert!(
            text.contains("exit code 7") && text.contains("it broke") && !text.contains("second"),
            "{text}"
        );
        assert_eq!(why.exit_code(), 4);
        // An exit code the caller lists is an answer.
        let q = script(&dir, "u", "echo out; exit 1");
        let mut call = Call::new("u", &q, &[]);
        call.ok_exit_codes = &[0, 1];
        let out = fast().run(&call).unwrap();
        assert_eq!(
            (out.exit_code, out.stdout.as_slice()),
            (1, b"out\n".as_slice())
        );
    }

    #[test]
    fn a_program_that_cannot_start_is_a_tool_failure() {
        let dir = TempDir::new("run-missing");
        let why = fast()
            .run(&Call::new("t", &dir.path().join("missing"), &[]))
            .unwrap_err();
        assert!(matches!(why, ToolError::Failed { .. }), "{why}");
    }

    #[test]
    fn a_checksum_mismatch_exits_3_and_the_tool_is_not_started() {
        let dir = TempDir::new("run-sha");
        let marker = dir.path().join("ran");
        let p = script(&dir, "t", &format!("touch {}", marker.display()));
        let good = sha256_hex(&std::fs::read(&p).unwrap());
        let mut call = Call::new("t", &p, &[]);
        let wrong = "0".repeat(64);
        call.expected_sha256 = Some(&wrong);
        let why = fast().run(&call).unwrap_err();
        assert!(matches!(why, ToolError::Unverifiable { .. }), "{why}");
        assert_eq!(why.exit_code(), 3);
        assert!(!marker.exists());
        call.expected_sha256 = Some(&good);
        fast().run(&call).unwrap();
        assert!(marker.exists());
        // Changed after it was pinned: refused again on the next call.
        std::fs::write(&p, "#!/bin/sh\ntrue\n# changed\n").unwrap();
        assert_eq!(fast().run(&call).unwrap_err().exit_code(), 3);
        let upper = good.to_ascii_uppercase();
        call.expected_sha256 = Some(&upper);
        assert_eq!(fast().run(&call).unwrap_err().exit_code(), 3);
    }

    fn lodwright() -> Contract {
        crate::builtin("lodwright-plan").unwrap()
    }

    #[test]
    fn contract_calls_validate_the_output_and_name_the_tool_and_the_first_error() {
        let dir = TempDir::new("run-contract");
        let good = json!({"worlds": []}).to_string();
        let p = script(&dir, "good", &format!("echo '{good}'"));
        assert_eq!(
            call_contract(&fast(), &Call::new("lodwright-rs", &p, &[]), &lodwright())
                .unwrap()
                .0,
            1
        );

        let bad = json!({"worlds": [{"name": 3}]}).to_string();
        let p = script(&dir, "bad", &format!("echo '{bad}'"));
        let why =
            call_contract(&fast(), &Call::new("lodwright-rs", &p, &[]), &lodwright()).unwrap_err();
        assert!(
            matches!(&why, ToolError::Malformed { tool, reason } if tool == "lodwright-rs" && reason.contains("lodwright-plan")),
            "{why}"
        );
        assert_eq!(why.exit_code(), 4);

        let p = script(&dir, "text", "echo not json");
        let why =
            call_contract(&fast(), &Call::new("lodwright-rs", &p, &[]), &lodwright()).unwrap_err();
        assert!(matches!(why, ToolError::Malformed { .. }), "{why}");
    }

    #[test]
    fn a_newer_contract_version_is_refused_with_exit_2() {
        let dir = TempDir::new("run-newer");
        let p = script(&dir, "t", r#"echo '{"schema_version":2,"worlds":[]}'"#);
        let why =
            call_contract(&fast(), &Call::new("lodwright-rs", &p, &[]), &lodwright()).unwrap_err();
        assert!(
            matches!(&why, ToolError::Refused { reason, .. } if reason.contains("newer")),
            "{why}"
        );
        assert_eq!(why.exit_code(), 2);
    }
}
