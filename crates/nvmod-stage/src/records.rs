// SPDX-License-Identifier: GPL-3.0-only
//! The per-path record of the staging manifest: who provides a file of the tree and what the file looked like when
//! the tree was built, so that a later sweep can tell what a program did to it.

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::time::UNIX_EPOCH;

/// What one file of the Staging tree was when it was built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Record {
    /// The owner of the file: a Mod folder name, `Overwrite` or `the base copy`.
    pub provider: String,
    /// The path as the provider spells it (`/` separated, relative to the provider's folder).
    pub source: String,
    /// The file's identity (inode, or the file index on Windows); 0 when the system gives none.
    pub id: u64,
    /// Size in bytes.
    pub size: u64,
    /// Modification time in nanoseconds since the epoch.
    pub mtime_ns: u64,
    /// Names the file had (hard links); more than 1 means the provider's file and the tree's are one file.
    pub links: u64,
}

/// The identity and the number of names of the file at `path`.
#[cfg(unix)]
pub(crate) fn identity(_: &Path, meta: &fs::Metadata) -> (u64, u64) {
    use std::os::unix::fs::MetadataExt;
    (meta.ino(), meta.nlink())
}

/// The identity and the number of names of the file at `path`, from its handle.
#[cfg(windows)]
pub(crate) fn identity(path: &Path, _: &fs::Metadata) -> (u64, u64) {
    fs::File::open(path)
        .and_then(|file| winapi_util::file::information(&file))
        .map_or((0, 2), |info| (info.file_index(), info.number_of_links()))
}

#[cfg(not(any(unix, windows)))]
pub(crate) fn identity(_: &Path, _: &fs::Metadata) -> (u64, u64) {
    (0, 1)
}

/// The modification time of `meta` in nanoseconds since the epoch (0 when it cannot be read).
pub(crate) fn mtime_ns(meta: &fs::Metadata) -> u64 {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .and_then(|d| u64::try_from(d.as_nanos()).ok())
        .unwrap_or(0)
}

impl Record {
    /// The record of the file at `path` that `provider` provides as `source`.
    pub(crate) fn of(path: &Path, provider: &str, source: &str) -> Option<Record> {
        let meta = fs::metadata(path).ok()?;
        let (id, links) = identity(path, &meta);
        Some(Record {
            provider: provider.to_owned(),
            source: source.to_owned(),
            id,
            size: meta.len(),
            mtime_ns: mtime_ns(&meta),
            links,
        })
    }
}
