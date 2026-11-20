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

/// Whether the process `pid` is running. On Linux this reads `/proc`, and a zombie counts as gone. On macOS and
/// the other Unix systems it asks the kernel with signal 0 (`kill(pid, 0)`): a process that does not exist is gone,
/// one that exists is running, even when it belongs to another user or is a zombie that its parent has not reaped
/// yet. Elsewhere (Windows) it answers `true`, so a lock is never taken over by guesswork (delete `.nvmod/lock` by
/// hand there). On Unix, pid 0 and pids above `i32::MAX` are never a running process.
#[must_use]
pub fn pid_alive(pid: u32) -> bool {
    if cfg!(unix) && (pid == 0 || i32::try_from(pid).is_err()) {
        return false;
    }
    if cfg!(target_os = "linux") {
        match fs::read_to_string(format!("/proc/{pid}/stat")) {
            // The state letter follows the parenthesised name, which may itself contain parentheses.
            Ok(stat) => stat
                .rsplit_once(')')
                .and_then(|(_, rest)| rest.trim_start().chars().next())
                .is_none_or(|state| state != 'Z' && state != 'X'),
            // No readable entry: gone, or hidden because `/proc` is mounted with `hidepid` (which also reads as
            // not found). The kernel tells the two apart.
            Err(_) => signal_alive(pid),
        }
    } else {
        signal_alive(pid)
    }
}

/// `kill(pid, 0)`: no such process is gone; success, or a process nvmod may not signal, is running.
#[cfg(unix)]
fn signal_alive(pid: u32) -> bool {
    use nix::errno::Errno;
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    let Ok(raw) = i32::try_from(pid) else {
        return false;
    };
    !matches!(kill(Pid::from_raw(raw), None), Err(Errno::ESRCH))
}

#[cfg(not(unix))]
fn signal_alive(_: u32) -> bool {
    true
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
/// elsewhere `None`. `comm` holds 15 characters at most, so a longer name matches on its prefix. A zombie does not
/// count. `NVMOD_PROC_DIR` replaces `/proc` as the process source, so a test can name processes without starting
/// any and without seeing the host's; the entries of such a directory are taken as running unless their `stat` says
/// zombie.
#[must_use]
pub fn find_process(names: &[String]) -> Option<String> {
    match std::env::var_os("NVMOD_PROC_DIR") {
        Some(root) => find_in(Path::new(&root), names, false),
        None => find_in(Path::new("/proc"), names, true),
    }
}

/// The first process directory under `root` whose `comm` is one of `names` and that is alive: by the kernel when
/// `check_pid`, by its `stat` state otherwise.
fn find_in(root: &Path, names: &[String], check_pid: bool) -> Option<String> {
    let names: Vec<String> = names.iter().map(|n| n.to_ascii_lowercase()).collect();
    for entry in fs::read_dir(root).into_iter().flatten().flatten() {
        let file_name = entry.file_name().to_string_lossy().into_owned();
        if !file_name.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        let Ok(comm) = fs::read_to_string(entry.path().join("comm")) else {
            continue;
        };
        let comm = comm.trim().to_ascii_lowercase();
        let wanted = names
            .iter()
            .any(|n| *n == comm || (comm.len() == 15 && n.starts_with(&comm)));
        if !wanted {
            continue;
        }
        let alive = if check_pid {
            pid_alive(file_name.parse().unwrap_or(0))
        } else {
            !is_zombie(&entry.path())
        };
        if alive {
            return Some(comm);
        }
    }
    None
}

/// Whether the process directory `dir` holds a zombie: `stat` reads `pid (comm) S ...` and `S` is `Z`.
fn is_zombie(dir: &Path) -> bool {
    fs::read_to_string(dir.join("stat")).is_ok_and(|stat| {
        stat.rsplit_once(')')
            .is_some_and(|(_, rest)| rest.trim_start().starts_with('Z'))
    })
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

    fn proc_entry(root: &Path, pid: &str, comm: &str, state: &str) {
        let dir = root.join(pid);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("comm"), format!("{comm}\n")).unwrap();
        fs::write(dir.join("stat"), format!("{pid} ({comm}) {state} 1 1 1")).unwrap();
    }

    #[test]
    fn a_process_directory_names_the_running_programs_and_skips_zombies() {
        let dir = TempDir::new("proc-fake");
        proc_entry(dir.path(), "10", "bash", "S");
        proc_entry(dir.path(), "11", "ModOrganizer.exe", "S");
        proc_entry(dir.path(), "12", "other.exe", "Z");
        let names = vec!["modorganizer.exe".to_owned(), "other.exe".to_owned()];
        assert_eq!(
            find_in(dir.path(), &names, false).as_deref(),
            Some("modorganizer.exe")
        );
        // Only a zombie of the wanted name: nothing runs.
        let zombies = TempDir::new("proc-zombie");
        proc_entry(zombies.path(), "12", "other.exe", "Z");
        assert_eq!(find_in(zombies.path(), &names, false), None);
        // A missing source is nothing running, not a panic.
        assert_eq!(find_in(&dir.path().join("none"), &names, false), None);
        // A name longer than 15 characters matches on its prefix.
        let long = TempDir::new("proc-long");
        proc_entry(long.path(), "13", "modorganizer.ex", "S");
        assert_eq!(
            find_in(long.path(), &names, false).as_deref(),
            Some("modorganizer.ex")
        );
    }

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

    // Linux and the other Unix systems can tell that a pid is gone (pid_alive); Windows cannot.
    #[test]
    #[cfg(unix)]
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

    // On Windows a pid cannot be told dead, so the lock is kept rather than taken over by guesswork.
    #[test]
    #[cfg(not(unix))]
    fn elsewhere_a_lock_is_never_taken_over() {
        let dir = TempDir::new("lock-kept");
        fs::create_dir_all(dir.path().join(".nvmod")).unwrap();
        fs::write(
            dir.path().join(LOCK_FILE),
            r#"{"pid":4294967290,"started":1,"command":"run gone"}"#,
        )
        .unwrap();
        assert!(matches!(
            InstanceLock::acquire(dir.path(), "stage"),
            Err(LockError::Held(h)) if h.command == "run gone"
        ));
    }

    #[test]
    #[cfg(unix)]
    fn this_process_is_alive_and_impossible_pids_are_not() {
        assert!(pid_alive(std::process::id()));
        assert!(!pid_alive(0));
        // As an i32 this would be -6, which kill(2) reads as a process group.
        assert!(!pid_alive(4_294_967_290));
        assert!(!pid_alive(u32::MAX));
    }

    #[test]
    #[cfg(unix)]
    fn the_kill_check_finds_this_process_and_not_a_pid_beyond_any_limit() {
        // The check macOS uses, run here too. No system hands out a pid this high (Linux stops at 2^22).
        assert!(signal_alive(std::process::id()));
        assert!(!signal_alive(2_147_483_000));
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
