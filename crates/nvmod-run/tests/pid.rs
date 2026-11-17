// SPDX-License-Identifier: GPL-3.0-only
//! `pid_alive` against real child processes: one that runs, and one that has ended and been reaped.
#![cfg(unix)]

use nvmod_run::pid_alive;
use std::process::Command;

#[test]
fn a_running_process_is_alive_and_a_reaped_one_is_gone() {
    let mut sleeping = Command::new("sleep").arg("30").spawn().unwrap();
    assert!(pid_alive(sleeping.id()));
    sleeping.kill().unwrap();
    sleeping.wait().unwrap();

    let mut done = Command::new("true").spawn().unwrap();
    let pid = done.id();
    done.wait().unwrap();
    assert!(!pid_alive(pid));
}
