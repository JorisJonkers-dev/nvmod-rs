// SPDX-License-Identifier: GPL-3.0-only
//! The Instance lock: `.nvmod/lock` holds the pid, start time and command of whoever may change the Instance,
//! so the Staging tree cannot be rebuilt while a program runs through it. A lock whose pid is gone is taken over.
//!
//! `nvmod run` hands the lock to the program it started: the file then names the program's pid and stays until
//! that process ends, which is how a detached run keeps the tree safe after `nvmod` itself has exited.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// The lock file, relative to the Instance folder.
pub const LOCK_FILE: &str = ".nvmod/lock";

/// Who holds the lock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Holder {
    /// The process id.
    pub pid: u32,
    /// When the holder started, in seconds since the Unix epoch.
    pub started: u64,
    /// What it is doing, such as `run Fallout Launcher` or `stage`.
    pub command: String,
}

impl fmt::Display for Holder {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "pid {} (`{}`, started at Unix time {})",
            self.pid, self.command, self.started
        )
    }
}

/// Why the lock was not taken.
#[derive(Debug, PartialEq, Eq)]
pub enum LockError {
    /// A live process holds it (exit code 2).
    Held(Holder),
    /// The lock file could not be handled.
    Io(String),
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LockError::Held(h) => write!(f, "the Instance is in use by {h}"),
            LockError::Io(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for LockError {}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Whether the process `pid` is running. On Linux this reads `/proc` and a zombie counts as gone; elsewhere it
/// answers `true`, so a lock is never taken over by guesswork (delete `.nvmod/lock` by hand there).
#[must_use]
pub fn pid_alive(pid: u32) -> bool {
    if cfg!(target_os = "linux") {
        match fs::read_to_string(format!("/proc/{pid}/stat")) {
            // The state letter follows the parenthesised name, which may itself contain parentheses.
            Ok(stat) => stat
                .rsplit_once(')')
                .and_then(|(_, rest)| rest.trim_start().chars().next())
                .is_none_or(|state| state != 'Z' && state != 'X'),
            Err(_) => false,
        }
    } else {
        true
    }
}

/// A held lock. Dropping it removes the file unless the lock was handed to another process.
#[derive(Debug)]
pub struct InstanceLock {
    path: PathBuf,
    owned: bool,
    made_dir: bool,
    /// The dead holder this lock replaced, for an info line.
    pub took_over: Option<Holder>,
}

fn write_new(path: &Path, holder: &Holder) -> std::io::Result<()> {
    let text = serde_json::to_string(holder).unwrap_or_default();
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    file.write_all(format!("{text}\n").as_bytes())
}

impl InstanceLock {
    /// The holder named in the lock file, when there is a readable one (alive or not).
    #[must_use]
    pub fn read(root: &Path) -> Option<Holder> {
        serde_json::from_str(&fs::read_to_string(root.join(LOCK_FILE)).ok()?).ok()
    }

    /// Take the lock of the Instance at `root` for `command`.
    ///
    /// # Errors
    /// `Held` when a live process holds it; `Io` when the file cannot be written or an unreadable lock file
    /// stands in the way.
    pub fn acquire(root: &Path, command: &str) -> Result<InstanceLock, LockError> {
        let path = root.join(LOCK_FILE);
        let io = |e: std::io::Error| LockError::Io(format!("{}: {e}", path.display()));
        let dir = path.parent().unwrap_or(root);
        let made_dir = !dir.exists();
        fs::create_dir_all(dir).map_err(io)?;
        let me = Holder {
            pid: std::process::id(),
            started: now(),
            command: command.to_owned(),
        };
        let mut took_over = None;
        for _ in 0..3 {
            match write_new(&path, &me) {
                Ok(()) => {
                    return Ok(InstanceLock {
                        path,
                        owned: true,
                        made_dir,
                        took_over,
                    });
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                    let text = fs::read_to_string(&path).map_err(io)?;
                    let Ok(holder) = serde_json::from_str::<Holder>(&text) else {
                        return Err(LockError::Io(format!(
                            "{} is not a lock nvmod wrote; remove it if no program is running",
                            path.display()
                        )));
                    };
                    if pid_alive(holder.pid) {
                        return Err(LockError::Held(holder));
                    }
                    fs::remove_file(&path).map_err(io)?;
                    took_over = Some(holder);
                }
                Err(e) => return Err(io(e)),
            }
        }
        Err(LockError::Io(format!(
            "{} keeps changing; try again",
            path.display()
        )))
    }

    /// Name `pid` (the program that was started) as the holder and stop owning the file: it stays until that
    /// process ends, then counts as stale.
    ///
    /// # Errors
    /// `Io` when the file cannot be rewritten; the lock is then still ours.
    pub fn hand_over(&mut self, pid: u32, command: &str) -> Result<(), LockError> {
        let io = |e: std::io::Error| LockError::Io(format!("{}: {e}", self.path.display()));
        let holder = Holder {
            pid,
            started: now(),
            command: command.to_owned(),
        };
        let temp = self.path.with_extension("tmp");
        fs::write(
            &temp,
            format!("{}\n", serde_json::to_string(&holder).unwrap_or_default()),
        )
        .map_err(io)?;
        fs::rename(&temp, &self.path).map_err(|e| {
            let _ = fs::remove_file(&temp);
            io(e)
        })?;
        self.owned = false;
        self.made_dir = false;
        Ok(())
    }
}

impl Drop for InstanceLock {
    fn drop(&mut self) {
        if self.owned {
            let _ = fs::remove_file(&self.path);
            if self.made_dir {
                // Only when empty: a command that wrote more into `.nvmod` keeps it.
                let _ = self.path.parent().map(fs::remove_dir);
            }
        }
    }
}

/// The name of a running program from `names` (compared without regard to case), found in `/proc` on Linux;
/// elsewhere `None`. `comm` holds 15 characters at most, so a longer name matches on its prefix.
#[must_use]
pub fn find_process(names: &[String]) -> Option<String> {
    let names: Vec<String> = names.iter().map(|n| n.to_ascii_lowercase()).collect();
    for entry in fs::read_dir("/proc").into_iter().flatten().flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .bytes()
            .all(|b| b.is_ascii_digit())
        {
            continue;
        }
        let Ok(comm) = fs::read_to_string(entry.path().join("comm")) else {
            continue;
        };
        let comm = comm.trim().to_ascii_lowercase();
        if names
            .iter()
            .any(|n| *n == comm || (comm.len() == 15 && n.starts_with(&comm)))
            && pid_alive(entry.file_name().to_string_lossy().parse().unwrap_or(0))
        {
            return Some(comm);
        }
    }
    None
}

/// The Mod Organizer 2 process name when one runs: `ModOrganizer.exe` plus the comma separated names in
/// `NVMOD_MO2_PROCESSES`.
#[must_use]
pub fn mo2_running() -> Option<String> {
    let mut names = vec!["ModOrganizer.exe".to_owned()];
    if let Ok(extra) = std::env::var("NVMOD_MO2_PROCESSES") {
        names.extend(
            extra
                .split(',')
                .map(|n| n.trim().to_owned())
                .filter(|n| !n.is_empty()),
        );
    }
    find_process(&names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::TempDir;

    #[test]
    fn a_second_acquire_names_the_holder_and_release_frees_it() {
        let dir = TempDir::new("lock");
        let first = InstanceLock::acquire(dir.path(), "stage").unwrap();
        match InstanceLock::acquire(dir.path(), "run x") {
            Err(LockError::Held(h)) => {
                assert_eq!((h.pid, h.command.as_str()), (std::process::id(), "stage"));
            }
            other => panic!("{other:?}"),
        }
        drop(first);
        assert!(!dir.path().join(".nvmod").exists());
        assert!(InstanceLock::acquire(dir.path(), "run x").is_ok());
    }

    // Only Linux can tell that a pid is gone (pid_alive); elsewhere a lock is never taken over.
    #[test]
    #[cfg(target_os = "linux")]
    fn a_dead_pid_is_taken_over() {
        let dir = TempDir::new("lock-dead");
        fs::create_dir_all(dir.path().join(".nvmod")).unwrap();
        // A pid above any pid_max has never been alive.
        fs::write(
            dir.path().join(LOCK_FILE),
            r#"{"pid":4294967290,"started":1,"command":"run gone"}"#,
        )
        .unwrap();
        let lock = InstanceLock::acquire(dir.path(), "stage").unwrap();
        assert_eq!(lock.took_over.as_ref().unwrap().command, "run gone");
        assert_eq!(InstanceLock::read(dir.path()).unwrap().command, "stage");
    }

    #[test]
    fn a_hand_over_keeps_the_file_and_names_the_new_pid() {
        let dir = TempDir::new("lock-hand");
        let mut lock = InstanceLock::acquire(dir.path(), "run x").unwrap();
        lock.hand_over(1, "run x (program)").unwrap();
        drop(lock);
        assert_eq!(InstanceLock::read(dir.path()).unwrap().pid, 1);
    }

    #[test]
    fn a_file_that_is_not_a_lock_is_not_overwritten() {
        let dir = TempDir::new("lock-junk");
        fs::create_dir_all(dir.path().join(".nvmod")).unwrap();
        fs::write(dir.path().join(LOCK_FILE), "junk").unwrap();
        assert!(matches!(
            InstanceLock::acquire(dir.path(), "stage"),
            Err(LockError::Io(_))
        ));
        assert_eq!(
            fs::read_to_string(dir.path().join(LOCK_FILE)).unwrap(),
            "junk"
        );
    }
}
