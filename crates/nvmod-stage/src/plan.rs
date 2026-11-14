// SPDX-License-Identifier: GPL-3.0-only
//! The plan of a Staging tree: which source file ends up at which lowercased path.

use nvmod_core::Finding;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Files of one Mod that differ only in case.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Collision {
    /// The Mod (folder name) the files are in.
    pub mod_name: String,
    /// The paths as the Mod spells them, sorted.
    pub paths: Vec<String>,
    /// The one the tree keeps (the first in byte order).
    pub kept: String,
}

#[derive(Debug, Default)]
pub(crate) struct Plan {
    /// Lowercased `/` path to the winning source file.
    pub(crate) files: BTreeMap<String, PathBuf>,
    pub(crate) overridden: usize,
    pub(crate) collisions: Vec<Collision>,
    pub(crate) findings: Vec<Finding>,
}

/// Every file under `dir` as (original relative path with `/`, full path), sorted; symbolic links are skipped.
fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            walk(&entry.path(), &format!("{name}/"), out);
        } else if kind.is_file() {
            out.push((name, entry.path()));
        }
    }
}

impl Plan {
    /// Add the files of `dir`, replacing what earlier calls put at the same path. `base` is the game's `Data`
    /// folder; a Mod's `meta.ini` at its top is MO2's metadata, not a game file.
    pub(crate) fn add_dir(&mut self, owner: &str, dir: &Path, base: bool) {
        let mut files = Vec::new();
        walk(dir, "", &mut files);
        let mut seen: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut chosen: BTreeMap<String, PathBuf> = BTreeMap::new();
        for (relative, path) in files {
            if !base && relative.eq_ignore_ascii_case("meta.ini") {
                continue;
            }
            let key = relative.to_lowercase();
            seen.entry(key.clone()).or_default().push(relative);
            chosen.entry(key).or_insert(path);
        }
        for (key, paths) in seen {
            if paths.len() > 1 && !base {
                self.collisions.push(Collision {
                    mod_name: owner.to_owned(),
                    kept: paths[0].clone(),
                    paths,
                });
            }
            let path = chosen.remove(&key).unwrap_or_default();
            if self.files.insert(key, path).is_some() {
                self.overridden += 1;
            }
        }
    }
}
