// SPDX-License-Identifier: GPL-3.0-only
//! The staged tree job: the virtual Data folder of a profile, the file set `nvmod stage` builds and the layer that
//! provides each path, as `nvmod tree` shows it, run in process. Read only: nothing is written.
//!
//! Cancellation is honoured before the layers are read. Reading them is not interrupted once it has started.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
use nvmod_conflicts::{ConflictError, Tree};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const SHOW: &str = "tree.show";

/// What [`show`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TreeRequest {
    /// The nvmod Instance folder, read as it is: an older Instance file is read as migrated, not rewritten.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    #[serde(default)]
    pub profile: Option<String>,
    /// Only the paths at or under this folder, compared without case, `\` or `/`; `None` for every path.
    #[serde(default)]
    pub path: Option<String>,
}

/// The result of [`show`].
#[derive(Debug, Clone, Serialize)]
pub struct TreeOutput {
    /// The virtual Data folder: every path once, in path order, with the layer that provides it. The command prints this
    /// value alone as its JSON; the `tree` member exists only where the whole output is serialised.
    pub tree: Tree,
}

/// Read the virtual Data folder of the request's profile. Nothing is written.
///
/// A `path` that matches no path is a result with no entries, not a refusal.
///
/// Cancellation is honoured before the layers are read; reading them is not interrupted once it has started.
///
/// # Errors
/// [`ErrorKind::Refused`] for every refusal of a tree that cannot be made from its input: the folder cannot be
/// opened as an nvmod Instance (including an Instance file newer than this nvmod reads), the Instance has no such
/// profile, the profile or its `modlist.txt` cannot be read, or the base copy has no `Data` folder. Nothing else
/// fails: the job makes no other error kind.
pub fn show(
    request: &TreeRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<TreeOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("open-instance", "tree.show.opening"));
    let instance = crate::open(&request.instance, false, SHOW)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let profile_name = crate::profile(&instance, request.profile.as_deref(), SHOW)?;
    progress.report(&Event::step("read-layers", "tree.show.reading"));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let tree = nvmod_conflicts::tree(&instance, &profile_name)
        .map_err(|ConflictError::Refused(why)| JobError::new(ErrorKind::Refused, SHOW, why))?
        .under(request.path.as_deref().unwrap_or_default());
    Ok(Outcome::Done(TreeOutput { tree }))
}
