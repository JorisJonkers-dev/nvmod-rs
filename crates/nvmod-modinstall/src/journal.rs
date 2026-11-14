// SPDX-License-Identifier: GPL-3.0-only
//! The journal and the backups of an install, under `<instance>/.nvmod/journal/<id>/`: `journal.json` and a copy
//! of the three profile files as they were. `nvmod rollback` undoes the newest install that is not undone yet.

use nvmod_instance::{INSTANCE_FILE, KIND_MIGRATE, NvmodInstance};
use nvmod_stage::Options;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// The kind of journal that an install writes.
pub const KIND_INSTALL: &str = "install";
/// The kind of journal that `nvmod sort --apply` writes: it restores `plugins.txt` and `loadorder.txt` and the
/// Staging tree, and never touches a Mod folder or `modlist.txt`.
pub const KIND_SORT: &str = "sort";

/// The kind of journal that `nvmod mods enable|disable|reorder` writes: it restores `modlist.txt` and the Staging
/// tree, and never touches a Mod folder, `plugins.txt` or `loadorder.txt`.
pub const KIND_MODS: &str = "mods";

const FILES: [&str; 3] = ["modlist.txt", "plugins.txt", "loadorder.txt"];

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Journal {
    schema: u32,
    id: String,
    r#mod: String,
    profile: String,
    status: String,
    /// `install` (also what a journal without this field is) or `sort`.
    #[serde(default = "install_kind")]
    kind: String,
    /// Which of the three profile files existed before.
    existed: Vec<bool>,
    staging_existed: bool,
}

fn install_kind() -> String {
    KIND_INSTALL.to_owned()
}

/// An install in progress.
#[derive(Debug)]
pub struct Entry {
    /// The journal id.
    pub id: String,
    journal: Journal,
}

/// Why a rollback did not happen.
#[derive(Debug, PartialEq, Eq)]
pub enum RollbackError {
    /// Nothing to undo (exit code 2).
    Refused(String),
    /// A step failed (exit code 4).
    Failed(String),
}

/// What a rollback did.
#[derive(Debug, Clone, Serialize)]
pub struct RollbackReport {
    /// The journal undone.
    pub journal: String,
    /// What was undone: `install`, `sort`, `mods` or `migrate`.
    pub kind: String,
    /// The Mod removed (empty for a sort or a mods change).
    pub r#mod: String,
    /// The profile restored.
    pub profile: String,
}

/// A journal id: `{millis:016}-{counter:06}-{pid}`. Two journals of one process never share an id (the counter
/// grows with every call, even inside one millisecond), and ids sort in creation order: the milliseconds never go
/// back within a process, then the counter, and a restart of the process gets later milliseconds.
fn next_id() -> String {
    use std::sync::Mutex;
    // The last milliseconds handed out and the counter: a clock that steps back cannot reorder ids.
    static LAST: Mutex<(u128, u64)> = Mutex::new((0, 0));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let mut last = LAST
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    last.0 = last.0.max(now);
    last.1 += 1;
    format!("{:016}-{:06}-{}", last.0, last.1, std::process::id())
}

fn journal_root(instance: &NvmodInstance) -> PathBuf {
    instance.root.join(".nvmod").join("journal")
}

/// Write `bytes` to `path` through a temporary file in the same folder, then rename.
///
/// # Errors
/// The write or the rename failed.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("nvmod-tmp");
    let io = |e: std::io::Error| format!("{}: {e}", path.display());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(io)?;
    }
    fs::write(&tmp, bytes).map_err(io)?;
    fs::rename(&tmp, path).map_err(io)
}

fn save(root: &Path, journal: &Journal) -> Result<(), String> {
    let text = serde_json::to_string_pretty(journal).map_err(|e| e.to_string())?;
    write_atomic(&root.join("journal.json"), text.as_bytes())
}

pub(crate) fn staging_exists(instance: &NvmodInstance, profile: &str) -> bool {
    instance.staging_dir(profile).exists()
        || instance
            .root
            .join("staging")
            .join(format!("{profile}.json"))
            .exists()
}

/// Write the journal and back up the profile files, before anything else is touched.
///
/// # Errors
/// A reason, when the journal or a backup could not be written (nothing else was changed).
pub fn begin(instance: &NvmodInstance, profile: &str, name: &str) -> Result<Entry, String> {
    begin_kind(instance, profile, name, KIND_INSTALL)
}

/// Like [`begin`], for a journal of the given kind (`name` is empty for a sort).
///
/// # Errors
/// A reason, when the journal or a backup could not be written (nothing else was changed).
pub fn begin_kind(
    instance: &NvmodInstance,
    profile: &str,
    name: &str,
    kind: &str,
) -> Result<Entry, String> {
    let id = next_id();
    let root = journal_root(instance).join(&id);
    let backup = root.join("backup");
    fs::create_dir_all(&backup).map_err(|e| format!("{}: {e}", backup.display()))?;
    let dir = instance.profile_dir(profile);
    let mut existed = Vec::new();
    for file in FILES {
        let source = dir.join(file);
        existed.push(source.is_file());
        if source.is_file() {
            fs::copy(&source, backup.join(file)).map_err(|e| format!("backup of {file}: {e}"))?;
        }
    }
    let journal = Journal {
        schema: 1,
        id: id.clone(),
        r#mod: name.to_owned(),
        profile: profile.to_owned(),
        status: "in-progress".to_owned(),
        kind: kind.to_owned(),
        existed,
        staging_existed: staging_exists(instance, profile),
    };
    save(&root, &journal)?;
    Ok(Entry { id, journal })
}

/// Mark the install complete.
///
/// # Errors
/// The journal could not be written.
pub fn complete(instance: &NvmodInstance, entry: &Entry) -> Result<(), String> {
    let mut journal = entry.journal.clone();
    "complete".clone_into(&mut journal.status);
    save(&journal_root(instance).join(&entry.id), &journal)
}

/// Put the Instance back as the journal says it was, and mark the journal rolled back.
///
/// # Errors
/// A reason, when a restore step failed.
pub fn undo(instance: &NvmodInstance, entry: &Entry) -> Result<(), String> {
    let journal = &entry.journal;
    let root = journal_root(instance).join(&journal.id);
    if journal.kind == KIND_MIGRATE {
        // A schema migration only rewrote the Instance file: put the old file back.
        let old = fs::read(root.join("backup").join(INSTANCE_FILE))
            .map_err(|e| format!("backup of {INSTANCE_FILE}: {e}"))?;
        write_atomic(&instance.root.join(INSTANCE_FILE), &old)?;
        let mut done = journal.clone();
        "rolled-back".clone_into(&mut done.status);
        return save(&root, &done);
    }
    let sort = journal.kind == KIND_SORT;
    let mods = journal.kind == KIND_MODS;
    if !sort && !mods {
        let mod_dir = instance.mod_dir(&journal.r#mod);
        if mod_dir.exists() {
            fs::remove_dir_all(&mod_dir).map_err(|e| format!("{}: {e}", mod_dir.display()))?;
        }
    }
    let dir = instance.profile_dir(&journal.profile);
    // plugins.txt before loadorder.txt, so loadorder.txt stays the newest. A sort leaves modlist.txt alone.
    for (index, file) in FILES.iter().enumerate().rev() {
        // A sort leaves modlist.txt alone; a mods change only touches modlist.txt.
        if (sort && index == 0) || (mods && index != 0) {
            continue;
        }
        let target = dir.join(file);
        if journal.existed[index] {
            let bytes = fs::read(root.join("backup").join(file))
                .map_err(|e| format!("backup of {file}: {e}"))?;
            write_atomic(&target, &bytes)?;
        } else if target.exists() {
            fs::remove_file(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        }
    }
    if journal.staging_existed {
        nvmod_stage::stage(instance, &journal.profile, Options::default()).map_err(
            |e| match e {
                nvmod_stage::StageError::Refused(m) | nvmod_stage::StageError::Failed(m) => m,
            },
        )?;
    } else {
        let _ = fs::remove_dir_all(instance.staging_dir(&journal.profile));
        let _ = fs::remove_file(
            instance
                .root
                .join("staging")
                .join(format!("{}.json", journal.profile)),
        );
    }
    let mut done = journal.clone();
    "rolled-back".clone_into(&mut done.status);
    save(&root, &done)
}

/// Undo the newest install that is not undone yet.
///
/// # Errors
/// `Refused` when there is nothing to undo; `Failed` when a restore step fails (run it again after fixing that).
pub fn rollback(instance: &NvmodInstance) -> Result<RollbackReport, RollbackError> {
    let mut ids: Vec<String> = fs::read_dir(journal_root(instance))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort();
    for id in ids.into_iter().rev() {
        let path = journal_root(instance).join(&id).join("journal.json");
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let journal: Journal = serde_json::from_str(&text)
            .map_err(|e| RollbackError::Failed(format!("{}: {e}", path.display())))?;
        if journal.status == "rolled-back" {
            continue;
        }
        let report = RollbackReport {
            journal: journal.id.clone(),
            kind: journal.kind.clone(),
            r#mod: journal.r#mod.clone(),
            profile: journal.profile.clone(),
        };
        undo(instance, &Entry { id, journal }).map_err(RollbackError::Failed)?;
        return Ok(report);
    }
    Err(RollbackError::Refused(
        "there is no install to roll back".to_owned(),
    ))
}

#[cfg(test)]
mod tests {
    use super::next_id;

    #[test]
    fn ids_made_in_one_process_are_unique_and_sort_in_creation_order() {
        let ids: Vec<String> = (0..2000).map(|_| next_id()).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "ids sort in creation order");
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "no two ids are equal");
    }

    #[test]
    fn ids_made_on_several_threads_never_collide() {
        let handles: Vec<_> = (0..4)
            .map(|_| std::thread::spawn(|| (0..500).map(|_| next_id()).collect::<Vec<_>>()))
            .collect();
        let mut all: Vec<String> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();
        let n = all.len();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), n);
    }
}
