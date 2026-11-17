// SPDX-License-Identifier: GPL-3.0-only
//! The saves of a profile.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
use nvmod_backup::clock;
use nvmod_profile::Target;
pub use nvmod_saves::{
    BackupReport, CheckReport, RestoreReport, SaveCheck, SaveInfo, TransferReport,
};
use nvmod_saves::{Error, Location};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

const LIST: &str = "saves.list";
const CHECK: &str = "saves.check";
const CHECKSUM: &str = "saves.checksum";
const BACKUP: &str = "saves.backup";
const RESTORE: &str = "saves.restore";
const TRANSFER: &str = "saves.transfer";

/// What [`list`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
    /// The Wine prefix whose `My Games` holds the saves of a profile without local saves; `None` for the
    /// Instance's `compatdata`.
    #[serde(default)]
    pub prefix: Option<PathBuf>,
}

/// The result of [`list`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListOutput {
    /// The profile.
    pub profile: String,
    /// The save folder.
    pub dir: PathBuf,
    /// Whether it is the profile's own `saves/` folder (local saves).
    pub local: bool,
    /// The saves, sorted by name, with the plugin count from each header.
    pub saves: Vec<SaveInfo>,
}

/// What [`check`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
    /// The Wine prefix whose `My Games` holds the saves of a profile without local saves; `None` for the
    /// Instance's `compatdata`.
    #[serde(default)]
    pub prefix: Option<PathBuf>,
}

/// The result of [`check`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckOutput {
    /// The profile.
    pub profile: String,
    /// The save folder.
    pub dir: PathBuf,
    /// Whether it is the profile's own `saves/` folder (local saves).
    pub local: bool,
    /// The report of checking every save against the active plugins.
    pub report: CheckReport,
    /// Whether every save is `ok`.
    pub clean: bool,
    /// Number of active plugins of the profile.
    pub active: usize,
    /// Every save.
    pub saves: Vec<SaveCheck>,
}

/// What [`checksum`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChecksumRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
    /// The Wine prefix whose `My Games` holds the saves of a profile without local saves; `None` for the
    /// Instance's `compatdata`.
    #[serde(default)]
    pub prefix: Option<PathBuf>,
}

/// The result of [`checksum`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChecksumOutput {
    /// The profile.
    pub profile: String,
    /// The save folder.
    pub dir: PathBuf,
    /// Whether it is the profile's own `saves/` folder (local saves).
    pub local: bool,
    /// The size and SHA-256 of every save file, by name.
    pub files: BTreeMap<String, (u64, String)>,
    /// The same sums, under the name the job uses.
    pub checksums: BTreeMap<String, (u64, String)>,
}

/// What [`backup`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BackupRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
    /// The Wine prefix whose `My Games` holds the saves of a profile without local saves; `None` for the
    /// Instance's `compatdata`.
    #[serde(default)]
    pub prefix: Option<PathBuf>,
}

/// The result of [`backup`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackupOutput {
    /// The profile.
    pub profile: String,
    /// The save folder.
    pub dir: PathBuf,
    /// Whether it is the profile's own `saves/` folder (local saves).
    pub local: bool,
    /// The backup that was written.
    pub backup: BackupReport,
    /// The backup id (its folder name).
    pub id: String,
    /// Files copied.
    pub files: usize,
    /// Bytes copied.
    pub bytes: u64,
    /// Symbolic links that were not followed or copied.
    pub skipped_links: usize,
}

/// What [`restore`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
    /// The Wine prefix whose `My Games` holds the saves of a profile without local saves; `None` for the
    /// Instance's `compatdata`.
    #[serde(default)]
    pub prefix: Option<PathBuf>,
    /// The backup id, or `latest`.
    pub id: String,
}

/// The result of [`restore`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestoreOutput {
    /// The profile.
    pub profile: String,
    /// The save folder.
    pub dir: PathBuf,
    /// Whether it is the profile's own `saves/` folder (local saves).
    pub local: bool,
    /// The backup restored.
    pub restored: String,
    /// The backup of the folder as it was before (restore it to undo); `None` when the folder was empty.
    pub pre_restore: Option<String>,
    /// Files written.
    pub files: usize,
    /// Files in the folder that the backup does not have; they were left alone.
    pub kept_extra: Vec<String>,
    /// The report of what was restored.
    pub report: RestoreReport,
}

/// What [`transfer`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransferRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile copied from.
    pub from: String,
    /// The profile copied to.
    pub to: String,
    /// The Wine prefix whose `My Games` holds the saves of a profile without local saves; `None` for the
    /// Instance's `compatdata`.
    #[serde(default)]
    pub prefix: Option<PathBuf>,
    /// Copy saves that have no `.nvse` co-save.
    #[serde(default)]
    pub force: bool,
}

/// The result of [`transfer`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransferOutput {
    /// Files written, saves and co-saves.
    pub copied: Vec<String>,
    /// Files that were already there with the same content.
    pub already_there: Vec<String>,
    /// Saves copied without a co-save (only with `force`).
    pub without_cosave: Vec<String>,
    /// The report of what was transferred.
    pub report: TransferReport,
}

/// The saves of a profile: its `saves/` folder with local saves on, else the prefix's. Reads only.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance, the profile cannot be chosen, or its saves are in a
/// prefix that was not found.
pub fn list(
    request: &ListRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<ListOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, false, LIST)?;
    // The profile first: a profile that cannot be chosen is refused before the prefix matters, as it always was.
    let profile = crate::profile(&instance, request.profile.as_deref(), LIST)?;
    let prefix = request
        .prefix
        .clone()
        .unwrap_or_else(|| instance.root.join("compatdata"));
    let target = Target::from_prefix(&prefix);
    let Location { dir, local } = nvmod_saves::locate(&instance, &profile, target.as_ref().ok())
        .map_err(|e| match (e, &target) {
            (Error::Refused(why), Err(no_prefix)) => {
                JobError::new(ErrorKind::Refused, LIST, format!("{why}; {no_prefix}"))
            }
            (Error::Refused(why), Ok(_)) => JobError::new(ErrorKind::Refused, LIST, why),
            (Error::Corrupt(why), _) => JobError::new(ErrorKind::Corrupt, LIST, why),
            (Error::Failed(why), _) => JobError::new(ErrorKind::Failed, LIST, why),
        })?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("read-saves", "saves.list.reading"));
    let saves = nvmod_saves::list(&dir);
    Ok(Outcome::Done(ListOutput {
        profile,
        dir,
        local,
        saves,
    }))
}

/// Check every save of a profile against its active plugins. Reads only.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance, the profile cannot be chosen, its saves are in a
/// prefix that was not found, or the profile has no `plugins.txt`.
pub fn check(
    request: &CheckRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<CheckOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, false, CHECK)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), CHECK)?;
    let prefix = request
        .prefix
        .clone()
        .unwrap_or_else(|| instance.root.join("compatdata"));
    let target = Target::from_prefix(&prefix);
    let Location { dir, local } = nvmod_saves::locate(&instance, &profile, target.as_ref().ok())
        .map_err(|e| match (e, &target) {
            (Error::Refused(why), Err(no_prefix)) => {
                JobError::new(ErrorKind::Refused, CHECK, format!("{why}; {no_prefix}"))
            }
            (Error::Refused(why), Ok(_)) => JobError::new(ErrorKind::Refused, CHECK, why),
            (Error::Corrupt(why), _) => JobError::new(ErrorKind::Corrupt, CHECK, why),
            (Error::Failed(why), _) => JobError::new(ErrorKind::Failed, CHECK, why),
        })?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("check-saves", "saves.check.checking"));
    let report = nvmod_saves::check(
        &instance,
        &profile,
        &Location {
            dir: dir.clone(),
            local,
        },
    )
    .map_err(|e| match e {
        Error::Refused(why) => JobError::new(ErrorKind::Refused, CHECK, why),
        Error::Corrupt(why) => JobError::new(ErrorKind::Corrupt, CHECK, why),
        Error::Failed(why) => JobError::new(ErrorKind::Failed, CHECK, why),
    })?;
    Ok(Outcome::Done(CheckOutput {
        profile: profile.clone(),
        dir: dir.clone(),
        local,
        clean: report.clean,
        active: report.active,
        saves: report.saves.clone(),
        report,
    }))
}

/// The size and SHA-256 of every save file of a profile. Reads only.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance, the profile cannot be chosen, or its saves are in a
/// prefix that was not found.
pub fn checksum(
    request: &ChecksumRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<ChecksumOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, false, CHECKSUM)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), CHECKSUM)?;
    let prefix = request
        .prefix
        .clone()
        .unwrap_or_else(|| instance.root.join("compatdata"));
    let target = Target::from_prefix(&prefix);
    let Location { dir, local } = nvmod_saves::locate(&instance, &profile, target.as_ref().ok())
        .map_err(|e| match (e, &target) {
            (Error::Refused(why), Err(no_prefix)) => {
                JobError::new(ErrorKind::Refused, CHECKSUM, format!("{why}; {no_prefix}"))
            }
            (Error::Refused(why), Ok(_)) => JobError::new(ErrorKind::Refused, CHECKSUM, why),
            (Error::Corrupt(why), _) => JobError::new(ErrorKind::Corrupt, CHECKSUM, why),
            (Error::Failed(why), _) => JobError::new(ErrorKind::Failed, CHECKSUM, why),
        })?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("read-saves", "saves.checksum.reading"));
    let sums = nvmod_saves::checksums(&dir);
    Ok(Outcome::Done(ChecksumOutput {
        profile,
        dir,
        local,
        files: sums.clone(),
        checksums: sums,
    }))
}

/// Back up the save folder of a profile. Writes a copy under `.nvmod/saves-backup/`.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance, the profile cannot be chosen, its saves are in a
/// prefix that was not found, or there is no save folder; [`ErrorKind::Failed`] when a copy fails.
pub fn backup(
    request: &BackupRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<BackupOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, true, BACKUP)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), BACKUP)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let prefix = request
        .prefix
        .clone()
        .unwrap_or_else(|| instance.root.join("compatdata"));
    let target = Target::from_prefix(&prefix);
    let Location { dir, local } = nvmod_saves::locate(&instance, &profile, target.as_ref().ok())
        .map_err(|e| match (e, &target) {
            (Error::Refused(why), Err(no_prefix)) => {
                JobError::new(ErrorKind::Refused, BACKUP, format!("{why}; {no_prefix}"))
            }
            (Error::Refused(why), Ok(_)) => JobError::new(ErrorKind::Refused, BACKUP, why),
            (Error::Corrupt(why), _) => JobError::new(ErrorKind::Corrupt, BACKUP, why),
            (Error::Failed(why), _) => JobError::new(ErrorKind::Failed, BACKUP, why),
        })?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("backup-saves", "saves.backup.backing-up"));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let report = nvmod_saves::backup(
        &instance,
        &profile,
        &Location {
            dir: dir.clone(),
            local,
        },
        clock::now(),
    )
    .map_err(|e| match e {
        Error::Refused(why) => JobError::new(ErrorKind::Refused, BACKUP, why),
        Error::Corrupt(why) => JobError::new(ErrorKind::Corrupt, BACKUP, why),
        Error::Failed(why) => JobError::new(ErrorKind::Failed, BACKUP, why),
    })?;
    Ok(Outcome::Done(BackupOutput {
        profile,
        dir,
        local,
        id: report.id.clone(),
        files: report.files,
        bytes: report.bytes,
        skipped_links: report.skipped_links,
        backup: report,
    }))
}

/// Restore a backup into the save folder of a profile.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance, the profile cannot be chosen, its saves are in a
/// prefix that was not found, the backup is unknown or of another profile, or the game is running;
/// [`ErrorKind::Corrupt`] when a file of the backup does not match its manifest; [`ErrorKind::Failed`] when a
/// write fails.
pub fn restore(
    request: &RestoreRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<RestoreOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, true, RESTORE)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), RESTORE)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let prefix = request
        .prefix
        .clone()
        .unwrap_or_else(|| instance.root.join("compatdata"));
    let target = Target::from_prefix(&prefix);
    let Location { dir, local } = nvmod_saves::locate(&instance, &profile, target.as_ref().ok())
        .map_err(|e| match (e, &target) {
            (Error::Refused(why), Err(no_prefix)) => {
                JobError::new(ErrorKind::Refused, RESTORE, format!("{why}; {no_prefix}"))
            }
            (Error::Refused(why), Ok(_)) => JobError::new(ErrorKind::Refused, RESTORE, why),
            (Error::Corrupt(why), _) => JobError::new(ErrorKind::Corrupt, RESTORE, why),
            (Error::Failed(why), _) => JobError::new(ErrorKind::Failed, RESTORE, why),
        })?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("restore-saves", "saves.restore.restoring"));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let report = nvmod_saves::restore(
        &instance,
        &profile,
        &Location {
            dir: dir.clone(),
            local,
        },
        &request.id,
        clock::now(),
    )
    .map_err(|e| match e {
        Error::Refused(why) => JobError::new(ErrorKind::Refused, RESTORE, why),
        Error::Corrupt(why) => JobError::new(ErrorKind::Corrupt, RESTORE, why),
        Error::Failed(why) => JobError::new(ErrorKind::Failed, RESTORE, why),
    })?;
    Ok(Outcome::Done(RestoreOutput {
        profile,
        dir,
        local,
        restored: report.restored.clone(),
        pre_restore: report.pre_restore.clone(),
        files: report.files,
        kept_extra: report.kept_extra.clone(),
        report,
    }))
}

/// Transfer the saves of one profile to another.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance, a profile cannot be chosen, its saves are in a
/// prefix that was not found, the folders are the same, there is nothing to copy, a co-save is missing (without
/// `force`) or a different file is in the way; [`ErrorKind::Failed`] when a copy fails.
pub fn transfer(
    request: &TransferRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<TransferOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, true, TRANSFER)?;
    for name in [&request.from, &request.to] {
        instance
            .resolve_profile(Some(name))
            .map_err(|e| JobError::new(ErrorKind::Refused, TRANSFER, e.to_string()))?;
    }
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let prefix = request
        .prefix
        .clone()
        .unwrap_or_else(|| instance.root.join("compatdata"));
    let target = Target::from_prefix(&prefix);
    let from_location = nvmod_saves::locate(&instance, &request.from, target.as_ref().ok())
        .map_err(|e| match (e, &target) {
            (Error::Refused(why), Err(no_prefix)) => {
                JobError::new(ErrorKind::Refused, TRANSFER, format!("{why}; {no_prefix}"))
            }
            (Error::Refused(why), Ok(_)) => JobError::new(ErrorKind::Refused, TRANSFER, why),
            (Error::Corrupt(why), _) => JobError::new(ErrorKind::Corrupt, TRANSFER, why),
            (Error::Failed(why), _) => JobError::new(ErrorKind::Failed, TRANSFER, why),
        })?;
    let to_location =
        nvmod_saves::locate(&instance, &request.to, target.as_ref().ok()).map_err(|e| {
            match (e, &target) {
                (Error::Refused(why), Err(no_prefix)) => {
                    JobError::new(ErrorKind::Refused, TRANSFER, format!("{why}; {no_prefix}"))
                }
                (Error::Refused(why), Ok(_)) => JobError::new(ErrorKind::Refused, TRANSFER, why),
                (Error::Corrupt(why), _) => JobError::new(ErrorKind::Corrupt, TRANSFER, why),
                (Error::Failed(why), _) => JobError::new(ErrorKind::Failed, TRANSFER, why),
            }
        })?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step(
        "transfer-saves",
        "saves.transfer.transferring",
    ));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let report = nvmod_saves::transfer(
        (&request.from, &from_location),
        (&request.to, &to_location),
        request.force,
    )
    .map_err(|e| match e {
        Error::Refused(why) => JobError::new(ErrorKind::Refused, TRANSFER, why),
        Error::Corrupt(why) => JobError::new(ErrorKind::Corrupt, TRANSFER, why),
        Error::Failed(why) => JobError::new(ErrorKind::Failed, TRANSFER, why),
    })?;
    Ok(Outcome::Done(TransferOutput {
        copied: report.copied.clone(),
        already_there: report.already_there.clone(),
        without_cosave: report.without_cosave.clone(),
        report,
    }))
}
