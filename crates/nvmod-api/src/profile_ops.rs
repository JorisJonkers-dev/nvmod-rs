// SPDX-License-Identifier: GPL-3.0-only
//! Managing the Profiles of an Instance: list them with their counts, copy, rename and delete one, select the active
//! one and set the per-Profile switches of `settings.ini`. Making a Profile is [`crate::profile::create`].
//!
//! nvmod records the switches; applying them when the game starts (local saves and INIs) is part of the launch.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
use nvmod_formats::modlist::{EntryKind, ModState, Modlist};
use nvmod_formats::plugins::parse_plugin_list;
use nvmod_instance::{NvmodInstance, find_ci};
use nvmod_modinstall::mods::ModsError;
pub use nvmod_modinstall::profiles::Report;
use nvmod_modinstall::profiles::{self as inner, Action};
use nvmod_profile::Settings;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const LIST: &str = "profile.list";
const CHANGE: &str = "profile.change";
const SELECT: &str = "profile.select";
const SET: &str = "profile.set";

/// The version of the layout of [`ListReport`] (`schemas/profile-list.v1.json`).
pub const LIST_SCHEMA_VERSION: u32 = 1;

/// What [`list`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRequest {
    /// The Instance folder.
    pub instance: PathBuf,
}

/// One Profile of an Instance.
#[allow(clippy::struct_excessive_bools)] // the Profile's independent switches, reported as such in JSON
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ProfileRow {
    /// The Profile's name (its folder name).
    pub name: String,
    /// Whether this is the active Profile.
    pub selected: bool,
    /// Mods listed and enabled in `modlist.txt` (separators and foreign entries are not counted).
    pub mods_enabled: usize,
    /// Mods listed in `modlist.txt`, enabled or not (separators and foreign entries are not counted).
    pub mods_total: usize,
    /// Plugins in `plugins.txt`: the active ones.
    pub plugins_active: usize,
    /// Plugins in `loadorder.txt`: all of them.
    pub plugins_total: usize,
    /// `LocalSettings` of `settings.ini`: the Profile holds its own game INIs.
    pub local_settings: bool,
    /// `LocalSaves` of `settings.ini`: the Profile's `saves/` folder is the save target.
    pub local_saves: bool,
    /// `AutomaticArchiveInvalidation` of `settings.ini`.
    pub archive_invalidation: bool,
}

/// `nvmod profile list --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ListReport {
    /// The version of this layout; see [`LIST_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// The active Profile, `null` when there are several and none was selected.
    pub selected: Option<String>,
    /// The Profiles, sorted by name.
    pub profiles: Vec<ProfileRow>,
}

/// The JSON Schema of [`ListReport`], as pretty JSON with a trailing newline (`schemas/profile-list.v1.json`).
///
/// # Panics
/// Never in practice: a generated schema always serialises.
#[must_use]
pub fn list_schema() -> String {
    let mut text = serde_json::to_string_pretty(&schemars::schema_for!(ListReport))
        .expect("a schema serialises");
    text.push('\n');
    text
}

fn read(path: Option<PathBuf>) -> Vec<u8> {
    path.and_then(|p| nvmod_formats::text::read_limited(&p).ok())
        .unwrap_or_default()
}

/// The Profiles of the Instance, with what each holds. Reads only.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance.
pub fn list(
    request: &ListRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<ListReport>, JobError> {
    let _scope = cancel.scope();
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, false, LIST)?;
    progress.report(&Event::step("list-profiles", "profile.list.reading"));
    let selected = instance.active_profile();
    let mut profiles = Vec::new();
    for name in instance.profiles() {
        if cancel.is_cancelled() {
            return Ok(Outcome::Cancelled);
        }
        let dir = instance.profile_dir(&name);
        let modlist = Modlist::parse(&nvmod_formats::text::decode(&read(find_ci(
            &dir,
            "modlist.txt",
        ))));
        let mods = modlist
            .entries
            .iter()
            .filter(|e| e.kind == EntryKind::Mod && e.state != ModState::Foreign);
        let settings = Settings::read(&dir);
        profiles.push(ProfileRow {
            selected: selected.as_deref() == Some(name.as_str()),
            mods_enabled: mods
                .clone()
                .filter(|e| e.state == ModState::Enabled)
                .count(),
            mods_total: mods.count(),
            plugins_active: parse_plugin_list(&read(find_ci(&dir, "plugins.txt"))).len(),
            plugins_total: parse_plugin_list(&read(find_ci(&dir, "loadorder.txt"))).len(),
            local_settings: settings.local_settings,
            local_saves: settings.local_saves,
            archive_invalidation: settings.archive_invalidation,
            name,
        });
    }
    Ok(Outcome::Done(ListReport {
        schema_version: LIST_SCHEMA_VERSION,
        selected,
        profiles,
    }))
}

/// Refuse while the game or Mod Organizer 2 runs, or another process holds the Instance lock (the game started
/// through `nvmod run` holds it for as long as it runs). The lock this process holds itself does not count.
fn refuse_when_in_use(instance: &NvmodInstance, job: &'static str) -> Result<(), JobError> {
    let refused = |why: String| Err(JobError::new(ErrorKind::Refused, job, why));
    if let Some(name) = nvmod_backup::game_running()
        .or_else(|| nvmod_backup::process_running(&["ModOrganizer.exe"]))
    {
        return refused(format!(
            "`{name}` is running; close it first (a dry run is allowed)"
        ));
    }
    if let Some(holder) = nvmod_run::InstanceLock::read(&instance.root)
        && holder.pid != std::process::id()
        && nvmod_run::pid_alive(holder.pid)
    {
        return refused(format!(
            "the Instance is in use by {holder}; wait for it to end (a dry run is allowed)"
        ));
    }
    Ok(())
}

/// What to do to a Profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum Change {
    /// Copy the lists and INIs, and `saves/` with `with_saves`, to a new Profile. Never the Mods.
    Copy {
        /// The new Profile's name.
        to: String,
        /// Copy `saves/` too.
        #[serde(default)]
        with_saves: bool,
    },
    /// Give the Profile a new name.
    Rename {
        /// The new name.
        to: String,
    },
    /// Move the Profile to `.nvmod/trash/`.
    Delete,
}

/// What [`change`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The Profile to act on.
    pub profile: String,
    /// The change.
    pub change: Change,
    /// Plan only: nothing is written.
    #[serde(default)]
    pub dry_run: bool,
}

/// Copy, rename or delete a Profile. Rename and delete refuse the selected Profile and one that a running program
/// uses; both are journaled, so `nvmod rollback` undoes them (a deleted Profile comes back from the trash). The
/// token is checked before the change starts; once the journaled change has begun it runs to its end.
///
/// # Errors
/// [`ErrorKind::Refused`] before anything was written (no Instance or Profile, an unusable or taken name, the
/// selected Profile, the game running); [`ErrorKind::Failed`] when a step failed and the Instance was put back.
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
    if !request.dry_run {
        refuse_when_in_use(&instance, CHANGE)?;
    }
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let action = match &request.change {
        Change::Copy { to, with_saves } => Action::Copy {
            to: to.clone(),
            with_saves: *with_saves,
        },
        Change::Rename { to } => Action::Rename { to: to.clone() },
        Change::Delete => Action::Delete,
    };
    progress.report(&Event::step("change-profile", "profile.change.writing"));
    inner::apply(&instance, &request.profile, &action, request.dry_run)
        .map(Outcome::Done)
        .map_err(|e| match e {
            ModsError::Refused(why) => JobError::new(ErrorKind::Refused, CHANGE, why),
            ModsError::Failed(why) => JobError::new(ErrorKind::Failed, CHANGE, why),
        })
}

/// What [`select`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The Profile to make active.
    pub profile: String,
}

/// The result of [`select`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SelectOutput {
    /// The Profile that is active now, as its folder is spelled.
    pub selected: String,
    /// The Profile that was active before, when there was one.
    pub previous: Option<String>,
    /// Whether `nvmod-instance.json` changed.
    pub changed: bool,
}

/// Record the active Profile in `nvmod-instance.json`. Only that file is written (temporary file, then a rename).
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance, it has no such Profile or a program uses the
/// Instance; [`ErrorKind::Failed`] when the file could not be written (the old one is still in place).
pub fn select(
    request: &SelectRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<SelectOutput>, JobError> {
    let _scope = cancel.scope();
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let mut instance = crate::open(&request.instance, true, SELECT)?;
    let wanted = request.profile.as_str();
    let names = instance.profiles();
    let name = names
        .iter()
        .find(|n| n.as_str() == wanted)
        .or_else(|| {
            let mut like = names.iter().filter(|n| n.eq_ignore_ascii_case(wanted));
            like.next().filter(|_| like.next().is_none())
        })
        .cloned()
        .ok_or_else(|| {
            JobError::new(
                ErrorKind::Refused,
                SELECT,
                format!("the Instance has no profile `{wanted}`"),
            )
        })?;
    let previous = instance.active_profile();
    if previous.as_deref() == Some(name.as_str())
        && instance.config.selected_profile.as_deref() == Some(name.as_str())
    {
        return Ok(Outcome::Done(SelectOutput {
            selected: name,
            previous,
            changed: false,
        }));
    }
    refuse_when_in_use(&instance, SELECT)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("select-profile", "profile.select.writing"));
    instance
        .set_selected_profile(Some(&name))
        .map_err(|e| JobError::new(ErrorKind::Failed, SELECT, e.to_string()))?;
    Ok(Outcome::Done(SelectOutput {
        selected: name,
        previous,
        changed: true,
    }))
}

/// What [`set`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SetRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The Profile, `None` for the active one.
    #[serde(default)]
    pub profile: Option<String>,
    /// `LocalSaves`, `LocalSettings` or `AutomaticArchiveInvalidation`, in any case.
    pub key: String,
    /// The new value.
    pub value: bool,
    /// Plan only: nothing is written.
    #[serde(default)]
    pub dry_run: bool,
}

/// The result of [`set`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SetOutput {
    /// The Profile.
    pub profile: String,
    /// The key, as `settings.ini` spells it.
    pub key: String,
    /// The value.
    pub value: bool,
    /// Whether `settings.ini` changed (or would, for a dry run).
    pub changed: bool,
    /// Nothing was written.
    pub dry_run: bool,
}

/// Set one per-Profile switch in the Profile's `settings.ini` with a line-preserving edit. Nothing is applied: the
/// launch places local saves and INIs.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance, the Profile is unknown, the key is not one of
/// [`nvmod_profile::settings::KEYS`] or a program uses the Instance; [`ErrorKind::Failed`] when the file could not
/// be written.
pub fn set(
    request: &SetRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<SetOutput>, JobError> {
    let _scope = cancel.scope();
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, true, SET)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), SET)?;
    let Some(key) = nvmod_profile::settings::known_key(&request.key) else {
        return Err(JobError::new(
            ErrorKind::Refused,
            SET,
            format!(
                "`{}` is not a profile setting (one of {})",
                request.key,
                nvmod_profile::settings::KEYS.join(", ")
            ),
        ));
    };
    let dir = instance.profile_dir(&profile);
    let out = |changed| SetOutput {
        profile: profile.clone(),
        key: key.to_owned(),
        value: request.value,
        changed,
        dry_run: request.dry_run,
    };
    if request.dry_run {
        let current = Settings::read(&dir);
        let now = match key {
            "LocalSaves" => current.local_saves,
            "LocalSettings" => current.local_settings,
            _ => current.archive_invalidation,
        };
        return Ok(Outcome::Done(out(now != request.value)));
    }
    refuse_when_in_use(&instance, SET)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("set-profile", "profile.set.writing"));
    Settings::set(&dir, key, request.value)
        .map(|changed| Outcome::Done(out(changed)))
        .map_err(|why| JobError::new(ErrorKind::Failed, SET, why))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checked_in_list_schema_is_the_generated_one() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas/profile-list.v1.json");
        if std::env::var_os("NVMOD_BLESS").is_some() {
            std::fs::write(&path, list_schema()).unwrap();
        }
        let checked_in = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            checked_in == list_schema(),
            "schemas/profile-list.v1.json differs from the generated schema; bump the schema version if the \
             layout changed, then run `NVMOD_BLESS=1 cargo test -p nvmod-api` to rewrite it"
        );
    }
}
