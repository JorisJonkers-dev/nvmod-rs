// SPDX-License-Identifier: GPL-3.0-only
//! Making an empty profile in an Instance: the three list files of MO2's format, with the base copy's masters as
//! the only plugins.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
use nvmod_instance::InstanceError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const CREATE: &str = "profile.create";

/// The name of the profile a request without one makes, as the command line defaults to.
pub const DEFAULT_NAME: &str = "Default";

/// What [`create`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile's name; [`DEFAULT_NAME`] when `None`.
    #[serde(default)]
    pub name: Option<String>,
}

/// The profile that was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateOutput {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The name of the profile that was made.
    pub profile: String,
}

/// Make the empty profile `request.name` in the Instance at `request.instance`.
///
/// The profile's `modlist.txt` holds only the comment header, and its `plugins.txt` and `loadorder.txt` hold the
/// masters of the base copy that the game loads: `FalloutNV.esm`, then the DLC and pack masters its `Data` folder
/// holds, in the game's order, all active. So `install`, `sort`, LOD planning and the Doctor have a profile to work
/// on that matches what the game runs.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance, the name is not usable or the Instance already has
/// that profile, before anything was written; [`ErrorKind::Failed`] when writing failed and the folder was removed
/// again.
pub fn create(
    request: &CreateRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<CreateOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, true, CREATE)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let name = request.name.as_deref().unwrap_or(DEFAULT_NAME);
    progress.report(&Event::step("create-profile", "profile.create.creating"));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    match instance.create_profile(name) {
        Ok(()) => Ok(Outcome::Done(CreateOutput {
            instance: instance.root,
            profile: name.to_owned(),
        })),
        Err(InstanceError::Refused(why) | InstanceError::Invalid(why)) => {
            Err(JobError::new(ErrorKind::Refused, CREATE, why))
        }
        Err(InstanceError::Io(why)) => Err(JobError::new(ErrorKind::Failed, CREATE, why)),
    }
}
