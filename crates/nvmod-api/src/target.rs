// SPDX-License-Identifier: GPL-3.0-only
//! Choosing what an Instance runs on: the vanilla engine with xNVSE's DLLs, or nv-rs with them built in. The
//! Doctor reads the choice.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
pub use nvmod_core::Target;
use nvmod_instance::InstanceError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const SET: &str = "target.set";

/// What [`set`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// What the Instance should run on.
    pub target: Target,
}

/// The result of [`set`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetOutput {
    /// The Instance folder.
    pub instance: PathBuf,
    /// What the Instance now runs on.
    pub target: Target,
}

/// Set what the Instance at `request.instance` runs on and write `nvmod-instance.json` (to a temporary file, then
/// a rename).
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance or its file is newer than this nvmod reads, before
/// anything was written; [`ErrorKind::Failed`] when writing failed and the old file is still in place.
pub fn set(
    request: &SetRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<SetOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let mut instance = crate::open(&request.instance, true, SET)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("set-target", "target.set.setting"));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    match instance.set_target(request.target) {
        Ok(()) => Ok(Outcome::Done(SetOutput {
            instance: instance.root,
            target: request.target,
        })),
        Err(InstanceError::Io(why)) => Err(JobError::new(ErrorKind::Failed, SET, why)),
        Err(InstanceError::Refused(why) | InstanceError::Invalid(why)) => {
            Err(JobError::new(ErrorKind::Refused, SET, why))
        }
    }
}
