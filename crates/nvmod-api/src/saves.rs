// SPDX-License-Identifier: GPL-3.0-only
//! The saves of a profile.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
use nvmod_profile::Target;
pub use nvmod_saves::{CheckReport, SaveCheck, SaveInfo};
use nvmod_saves::{Error, Location};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

const LIST: &str = "saves.list";
const CHECK: &str = "saves.check";
const CHECKSUM: &str = "saves.checksum";

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
