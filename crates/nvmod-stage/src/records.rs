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
    /// Built as a hard link (a plugin detached for its file time still counts: it was linked first).
    #[serde(default)]
    pub linked: bool,
    /// The provider's file when the tree was built: identity, size and time, to tell a changed Mod file later.
    #[serde(default)]
    pub from_id: u64,
    /// Size of the provider's file.
    #[serde(default)]
    pub from_size: u64,
    /// Modification time of the provider's file in nanoseconds since the epoch.
    #[serde(default)]
    pub from_mtime_ns: u64,
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
    /// The record of the tree file at `path`, made from `from` (the provider's file) as `source` of `provider`.
    pub(crate) fn of(
        path: &Path,
        from: &Path,
        provider: &str,
        source: &str,
        linked: bool,
    ) -> Option<Record> {
        let meta = fs::metadata(path).ok()?;
        let (id, links) = identity(path, &meta);
        let origin = fs::metadata(from).ok()?;
        let (from_id, _) = identity(from, &origin);
        Some(Record {
            provider: provider.to_owned(),
            source: source.to_owned(),
            id,
            size: meta.len(),
            mtime_ns: mtime_ns(&meta),
            links,
            linked,
            from_id,
            from_size: origin.len(),
            from_mtime_ns: mtime_ns(&origin),
        })
    }

    /// Whether the provider's file at `from` is still the file the tree was built from.
    pub(crate) fn source_unchanged(&self, from: &Path) -> bool {
        fs::metadata(from).is_ok_and(|m| {
            identity(from, &m).0 == self.from_id
                && m.len() == self.from_size
                && mtime_ns(&m) == self.from_mtime_ns
        })
    }

    /// Whether the tree file with `meta` at `path` is still the file this record made.
    pub(crate) fn tree_unchanged(&self, path: &Path, meta: &fs::Metadata) -> bool {
        identity(path, meta).0 == self.id
            && meta.len() == self.size
            && mtime_ns(meta) == self.mtime_ns
    }
}
