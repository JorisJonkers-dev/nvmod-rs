// SPDX-License-Identifier: GPL-3.0-only
//! The Doctor: the checks of `nvmod doctor` on a base copy, on an nvmod Instance or on a Mod Organizer 2 instance,
//! run in process. Read only: nothing is written.

use crate::verify::{default_fail_on, severity};
use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
use nvmod_catalog::Catalog;
pub use nvmod_core::Severity;
use nvmod_doctor::BaseReport;
use nvmod_formats::Mo2Instance;
use nvmod_instance::{InstanceError, NvmodInstance};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const CHECK: &str = "doctor.check";

/// The job name of a refusal to open an Instance whose schema is newer than this nvmod reads. The command line
/// prints that refusal on its error stream even with `--json`, where every other refusal goes to its output, so
/// the refusal names itself apart.
pub const OPEN_REFUSAL: &str = "doctor.open";

/// Whether `error` is the refusal to open an nvmod Instance whose schema is newer than this nvmod reads. The
/// command line sends that refusal to its error stream even with `--json`, so it is told apart here, in the module
/// that makes it, rather than by its job name at the call site.
#[must_use]
pub fn is_open_refusal(error: &JobError) -> bool {
    error.kind == ErrorKind::Refused && error.job == OPEN_REFUSAL
}

/// What the Doctor checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Target {
    /// A base copy of the game, read as it is.
    Base {
        /// The base folder.
        folder: PathBuf,
    },
    /// An Instance: an nvmod Instance, or else a Mod Organizer 2 instance read as it is.
    Instance {
        /// The Instance folder.
        folder: PathBuf,
        /// The profile, `None` for the only one there is (or the selected one of a Mod Organizer 2 instance).
        #[serde(default)]
        profile: Option<String>,
        /// The game folder a Mod Organizer 2 instance is checked against; not read for an nvmod Instance.
        #[serde(default)]
        game: Option<PathBuf>,
        /// The masterlist the Load order of an nvmod Instance is checked against; `None` checks the plugin headers
        /// only. Not read for a Mod Organizer 2 instance.
        #[serde(default)]
        masterlist: Option<PathBuf>,
        /// The catalogue the NVSE plugins of an nvmod Instance are checked against; `None` for the one shipped with
        /// nvmod. Not read for a Mod Organizer 2 instance, which is always checked against the shipped one.
        #[serde(default)]
        catalog: Option<PathBuf>,
    },
}

/// What [`check`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckRequest {
    /// The folder to check.
    pub target: Target,
    /// The severity at or above which a finding makes the check found a problem.
    #[serde(default = "default_fail_on", deserialize_with = "severity")]
    pub fail_on: Severity,
}

/// The report of a check. Serialises as the report it holds.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Report {
    /// A base copy.
    Base(BaseReport),
    /// An nvmod Instance: its base copy and the profile checked.
    Instance(BaseReport),
    /// A Mod Organizer 2 instance and its profile.
    Mo2(nvmod_doctor::Report),
}

impl Report {
    /// The most serious finding, `None` when there is none.
    #[must_use]
    pub fn worst(&self) -> Option<Severity> {
        match self {
            Self::Base(report) | Self::Instance(report) => report.worst(),
            Self::Mo2(report) => report.worst(),
        }
    }

    /// The report as plain text, the way `nvmod doctor` prints it.
    #[must_use]
    pub fn to_text(&self) -> String {
        match self {
            Self::Base(report) | Self::Instance(report) => report.to_text(),
            Self::Mo2(report) => report.to_text(),
        }
    }
}

/// The result of [`check`].
#[derive(Debug, Clone, Serialize)]
pub struct CheckOutput {
    /// Whether the Doctor found a problem at or above the request's `fail_on`.
    pub found: bool,
    /// The report.
    pub report: Report,
}

/// Run the Doctor on the folder of `request`. Nothing is written.
///
/// A check that finds a problem is a result, not an error: [`CheckOutput::found`] is `true` and the report says
/// what it is.
///
/// # Errors
/// [`ErrorKind::Refused`] for every refusal of a check that cannot run on its input: the base copy is refused or
/// is not a folder, the Instance's catalogue cannot be read or parsed, or the nvmod Instance has no such profile.
/// An nvmod Instance file newer than this nvmod reads is also [`ErrorKind::Refused`], under job [`OPEN_REFUSAL`]
/// (see [`is_open_refusal`]). [`ErrorKind::Failed`] when the Instance folder cannot be read as an nvmod Instance or
/// a Mod Organizer 2 instance, when a Mod Organizer 2 instance names no selected profile (and none is passed), or
/// when its profile cannot be loaded.
pub fn check(
    request: &CheckRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<CheckOutput>, JobError> {
    let _scope = cancel.scope();
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let report = match &request.target {
        Target::Base { folder } => {
            progress.report(&Event::step("check-base", "doctor.check.checking-base"));
            Report::Base(nvmod_doctor::check_base(folder).map_err(refused)?)
        }
        Target::Instance {
            folder,
            profile,
            masterlist,
            catalog,
            ..
        } if NvmodInstance::is_instance(folder) => {
            progress.report(&Event::step("open-instance", "doctor.check.opening"));
            let instance = open(folder)?;
            if cancel.is_cancelled() {
                return Ok(Outcome::Cancelled);
            }
            progress.report(&Event::step("read-catalog", "doctor.check.reading-catalog"));
            let catalog = load_catalog(catalog.as_deref())?;
            if cancel.is_cancelled() {
                return Ok(Outcome::Cancelled);
            }
            progress.report(&Event::step("check-instance", "doctor.check.checking"));
            Report::Instance(
                nvmod_doctor::check_instance(
                    &instance,
                    profile.as_deref(),
                    masterlist.as_deref(),
                    &catalog,
                )
                .map_err(refused)?,
            )
        }
        Target::Instance {
            folder,
            profile,
            game,
            ..
        } => {
            progress.report(&Event::step("open-instance", "doctor.check.opening"));
            let instance = Mo2Instance::open(folder).map_err(|why| failed(why.to_string()))?;
            let name = profile
                .clone()
                .or_else(|| instance.selected_profile.clone())
                .ok_or_else(|| {
                    failed(
                        "the instance names no selected profile; pass --profile <name>".to_owned(),
                    )
                })?;
            let loaded = instance
                .load_profile(&name)
                .map_err(|why| failed(why.to_string()))?;
            if cancel.is_cancelled() {
                return Ok(Outcome::Cancelled);
            }
            progress.report(&Event::step("check-instance", "doctor.check.checking"));
            Report::Mo2(nvmod_doctor::run(
                &instance,
                &loaded,
                game.as_deref(),
                &Catalog::builtin(),
            ))
        }
    };
    let found = report.worst().is_some_and(|worst| worst >= request.fail_on);
    Ok(Outcome::Done(CheckOutput { found, report }))
}

/// The refusal of a check that cannot run on its input.
fn refused(why: String) -> JobError {
    JobError::new(ErrorKind::Refused, CHECK, why)
}

/// The failure of a folder that cannot be read.
fn failed(why: String) -> JobError {
    JobError::new(ErrorKind::Failed, CHECK, why)
}

/// Open the nvmod Instance at `folder` read only. A newer schema is a refusal under [`OPEN_REFUSAL`].
fn open(folder: &Path) -> Result<NvmodInstance, JobError> {
    NvmodInstance::open_read_only(folder).map_err(|e| match e {
        InstanceError::Refused(why) => JobError::new(ErrorKind::Refused, OPEN_REFUSAL, why),
        other => failed(other.to_string()),
    })
}

/// The catalogue to check against: the one at `file`, or the one shipped with nvmod.
fn load_catalog(file: Option<&Path>) -> Result<Catalog, JobError> {
    let Some(file) = file else {
        return Ok(Catalog::builtin());
    };
    let text =
        std::fs::read_to_string(file).map_err(|e| refused(format!("{}: {e}", file.display())))?;
    Catalog::parse(&text).map_err(|e| refused(e.to_string()))
}
