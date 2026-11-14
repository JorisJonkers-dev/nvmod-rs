// SPDX-License-Identifier: GPL-3.0-only
//! `saves list`: the saves of a folder with the plugin count from the header.

use crate::{cosave_name, save_files};
use nvmod_formats::savegame;
use serde::Serialize;
use std::path::Path;

/// One save as `list` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SaveInfo {
    /// The file name.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// Modification time, Unix seconds.
    pub modified: Option<i64>,
    /// `ok`, or `unreadable` when the header could not be read.
    pub status: &'static str,
    /// Why the header could not be read.
    pub problem: Option<String>,
    /// The number of plugins in the save's list.
    pub plugins: Option<usize>,
    /// The player's name.
    pub player: Option<String>,
    /// Whether the `.nvse` co-save is next to it.
    pub cosave: bool,
}

/// The saves in `dir`, sorted by name. A save whose header cannot be read is listed as `unreadable`.
#[must_use]
pub fn list(dir: &Path) -> Vec<SaveInfo> {
    save_files(dir)
        .into_iter()
        .map(|(name, path)| {
            let meta = std::fs::metadata(&path).ok();
            let header = savegame::read_header(&path);
            let (status, problem, plugins, player) = match header {
                Ok(h) => ("ok", None, Some(h.plugins.len()), Some(h.player)),
                Err(e) => ("unreadable", Some(e.to_string()), None, None),
            };
            SaveInfo {
                size: meta.as_ref().map_or(0, std::fs::Metadata::len),
                modified: meta
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .and_then(|d| i64::try_from(d.as_secs()).ok()),
                cosave: dir.join(cosave_name(&name)).is_file(),
                name,
                status,
                problem,
                plugins,
                player,
            }
        })
        .collect()
}
