// SPDX-License-Identifier: GPL-3.0-only
//! Rotation: the 21st run removes the oldest log.

use nvmod_runlog::{Config, Redactor, Session};
use nvmod_testkit::TempDir;
use nvmod_testkit::{covers, ids};
use tracing::level_filters::LevelFilter;

fn start(state: &TempDir) -> Session {
    Session::start(Config {
        op: "doctor".into(),
        version: "0.0.0".into(),
        state_dir: Some(state.path().to_owned()),
        redactor: Redactor::new(),
        stderr_level: LevelFilter::OFF,
        write_file: true,
    })
}

#[test]
fn the_twenty_first_run_removes_the_oldest() {
    covers(&[ids::api::cli::RUN_LOG_ROTATION]);
    let state = TempDir::new("rotation");
    let first = start(&state).log_path().unwrap().to_owned();
    assert!(first.exists());
    let mut paths = vec![first.clone()];
    for _ in 0..19 {
        paths.push(start(&state).log_path().unwrap().to_owned());
    }
    assert!(paths.iter().all(|p| p.exists()), "twenty runs are kept");
    let newest = start(&state).log_path().unwrap().to_owned();
    assert!(!first.exists(), "the 21st run removes the oldest");
    assert!(paths[1..].iter().all(|p| p.exists()) && newest.exists());
    assert_eq!(
        std::fs::read_dir(state.path().join("logs"))
            .unwrap()
            .count(),
        20
    );
}
