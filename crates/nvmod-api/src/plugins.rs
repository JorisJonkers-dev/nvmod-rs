// SPDX-License-Identifier: GPL-3.0-only
//! The plugin list of a profile: list it, and enable, disable, move, lock or unlock plugins.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
use nvmod_plugins::{self as inner, Edit as InnerEdit, Place as InnerPlace};
pub use nvmod_plugins::{EditReport, ListReport, PluginsError as InnerError};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const LIST: &str = "plugins.list";
const CHANGE: &str = "plugins.change";

/// What [`list`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
}

/// A place in the priority list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "at", content = "of")]
pub enum Place {
    /// First, after the forced primary plugins.
    Top,
    /// Last.
    Bottom,
    /// Right before this plugin.
    Before(String),
    /// Right after this plugin.
    After(String),
}

impl Place {
    fn inner(&self) -> InnerPlace {
        match self {
            Self::Top => InnerPlace::Top,
            Self::Bottom => InnerPlace::Bottom,
            Self::Before(name) => InnerPlace::Before(name.clone()),
            Self::After(name) => InnerPlace::After(name.clone()),
        }
    }
}

/// What to change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "action")]
pub enum Change {
    /// Activate plugins; with `with_masters` also the masters they need, in order.
    Enable {
        /// The plugins, by file name.
        #[serde(default)]
        names: Vec<String>,
        /// Enable missing masters too.
        #[serde(default)]
        with_masters: bool,
    },
    /// Deactivate plugins.
    Disable {
        /// The plugins, by file name.
        #[serde(default)]
        names: Vec<String>,
    },
    /// Move one plugin.
    Move {
        /// The plugin.
        name: String,
        /// Where it goes.
        to: Place,
    },
    /// Hold a plugin at a 1-based position (it is moved there now and kept there by `sort`).
    Lock {
        /// The plugin.
        name: String,
        /// The 1-based position.
        position: usize,
    },
    /// Remove a lock.
    Unlock {
        /// The plugin.
        name: String,
    },
}

impl Change {
    fn inner(&self) -> InnerEdit {
        match self {
            Self::Enable {
                names,
                with_masters,
            } => InnerEdit::Enable {
                names: names.clone(),
                with_masters: *with_masters,
            },
            Self::Disable { names } => InnerEdit::Disable(names.clone()),
            Self::Move { name, to } => InnerEdit::Move {
                name: name.clone(),
                to: to.inner(),
            },
            Self::Lock { name, position } => InnerEdit::Lock {
                name: name.clone(),
                position: *position,
            },
            Self::Unlock { name } => InnerEdit::Unlock(name.clone()),
        }
    }
}

/// What [`change`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
    /// The change.
    pub change: Change,
    /// Plan only: nothing is written.
    #[serde(default)]
    pub dry_run: bool,
}

fn failure(error: inner::PluginsError) -> JobError {
    match error {
        inner::PluginsError::Refused(why) => JobError::new(ErrorKind::Refused, CHANGE, why),
        inner::PluginsError::Failed(why) => JobError::new(ErrorKind::Failed, CHANGE, why),
    }
}

/// The plugin list of a profile with masters, providers and locks. Reads only.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance or the profile cannot be chosen.
pub fn list(
    request: &ListRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<ListReport>, JobError> {
    let _scope = cancel.scope();
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("read-plugins", "plugins.list.reading"));
    let instance = crate::open(&request.instance, false, LIST)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), LIST)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let report = inner::list(&instance, &profile).map_err(|e| match e {
        inner::PluginsError::Refused(why) => JobError::new(ErrorKind::Refused, LIST, why),
        inner::PluginsError::Failed(why) => JobError::new(ErrorKind::Failed, LIST, why),
    })?;
    Ok(Outcome::Done(report))
}

/// Enable, disable, move, lock or unlock plugins of a profile. `plugins.txt`, `loadorder.txt` (and
/// `lockedorder.txt` for a lock) change together; the profile is staged again when it was staged, and
/// `nvmod rollback` undoes the change. The token is checked before the change starts; once the journaled write
/// has begun it runs to its end, so the profile is never left half changed.
///
/// # Errors
/// [`ErrorKind::Refused`] before anything was written (no Instance, no profile, an unknown plugin, a missing
/// master, a forced or locked plugin, a master order that would break); [`ErrorKind::Failed`] when a step failed
/// and the Instance was put back.
pub fn change(
    request: &ChangeRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<EditReport>, JobError> {
    let _scope = cancel.scope();
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, true, CHANGE)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), CHANGE)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("write-plugins", "plugins.change.writing"));
    inner::apply(
        &instance,
        &profile,
        &request.change.inner(),
        request.dry_run,
    )
    .map(Outcome::Done)
    .map_err(failure)
}
