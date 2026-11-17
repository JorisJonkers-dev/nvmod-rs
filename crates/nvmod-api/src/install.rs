// SPDX-License-Identifier: GPL-3.0-only
//! Installing a Mod from an archive into a profile, and taking the last install back.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
pub use nvmod_modinstall::Report;
pub use nvmod_modinstall::RollbackReport;
use nvmod_modinstall::{InstallError, Request as ModInstallRequest};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const INSTALL: &str = "install.install";
const ROLLBACK: &str = "install.rollback";

/// What [`install`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
    /// The archive, or a folder to install as it is.
    pub archive: PathBuf,
    /// The Mod's folder name; the archive's file name without its extension when `None`.
    #[serde(default)]
    pub name: Option<String>,
    /// Plan only: nothing is written.
    #[serde(default)]
    pub dry_run: bool,
}

/// What [`rollback`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackRequest {
    /// The Instance folder.
    pub instance: PathBuf,
}

/// Install `request.archive` into `request.profile` of the Instance, or plan to.
///
/// # Errors
/// [`ErrorKind::Refused`] before anything was written; [`ErrorKind::Failed`] when a step failed and the Instance
/// was put back.
pub fn install(
    request: &InstallRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<Report>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, true, INSTALL)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), INSTALL)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("install", "install.install.installing"));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let mod_request = ModInstallRequest {
        instance: &instance,
        profile: &profile,
        archive: &request.archive,
        name: request.name.as_deref(),
        dry_run: request.dry_run,
        answers: None,
        extract_tool: None,
        limits: nvmod_modinstall::Limits::default(),
    };
    match nvmod_modinstall::install(&mod_request) {
        Ok(report) => Ok(Outcome::Done(report)),
        Err(InstallError::Refused(why)) => Err(JobError::new(ErrorKind::Refused, INSTALL, why)),
        Err(InstallError::Questions(_)) => Err(JobError::new(
            ErrorKind::Refused,
            INSTALL,
            "the FOMOD needs answers",
        )),
        Err(InstallError::Failed(why)) => Err(JobError::new(ErrorKind::Failed, INSTALL, why)),
    }
}

/// Undo the last install, sort or Mods change of the Instance.
///
/// # Errors
/// [`ErrorKind::Refused`] when there is nothing to undo; [`ErrorKind::Failed`] when a restore step failed.
pub fn rollback(
    request: &RollbackRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<RollbackReport>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, false, ROLLBACK)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("rollback", "install.rollback.rolling-back"));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    match nvmod_modinstall::rollback(&instance) {
        Ok(report) => Ok(Outcome::Done(report)),
        Err(nvmod_modinstall::RollbackError::Refused(why)) => {
            Err(JobError::new(ErrorKind::Refused, ROLLBACK, why))
        }
        Err(nvmod_modinstall::RollbackError::Failed(why)) => {
            Err(JobError::new(ErrorKind::Failed, ROLLBACK, why))
        }
    }
}
