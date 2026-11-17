// SPDX-License-Identifier: GPL-3.0-only
//! The saves of a profile.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
use nvmod_profile::Target;
pub use nvmod_saves::SaveInfo;
use nvmod_saves::{Error, Location};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const LIST: &str = "saves.list";

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
