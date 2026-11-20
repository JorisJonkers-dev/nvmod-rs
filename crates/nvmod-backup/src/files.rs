// SPDX-License-Identifier: GPL-3.0-only
//! Which files a snapshot holds, copying with a hash, and finding the game process.

use crate::{Snapshot, clock};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// The text lists MO2 keeps per profile and snapshots cover.
const LISTS: [&str; 5] = [
    "modlist.txt",
    "plugins.txt",
    "loadorder.txt",
    "lockedorder.txt",
    "archives.txt",
];

/// Largest single list or INI a snapshot takes, so a snapshot stays small.
const MAX_FILE: u64 = 16 * 1024 * 1024;

/// Whether a top-level file of a profile folder belongs in a snapshot: the text lists, `profile.json`, and every
/// `.ini` (the game INIs and `settings.ini`).
#[must_use]
pub fn tracked(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    LISTS.contains(&lower.as_str())
        || lower == "profile.json"
        || Path::new(&lower).extension().is_some_and(|e| e == "ini")
}

/// A path in a manifest that is absolute or leaves the snapshot folder.
pub(crate) fn unsafe_path(path: &str) -> bool {
    path.is_empty()
        || path.starts_with(['/', '\\'])
        || path.contains(':')
        || path.contains('\\')
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
}

/// The files to copy, `(relative path with /, source)`, sorted. Symbolic links and other non-files are never
/// followed: a tracked name that is not a regular file is refused.
pub(crate) fn collect(
    profile_dir: &Path,
    with_saves: bool,
) -> Result<Vec<(String, std::path::PathBuf)>, String> {
    let mut out = Vec::new();
    for entry in fs::read_dir(profile_dir)
        .map_err(|e| format!("{}: {e}", profile_dir.display()))?
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !tracked(&name) {
            continue;
        }
        let meta = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
        if !meta.is_file() {
            return Err(format!(
                "`{name}` is not a regular file; a snapshot never follows links"
            ));
        }
        if meta.len() > MAX_FILE {
            return Err(format!(
                "`{name}` is larger than {MAX_FILE} bytes; a snapshot holds text and INIs only"
            ));
        }
        out.push((name, entry.path()));
    }
    if with_saves {
        walk_saves(&profile_dir.join("saves"), "saves", &mut out);
    }
    out.sort();
    Ok(out)
}

fn walk_saves(dir: &Path, prefix: &str, out: &mut Vec<(String, std::path::PathBuf)>) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = format!("{prefix}/{}", entry.file_name().to_string_lossy());
        let Ok(meta) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if meta.is_dir() {
            walk_saves(&entry.path(), &name, out);
        } else if meta.is_file() {
            out.push((name, entry.path()));
        }
    }
}

/// Copy `from` to `to` (a real copy) and return its size and SHA-256.
pub(crate) fn copy_hashed(from: &Path, to: &Path) -> Result<(u64, String), String> {
    let mut source = fs::File::open(from).map_err(|e| format!("{}: {e}", from.display()))?;
    let mut target = fs::File::create(to).map_err(|e| format!("{}: {e}", to.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let n = source
            .read(&mut buffer)
            .map_err(|e| format!("{}: {e}", from.display()))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        target
            .write_all(&buffer[..n])
            .map_err(|e| format!("{}: {e}", to.display()))?;
        size += n as u64;
    }
    target
        .sync_all()
        .map_err(|e| format!("{}: {e}", to.display()))?;
    Ok((size, hex(&hasher.finalize())))
}

/// The size and SHA-256 of a file.
pub(crate) fn hash(path: &Path) -> Result<(u64, String), String> {
    let mut file = fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        size += n as u64;
    }
    Ok((size, hex(&hasher.finalize())))
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// A list file with an MO2 stamp: `(list name, stamp)` for `plugins.txt.2026_11_14_15_30_12`.
pub(crate) fn mo2_stamped(name: &str) -> Option<(&str, &str)> {
    let (base, stamp) = name.rsplit_once('.')?;
    let ok = LISTS[..4].contains(&base)
        && stamp.len() == 19
        && stamp.bytes().enumerate().all(|(i, b)| {
            if [4, 7, 10, 13, 16].contains(&i) {
                b == b'_'
            } else {
                b.is_ascii_digit()
            }
        });
    ok.then_some((base, stamp))
}

/// The Unix time of an MO2 stamp `yyyy_MM_dd_hh_mm_ss`, read as UTC.
pub(crate) fn mo2_time(stamp: &str) -> Option<i64> {
    let s: String = stamp.chars().filter(|c| *c != '_').collect();
    clock::parse_utc(&format!("{}T{}Z", &s[..8], &s[8..]))
}

/// The backups MO2 made in a profile folder, grouped by stamp.
pub(crate) fn mo2_backups(profile_dir: &Path) -> Vec<Snapshot> {
    let mut groups: std::collections::BTreeMap<String, (usize, u64)> =
        std::collections::BTreeMap::new();
    for entry in fs::read_dir(profile_dir).into_iter().flatten().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if let Some((_, stamp)) = mo2_stamped(&name)
            && let Ok(meta) = fs::symlink_metadata(entry.path())
            && meta.is_file()
        {
            let group = groups.entry(stamp.to_owned()).or_default();
            group.0 += 1;
            group.1 += meta.len();
        }
    }
    groups
        .into_iter()
        .filter_map(|(stamp, (files, bytes))| {
            let time = mo2_time(&stamp)?;
            Some(Snapshot {
                id: format!("mo2-{stamp}"),
                source: "mo2",
                created_unix: time,
                created: clock::format_utc(time),
                files,
                bytes,
                with_saves: false,
                problem: None,
            })
        })
        .collect()
}

/// The name of a running game process, when one is found. Linux only (it reads `/proc`); elsewhere `None`.
/// The names are `FalloutNV.exe`, `nvse_loader.exe` and `nv-viewer`, plus those in `NVMOD_GAME_PROCESSES`
/// (comma separated), matched against the process name without regard to case. A zombie (a process that
/// has exited and is waiting to be reaped) does not count. `NVMOD_PROC_DIR` replaces `/proc` as the process
/// source, so a test can name processes without starting any (and without seeing the host's).
#[must_use]
pub fn game_running() -> Option<String> {
    process_running(&["FalloutNV.exe", "nvse_loader.exe", "nv-viewer"])
}

/// The name of a running process among `wanted` (and those in `NVMOD_GAME_PROCESSES`), matched without regard to
/// case; see [`game_running`]. Linux only; elsewhere `None`.
#[must_use]
pub fn process_running(wanted: &[&str]) -> Option<String> {
    let mut names: Vec<String> = wanted.iter().map(|n| n.to_ascii_lowercase()).collect();
    if let Ok(extra) = std::env::var("NVMOD_GAME_PROCESSES") {
        names.extend(
            extra
                .split(',')
                .map(|n| n.trim().to_ascii_lowercase())
                .filter(|n| !n.is_empty()),
        );
    }
    let root = std::env::var_os("NVMOD_PROC_DIR").map_or_else(|| "/proc".into(), PathBuf::from);
    find_in(&root, &names)
}

/// The first process directory under `root` whose `comm` is one of `names` (lower case) and not a zombie.
fn find_in(root: &Path, names: &[String]) -> Option<String> {
    for entry in fs::read_dir(root).into_iter().flatten().flatten() {
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
        if is_zombie(&entry.path()) {
            continue;
        }
        // `comm` holds at most 15 characters, so a longer name matches on its prefix.
        if names
            .iter()
            .any(|n| *n == comm || (comm.len() == 15 && n.starts_with(&comm)))
        {
            return Some(comm);
        }
    }
    None
}

/// Whether the process directory `dir` holds a zombie: `stat` reads `pid (comm) S ...`, and `S` is `Z`.
/// Unreadable or odd content is not a zombie.
fn is_zombie(dir: &Path) -> bool {
    fs::read_to_string(dir.join("stat")).is_ok_and(|stat| {
        stat.rsplit_once(')')
            .is_some_and(|(_, rest)| rest.trim_start().starts_with('Z'))
    })
}

#[cfg(test)]
mod proc_tests {
    use super::*;

    fn proc_entry(root: &Path, pid: &str, comm: &str, state: &str) {
        let dir = root.join(pid);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("comm"), format!("{comm}\n")).unwrap();
        fs::write(dir.join("stat"), format!("{pid} ({comm}) {state} 1 1 1")).unwrap();
    }

    fn scratch(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nvmod-proc-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn names() -> Vec<String> {
        vec!["falloutnv.exe".to_owned()]
    }

    #[test]
    fn a_listed_process_is_found_case_insensitively() {
        let root = scratch("found");
        proc_entry(&root, "10", "bash", "S");
        proc_entry(&root, "11", "FalloutNV.exe", "S");
        assert_eq!(find_in(&root, &names()).as_deref(), Some("falloutnv.exe"));
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_zombie_and_a_non_process_entry_are_not_a_running_game() {
        let root = scratch("zombie");
        proc_entry(&root, "12", "FalloutNV.exe", "Z");
        proc_entry(&root, "self", "FalloutNV.exe", "S");
        assert_eq!(find_in(&root, &names()), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn a_missing_source_finds_nothing() {
        assert_eq!(
            find_in(Path::new("/nonexistent-nvmod-proc"), &names()),
            None
        );
    }
}
