// SPDX-License-Identifier: GPL-3.0-only
//! The run log of a command (ADR-0012), on a scratch Instance made by `nvmod init`. No game data is used.

use nvmod_testkit::{BaseBuilder, TempDir, write_file};
use std::fs;
use std::path::Path;
use std::process::Command;

fn nvmod(args: &[&str], home: &Path, state: &Path) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_nvmod"))
        .args(args)
        .env("HOME", home)
        .env("NVMOD_STATE_DIR", state)
        .env_remove("NVMOD_LOG")
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn logs(state: &Path) -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = fs::read_dir(state.join("logs"))
        .map(|d| d.map(|e| e.unwrap().path()).collect())
        .unwrap_or_default();
    v.sort();
    v
}

#[test]
fn a_command_on_an_instance_leaves_a_redacted_log_and_keeps_its_output() {
    let home = TempDir::new("runlog-home");
    let state = TempDir::new("runlog-state");
    let base = home.path().join("steam/fnv");
    fs::create_dir_all(&base).unwrap();
    BaseBuilder::steam().write(&base);
    let instance = home.path().join("inst");
    let (code, _, err) = nvmod(
        &[
            "init",
            "--base",
            base.to_str().unwrap(),
            "--instance",
            instance.to_str().unwrap(),
        ],
        home.path(),
        state.path(),
    );
    assert_eq!(code, 0, "{err}");
    write_file(&instance, "profiles/Default/modlist.txt", b"+Alpha\r\n");
    assert!(
        err.is_empty(),
        "nothing new on standard error by default: {err}"
    );
    let (code, stdout, err) = nvmod(
        &[
            "mods",
            "list",
            "--instance",
            instance.to_str().unwrap(),
            "--json",
        ],
        home.path(),
        state.path(),
    );
    assert_eq!(code, 0, "{err}");
    assert!(
        stdout.starts_with('{') || stdout.starts_with('['),
        "{stdout}"
    );
    assert!(err.is_empty(), "{err}");

    let files = logs(state.path());
    assert_eq!(files.len(), 2);
    let text = fs::read_to_string(&files[1]).unwrap();
    assert!(
        !text.contains(&home.path().display().to_string()),
        "home leaked:\n{text}"
    );
    let lines: Vec<serde_json::Value> = text
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(lines[0]["kind"], "header");
    assert_eq!(lines[0]["log_schema"], 1);
    let last = lines.last().unwrap();
    for key in [
        "ts",
        "level",
        "run_id",
        "op",
        "step",
        "tool",
        "tool_version",
        "exit_code",
        "duration_ms",
        "msg",
    ] {
        assert!(last.get(key).is_some(), "{key} missing in {last}");
    }
    assert_eq!(
        (last["op"].as_str(), last["exit_code"].as_u64()),
        (Some("mods list"), Some(0))
    );
    assert_eq!(last["run_id"], lines[0]["run_id"]);
}

#[test]
fn log_level_is_taken_out_and_checked_and_help_writes_no_file() {
    let home = TempDir::new("runlog-level-home");
    let state = TempDir::new("runlog-level-state");
    let (code, _, err) = nvmod(
        &["catalog", "--log-level", "loud"],
        home.path(),
        state.path(),
    );
    assert_eq!(code, 5);
    assert!(err.contains("--log-level"), "{err}");
    let (code, out, _) = nvmod(&["--help"], home.path(), state.path());
    assert_eq!(code, 0);
    assert!(out.contains("run log"), "{out}");
    let (code, _, err) = nvmod(&["--version"], home.path(), state.path());
    assert_eq!((code, err.as_str()), (0, ""));
    assert_eq!(logs(state.path()).len(), 0);
    let (code, _, _) = nvmod(
        &["catalog", "--json", "--log-level", "debug"],
        home.path(),
        state.path(),
    );
    assert_eq!(code, 0);
    assert_eq!(logs(state.path()).len(), 1);
}
