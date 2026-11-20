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
const REMOVE: &str = "mods.remove";

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
    let _scope = cancel.scope();
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
    let _scope = cancel.scope();
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

/// What [`remove`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemoveRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    #[serde(default)]
    pub profile: Option<String>,
    /// The Mod, by folder name.
    pub name: String,
    /// Remove although active plugins of other Mods need its plugins.
    #[serde(default)]
    pub force: bool,
    /// Plan only: nothing is written.
    #[serde(default)]
    pub dry_run: bool,
}

/// The active plugins of other Mods that need a plugin the Mod `target` provides. Empty when the plugins cannot be
/// read.
#[must_use]
pub fn plugin_dependants(
    instance: &nvmod_instance::NvmodInstance,
    profile: &str,
    target: &str,
) -> Vec<String> {
    let Ok(list) = nvmod_plugins::list(instance, profile) else {
        return Vec::new();
    };
    let theirs: Vec<&str> = list
        .plugins
        .iter()
        .filter(|p| {
            p.provider
                .as_deref()
                .is_some_and(|m| m.eq_ignore_ascii_case(target))
        })
        .map(|p| p.name.as_str())
        .collect();
    list.plugins
        .iter()
        .filter(|p| p.active && !theirs.contains(&p.name.as_str()))
        .filter(|p| {
            p.masters
                .iter()
                .any(|m| theirs.iter().any(|t| t.eq_ignore_ascii_case(&m.name)))
        })
        .map(|p| p.name.clone())
        .collect()
}

/// Remove a Mod as `nvmod mods remove` does: the folder goes to `.nvmod/trash`, its line leaves `modlist.txt`, its
/// plugins leave `plugins.txt` and `loadorder.txt`, the profile is staged again when it was staged, and
/// `nvmod rollback` undoes it. The token is checked before the change starts; once the journaled change has begun
/// it runs to its end, so the Instance is never left half changed.
///
/// # Errors
/// [`ErrorKind::Refused`] before anything was written (no Instance or profile, an unknown Mod, active plugins of
/// other Mods need its plugins and `force` is not set, the game or Mod Organizer 2 is running); [`ErrorKind::Failed`]
/// when a step failed and the Instance was put back.
pub fn remove(
    request: &RemoveRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<nvmod_modinstall::lifecycle::Report>, JobError> {
    use nvmod_modinstall::lifecycle::{self, Action};
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, true, REMOVE)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), REMOVE)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    if !request.dry_run
        && let Some(name) = nvmod_backup::game_running()
            .or_else(|| nvmod_backup::process_running(&["ModOrganizer.exe"]))
    {
        return Err(JobError::new(
            ErrorKind::Refused,
            REMOVE,
            format!("`{name}` is running; close it first (a dry run is allowed)"),
        ));
    }
    let action = Action::Remove {
        force: request.force,
        dependants: plugin_dependants(&instance, &profile, &request.name),
    };
    progress.report(&Event::step("remove-mod", "mods.remove.removing"));
    lifecycle::apply(&instance, &profile, &request.name, &action, request.dry_run)
        .map(Outcome::Done)
        .map_err(|e| match e {
            ModsError::Refused(why) => JobError::new(ErrorKind::Refused, REMOVE, why),
            ModsError::Failed(why) => JobError::new(ErrorKind::Failed, REMOVE, why),
        })
}
