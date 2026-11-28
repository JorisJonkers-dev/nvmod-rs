// SPDX-License-Identifier: GPL-3.0-only
//! The journal and the backups of an install, under `<instance>/.nvmod/journal/<id>/`: `journal.json` and a copy
//! of the three profile files as they were. `nvmod rollback` undoes the newest install that is not undone yet.

use nvmod_instance::{INSTANCE_FILE, KIND_MIGRATE, NvmodInstance};
use nvmod_stage::Options;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
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

/// The kind of journal that `nvmod plugins ...` writes: it restores `plugins.txt`, `loadorder.txt` and
/// `lockedorder.txt` and the Staging tree, and never touches a Mod folder or `modlist.txt`.
pub const KIND_PLUGINS: &str = "plugins";

/// The kind of journal that the Mod lifecycle commands (`mods remove|rename|create|clone|hide|unhide|note`,
/// `mods separator ...`, `mods category set`) write: it restores the four profile files and undoes the recorded
/// file operations (moves, created folders, rewritten files) in reverse, and never removes a Mod by name.
pub const KIND_LIFECYCLE: &str = "lifecycle";

/// The kind of journal that `nvmod profile copy|rename|delete` write: it undoes the recorded file operations (a
/// copy is removed, a moved Profile folder goes back, a rewritten `profile.json` is restored) and touches no
/// Mod folder, no list of another Profile and no Staging tree beyond them.
pub const KIND_PROFILE: &str = "profile";

/// The kind of journal that `nvmod sweep` writes. It records what was moved (for the trash and for audit) but is
/// never rolled back: the files a program wrote are in Overwrite after it, an interrupted sweep is finished by
/// running it again, and building the tree again would drop what the program wrote there.
pub const KIND_SWEEP: &str = "sweep";

const FILES: [&str; 5] = [
    "modlist.txt",
    "plugins.txt",
    "loadorder.txt",
    "lockedorder.txt",
    "deactivated.txt",
];

/// Whether a journal of `kind` changes (and so restores) `FILES[index]`: an install the first three, a sort the
/// two plugin lists, a mods change `modlist.txt`, `plugins.txt` and `deactivated.txt` (a disable takes plugins out of
/// the active list and notes them, an enable puts them back), a plugins change the plugin lists and `lockedorder.txt`.
fn restores(kind: &str, index: usize) -> bool {
    match kind {
        KIND_SORT => index == 1 || index == 2,
        KIND_MODS => index <= 1 || index == 4,
        KIND_PLUGINS => (1..=3).contains(&index),
        KIND_LIFECYCLE => true,
        _ => index < 3,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Journal {
    schema: u32,
    id: String,
    r#mod: String,
    profile: String,
    status: String,
    /// A human-readable label for the change (empty when the journal was written before labels existed).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    label: String,
    /// The time the entry was created, in milliseconds since the Unix epoch. When absent the time is taken from
    /// the millisecond prefix of the id.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    at_ms: Option<u64>,
    /// `install` (also what a journal without this field is) or `sort`.
    #[serde(default = "install_kind")]
    kind: String,
    /// Which of the profile files existed before (an older journal has three entries).
    existed: Vec<bool>,
    staging_existed: bool,
    /// File operations recorded since the journal began, oldest first (lifecycle journals only).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    ops: Vec<Op>,
}

/// One file operation of a lifecycle change, with paths relative to the Instance folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum Op {
    /// `from` was renamed to `to` (undone by renaming back).
    Move {
        /// The old place.
        from: String,
        /// The new place.
        to: String,
    },
    /// A file or folder was created at this place (undone by removing it).
    Create {
        /// The new file or folder.
        path: String,
    },
    /// A file was rewritten; `saved` names the copy of the old one in the journal (`None`: it did not exist).
    Restore {
        /// The rewritten file.
        path: String,
        /// The saved copy, a name under the journal's `files` folder.
        saved: Option<String>,
    },
}

fn install_kind() -> String {
    KIND_INSTALL.to_owned()
}

/// An install in progress.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The journal id.
    pub id: String,
    journal: Journal,
}

impl Entry {
    /// The profile the entry belongs to.
    #[must_use]
    pub fn profile(&self) -> &str {
        &self.journal.profile
    }

    /// The kind of change (`install`, `sort`, `mods`, `plugins`, `lifecycle`, `profile` or `migrate`).
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.journal.kind
    }

    /// The Mod involved (empty for sorts, mods changes and so on).
    #[must_use]
    pub fn mod_name(&self) -> &str {
        &self.journal.r#mod
    }

    /// The entry status: `in-progress`, `complete`, `rolled-back` or `redone`.
    #[must_use]
    pub fn status(&self) -> &str {
        &self.journal.status
    }

    /// The human-readable label for the change (empty when none was given).
    #[must_use]
    pub fn label(&self) -> &str {
        &self.journal.label
    }

    /// The time the entry was created, in milliseconds since the Unix epoch.
    #[must_use]
    pub fn at_ms(&self) -> u64 {
        self.journal
            .at_ms
            .unwrap_or_else(|| at_ms_from_id(&self.journal.id))
    }
}

/// A summary of a journal entry for listing and loading.
#[derive(Debug, Clone, Serialize)]
pub struct EntrySummary {
    /// The journal id.
    pub id: String,
    /// The profile the entry belongs to.
    pub profile: String,
    /// The kind of change.
    pub kind: String,
    /// The Mod involved (empty when none).
    pub mod_name: String,
    /// The entry status.
    pub status: String,
    /// The human-readable label for the change.
    pub label: String,
    /// The time the entry was created, in milliseconds since the Unix epoch.
    pub at_ms: u64,
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
    /// What was undone: `install`, `sort`, `mods`, `plugins`, `lifecycle`, `profile` or `migrate`.
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

/// Extract the millisecond timestamp from the prefix of a journal id.
fn at_ms_from_id(id: &str) -> u64 {
    id.split('-')
        .next()
        .and_then(|prefix| prefix.parse::<u64>().ok())
        .unwrap_or(0)
}

/// Build an [`EntrySummary`] from a loaded [`Journal`].
fn summarize(journal: &Journal) -> EntrySummary {
    EntrySummary {
        id: journal.id.clone(),
        profile: journal.profile.clone(),
        kind: journal.kind.clone(),
        mod_name: journal.r#mod.clone(),
        status: journal.status.clone(),
        label: journal.label.clone(),
        at_ms: journal.at_ms.unwrap_or_else(|| at_ms_from_id(&journal.id)),
    }
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

/// Whether the Staging tree of `profile` exists (what an undo of a stage must put back).
#[must_use]
pub fn staging_exists(instance: &NvmodInstance, profile: &str) -> bool {
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
        label: String::new(),
        at_ms: Some(at_ms_from_id(&id)),
        kind: kind.to_owned(),
        existed,
        staging_existed: staging_exists(instance, profile),
        ops: Vec::new(),
    };
    save(&root, &journal)?;
    Ok(Entry { id, journal })
}

/// Like [`begin_kind`], but stores a human-readable `label` with the entry.
///
/// # Errors
/// A reason, when the journal or a backup could not be written (nothing else was changed).
pub fn begin_labelled(
    instance: &NvmodInstance,
    profile: &str,
    name: &str,
    kind: &str,
    label: &str,
) -> Result<Entry, String> {
    let mut entry = begin_kind(instance, profile, name, kind)?;
    label.clone_into(&mut entry.journal.label);
    save(&journal_root(instance).join(&entry.id), &entry.journal)?;
    Ok(entry)
}

impl Entry {
    /// Record that `from` is about to be renamed to `to` (paths relative to the Instance folder); the journal is
    /// saved before the caller does it, so a stop half way is still undone.
    ///
    /// # Errors
    /// The journal could not be written.
    pub fn record_move(
        &mut self,
        instance: &NvmodInstance,
        from: &str,
        to: &str,
    ) -> Result<(), String> {
        self.record(
            instance,
            Op::Move {
                from: from.to_owned(),
                to: to.to_owned(),
            },
        )
    }

    /// Record that a file or folder is about to be created at `path` (relative to the Instance folder).
    ///
    /// # Errors
    /// The journal could not be written.
    pub fn record_create(&mut self, instance: &NvmodInstance, path: &str) -> Result<(), String> {
        self.record(
            instance,
            Op::Create {
                path: path.to_owned(),
            },
        )
    }

    /// Save the file at `path` (relative to the Instance folder; it may not exist) and record that it is about
    /// to be rewritten.
    ///
    /// # Errors
    /// The copy or the journal could not be written.
    pub fn record_file(&mut self, instance: &NvmodInstance, path: &str) -> Result<(), String> {
        let source = instance.root.join(path);
        let saved = if source.is_file() {
            let name = format!("{}", self.journal.ops.len());
            let dir = journal_root(instance).join(&self.id).join("files");
            fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
            fs::copy(&source, dir.join(&name)).map_err(|e| format!("backup of {path}: {e}"))?;
            Some(name)
        } else {
            None
        };
        self.record(
            instance,
            Op::Restore {
                path: path.to_owned(),
                saved,
            },
        )
    }

    fn record(&mut self, instance: &NvmodInstance, op: Op) -> Result<(), String> {
        self.journal.ops.push(op);
        save(&journal_root(instance).join(&self.id), &self.journal)
    }
}

fn undo_ops(instance: &NvmodInstance, journal: &Journal, root: &Path) -> Result<(), String> {
    for op in journal.ops.iter().rev() {
        let io = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
        match op {
            Op::Move { from, to } => {
                let (from, to) = (instance.root.join(from), instance.root.join(to));
                if to.exists() && !from.exists() {
                    if let Some(parent) = from.parent() {
                        fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
                    }
                    fs::rename(&to, &from).map_err(|e| io(&to, e))?;
                }
            }
            Op::Create { path } => {
                let at = instance.root.join(path);
                if at.is_dir() {
                    fs::remove_dir_all(&at).map_err(|e| io(&at, e))?;
                } else if at.exists() {
                    fs::remove_file(&at).map_err(|e| io(&at, e))?;
                }
            }
            Op::Restore { path, saved } => {
                let at = instance.root.join(path);
                if let Some(saved) = saved {
                    let bytes = fs::read(root.join("files").join(saved))
                        .map_err(|e| format!("saved copy of {path}: {e}"))?;
                    write_atomic(&at, &bytes)?;
                } else if at.exists() {
                    fs::remove_file(&at).map_err(|e| io(&at, e))?;
                }
            }
        }
    }
    Ok(())
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
    if journal.kind == KIND_PROFILE {
        // The operations hold everything a Profile change did, the Staging tree included.
        undo_ops(instance, journal, &root)?;
        let mut done = journal.clone();
        "rolled-back".clone_into(&mut done.status);
        return save(&root, &done);
    }
    let sort = journal.kind == KIND_SORT;
    let mods = journal.kind == KIND_MODS;
    if journal.kind == KIND_LIFECYCLE {
        undo_ops(instance, journal, &root)?;
    } else if !sort && !mods && journal.kind != KIND_PLUGINS {
        let mod_dir = instance.mod_dir(&journal.r#mod);
        if mod_dir.exists() {
            fs::remove_dir_all(&mod_dir).map_err(|e| format!("{}: {e}", mod_dir.display()))?;
        }
    }
    let dir = instance.profile_dir(&journal.profile);
    // plugins.txt before loadorder.txt, so loadorder.txt stays the newest. Each kind restores only its own files.
    for (index, file) in FILES.iter().enumerate().rev() {
        if !restores(&journal.kind, index) {
            continue;
        }
        let target = dir.join(file);
        if journal.existed.get(index).copied().unwrap_or(false) {
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
        if journal.status == "rolled-back" || journal.kind == KIND_SWEEP {
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

/// Read every journal id under the instance's journal root.
fn read_ids(instance: &NvmodInstance) -> Vec<String> {
    fs::read_dir(journal_root(instance))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect()
}

/// Load and parse one journal file.
fn read_journal(path: &Path) -> Result<Journal, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// List the journal entries of one profile, oldest first.
///
/// Entries whose journal file cannot be read are skipped.
#[must_use]
pub fn list(instance: &NvmodInstance, profile: &str) -> Vec<EntrySummary> {
    let root = journal_root(instance);
    let mut ids = read_ids(instance);
    ids.sort();
    ids.into_iter()
        .filter_map(|id| {
            let journal = read_journal(&root.join(&id).join("journal.json")).ok()?;
            if journal.profile != profile {
                return None;
            }
            Some(summarize(&journal))
        })
        .collect()
}

/// Load a journal entry by id.
///
/// # Errors
/// A reason, when the journal file is missing or cannot be parsed.
pub fn load(instance: &NvmodInstance, id: &str) -> Result<Entry, String> {
    let path = journal_root(instance).join(id).join("journal.json");
    let journal = read_journal(&path)?;
    Ok(Entry {
        id: journal.id.clone(),
        journal,
    })
}

/// Undo the newest not-rolled-back entry of a single profile.
///
/// # Errors
/// `Refused` when the profile has nothing to undo; `Failed` when a restore step fails.
pub fn rollback_profile(
    instance: &NvmodInstance,
    profile: &str,
) -> Result<RollbackReport, RollbackError> {
    let mut ids = read_ids(instance);
    ids.sort();
    for id in ids.into_iter().rev() {
        let path = journal_root(instance).join(&id).join("journal.json");
        let journal = read_journal(&path)
            .map_err(|e| RollbackError::Failed(format!("{}: {e}", path.display())))?;
        if journal.profile != profile {
            continue;
        }
        if journal.status == "rolled-back" || journal.kind == KIND_SWEEP {
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
    Err(RollbackError::Refused(format!(
        "there is no entry to roll back for profile `{profile}`"
    )))
}

/// Keep the `keep_per_profile` newest entries for each profile and remove the rest.
///
/// # Errors
/// A reason, when a journal directory could not be removed.
pub fn prune(instance: &NvmodInstance, keep_per_profile: usize) -> Result<(), String> {
    let root = journal_root(instance);
    let mut ids = read_ids(instance);
    ids.sort();
    let mut kept: HashMap<String, usize> = HashMap::new();
    for id in ids.into_iter().rev() {
        let path = root.join(&id).join("journal.json");
        let Ok(journal) = read_journal(&path) else {
            continue;
        };
        let count = kept.entry(journal.profile).or_insert(0);
        if *count < keep_per_profile {
            *count += 1;
        } else {
            fs::remove_dir_all(root.join(&id))
                .map_err(|e| format!("{}: {e}", root.join(&id).display()))?;
        }
    }
    Ok(())
}

/// Roll back every completed entry of `profile` that is newer than `id`, newest first.
///
/// # Errors
/// `Refused` when `id` does not exist or does not belong to `profile`; `Failed` when a restore step fails.
pub fn restore_to(
    instance: &NvmodInstance,
    profile: &str,
    id: &str,
) -> Result<Vec<RollbackReport>, RollbackError> {
    let target = load(instance, id).map_err(RollbackError::Refused)?;
    if target.profile() != profile {
        return Err(RollbackError::Refused(format!(
            "journal `{id}` does not belong to profile `{profile}`"
        )));
    }

    let mut ids = read_ids(instance);
    ids.sort();
    let mut reports = Vec::new();
    for candidate in ids.into_iter().rev() {
        if candidate.as_str() <= id {
            break;
        }
        let path = journal_root(instance).join(&candidate).join("journal.json");
        let journal = read_journal(&path)
            .map_err(|e| RollbackError::Failed(format!("{}: {e}", path.display())))?;
        if journal.profile != profile {
            continue;
        }
        if !matches!(journal.status.as_str(), "complete" | "redone") || journal.kind == KIND_SWEEP {
            continue;
        }
        let report = RollbackReport {
            journal: journal.id.clone(),
            kind: journal.kind.clone(),
            r#mod: journal.r#mod.clone(),
            profile: journal.profile.clone(),
        };
        undo(
            instance,
            &Entry {
                id: candidate,
                journal,
            },
        )
        .map_err(RollbackError::Failed)?;
        reports.push(report);
    }
    Ok(reports)
}

/// Mark a previously rolled-back entry as `redone`.
///
/// This records that the caller has applied the change again (for example by running the same change once more);
/// it does not restore any file itself, because the journal keeps only the state before a change. A redone entry
/// counts as applied again: [`restore_to`] and [`rollback_profile`] can roll it back.
///
/// # Errors
/// `Refused` when the entry is not rolled back; `Failed` when the journal could not be written.
pub fn redo(instance: &NvmodInstance, id: &str) -> Result<(), RollbackError> {
    let path = journal_root(instance).join(id).join("journal.json");
    let mut journal = read_journal(&path)
        .map_err(|e| RollbackError::Failed(format!("{}: {e}", path.display())))?;
    if journal.status != "rolled-back" {
        return Err(RollbackError::Refused(format!(
            "journal `{id}` is not rolled back"
        )));
    }
    "redone".clone_into(&mut journal.status);
    save(&journal_root(instance).join(id), &journal).map_err(RollbackError::Failed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{BaseBuilder, TempDir, write_file};

    fn steam_instance() -> (TempDir, NvmodInstance) {
        let base = TempDir::new("jr-base");
        BaseBuilder::steam().write(base.path());
        let tmp = TempDir::new("jr-inst");
        let root = tmp.path().join("fnv");
        let instance = NvmodInstance::create(&root, base.path()).expect("create the Instance");
        instance.create_profile("Default").expect("create Default");
        instance.create_profile("Other").expect("create Other");
        (tmp, instance)
    }

    fn complete_mods_entry(
        instance: &NvmodInstance,
        profile: &str,
        label: &str,
        content: &[u8],
    ) -> Entry {
        let entry = if label.is_empty() {
            begin_kind(instance, profile, "", KIND_MODS).unwrap()
        } else {
            begin_labelled(instance, profile, "", KIND_MODS, label).unwrap()
        };
        write_file(&instance.profile_dir(profile), "modlist.txt", content);
        complete(instance, &entry).unwrap();
        entry
    }

    #[test]
    fn list_returns_profile_entries_oldest_first_and_load_reads_one() {
        let (_tmp, instance) = steam_instance();
        let a = complete_mods_entry(&instance, "Default", "", b"A\n");
        let b = complete_mods_entry(&instance, "Default", "", b"B\n");
        let other = complete_mods_entry(&instance, "Other", "", b"O\n");

        let summaries = list(&instance, "Default");
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].id, a.id);
        assert_eq!(summaries[1].id, b.id);

        let loaded = load(&instance, &b.id).unwrap();
        assert_eq!(loaded.id, b.id);
        assert_eq!(loaded.profile(), "Default");
        assert_eq!(loaded.kind(), KIND_MODS);
        assert_eq!(loaded.status(), "complete");

        assert_eq!(list(&instance, "Other").len(), 1);
        assert_eq!(list(&instance, "Other")[0].id, other.id);
    }

    #[test]
    fn label_round_trips_and_at_ms_is_set() {
        let (_tmp, instance) = steam_instance();
        let entry = begin_labelled(&instance, "Default", "", KIND_MODS, "Enable foo").unwrap();
        complete(&instance, &entry).unwrap();

        let summary = list(&instance, "Default").pop().unwrap();
        assert_eq!(summary.label, "Enable foo");
        assert!(summary.at_ms > 0);

        let loaded = load(&instance, &entry.id).unwrap();
        assert_eq!(loaded.label(), "Enable foo");
        assert_eq!(loaded.at_ms(), summary.at_ms);
    }

    #[test]
    fn old_journal_without_label_or_at_ms_loads_with_fallbacks() {
        let (_tmp, instance) = steam_instance();
        let id = "0000000000123456-000001-12345";
        let dir = journal_root(&instance).join(id);
        let journal = format!(
            r#"{{"schema":1,"id":"{id}","mod":"Old","profile":"Default","status":"complete","existed":[true,false,false],"staging_existed":false}}"#
        );
        write_file(&dir, "journal.json", journal.as_bytes());

        let entry = load(&instance, id).unwrap();
        assert_eq!(entry.label(), "");
        assert_eq!(entry.at_ms(), 12_3456);
        let summary = list(&instance, "Default").pop().unwrap();
        assert_eq!(summary.label, "");
        assert_eq!(summary.at_ms, 12_3456);
    }

    #[test]
    fn rollback_profile_only_affects_its_profile() {
        let (_tmp, instance) = steam_instance();
        let default = complete_mods_entry(&instance, "Default", "", b"+Default\n");
        let other = complete_mods_entry(&instance, "Other", "", b"+Other\n");

        let report = rollback_profile(&instance, "Default").unwrap();
        assert_eq!(report.journal, default.id);
        assert_eq!(report.profile, "Default");

        let default_content =
            fs::read_to_string(instance.profile_dir("Default").join("modlist.txt")).unwrap();
        assert!(!default_content.contains("+Default"));

        let other_entry = load(&instance, &other.id).unwrap();
        assert_eq!(other_entry.status(), "complete");
    }

    #[test]
    fn prune_keeps_the_newest_n_per_profile() {
        let (_tmp, instance) = steam_instance();
        let mut default_ids = Vec::new();
        let mut other_ids = Vec::new();
        for i in 0..4 {
            default_ids.push(
                complete_mods_entry(&instance, "Default", "", format!("D{i}\n").as_bytes()).id,
            );
        }
        for i in 0..3 {
            other_ids
                .push(complete_mods_entry(&instance, "Other", "", format!("O{i}\n").as_bytes()).id);
        }

        prune(&instance, 2).unwrap();

        let default_remaining: Vec<String> = list(&instance, "Default")
            .into_iter()
            .map(|s| s.id)
            .collect();
        let other_remaining: Vec<String> =
            list(&instance, "Other").into_iter().map(|s| s.id).collect();
        assert_eq!(default_remaining.len(), 2);
        assert_eq!(other_remaining.len(), 2);
        assert_eq!(default_remaining, default_ids[2..]);
        assert_eq!(other_remaining, other_ids[1..]);
    }

    #[test]
    fn restore_to_rolls_back_only_newer_completed_entries_of_the_profile() {
        let (_tmp, instance) = steam_instance();
        let a = complete_mods_entry(&instance, "Default", "", b"A\n");
        let b = complete_mods_entry(&instance, "Default", "", b"B\n");
        let c = complete_mods_entry(&instance, "Default", "", b"C\n");
        let other = complete_mods_entry(&instance, "Other", "", b"O\n");

        let reports = restore_to(&instance, "Default", &a.id).unwrap();
        assert_eq!(reports.len(), 2);
        assert_eq!(reports[0].journal, c.id);
        assert_eq!(reports[1].journal, b.id);

        let content =
            fs::read_to_string(instance.profile_dir("Default").join("modlist.txt")).unwrap();
        assert!(content.contains('A'));

        let other_entry = load(&instance, &other.id).unwrap();
        assert_eq!(other_entry.status(), "complete");
    }

    #[test]
    fn redo_marks_a_rolled_back_entry_redone() {
        let (_tmp, instance) = steam_instance();
        let entry = complete_mods_entry(&instance, "Default", "", b"X\n");
        rollback_profile(&instance, "Default").unwrap();

        redo(&instance, &entry.id).unwrap();
        let loaded = load(&instance, &entry.id).unwrap();
        assert_eq!(loaded.status(), "redone");

        assert!(matches!(
            redo(&instance, &entry.id),
            Err(RollbackError::Refused(_))
        ));
    }

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
