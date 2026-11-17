// SPDX-License-Identifier: GPL-3.0-only
//! Plugin file modification times: plugin number `n` of the Load order gets `BASE_TIME_SECS + n * STEP_SECS`, so the
//! times are strictly increasing in Load order and the same for the same order (running twice changes nothing).

use crate::{Slot, staged_file};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Time of the first plugin: 2001-09-09 01:46:40 UTC, long before any file of a real install.
pub const BASE_TIME_SECS: u64 = 1_000_000_000;
/// Seconds between two plugins; more than the 2-second resolution of some file systems.
pub const STEP_SECS: u64 = 60;

/// The modification time plugin number `index` of the Load order gets.
#[must_use]
pub fn target_time(index: usize) -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(BASE_TIME_SECS + STEP_SECS * index as u64)
}

/// A plugin file that needs a new time.
#[derive(Debug)]
pub(crate) struct Stamp {
    path: PathBuf,
    time: SystemTime,
    linked: bool,
}

/// Whether the file has more than one name (a hard link), so that a new time would show in the Mod folder too.
#[cfg(unix)]
fn shares_inode(_: &Path, meta: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    meta.nlink() > 1
}

/// Whether the file has more than one name (a hard link). NTFS keeps the times with the file, not the name, just
/// like an inode does, so a hard-linked plugin needs a copy of its own there too. The link count comes from the
/// file's handle; the standard library does not give it on stable Rust. When it cannot be read the file counts as
/// linked: a copy too many costs a little time, a stamp through a link changes the Mod folder.
#[cfg(windows)]
fn shares_inode(path: &Path, _: &fs::Metadata) -> bool {
    fs::File::open(path)
        .and_then(|file| winapi_util::file::information(&file))
        .map_or(true, |info| info.number_of_links() > 1)
}

#[cfg(not(any(unix, windows)))]
fn shares_inode(_: &Path, _: &fs::Metadata) -> bool {
    false
}

/// The plugin files of `order` that are in the Staging tree and whose time is not right yet, or that still share
/// their inode with a Mod folder's file.
pub(crate) fn plan(data: &Path, order: &[Slot]) -> Vec<Stamp> {
    let mut stamps = Vec::new();
    for (index, slot) in order.iter().enumerate() {
        let Some(path) = staged_file(data, &slot.name) else {
            continue;
        };
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        let time = target_time(index);
        let linked = shares_inode(&path, &meta);
        if linked || meta.modified().ok() != Some(time) {
            stamps.push(Stamp { path, time, linked });
        }
    }
    stamps
}

/// Replace the link of a plugin file by a copy: a new inode with the same content under the same name.
fn detach(path: &Path) -> Result<(), String> {
    let io = |e: std::io::Error| format!("{}: {e}", path.display());
    let tmp = path.with_extension("nvmod-copy");
    fs::copy(path, &tmp).map_err(io)?;
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io(e)
    })
}

/// Detach and stamp every file. Returns how many were detached.
pub(crate) fn apply(stamps: &[Stamp]) -> Result<usize, String> {
    let mut detached = 0;
    for stamp in stamps {
        let io = |e: std::io::Error| format!("{}: {e}", stamp.path.display());
        if stamp.linked {
            detach(&stamp.path)?;
            detached += 1;
        }
        fs::OpenOptions::new()
            .write(true)
            .open(&stamp.path)
            .and_then(|file| file.set_modified(stamp.time))
            .map_err(io)?;
    }
    Ok(detached)
}
