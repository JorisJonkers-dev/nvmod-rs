// SPDX-License-Identifier: GPL-3.0-only
//! The Mods of a profile: list them, enable or disable some, move one.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
pub use nvmod_modinstall::lifecycle::CategorisedRow;
use nvmod_modinstall::mods::{self as inner, ModsError};
pub use nvmod_modinstall::mods::{ModRow, Report};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const LIST: &str = "mods.list";
const CHANGE: &str = "mods.change";

/// What [`list`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
    /// Only the Mods that have this category id.
    #[serde(default)]
    pub category: Option<u32>,
}

/// The result of [`list`].
#[derive(Debug, Clone, Serialize)]
pub struct ListOutput {
    /// The profile that was listed.
    pub profile: String,
    /// Its Mods in priority order (1 is the highest and wins a file conflict), with their categories.
    pub mods: Vec<CategorisedRow>,
}

/// The Mods of a profile in priority order, with their categories. Reads only.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance or the profile cannot be chosen.
pub fn list(
    request: &ListRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<ListOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("read-modlist", "mods.list.reading"));
    let instance = crate::open(&request.instance, false, LIST)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), LIST)?;
    let mods = nvmod_modinstall::lifecycle::list_categorised(&instance, &profile, request.category);
    Ok(Outcome::Done(ListOutput { profile, mods }))
}

/// A place in the priority list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "at", content = "of")]
pub enum Place {
    /// The highest priority.
    Top,
    /// The lowest priority.
    Bottom,
    /// Just above (higher priority than) another entry.
    Before(String),
    /// Just below (lower priority than) another entry.
    After(String),
    /// This 1-based position, counted before the move.
    Position(usize),
}

/// What to change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "action")]
pub enum Change {
    /// Enable these Mods.
    Enable {
        /// The Mods, by folder name.
        mods: Vec<String>,
    },
    /// Disable these Mods.
    Disable {
        /// The Mods, by folder name.
        mods: Vec<String>,
    },
    /// Move one entry.
    Move {
        /// The entry to move.
        name: String,
        /// Where it goes.
        to: Place,
    },
}

impl Change {
    fn inner(&self) -> inner::Change {
        match self {
            Self::Enable { mods } => inner::Change::Enable(mods.clone()),
            Self::Disable { mods } => inner::Change::Disable(mods.clone()),
            Self::Move { name, to } => inner::Change::Move {
                name: name.clone(),
                to: match to {
                    Place::Top => inner::Place::Top,
                    Place::Bottom => inner::Place::Bottom,
                    Place::Before(other) => inner::Place::Before(other.clone()),
                    Place::After(other) => inner::Place::After(other.clone()),
                    Place::Position(at) => inner::Place::Position(*at),
                },
            },
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
    pub dry_run: bool,
}

/// Enable, disable or move Mods of a profile. Only `modlist.txt` changes; the profile is staged again when it was
/// staged, and `nvmod rollback` undoes the change. The token is checked before the change starts; once the
/// journaled write has begun it runs to its end, so the profile is never left half changed.
///
/// # Errors
/// [`ErrorKind::Refused`] before anything was written (no Instance, no profile, an unknown Mod, the game is
/// running); [`ErrorKind::Failed`] when a step failed and the Instance was put back.
pub fn change(
    request: &ChangeRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<Report>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, true, CHANGE)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), CHANGE)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("write-modlist", "mods.change.writing"));
    inner::apply(
        &instance,
        &profile,
        &request.change.inner(),
        request.dry_run,
    )
    .map(Outcome::Done)
    .map_err(|e| match e {
        ModsError::Refused(why) => JobError::new(ErrorKind::Refused, CHANGE, why),
        ModsError::Failed(why) => JobError::new(ErrorKind::Failed, CHANGE, why),
    })
}
