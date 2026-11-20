// SPDX-License-Identifier: GPL-3.0-only
//! `saves backup` and `saves restore`: real copies of the active save folder under
//! `.nvmod/saves-backup/<UTC time>/files/` with `manifest.json` (size and SHA-256 of every file). A restore
//! verifies the backup first, takes a backup of the current folder, and never deletes a file the backup lacks.

use crate::{Error, Location, copy_atomic, hash_file, io};
use nvmod_backup::clock;
use nvmod_instance::NvmodInstance;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const SCHEMA: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileEntry {
    path: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    schema_version: u32,
    id: String,
    profile: String,
    source: String,
    created: String,
    created_unix: i64,
    files: Vec<FileEntry>,
}

/// What `backup` made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackupReport {
    /// The backup id (its folder name).
    pub id: String,
    /// The folder of the backup.
    pub dir: String,
    /// Files copied.
    pub files: usize,
    /// Bytes copied.
    pub bytes: u64,
    /// Symbolic links that were not followed or copied.
    pub skipped_links: usize,
}

/// What `restore` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestoreReport {
    /// The backup restored.
    pub restored: String,
    /// The backup of the folder as it was before (restore it to undo); `None` when the folder was empty.
    pub pre_restore: Option<String>,
    /// Files written.
    pub files: usize,
    /// Files in the folder that the backup does not have; they were left alone.
    pub kept_extra: Vec<String>,
}

fn root(instance: &NvmodInstance) -> PathBuf {
    instance.root.join(".nvmod").join("saves-backup")
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, PathBuf)>, links: &mut usize) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        let Ok(meta) = fs::symlink_metadata(entry.path()) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            *links += 1;
        } else if meta.is_dir() {
            walk(&entry.path(), &format!("{name}/"), out, links);
        } else if meta.is_file() && !name.ends_with(".nvmod-tmp") {
            out.push((name, entry.path()));
        }
    }
}

fn unsafe_path(path: &str) -> bool {
    path.is_empty()
        || path.starts_with(['/', '\\'])
        || path.contains(':')
        || path.contains('\\')
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
}

/// Copy the save folder of `location` into a new backup.
///
/// # Errors
/// `Refused` when the save folder is not there; `Failed` when a copy fails (the partial backup is removed).
pub fn backup(
    instance: &NvmodInstance,
    profile: &str,
    location: &Location,
    now: i64,
) -> Result<BackupReport, Error> {
    if !location.dir.is_dir() {
        return Err(Error::Refused(format!(
            "there is no save folder at {}",
            location.dir.display()
        )));
    }
    let parent = root(instance);
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
    let mut found = Vec::new();
    let mut links = 0;
    walk(&location.dir, "", &mut found, &mut links);
    found.sort();
    let built = (|| -> Result<(Manifest, u64), String> {
        let mut entries = Vec::new();
        let mut bytes = 0;
        for (relative, source) in &found {
            nvmod_core::cancel::step_point("saves-backup")?;
            let (size, sha256) = copy_atomic(source, &partial.join("files").join(relative))?;
            bytes += size;
            entries.push(FileEntry {
                path: relative.clone(),
                size,
                sha256,
            });
        }
        let manifest = Manifest {
            schema_version: SCHEMA,
            id: id.clone(),
            profile: profile.to_owned(),
            source: location.dir.display().to_string(),
            created: clock::format_utc(now),
            created_unix: now,
            files: entries,
        };
        fs::create_dir_all(&partial).map_err(|e| io(&partial, &e))?;
        let text = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
        fs::write(partial.join("manifest.json"), text).map_err(|e| io(&partial, &e))?;
        fs::rename(&partial, parent.join(&id)).map_err(|e| io(&partial, &e))?;
        Ok((manifest, bytes))
    })();
    match built {
        Ok((manifest, bytes)) => Ok(BackupReport {
            dir: parent.join(&id).display().to_string(),
            id,
            files: manifest.files.len(),
            bytes,
            skipped_links: links,
        }),
        Err(why) => {
            let _ = fs::remove_dir_all(&partial);
            let _ = fs::remove_dir(&parent);
            Err(Error::Failed(why))
        }
    }
}

fn newest(instance: &NvmodInstance) -> Option<String> {
    let mut ids: Vec<String> = fs::read_dir(root(instance))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().join("manifest.json").is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    ids.sort();
    ids.pop()
}

fn read_manifest(dir: &Path) -> Result<Manifest, Error> {
    let path = dir.join("manifest.json");
    let text = fs::read_to_string(&path).map_err(|e| Error::Corrupt(io(&path, &e)))?;
    let manifest: Manifest = serde_json::from_str(&text)
        .map_err(|e| Error::Corrupt(format!("{}: {e}", path.display())))?;
    if manifest.schema_version != SCHEMA {
        return Err(Error::Refused(format!(
            "{} is schema {}, this nvmod reads {SCHEMA}",
            path.display(),
            manifest.schema_version
        )));
    }
    Ok(manifest)
}

/// Put the backup `id` (or `latest`) back into the save folder of `location`.
///
/// The order: find and verify the backup (`Corrupt` on any mismatch, nothing changes), refuse while the game
/// runs, back up the folder as it is, write each file (a temporary file and a rename). A file the backup lacks is
/// kept.
///
/// # Errors
/// `Refused` for an unknown id, another profile's backup or a running game; `Corrupt` for a damaged backup;
/// `Failed` when a write fails (the earlier backup of the folder is the way back).
pub fn restore(
    instance: &NvmodInstance,
    profile: &str,
    location: &Location,
    id: &str,
    now: i64,
) -> Result<RestoreReport, Error> {
    let id = if id == "latest" {
        newest(instance).ok_or_else(|| Error::Refused("there is no save backup yet".to_owned()))?
    } else {
        id.to_owned()
    };
    if id.is_empty() || id.contains(['/', '\\']) || id.starts_with('.') {
        return Err(Error::Refused(format!("`{id}` is not a backup id")));
    }
    let dir = root(instance).join(&id);
    if !dir.is_dir() {
        return Err(Error::Refused(format!("there is no save backup `{id}`")));
    }
    let manifest = read_manifest(&dir)?;
    if manifest.profile != profile {
        return Err(Error::Refused(format!(
            "the backup `{id}` is of profile `{}`, not `{profile}`",
            manifest.profile
        )));
    }
    for f in &manifest.files {
        if unsafe_path(&f.path) {
            return Err(Error::Corrupt(format!(
                "`{}` leaves the backup folder",
                f.path
            )));
        }
        let (size, sha) = hash_file(&dir.join("files").join(&f.path))
            .map_err(|why| Error::Corrupt(format!("{}: {why}", f.path)))?;
        if size != f.size || sha != f.sha256 {
            return Err(Error::Corrupt(format!(
                "{} does not match its hash",
                f.path
            )));
        }
    }
    if let Some(name) = nvmod_backup::game_running(&instance.root) {
        return Err(Error::Refused(format!(
            "the game is running (`{name}`); close it first"
        )));
    }
    let mut current = Vec::new();
    let mut links = 0;
    walk(&location.dir, "", &mut current, &mut links);
    let pre_restore = if current.is_empty() {
        None
    } else {
        Some(backup(instance, profile, location, now)?.id)
    };
    for f in &manifest.files {
        nvmod_core::cancel::step_point("saves-restore").map_err(Error::Failed)?;
        let (_, sha) = copy_atomic(
            &dir.join("files").join(&f.path),
            &location.dir.join(&f.path),
        )
        .map_err(Error::Failed)?;
        if sha != f.sha256 {
            return Err(Error::Failed(format!("{} was written wrongly", f.path)));
        }
    }
    let kept_extra = current
        .into_iter()
        .map(|(name, _)| name)
        .filter(|name| !manifest.files.iter().any(|f| &f.path == name))
        .collect();
    Ok(RestoreReport {
        restored: id,
        pre_restore,
        files: manifest.files.len(),
        kept_extra,
    })
}
