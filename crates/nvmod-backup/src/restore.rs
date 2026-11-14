// SPDX-License-Identifier: GPL-3.0-only
//! Restore a snapshot: verify it, snapshot the current state, write the files, stage again, and put the current
//! state back when a step fails. The journal is `restore.journal.json` beside the profile's snapshots.

use crate::{Error, Manifest, backups_dir, create, files, io, read_manifest, verify};
use nvmod_instance::NvmodInstance;
use serde::Serialize;
use std::fs;
use std::path::Path;

/// What a restore did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestoreReport {
    /// The snapshot restored.
    pub restored: String,
    /// The snapshot of the state before the restore; restoring it undoes this restore.
    pub pre_restore: String,
    /// Files written.
    pub files: usize,
    /// Files removed because the snapshot did not have them.
    pub removed: Vec<String>,
    /// Files in the Staging tree afterwards, `None` when the profile was not staged.
    pub staged_files: Option<usize>,
}

#[derive(Serialize)]
struct Journal<'a> {
    schema_version: u32,
    status: &'a str,
    restoring: &'a str,
    pre_restore: &'a str,
}

fn write_journal(
    instance: &NvmodInstance,
    profile: &str,
    status: &str,
    from: &str,
    pre: &str,
) -> Result<(), String> {
    let dir = backups_dir(instance, profile);
    let text = serde_json::to_string_pretty(&Journal {
        schema_version: 1,
        status,
        restoring: from,
        pre_restore: pre,
    })
    .map_err(|e| e.to_string())?;
    write_atomic(&dir.join("restore.journal.json"), text.as_bytes())
}

fn write_atomic(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = path.with_extension("nvmod-tmp");
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).map_err(|e| io(dir, &e))?;
    }
    fs::write(&tmp, bytes).map_err(|e| io(&tmp, &e))?;
    fs::rename(&tmp, path).map_err(|e| io(path, &e))
}

/// What a snapshot holds: its folder, and `(profile-relative path, source file)` for each file.
struct Source {
    files: Vec<(String, std::path::PathBuf)>,
    /// Remove tracked files of the profile that the snapshot lacks (nvmod's own snapshots only).
    exact: bool,
}

fn source_of(
    instance: &NvmodInstance,
    profile: &str,
    id: &str,
) -> Result<(Source, Option<Manifest>), Error> {
    if let Some(stamp) = id.strip_prefix("mo2-") {
        let dir = instance.profile_dir(profile);
        let mut found = Vec::new();
        for entry in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some((base, s)) = files::mo2_stamped(&name)
                && s == stamp
            {
                found.push((base.to_owned(), entry.path()));
            }
        }
        if found.is_empty() {
            return Err(Error::Refused(format!(
                "the profile has no MO2 backup `{id}`"
            )));
        }
        found.sort();
        return Ok((
            Source {
                files: found,
                exact: false,
            },
            None,
        ));
    }
    if id.is_empty() || id.contains(['/', '\\']) || id.starts_with('.') {
        return Err(Error::Refused(format!("`{id}` is not a snapshot id")));
    }
    let dir = backups_dir(instance, profile).join(id);
    if !dir.is_dir() {
        return Err(Error::Refused(format!(
            "the profile has no snapshot `{id}` (see `nvmod backup list`)"
        )));
    }
    let manifest = verify(instance, profile, id)?;
    let files = manifest
        .files
        .iter()
        .map(|f| (f.path.clone(), dir.join(&f.path)))
        .collect();
    Ok((Source { files, exact: true }, Some(manifest)))
}

/// Write the files of `source` into the profile folder (a temporary file and a rename each) and remove tracked
/// files the snapshot does not have. Returns the removed names.
fn apply(profile_dir: &Path, source: &Source, step: bool) -> Result<Vec<String>, String> {
    for (relative, from) in &source.files {
        if step {
            nvmod_core::cancel::step_point("backup-restore")?;
        }
        let bytes = fs::read(from).map_err(|e| io(from, &e))?;
        write_atomic(&profile_dir.join(relative), &bytes)?;
    }
    let mut removed = Vec::new();
    if source.exact {
        let keep: std::collections::BTreeSet<&str> =
            source.files.iter().map(|(r, _)| r.as_str()).collect();
        for entry in fs::read_dir(profile_dir).into_iter().flatten().flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if files::tracked(&name) && !keep.contains(name.as_str()) && entry.path().is_file() {
                fs::remove_file(entry.path()).map_err(|e| io(&entry.path(), &e))?;
                removed.push(name);
            }
        }
    }
    removed.sort();
    Ok(removed)
}

fn staged(instance: &NvmodInstance, profile: &str) -> bool {
    instance.staging_dir(profile).exists()
        || instance
            .root
            .join("staging")
            .join(format!("{profile}.json"))
            .exists()
}

fn restage(instance: &NvmodInstance, profile: &str) -> Result<usize, String> {
    nvmod_stage::stage(instance, profile, nvmod_stage::Options::default())
        .map(|r| r.manifest.files)
        .map_err(|e| match e {
            nvmod_stage::StageError::Refused(m) | nvmod_stage::StageError::Failed(m) => m,
        })
}

/// Restore the snapshot `id` of `profile` (an nvmod timestamp or an `mo2-<stamp>` id; `latest` is the newest of
/// nvmod's own). The caller refuses while the game runs.
///
/// # Errors
/// `Refused` for an unknown snapshot; `Corrupt` when a file of the snapshot does not match its manifest (nothing
/// is changed); `Failed` when a step failed, after the state before the restore was put back.
pub fn restore(
    instance: &NvmodInstance,
    profile: &str,
    id: &str,
    now: i64,
) -> Result<RestoreReport, Error> {
    let id = if id == "latest" {
        crate::list(instance, profile)
            .into_iter()
            .find(|s| s.source == "nvmod")
            .map(|s| s.id)
            .ok_or_else(|| Error::Refused("the profile has no snapshot yet".to_owned()))?
    } else {
        id.to_owned()
    };
    let (source, manifest) = source_of(instance, profile, &id)?;
    let profile_dir = instance.profile_dir(profile);
    let with_saves = manifest.as_ref().is_some_and(|m| m.with_saves);
    // The state before: itself a snapshot, so a restore is never a one way door.
    let pre = create(instance, profile, now, with_saves)?;
    let was_staged = staged(instance, profile);
    write_journal(instance, profile, "in-progress", &id, &pre.id).map_err(Error::Failed)?;
    let outcome = (|| -> Result<(Vec<String>, Option<usize>), String> {
        let removed = apply(&profile_dir, &source, true)?;
        nvmod_core::cancel::step_point("backup-stage")?;
        let staged_files = if was_staged {
            Some(restage(instance, profile)?)
        } else {
            None
        };
        Ok((removed, staged_files))
    })();
    match outcome {
        Ok((removed, staged_files)) => {
            write_journal(instance, profile, "complete", &id, &pre.id).map_err(Error::Failed)?;
            Ok(RestoreReport {
                restored: id,
                pre_restore: pre.id,
                files: source.files.len(),
                removed,
                staged_files,
            })
        }
        Err(why) => {
            let back = undo(instance, profile, &pre, was_staged);
            let status = if back.is_ok() {
                "rolled-back"
            } else {
                "failed"
            };
            let _ = write_journal(instance, profile, status, &id, &pre.id);
            Err(Error::Failed(match back {
                Ok(()) => format!("{why}; the profile was put back as it was"),
                Err(undo) => format!(
                    "{why}; putting the profile back failed too ({undo}); restore the snapshot `{}`",
                    pre.id
                ),
            }))
        }
    }
}

/// Put the profile back as the pre-restore snapshot `pre` has it.
fn undo(
    instance: &NvmodInstance,
    profile: &str,
    pre: &Manifest,
    was_staged: bool,
) -> Result<(), String> {
    let dir = backups_dir(instance, profile).join(&pre.id);
    let manifest = read_manifest(&dir).map_err(|e| format!("{e:?}"))?;
    let source = Source {
        files: manifest
            .files
            .iter()
            .map(|f| (f.path.clone(), dir.join(&f.path)))
            .collect(),
        exact: true,
    };
    apply(&instance.profile_dir(profile), &source, false)?;
    if was_staged {
        restage(instance, profile)?;
    }
    Ok(())
}
