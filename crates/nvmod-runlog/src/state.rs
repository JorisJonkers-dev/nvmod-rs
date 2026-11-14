// SPDX-License-Identifier: GPL-3.0-only
//! Where the run logs live, and how many are kept.

use std::fs;
use std::path::{Path, PathBuf};

/// The most run logs kept.
pub const KEEP_RUNS: usize = 20;
/// The most bytes of run logs kept.
pub const KEEP_BYTES: u64 = 50 * 1024 * 1024;

/// The state directory of nvmod: `NVMOD_STATE_DIR`; else `$XDG_STATE_HOME/nvmod`, or `~/.local/state/nvmod`;
/// on Windows `%LOCALAPPDATA%\nvmod`. The run logs are in its `logs` folder.
#[must_use]
pub fn state_dir() -> PathBuf {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if let Some(dir) = var("NVMOD_STATE_DIR") {
        return dir;
    }
    if cfg!(windows) {
        return var("LOCALAPPDATA")
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nvmod");
    }
    var("XDG_STATE_HOME")
        .or_else(|| var("HOME").map(|h| h.join(".local").join("state")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nvmod")
}

/// The folder of the run logs inside the state directory `state`.
#[must_use]
pub fn logs_dir(state: &Path) -> PathBuf {
    state.join("logs")
}

/// Delete the oldest `*.ndjson` files in `logs` until at most `keep_runs` files and `keep_bytes` bytes are left
/// (the newest file always stays). Run ids start with a UTC timestamp, so the name order is the age order.
/// Failures are ignored: a log that cannot be removed is not a reason to fail a command.
pub fn prune(logs: &Path, keep_runs: usize, keep_bytes: u64) {
    let Ok(entries) = fs::read_dir(logs) else {
        return;
    };
    let mut files: Vec<(String, u64)> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "ndjson"))
        .map(|e| {
            (
                e.file_name().to_string_lossy().into_owned(),
                e.metadata().map_or(0, |m| m.len()),
            )
        })
        .collect();
    // By run id (the name without its extension), so `x-pid` comes before its `x-pid-02`.
    files.sort_by(|a, b| {
        a.0.trim_end_matches(".ndjson")
            .cmp(b.0.trim_end_matches(".ndjson"))
    });
    let mut total: u64 = files.iter().map(|(_, n)| n).sum();
    let mut count = files.len();
    for (name, size) in &files {
        if count <= keep_runs.max(1) && total <= keep_bytes || count <= 1 {
            break;
        }
        if fs::remove_file(logs.join(name)).is_ok() {
            total -= size;
        }
        count -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::TempDir;

    #[test]
    fn the_oldest_runs_go_first() {
        let dir = TempDir::new("prune-count");
        for n in 0..5 {
            fs::write(dir.path().join(format!("2026010{n}T000000-1.ndjson")), "x").unwrap();
        }
        fs::write(dir.path().join("keep.txt"), "x").unwrap();
        prune(dir.path(), 3, u64::MAX);
        let mut left: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(
            left,
            [
                "20260102T000000-1.ndjson",
                "20260103T000000-1.ndjson",
                "20260104T000000-1.ndjson",
                "keep.txt"
            ]
        );
    }

    #[test]
    fn the_size_cap_removes_old_runs_but_never_the_newest() {
        let dir = TempDir::new("prune-bytes");
        for n in 0..3 {
            fs::write(
                dir.path().join(format!("2026010{n}T000000-1.ndjson")),
                "0123456789",
            )
            .unwrap();
        }
        prune(dir.path(), 20, 15);
        let left = fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(left, 1);
        assert!(dir.path().join("20260102T000000-1.ndjson").exists());
        prune(dir.path(), 20, 0);
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }
}
