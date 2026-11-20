// SPDX-License-Identifier: GPL-3.0-only
//! Importing one Mod Organizer 2 profile into a new nvmod Instance. The MO2 instance is only read.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
pub use nvmod_core::Finding;
use nvmod_import::ImportError;
pub use nvmod_import::Report;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const IMPORT: &str = "import_mo2.import";

/// What [`import`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportRequest {
    /// The MO2 instance folder (read only).
    pub mo2: PathBuf,
    /// The folder of the new nvmod Instance (must not exist, must not lie inside the MO2 instance).
    pub instance: PathBuf,
    /// The base copy of the game the new Instance points at.
    pub base: PathBuf,
    /// The profile to import; MO2's selected profile (or the only one) when `None`.
    #[serde(default)]
    pub profile: Option<String>,
    /// Copy every file instead of hard-linking.
    #[serde(default)]
    pub copy_only: bool,
}

/// How an import ended.
#[derive(Debug, Clone, Serialize)]
pub enum ImportOutput {
    /// The profile was imported.
    Imported(Report),
    /// The profile files contradict each other; nothing was written.
    Disagreements(Vec<Finding>),
}

/// Import the MO2 profile of `request` into a new nvmod Instance.
///
/// # Errors
/// [`ErrorKind::Refused`] before anything was written; [`ErrorKind::Failed`] when a step failed and the new
/// Instance was removed again.
pub fn import(
    request: &ImportRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<ImportOutput>, JobError> {
    let _scope = cancel.scope();
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("import", "import_mo2.import.importing"));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let mo2_request = nvmod_import::Request {
        mo2: &request.mo2,
        instance: &request.instance,
        base: &request.base,
        profile: request.profile.as_deref(),
        copy_only: request.copy_only,
    };
    match nvmod_import::import(&mo2_request) {
        Ok(report) => Ok(Outcome::Done(ImportOutput::Imported(report))),
        Err(ImportError::Disagree(findings)) => {
            Ok(Outcome::Done(ImportOutput::Disagreements(findings)))
        }
        Err(ImportError::Refused(why)) => Err(JobError::new(ErrorKind::Refused, IMPORT, why)),
        Err(ImportError::Failed(why)) => Err(JobError::new(ErrorKind::Failed, IMPORT, why)),
    }
}
