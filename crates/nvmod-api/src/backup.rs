// SPDX-License-Identifier: GPL-3.0-only
//! Snapshots of a profile's lists and INIs: create, list, prune and restore.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
pub use nvmod_backup::{Config, Manifest, PruneReport, RestoreReport, Snapshot};
use nvmod_backup::{Error as BackupError, clock};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const CREATE: &str = "backup.create";
const LIST: &str = "backup.list";
const PRUNE: &str = "backup.prune";
const RESTORE: &str = "backup.restore";

/// What [`create`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
    /// Whether `saves/` is copied into the snapshot.
    #[serde(default)]
    pub with_saves: bool,
    /// The time to record, in seconds since the epoch; `None` reads the clock.
    #[serde(default)]
    pub now: Option<i64>,
}

/// The result of [`create`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateOutput {
    /// The profile.
    pub profile: String,
    /// The snapshot that was written.
    pub snapshot: Manifest,
}

/// What [`list`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
}

/// The result of [`list`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListOutput {
    /// The profile.
    pub profile: String,
    /// The snapshots, newest first.
    pub snapshots: Vec<Snapshot>,
}

/// What [`prune`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PruneRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
    /// Keep this many of the newest snapshots, when set; otherwise the Instance's configured value.
    #[serde(default)]
    pub keep: Option<usize>,
    /// Also keep the newest snapshot of each of the last this many UTC days, when set; otherwise the Instance's
    /// configured value.
    #[serde(default)]
    pub days: Option<usize>,
    /// Plan only: nothing is removed.
    #[serde(default)]
    pub dry_run: bool,
    /// The time to record, in seconds since the epoch; `None` reads the clock.
    #[serde(default)]
    pub now: Option<i64>,
}

/// The result of [`prune`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PruneOutput {
    /// The profile.
    pub profile: String,
    /// True when nothing was removed.
    pub dry_run: bool,
    /// Ids kept, newest first.
    pub kept: Vec<String>,
    /// Ids removed (or that would be), newest first.
    pub removed: Vec<String>,
    /// The same report, as `nvmod-backup` returns it.
    pub report: PruneReport,
}

/// What [`restore`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RestoreRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
    /// The snapshot id, or `latest`.
    pub id: String,
    /// The time to record, in seconds since the epoch; `None` reads the clock.
    #[serde(default)]
    pub now: Option<i64>,
}

/// The result of [`restore`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RestoreOutput {
    /// The profile.
    pub profile: String,
    /// The report of what was restored.
    pub report: RestoreReport,
    /// The snapshot restored.
    pub restored: String,
    /// The snapshot of the state before the restore.
    pub pre_restore: String,
    /// Files written.
    pub files: usize,
    /// Files removed because the snapshot did not have them.
    pub removed: Vec<String>,
    /// Files in the Staging tree afterwards, `None` when the profile was not staged.
    pub staged_files: Option<usize>,
}

/// Take a snapshot of `profile` at the current time.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance or the profile cannot be chosen;
/// [`ErrorKind::Failed`] when a step failed.
pub fn create(
    request: &CreateRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<CreateOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, true, CREATE)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), CREATE)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("create-snapshot", "backup.create.creating"));
    let now = request.now.unwrap_or_else(clock::now);
    let snapshot = nvmod_backup::create(&instance, &profile, now, request.with_saves).map_err(
        |e| match e {
            BackupError::Refused(why) => JobError::new(ErrorKind::Refused, CREATE, why),
            BackupError::Corrupt(why) => JobError::new(ErrorKind::Corrupt, CREATE, why),
            BackupError::Failed(why) => JobError::new(ErrorKind::Failed, CREATE, why),
        },
    )?;
    Ok(Outcome::Done(CreateOutput { profile, snapshot }))
}

/// The snapshots of `profile`, newest first.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance or the profile cannot be chosen.
pub fn list(
    request: &ListRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<ListOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("list-snapshots", "backup.list.listing"));
    let instance = crate::open(&request.instance, false, LIST)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), LIST)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let snapshots = nvmod_backup::list(&instance, &profile);
    Ok(Outcome::Done(ListOutput { profile, snapshots }))
}

/// Remove the snapshots that the retention policy does not keep.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance or the profile cannot be chosen;
/// [`ErrorKind::Failed`] when a snapshot folder cannot be removed.
pub fn prune(
    request: &PruneRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<PruneOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, true, PRUNE)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), PRUNE)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("prune-snapshots", "backup.prune.pruning"));
    let mut config = nvmod_backup::load_config(&instance);
    if let Some(n) = request.keep {
        config.keep_newest = n;
    }
    if let Some(n) = request.days {
        config.daily_days = u32::try_from(n).unwrap_or(u32::MAX);
    }
    let now = request.now.unwrap_or_else(clock::now);
    let report = nvmod_backup::prune(&instance, &profile, &config, now, request.dry_run).map_err(
        |e| match e {
            BackupError::Refused(why) => JobError::new(ErrorKind::Refused, PRUNE, why),
            BackupError::Corrupt(why) => JobError::new(ErrorKind::Corrupt, PRUNE, why),
            BackupError::Failed(why) => JobError::new(ErrorKind::Failed, PRUNE, why),
        },
    )?;
    Ok(Outcome::Done(PruneOutput {
        profile,
        dry_run: report.dry_run,
        kept: report.kept.clone(),
        removed: report.removed.clone(),
        report,
    }))
}

/// Restore the snapshot `id` of `profile`.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance, the profile cannot be chosen, the game is running
/// or the snapshot is unknown; [`ErrorKind::Corrupt`] when a file of the snapshot does not match its manifest;
/// [`ErrorKind::Failed`] when a step failed and the profile was put back.
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
    if let Some(name) = nvmod_backup::game_running() {
        return Err(JobError::new(
            ErrorKind::Refused,
            RESTORE,
            format!("the game is running (`{name}`); close it first"),
        ));
    }
    progress.report(&Event::step("restore-snapshot", "backup.restore.restoring"));
    let now = request.now.unwrap_or_else(clock::now);
    let report =
        nvmod_backup::restore(&instance, &profile, &request.id, now).map_err(|e| match e {
            BackupError::Refused(why) => JobError::new(ErrorKind::Refused, RESTORE, why),
            BackupError::Corrupt(why) => JobError::new(ErrorKind::Corrupt, RESTORE, why),
            BackupError::Failed(why) => JobError::new(ErrorKind::Failed, RESTORE, why),
        })?;
    Ok(Outcome::Done(RestoreOutput {
        profile,
        restored: report.restored.clone(),
        pre_restore: report.pre_restore.clone(),
        files: report.files,
        removed: report.removed.clone(),
        staged_files: report.staged_files,
        report,
    }))
}
