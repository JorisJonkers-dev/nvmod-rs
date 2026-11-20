// SPDX-License-Identifier: GPL-3.0-only
//! The running-game guard names the run that holds the Instance lock, whatever program the run started.

/// The long-running stand-in the test below starts as a separate process: this test binary itself.
#[test]
#[ignore = "only started by another test, as a process that stays alive"]
fn sleeper() {
    std::thread::sleep(std::time::Duration::from_secs(120));
}

#[test]
fn a_lock_of_a_live_process_names_the_run_whatever_the_program_is() {
    let dir = nvmod_testkit::TempDir::new("gr-live");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "sleeper", "--ignored", "--nocapture"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();
    std::fs::create_dir_all(dir.path().join(".nvmod")).unwrap();
    std::fs::write(
        dir.path().join(".nvmod/lock"),
        format!(
            r#"{{"pid":{},"started":1,"command":"run raul --play-mode"}}"#,
            child.id()
        ),
    )
    .unwrap();
    let found = nvmod_backup::game_running(dir.path());
    child.kill().unwrap();
    child.wait().unwrap();
    assert_eq!(
        found,
        Some(format!("run raul --play-mode, pid {}", child.id()))
    );
}
