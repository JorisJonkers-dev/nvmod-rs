// SPDX-License-Identifier: GPL-3.0-only
//! Building the Staging tree of a profile: the merged view of the game's `Data` files and the active Mods, as a
//! tree of lowercased paths made of hard links (copies where links cannot be made).

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
use nvmod_instance::NvmodInstance;
pub use nvmod_stage::Report;
use nvmod_stage::{Options, StageError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const STAGE: &str = "stage.stage";

/// What [`stage`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StageRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
    /// Never try a hard link; copy every file.
    #[serde(default)]
    pub copy: bool,
}

/// The result of [`stage`].
#[derive(Debug, Clone, Serialize)]
pub struct StageOutput {
    /// The profile that was staged.
    pub profile: String,
    /// The report of the run, as `nvmod-stage` returns it: the manifest of the tree, where the tree is, and what
    /// the run found.
    pub report: Report,
}

/// Build the Staging tree of `request.profile`, replacing the old one only when the new one is complete.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance, the profile cannot be chosen or the base copy has no
/// `Data` folder, before anything was written; [`ErrorKind::Failed`] when a file cannot be linked or copied, in
/// which case the partial tree was removed and the old tree is untouched.
pub fn stage(
    request: &StageRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<StageOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    // A folder that is not an Instance at all is refused in the words the command line uses; `open` would name the
    // Instance file it could not read instead.
    if !NvmodInstance::is_instance(&request.instance) {
        return Err(JobError::new(
            ErrorKind::Refused,
            STAGE,
            format!(
                "{} is not an nvmod Instance (made by `nvmod init`)",
                request.instance.display()
            ),
        ));
    }
    let instance = crate::open(&request.instance, true, STAGE)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), STAGE)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("build-tree", "stage.stage.building-tree"));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let options = Options {
        copy_only: request.copy,
    };
    let report = nvmod_stage::stage(&instance, &profile, options).map_err(|e| match e {
        StageError::Refused(why) => JobError::new(ErrorKind::Refused, STAGE, why),
        StageError::Failed(why) => JobError::new(ErrorKind::Failed, STAGE, why),
    })?;
    Ok(Outcome::Done(StageOutput { profile, report }))
}
