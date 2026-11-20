// SPDX-License-Identifier: GPL-3.0-only
//! Making an Instance skeleton for a Steam or GOG base copy. The base copy is only read.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
use nvmod_instance::{InstanceError, NvmodInstance};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const INIT: &str = "init.create";

/// What [`create`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateRequest {
    /// Where the Instance goes.
    pub instance: PathBuf,
    /// The base copy of the game, which is only read.
    pub base: PathBuf,
}

/// The Instance that was made.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CreateOutput {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The storefront the base copy came from.
    pub storefront: String,
    /// The base copy the Instance was made for.
    pub base: PathBuf,
}

/// Make an Instance at `request.instance` for the base copy at `request.base`.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folders do not allow it, before anything was written; [`ErrorKind::Failed`] when
/// a step failed.
pub fn create(
    request: &CreateRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<CreateOutput>, JobError> {
    let _scope = cancel.scope();
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("create-instance", "init.create.creating"));
    match NvmodInstance::create(&request.instance, &request.base) {
        Ok(instance) => Ok(Outcome::Done(CreateOutput {
            instance: instance.root.clone(),
            storefront: instance.config.storefront.to_string(),
            base: instance.config.base.clone(),
        })),
        Err(InstanceError::Refused(why)) => Err(JobError::new(ErrorKind::Refused, INIT, why)),
        Err(why) => Err(JobError::new(ErrorKind::Failed, INIT, why.to_string())),
    }
}
