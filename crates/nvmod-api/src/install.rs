// SPDX-License-Identifier: GPL-3.0-only
//! Installing a Mod from an archive into a profile, and taking the last install back.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
pub use nvmod_journal::RollbackReport;
pub use nvmod_modinstall::LimitOverrides;
pub use nvmod_modinstall::Question;
pub use nvmod_modinstall::Report;
use nvmod_modinstall::{InstallError, Request as ModInstallRequest};
pub use nvmod_modinstall::{OnConflict, Placement};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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
    /// A JSON file with answers to the FOMOD's questions.
    #[serde(default)]
    pub answers: Option<PathBuf>,
    /// Answers given in memory (question id to the chosen option names); they replace the file's answer for the
    /// same group. This is how a caller that asked the questions step by step (see [`crate::fomod`]) gives the
    /// result back.
    #[serde(default)]
    pub fomod_answers: Option<BTreeMap<String, Vec<String>>>,
    /// What to do when a Mod folder of that name exists: refuse (the default), replace, merge or rename. The
    /// report's `mod` is the folder that was written, and its `conflict` says what happened.
    #[serde(default)]
    pub on_conflict: OnConflict,
    /// Where the new Mod goes in `modlist.txt`: the top (highest priority, the default), a position counted from
    /// the top, or right under a named entry.
    #[serde(default)]
    pub placement: Placement,
    /// The most bytes the extraction may write; `None` for the default limit.
    #[serde(default)]
    pub max_extract_bytes: Option<u64>,
    /// The other limits of the extraction (entries, bytes per entry, expansion ratio, folder depth, steps of work,
    /// seconds); a value left `None` keeps its default. `max_extract_bytes`, when set, wins over `max_bytes` here.
    #[serde(default)]
    pub extract_limits: LimitOverrides,
}

/// The result of [`install`].
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "kebab-case", tag = "outcome", content = "result")]
pub enum InstallOutput {
    /// The Mod was installed, or would be with a dry run.
    Installed(Report),
    /// The FOMOD needs answers before anything can be installed; nothing was written.
    Questions(Vec<Question>),
}

/// What [`reinstall`] and [`reconfigure`] need.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReinstallRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
    /// The Mod to install again, by folder name.
    pub r#mod: String,
    /// The archive, when it is not where the Mod's saved answers say it was installed from.
    #[serde(default)]
    pub archive: Option<PathBuf>,
    /// [`reconfigure`] only: answers that replace the saved answer of each group they name.
    #[serde(default)]
    pub answers: Option<BTreeMap<String, Vec<String>>>,
    /// Plan only: nothing is written.
    #[serde(default)]
    pub dry_run: bool,
    /// The most bytes the extraction may write; `None` for the default limit.
    #[serde(default)]
    pub max_extract_bytes: Option<u64>,
    /// The other limits of the extraction; see [`InstallRequest::extract_limits`].
    #[serde(default)]
    pub extract_limits: LimitOverrides,
}

fn replay(
    request: &ReinstallRequest,
    job: &'static str,
    run: fn(&nvmod_modinstall::ReinstallRequest) -> Result<Report, InstallError>,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<InstallOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, true, job)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), job)?;
    progress.report(&Event::step("install", "install.install.installing"));
    let mut overrides = request.extract_limits;
    if request.max_extract_bytes.is_some() {
        overrides.max_bytes = request.max_extract_bytes;
    }
    let inner = nvmod_modinstall::ReinstallRequest {
        instance: &instance,
        profile: &profile,
        r#mod: &request.r#mod,
        archive: request.archive.as_deref(),
        answers: request.answers.as_ref(),
        dry_run: request.dry_run,
        limits: nvmod_modinstall::Limits::with_overrides(&overrides),
    };
    match run(&inner) {
        Ok(report) => Ok(Outcome::Done(InstallOutput::Installed(report))),
        Err(InstallError::Questions(questions)) => {
            Ok(Outcome::Done(InstallOutput::Questions(questions)))
        }
        Err(InstallError::Refused(why)) => Err(JobError::new(ErrorKind::Refused, job, why)),
        Err(InstallError::Failed(why)) => Err(JobError::new(ErrorKind::Failed, job, why)),
    }
}

/// Install a Mod again from its archive with the FOMOD answers saved next to it
/// (`mods/<name>/.nvmod/fomod-answers.json`). The Mod folder is replaced; its priority, enabled state, Plugin
/// states, notes and categories stay.
///
/// # Errors
/// [`ErrorKind::Refused`] when the Mod has no saved answers or its archive is not there;
/// [`ErrorKind::Failed`] when a step failed and the Instance was put back.
pub fn reinstall(
    request: &ReinstallRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<InstallOutput>, JobError> {
    replay(
        request,
        "install.reinstall",
        nvmod_modinstall::reinstall,
        progress,
        cancel,
    )
}

/// Like [`reinstall`], with `request.answers` replacing the saved answer of each group they name. A group the
/// change opens up is a [`InstallOutput::Questions`] result and nothing is written; [`crate::fomod::next`] with
/// `reconfigure` set asks those step by step with the old choices ticked.
///
/// # Errors
/// As [`reinstall`].
pub fn reconfigure(
    request: &ReinstallRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<InstallOutput>, JobError> {
    replay(
        request,
        "install.reconfigure",
        nvmod_modinstall::reconfigure,
        progress,
        cancel,
    )
}

/// What [`rollback`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RollbackRequest {
    /// The Instance folder.
    pub instance: PathBuf,
}

/// Install `request.archive` into `request.profile` of the Instance, or plan to.
///
/// A FOMOD whose choices cannot be inferred is a result, not an error: [`InstallOutput::Questions`] carries the
/// questions and nothing was written, and a second call with `request.answers` completes the install.
///
/// # Errors
/// [`ErrorKind::Refused`] before anything was written; [`ErrorKind::Failed`] when a step failed and the Instance
/// was put back.
pub fn install(
    request: &InstallRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<InstallOutput>, JobError> {
    let _scope = cancel.scope();
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
    let mut overrides = request.extract_limits;
    if request.max_extract_bytes.is_some() {
        overrides.max_bytes = request.max_extract_bytes;
    }
    let limits = nvmod_modinstall::Limits::with_overrides(&overrides);
    let mod_request = ModInstallRequest {
        instance: &instance,
        profile: &profile,
        archive: &request.archive,
        name: request.name.as_deref(),
        dry_run: request.dry_run,
        answers: request.answers.as_deref(),
        answers_inline: request.fomod_answers.as_ref(),
        limits,
        on_conflict: request.on_conflict,
        placement: request.placement.clone(),
    };
    match nvmod_modinstall::install(&mod_request) {
        Ok(report) => Ok(Outcome::Done(InstallOutput::Installed(report))),
        Err(InstallError::Questions(questions)) => {
            Ok(Outcome::Done(InstallOutput::Questions(questions)))
        }
        Err(InstallError::Refused(why)) => Err(JobError::new(ErrorKind::Refused, INSTALL, why)),
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
    let _scope = cancel.scope();
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
    match nvmod_journal::rollback(&instance) {
        Ok(report) => Ok(Outcome::Done(report)),
        Err(nvmod_journal::RollbackError::Refused(why)) => {
            Err(JobError::new(ErrorKind::Refused, ROLLBACK, why))
        }
        Err(nvmod_journal::RollbackError::Failed(why)) => {
            Err(JobError::new(ErrorKind::Failed, ROLLBACK, why))
        }
    }
}
