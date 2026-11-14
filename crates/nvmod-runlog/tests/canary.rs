// SPDX-License-Identifier: GPL-3.0-only
//! ADR-0012's canary: a fake token and a path outside the home directory are sent through every logging path
//! (a message, a field, a `Secret`, a span field, a tool's standard error); neither may reach the file. One test,
//! because the subscriber is global to the process.

use std::path::Path;

use nvmod_runlog::{Config, Redactor, Secret, Session};
use nvmod_testkit::TempDir;
use tracing::level_filters::LevelFilter;

const TOKEN: &str = "ghp_CanaryToken0123456789abcdefXYZ";
const OUTSIDE: &str = "/mnt/elsewhere/SteamLibrary/canary-game";

#[test]
fn a_planted_token_and_an_outside_path_never_reach_the_run_log() {
    let state = TempDir::new("canary-state");
    let home = TempDir::new("canary-home");
    let instance = home.path().join("games/fnv");
    let redactor = Redactor::new()
        .with_folder(home.path(), "~")
        .with_folder(&instance, "$INSTANCE");
    let session = Session::start(Config {
        op: format!("install {TOKEN}"),
        version: "0.0.0".into(),
        state_dir: Some(state.path().to_owned()),
        redactor,
        stderr_level: LevelFilter::OFF,
        write_file: true,
    });
    let secret = Secret::new(TOKEN.to_owned());
    tracing::info!("plain message with {TOKEN} and {OUTSIDE}/x.esm");
    tracing::info!(token = TOKEN, path = OUTSIDE, "fields");
    tracing::info!(secret = ?secret, shown = %secret, "a secret value");
    tracing::info!(path = %instance.join("mods/a").display(), "inside the instance");
    tracing::info!(path = %home.path().join("cfg").display(), "inside the home");
    {
        let _span = tracing::info_span!(
            "tool",
            tool = "fake-tool",
            tool_version = "1.2",
            step = TOKEN
        )
        .entered();
        tracing::info!(
            stderr = format!("error: cannot open {OUTSIDE}\nAuthorization: Bearer {TOKEN}"),
            "tool standard error"
        );
    }
    let path = session.log_path().unwrap().to_owned();
    session.finish(3);

    let log = std::fs::read_to_string(&path).unwrap();
    assert!(!log.contains(TOKEN), "token reached the log:\n{log}");
    assert!(
        !log.contains("canary-game") && !log.contains("/mnt/elsewhere"),
        "outside path reached the log:\n{log}"
    );
    assert!(
        !log.contains(&home.path().display().to_string()),
        "home reached the log:\n{log}"
    );
    assert!(
        log.contains("$INSTANCE/mods/a") && log.contains("~/cfg"),
        "{log}"
    );

    let lines: Vec<serde_json::Value> = log
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
    assert_eq!(last["exit_code"], 3);
    assert_eq!(last["msg"], "command finished");
    let tool = lines
        .iter()
        .find(|l| {
            l["msg"]
                .as_str()
                .is_some_and(|m| m.contains("tool standard error"))
        })
        .unwrap();
    assert_eq!(tool["tool"], "fake-tool");
    assert_eq!(tool["run_id"], lines[0]["run_id"]);
    assert!(Path::new(&path).starts_with(state.path().join("logs")));
}
