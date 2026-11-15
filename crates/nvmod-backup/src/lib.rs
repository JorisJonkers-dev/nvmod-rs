// SPDX-License-Identifier: GPL-3.0-only
//! Snapshots of a profile's text lists and INI files, under `<instance>/.nvmod/backups/<profile>/<UTC time>/`.
//!
//! A snapshot is a folder of real copies (never links) plus `manifest.json` with the size and SHA-256 of every
//! file. `create` writes one, `list` shows them (and the timestamped backups MO2 itself left in a profile), `prune`
//! keeps the newest few and one per day for a while, and `restore` puts one back: the snapshot is verified first,
//! the current state is snapshotted, and a step that fails puts the current state back. Every function takes the
//! time as an argument, so tests run on a fake clock.

pub mod clock;
mod files;
mod restore;
mod retention;

use nvmod_instance::NvmodInstance;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub use files::{game_running, process_running, tracked};
pub use restore::{RestoreReport, restore};
pub use retention::{PruneReport, prune, select_keep};

/// The schema version of `manifest.json`.
pub const MANIFEST_SCHEMA: u32 = 1;
/// The file name of a snapshot's manifest.
pub const MANIFEST: &str = "manifest.json";

/// Why a backup command did not do its work.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Refused before anything was changed (exit code 2).
    Refused(String),
    /// A snapshot does not match its manifest (exit code 3); nothing was changed.
    Corrupt(String),
    /// A step failed and the profile was put back (exit code 4).
    Failed(String),
}

/// How long snapshots are kept and how often an automatic one is taken. Read from `.nvmod/backups.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// An automatic snapshot is taken before a writing command when the newest one is at least this old.
    pub interval_secs: u64,
    /// `prune` keeps this many of the newest snapshots.
    pub keep_newest: usize,
    /// `prune` also keeps the newest snapshot of each of the last this many UTC days.
    pub daily_days: u32,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            interval_secs: 3600,
            keep_newest: 10,
            daily_days: 14,
        }
    }
}

/// The configuration of the Instance; the defaults when the file is missing or cannot be read.
#[must_use]
pub fn load_config(instance: &NvmodInstance) -> Config {
    fs::read_to_string(instance.root.join(".nvmod").join("backups.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

/// One file of a snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileEntry {
    /// The path relative to the profile folder, with `/`.
    pub path: String,
    /// Size in bytes.
    pub size: u64,
    /// SHA-256, lowercase hex.
    pub sha256: String,
}

/// `manifest.json` of a snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// [`MANIFEST_SCHEMA`].
    pub schema_version: u32,
    /// The snapshot id: the folder name.
    pub id: String,
    /// The profile.
    pub profile: String,
    /// Creation time, `YYYYMMDDTHHMMSSZ`.
    pub created: String,
    /// Creation time, Unix seconds.
    pub created_unix: i64,
    /// Whether `saves/` was copied.
    pub with_saves: bool,
    /// The files, sorted by path.
    pub files: Vec<FileEntry>,
}

/// A snapshot as `list` shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    /// The id to give `restore`: a timestamp, or `mo2-<stamp>` for MO2's own backups.
    pub id: String,
    /// `nvmod` or `mo2`.
    pub source: &'static str,
    /// Creation time, Unix seconds.
    pub created_unix: i64,
    /// Creation time, `YYYYMMDDTHHMMSSZ`.
    pub created: String,
    /// Number of files.
    pub files: usize,
    /// Total size in bytes.
    pub bytes: u64,
    /// Whether `saves/` was copied.
    pub with_saves: bool,
    /// Set when the manifest cannot be read.
    pub problem: Option<String>,
}

pub(crate) fn backups_dir(instance: &NvmodInstance, profile: &str) -> PathBuf {
    instance.root.join(".nvmod").join("backups").join(profile)
}

pub(crate) fn io(path: &Path, e: &std::io::Error) -> String {
    format!("{}: {e}", path.display())
}

/// Read and check the shape of `manifest.json` in the snapshot folder `dir`.
pub(crate) fn read_manifest(dir: &Path) -> Result<Manifest, Error> {
    let path = dir.join(MANIFEST);
    let text = fs::read_to_string(&path).map_err(|e| {
        Error::Corrupt(format!(
            "the snapshot has no readable manifest ({})",
            io(&path, &e)
        ))
    })?;
    let manifest: Manifest = serde_json::from_str(&text)
        .map_err(|e| Error::Corrupt(format!("{}: {e}", path.display())))?;
    if manifest.schema_version != MANIFEST_SCHEMA {
        return Err(Error::Refused(format!(
            "{} is schema {}, this nvmod reads {MANIFEST_SCHEMA}",
            path.display(),
            manifest.schema_version
        )));
    }
    Ok(manifest)
}

/// Take a snapshot of `profile` at the time `now`.
///
/// # Errors
/// `Refused` for a missing profile or a file that is not a regular file or is too large; `Failed` when a copy
/// fails (nothing is left behind).
pub fn create(
    instance: &NvmodInstance,
    profile: &str,
    now: i64,
    with_saves: bool,
) -> Result<Manifest, Error> {
    let profile_dir = instance.profile_dir(profile);
    if !profile_dir.is_dir() {
        return Err(Error::Refused(format!(
            "the Instance has no profile `{profile}`"
        )));
    }
    let wanted = files::collect(&profile_dir, with_saves).map_err(Error::Refused)?;
    let parent = backups_dir(instance, profile);
    fs::create_dir_all(&parent).map_err(|e| Error::Failed(io(&parent, &e)))?;
    let base = clock::format_utc(now);
    let mut id = base.clone();
    let mut n = 1;
    while parent.join(&id).exists() {
        n += 1;
        id = format!("{base}-{n:02}");
    }
    let partial = parent.join(format!(".{id}.partial"));
    let _ = fs::remove_dir_all(&partial);
    let built = (|| -> Result<Manifest, String> {
        fs::create_dir_all(&partial).map_err(|e| io(&partial, &e))?;
        let mut entries = Vec::new();
        for (relative, source) in &wanted {
            nvmod_core::cancel::step_point("backup-create")?;
            let target = partial.join(relative);
            if let Some(dir) = target.parent() {
                fs::create_dir_all(dir).map_err(|e| io(dir, &e))?;
            }
            let (size, sha256) = files::copy_hashed(source, &target)?;
            entries.push(FileEntry {
                path: relative.clone(),
                size,
                sha256,
            });
        }
        let manifest = Manifest {
            schema_version: MANIFEST_SCHEMA,
            id: id.clone(),
            profile: profile.to_owned(),
            created: clock::format_utc(now),
            created_unix: now,
            with_saves,
            files: entries,
        };
        let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
        fs::write(partial.join(MANIFEST), text).map_err(|e| io(&partial, &e))?;
        fs::rename(&partial, parent.join(&id)).map_err(|e| io(&partial, &e))?;
        Ok(manifest)
    })();
    built.map_err(|why| {
        let _ = fs::remove_dir_all(&partial);
        Error::Failed(why)
    })
}

/// Check every file of the snapshot `id` against its manifest.
///
/// # Errors
/// `Corrupt` when the manifest cannot be read, a file is missing or a size or hash differs, or a path would
/// leave the snapshot.
pub fn verify(instance: &NvmodInstance, profile: &str, id: &str) -> Result<Manifest, Error> {
    let dir = backups_dir(instance, profile).join(id);
    let manifest = read_manifest(&dir)?;
    for entry in &manifest.files {
        if files::unsafe_path(&entry.path) {
            return Err(Error::Corrupt(format!(
                "the manifest names `{}`, which leaves the snapshot",
                entry.path
            )));
        }
        let path = dir.join(&entry.path);
        let (size, hash) =
            files::hash(&path).map_err(|why| Error::Corrupt(format!("{}: {why}", entry.path)))?;
        if size != entry.size || hash != entry.sha256 {
            return Err(Error::Corrupt(format!(
                "`{}` differs from the manifest (the snapshot is damaged)",
                entry.path
            )));
        }
    }
    Ok(manifest)
}

/// The snapshots of `profile`, newest first: nvmod's own, then the timestamped backups MO2 left in the profile
/// folder (`<list>.txt.<yyyy_MM_dd_hh_mm_ss>`, read as UTC).
#[must_use]
pub fn list(instance: &NvmodInstance, profile: &str) -> Vec<Snapshot> {
    let mut out = Vec::new();
    for entry in fs::read_dir(backups_dir(instance, profile))
        .into_iter()
        .flatten()
        .flatten()
    {
        let id = entry.file_name().to_string_lossy().into_owned();
        if id.starts_with('.') || !entry.path().is_dir() {
            continue;
        }
        let guess = clock::parse_utc(&id).unwrap_or(0);
        out.push(match read_manifest(&entry.path()) {
            Ok(m) => Snapshot {
                id,
                source: "nvmod",
                created_unix: m.created_unix,
                created: m.created.clone(),
                files: m.files.len(),
                bytes: m.files.iter().map(|f| f.size).sum(),
                with_saves: m.with_saves,
                problem: None,
            },
            Err(Error::Corrupt(why) | Error::Refused(why) | Error::Failed(why)) => Snapshot {
                created: clock::format_utc(guess),
                id,
                source: "nvmod",
                created_unix: guess,
                files: 0,
                bytes: 0,
                with_saves: false,
                problem: Some(why),
            },
        });
    }
    out.extend(files::mo2_backups(&instance.profile_dir(profile)));
    out.sort_by(|a, b| {
        b.created_unix
            .cmp(&a.created_unix)
            .then_with(|| b.id.cmp(&a.id))
    });
    out
}

/// Take an automatic snapshot before a writing command: one when the newest of nvmod's own is at least
/// `config.interval_secs` old, or there is none. `Ok(None)` when the newest is recent enough.
///
/// # Errors
/// As [`create`].
pub fn auto(
    instance: &NvmodInstance,
    profile: &str,
    now: i64,
    config: &Config,
) -> Result<Option<Manifest>, Error> {
    let newest = list(instance, profile)
        .into_iter()
        .find(|s| s.source == "nvmod" && s.problem.is_none())
        .map(|s| s.created_unix);
    let due = newest.is_none_or(|t| {
        now.saturating_sub(t) >= i64::try_from(config.interval_secs).unwrap_or(i64::MAX)
    });
    if due {
        create(instance, profile, now, false).map(Some)
    } else {
        Ok(None)
    }
}

/// Which of the folders a snapshot lives in existed before an automatic snapshot was taken.
#[derive(Debug, Clone, Copy)]
pub struct Presence {
    nvmod: bool,
    backups: bool,
}

/// Note which of `.nvmod` and `.nvmod/backups` exist now, to hand to [`discard`] later.
#[must_use]
pub fn presence(instance: &NvmodInstance) -> Presence {
    Presence {
        nvmod: instance.root.join(".nvmod").is_dir(),
        backups: instance.root.join(".nvmod").join("backups").is_dir(),
    }
}

/// Remove the snapshot `id`, and the folders that only it made (a failed command takes the automatic snapshot
/// it made back, so a command that fails leaves the Instance as it was).
pub fn discard(instance: &NvmodInstance, profile: &str, id: &str, before: Presence) {
    if id.is_empty() || id.contains(['/', '\\']) || id.starts_with('.') {
        return;
    }
    let _ = fs::remove_dir_all(backups_dir(instance, profile).join(id));
    let _ = fs::remove_dir(backups_dir(instance, profile));
    if !before.backups {
        let _ = fs::remove_dir(instance.root.join(".nvmod").join("backups"));
    }
    if !before.nvmod {
        let _ = fs::remove_dir(instance.root.join(".nvmod"));
    }
}
