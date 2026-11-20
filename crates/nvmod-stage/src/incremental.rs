// SPDX-License-Identifier: GPL-3.0-only
//! Incremental staging: change only the paths of the Staging tree that differ from the plan, instead of building
//! the whole tree again.
//!
//! The manifest records every file of the tree and the provider file it came from ([`Record`]). A run compares the
//! new plan with the records (and the tree with its records, so that a path a program changed is put right too)
//! and applies link, unlink and replace operations. The result is the tree a full build would make.
//!
//! A run is crash safe through a journal: `staging/<profile>.ops.json` lists the operations before the first one
//! runs, and a file that an operation removes or replaces is moved to `staging/<profile>.trash/<n>` first. While
//! the journal exists the tree may be half changed; the next run (or [`recover`]) puts every moved file back and
//! removes what the operations made, which is the old tree. Deleting the journal is the commit: after it the tree
//! is the new one, and a manifest that is still the old one only makes the next run repeat a few operations.

use crate::plan::Plan;
use crate::{Manifest, NvmodInstance, Slot, stamp};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

const KILL_AT: &str = "NVMOD_STAGE_KILL_AT";

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Op {
    /// `link`, `unlink` or `replace`.
    kind: String,
    /// The path in the tree as it is on disk now (for `link`: where the new file goes), `/` separated.
    path: String,
    /// Where the new file goes (`replace` and `link`), lowercased.
    key: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Journal {
    ops: Vec<Op>,
}

/// What an incremental run did.
#[derive(Debug)]
pub(crate) struct Applied {
    /// Link, unlink and replace operations done.
    pub(crate) operations: usize,
    /// The keys that are copies, not hard links.
    pub(crate) copied: BTreeSet<String>,
}

pub(crate) fn journal_path(instance: &NvmodInstance, profile: &str) -> PathBuf {
    instance.staging_dir(profile).with_extension("ops.json")
}

fn trash_path(instance: &NvmodInstance, profile: &str) -> PathBuf {
    instance.staging_dir(profile).with_extension("trash")
}

fn io(path: &Path, e: &std::io::Error) -> String {
    format!("{}: {e}", path.display())
}

/// Remove empty folders under `dir`, deepest first (a full build never makes one).
fn prune_empty(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            prune_empty(&entry.path());
            let _ = fs::remove_dir(entry.path());
        }
    }
}

/// Put the tree back as it was before an interrupted incremental run, when one left a journal.
///
/// # Errors
/// The reason, when a file cannot be moved back (run it again after fixing that).
pub(crate) fn recover(instance: &NvmodInstance, profile: &str) -> Result<bool, String> {
    let path = journal_path(instance, profile);
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(false);
    };
    let journal: Journal = serde_json::from_str(&text).unwrap_or_default();
    let data = instance.staging_dir(profile).join("data");
    let trash = trash_path(instance, profile);
    for (n, op) in journal.ops.iter().enumerate().rev() {
        let held = trash.join(n.to_string());
        if op.kind != "unlink" {
            let made = data.join(&op.key);
            if made.exists() && (held.exists() || op.kind == "link") {
                fs::remove_file(&made).map_err(|e| io(&made, &e))?;
            }
        }
        if held.exists() {
            let back = data.join(&op.path);
            if let Some(parent) = back.parent() {
                fs::create_dir_all(parent).map_err(|e| io(parent, &e))?;
            }
            fs::rename(&held, &back).map_err(|e| io(&back, &e))?;
        }
    }
    prune_empty(&data);
    fs::remove_file(&path).map_err(|e| io(&path, &e))?;
    let _ = fs::remove_dir_all(&trash);
    Ok(true)
}

/// Whether the manifest of the last run allows an incremental one: the same layout, the same way of building
/// (copies or links, protected paths) and a record for every file.
pub(crate) fn usable(
    instance: &NvmodInstance,
    profile: &str,
    old: Option<Manifest>,
    copy_only: bool,
    protect: &[String],
) -> Option<Manifest> {
    let old = old?;
    let data = instance.staging_dir(profile).join("data");
    (old.schema == crate::SCHEMA
        && old.profile == profile
        && old.copy_only == copy_only
        && old.protect == protect
        && old.records.len() == old.files
        && data.is_dir())
    .then_some(old)
}

/// Every path of the tree: the first spelling of each lowercased path in `out`, any further spelling of the same
/// lowercased path in `extra` (a program made `Meshes` beside `meshes`).
fn walk(
    dir: &Path,
    prefix: &str,
    out: &mut BTreeMap<String, String>,
    extra: &mut Vec<(String, String)>,
) {
    let mut entries: Vec<_> = fs::read_dir(dir).into_iter().flatten().flatten().collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            walk(&entry.path(), &format!("{name}/"), out, extra);
        } else {
            // A file, or a link or special file a program left: it is not in the plan either way.
            match out.entry(name.to_lowercase()) {
                std::collections::btree_map::Entry::Occupied(taken) => {
                    extra.push((taken.key().clone(), name));
                }
                std::collections::btree_map::Entry::Vacant(free) => {
                    free.insert(name);
                }
            }
        }
    }
}

/// The operations that turn the tree into the plan.
fn diff(plan: &Plan, old: &Manifest, data: &Path) -> Vec<Op> {
    let mut on_disk = BTreeMap::new();
    let mut extra = Vec::new();
    walk(data, "", &mut on_disk, &mut extra);
    let mut ops = Vec::new();
    for (key, from) in &plan.files {
        let owner = plan.owners.get(key);
        let there = on_disk.get(key);
        let kind = match (old.records.get(key), there) {
            (None, None) => "link",
            (Some(record), Some(real)) => {
                let same_provider =
                    owner.is_some_and(|(p, s)| *p == record.provider && *s == record.source);
                let meta = fs::symlink_metadata(data.join(real));
                if real == key
                    && same_provider
                    && record.source_unchanged(from)
                    && meta
                        .is_ok_and(|m| m.is_file() && record.tree_unchanged(&data.join(real), &m))
                {
                    continue;
                }
                "replace"
            }
            (None, Some(_)) | (Some(_), None) => {
                if there.is_some() {
                    "replace"
                } else {
                    "link"
                }
            }
        };
        ops.push(Op {
            kind: kind.to_owned(),
            path: there.cloned().unwrap_or_else(|| key.clone()),
            key: key.clone(),
        });
    }
    for (key, real) in &on_disk {
        if !plan.files.contains_key(key) {
            ops.push(Op {
                kind: "unlink".to_owned(),
                path: real.clone(),
                key: key.clone(),
            });
        }
    }
    for (key, real) in extra {
        ops.push(Op {
            kind: "unlink".to_owned(),
            path: real,
            key,
        });
    }
    ops
}

fn link_or_copy(from: &Path, to: &Path, copy_only: bool, protected: bool) -> Result<bool, String> {
    if let Some(parent) = to.parent() {
        fs::create_dir_all(parent).map_err(|e| io(parent, &e))?;
    }
    if !copy_only && !protected && fs::hard_link(from, to).is_ok() {
        return Ok(true);
    }
    fs::copy(from, to).map_err(|e| io(from, &e))?;
    Ok(false)
}

/// Bring the tree of `profile` to `plan`. On a failure the tree is put back as it was (unless the process was
/// "killed" by the test hook, which leaves the journal for the next run).
///
/// # Errors
/// The reason, with the old tree restored.
pub(crate) fn run(
    instance: &NvmodInstance,
    profile: &str,
    plan: &Plan,
    old: &Manifest,
    order: &[Slot],
    protect: &[String],
) -> Result<Applied, String> {
    let data = instance.staging_dir(profile).join("data");
    let ops = diff(plan, old, &data);
    let trash = trash_path(instance, profile);
    let journal = journal_path(instance, profile);
    let mut copied: BTreeSet<String> = old
        .records
        .iter()
        .filter(|(k, r)| !r.linked && plan.files.contains_key(*k))
        .map(|(k, _)| k.clone())
        .collect();
    if ops.is_empty() {
        // Nothing to change in the links; the file times may still need to follow the Load order.
        stamp_all(&data, order)?;
        return Ok(Applied {
            operations: 0,
            copied,
        });
    }
    let kill_at: Option<usize> = std::env::var(KILL_AT).ok().and_then(|v| v.parse().ok());
    let text = serde_json::to_string(&Journal { ops: ops.clone() }).map_err(|e| e.to_string())?;
    fs::create_dir_all(&trash).map_err(|e| io(&trash, &e))?;
    fs::write(&journal, text).map_err(|e| io(&journal, &e))?;
    let outcome = (|| -> Result<(), String> {
        if kill_at == Some(0) {
            return Err("killed".to_owned());
        }
        for (n, op) in ops.iter().enumerate() {
            nvmod_core::cancel::step_point("stage-op")?;
            if op.kind != "link" {
                let held = trash.join(n.to_string());
                fs::rename(data.join(&op.path), &held).map_err(|e| io(&held, &e))?;
            }
            if op.kind == "unlink" {
                copied.remove(&op.key);
            } else {
                let from = plan
                    .files
                    .get(&op.key)
                    .ok_or_else(|| format!("{} is not in the plan", op.key))?;
                let protected = protect.iter().any(|g| crate::glob::selects(g, &op.key));
                let linked = link_or_copy(from, &data.join(&op.key), copy_only(old), protected)?;
                if linked {
                    copied.remove(&op.key);
                } else {
                    copied.insert(op.key.clone());
                }
            }
            if kill_at == Some(n + 1) {
                return Err("killed".to_owned());
            }
        }
        stamp_all(&data, order)
    })();
    match outcome {
        Err(why) if why == "killed" => Err(why),
        Err(why) => {
            recover(instance, profile)?;
            Err(why)
        }
        Ok(()) => {
            // The commit: the tree is the new one from here on.
            fs::remove_file(&journal).map_err(|e| io(&journal, &e))?;
            let _ = fs::remove_dir_all(&trash);
            prune_empty(&data);
            Ok(Applied {
                operations: ops.len(),
                copied,
            })
        }
    }
}

fn copy_only(old: &Manifest) -> bool {
    old.copy_only
}

fn stamp_all(data: &Path, order: &[Slot]) -> Result<(), String> {
    stamp::apply(&stamp::plan(data, order)).map(|_| ())
}
