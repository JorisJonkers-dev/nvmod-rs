// SPDX-License-Identifier: GPL-3.0-only
//! Copying, renaming and deleting a Profile (`nvmod profile copy|rename|delete`).
//!
//! Each change is journaled as kind `profile` under `.nvmod/journal/<id>/`, so `nvmod rollback` undoes it: a copy
//! is removed again, a renamed Profile gets its old name back, and a deleted Profile (its folder moves to
//! `.nvmod/trash/<id>/profiles/`, nothing is deleted) comes back with every file. The Mods are never touched, and
//! the Profile that is active (see `NvmodInstance::active_profile`) can be neither renamed nor deleted.

use crate::journal::{self, Entry};
use crate::mods::ModsError;
use nvmod_instance::{NvmodInstance, PROFILE_FILE, check_profile_name};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// What to do to a Profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Copy the lists and INIs (and `saves/` with `with_saves`) to a new Profile.
    Copy {
        /// The new Profile's name.
        to: String,
        /// Copy the `saves/` folder too.
        with_saves: bool,
    },
    /// Give the Profile a new name.
    Rename {
        /// The new name.
        to: String,
    },
    /// Move the Profile to the trash.
    Delete,
}

impl Action {
    /// `copy`, `rename` or `delete`.
    #[must_use]
    pub fn name(&self) -> &'static str {
        match self {
            Action::Copy { .. } => "copy",
            Action::Rename { .. } => "rename",
            Action::Delete => "delete",
        }
    }
}

/// The result of a change or a dry run.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// `changed` or `dry-run`.
    pub status: &'static str,
    /// Nothing was written.
    pub dry_run: bool,
    /// `copy`, `rename` or `delete`.
    pub action: &'static str,
    /// The Profile acted on, as its folder is spelled.
    pub profile: String,
    /// The new Profile (copy and rename).
    pub new_profile: Option<String>,
    /// Files copied (copy only).
    pub files: Option<usize>,
    /// Where a deleted Profile went, relative to the Instance folder (delete only).
    pub trashed_to: Option<String>,
    /// Files in the Staging tree after a rename that had one.
    pub staged_files: Option<usize>,
    /// The journal id, for `nvmod rollback` (absent for a dry run).
    pub journal: Option<String>,
}

fn refuse<T>(why: impl Into<String>) -> Result<T, ModsError> {
    Err(ModsError::Refused(why.into()))
}

/// The folder name of `wanted`: an exact match, or the one that matches ignoring case.
fn resolve(instance: &NvmodInstance, wanted: &str) -> Result<String, ModsError> {
    let names = instance.profiles();
    if names.iter().any(|n| n == wanted) {
        return Ok(wanted.to_owned());
    }
    let mut like = names.iter().filter(|n| n.eq_ignore_ascii_case(wanted));
    match (like.next(), like.next()) {
        (Some(one), None) => Ok(one.clone()),
        (Some(_), Some(_)) => refuse(format!(
            "`{wanted}` matches several profiles that differ in case; spell it exactly"
        )),
        _ => refuse(format!("the Instance has no profile `{wanted}`")),
    }
}

/// The reason a Profile is in use by something outside the lists: a prefix it was placed in and not collected from.
fn placed_in_prefix(instance: &NvmodInstance, profile: &str) -> Option<String> {
    let root = instance.root.join(".nvmod").join("prefix");
    for entry in fs::read_dir(root).into_iter().flatten().flatten() {
        let Ok(text) = fs::read_to_string(entry.path().join("state.json")) else {
            continue;
        };
        let Ok(state) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        if state.get("profile").and_then(serde_json::Value::as_str) == Some(profile) {
            return Some(format!(
                "the files of `{profile}` are placed in a Wine prefix; run `nvmod profile finish` first"
            ));
        }
    }
    None
}

/// One file to copy: its path below the Profile folder, with `/` between the parts.
struct Item {
    rel: String,
}

fn collect(dir: &Path, rel: &str, with_saves: bool, out: &mut Vec<Item>) -> Result<(), ModsError> {
    let io = |p: &Path, e: std::io::Error| ModsError::Refused(format!("{}: {e}", p.display()));
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| io(dir, e))?
        .collect::<Result<_, _>>()
        .map_err(|e| io(dir, e))?;
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let here = if rel.is_empty() {
            name.clone()
        } else {
            format!("{rel}/{name}")
        };
        let kind = entry.file_type().map_err(|e| io(&entry.path(), e))?;
        if kind.is_file() {
            out.push(Item { rel: here });
        } else if kind.is_dir() {
            // Only `saves/` is part of a Profile besides its files; any other folder is left where it is.
            if with_saves && (!rel.is_empty() || name.eq_ignore_ascii_case("saves")) {
                collect(&entry.path(), &here, with_saves, out)?;
            }
        } else {
            return refuse(format!(
                "{} is a link or a special file; a Profile holds neither",
                entry.path().display()
            ));
        }
    }
    Ok(())
}

/// The profile record with `name` set, when `bytes` is one.
fn renamed_record(bytes: &[u8], name: &str) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(bytes).ok()?;
    let mut profile = nvmod_core::load_profile(text).ok()?;
    if profile.name == name {
        return None;
    }
    name.clone_into(&mut profile.name);
    let mut out = serde_json::to_string_pretty(&profile).ok()?;
    out.push('\n');
    Some(out.into_bytes())
}

fn new_name_free(instance: &NvmodInstance, from: &str, to: &str) -> Result<(), ModsError> {
    check_profile_name(to).map_err(ModsError::Refused)?;
    // A new spelling of the same name (`foo` to `Foo`) is not a clash with itself.
    let clash = instance
        .profiles()
        .into_iter()
        .any(|n| n.eq_ignore_ascii_case(to) && (n != from || from == to));
    if clash {
        return refuse(format!("the Instance already has a profile `{to}`"));
    }
    Ok(())
}

fn not_active(instance: &NvmodInstance, profile: &str, what: &str) -> Result<(), ModsError> {
    if instance.active_profile().as_deref() == Some(profile) {
        return refuse(format!(
            "`{profile}` is the selected profile and cannot be {what}; select another profile first"
        ));
    }
    placed_in_prefix(instance, profile).map_or(Ok(()), refuse)
}

fn io_err(p: &Path, e: &std::io::Error) -> String {
    format!("{}: {e}", p.display())
}

fn rel_profile(name: &str) -> String {
    format!("profiles/{name}")
}

fn move_to_trash(
    instance: &NvmodInstance,
    entry: &mut Entry,
    from: &str,
    kind: &str,
    name: &str,
) -> Result<String, String> {
    let to = format!(".nvmod/trash/{}/{kind}/{name}", entry.id);
    entry.record_move(instance, from, &to)?;
    let target = instance.root.join(&to);
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, &e))?;
    }
    fs::rename(instance.root.join(from), &target).map_err(|e| io_err(&target, &e))?;
    Ok(to)
}

/// Move the Staging tree of `profile` and its manifest to the trash, when they exist.
fn trash_staging(instance: &NvmodInstance, entry: &mut Entry, profile: &str) -> Result<(), String> {
    let tree = format!("staging/{profile}");
    if instance.root.join(&tree).exists() {
        move_to_trash(instance, entry, &tree, "staging", profile)?;
    }
    let manifest = format!("staging/{profile}.json");
    if instance.root.join(&manifest).exists() {
        move_to_trash(
            instance,
            entry,
            &manifest,
            "staging",
            &format!("{profile}.json"),
        )?;
    }
    Ok(())
}

fn copy_files(
    instance: &NvmodInstance,
    from: &str,
    to: &str,
    items: &[Item],
) -> Result<(), String> {
    let (src, dest) = (instance.profile_dir(from), instance.profile_dir(to));
    fs::create_dir_all(&dest).map_err(|e| io_err(&dest, &e))?;
    for item in items {
        nvmod_core::cancel::step_point("copy")?;
        let target: PathBuf = dest.join(&item.rel);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| io_err(parent, &e))?;
        }
        fs::copy(src.join(&item.rel), &target).map_err(|e| io_err(&target, &e))?;
    }
    if let Ok(bytes) = fs::read(dest.join(PROFILE_FILE))
        && let Some(new) = renamed_record(&bytes, to)
    {
        journal::write_atomic(&dest.join(PROFILE_FILE), &new)?;
    }
    Ok(())
}

struct Plan {
    profile: String,
    new_profile: Option<String>,
    items: Vec<Item>,
    staged: bool,
}

fn plan(instance: &NvmodInstance, wanted: &str, action: &Action) -> Result<Plan, ModsError> {
    let profile = resolve(instance, wanted)?;
    let mut plan = Plan {
        new_profile: None,
        items: Vec::new(),
        staged: journal::staging_exists(instance, &profile),
        profile,
    };
    match action {
        Action::Copy { to, with_saves } => {
            new_name_free(instance, "", to)?;
            collect(
                &instance.profile_dir(&plan.profile),
                "",
                *with_saves,
                &mut plan.items,
            )?;
            plan.new_profile = Some(to.clone());
        }
        Action::Rename { to } => {
            not_active(instance, &plan.profile, "renamed")?;
            new_name_free(instance, &plan.profile, to)?;
            plan.new_profile = Some(to.clone());
        }
        Action::Delete => not_active(instance, &plan.profile, "deleted")?,
    }
    Ok(plan)
}

/// Carry out `action` on the Profile `wanted`, or plan to.
///
/// The caller refuses while the game runs (a dry run is allowed), as `mods` does; this checks the Instance's own
/// state: the Profile exists, the new name is usable and free, the Profile is not the active one and is not placed
/// in a Wine prefix.
///
/// # Errors
/// [`ModsError::Refused`] before anything was written; [`ModsError::Failed`] when a step failed and the Instance
/// was put back.
pub fn apply(
    instance: &NvmodInstance,
    wanted: &str,
    action: &Action,
    dry_run: bool,
) -> Result<Report, ModsError> {
    let plan = plan(instance, wanted, action)?;
    let mut report = Report {
        status: if dry_run { "dry-run" } else { "changed" },
        dry_run,
        action: action.name(),
        profile: plan.profile.clone(),
        new_profile: plan.new_profile.clone(),
        files: matches!(action, Action::Copy { .. }).then_some(plan.items.len()),
        trashed_to: None,
        staged_files: None,
        journal: None,
    };
    if dry_run {
        return Ok(report);
    }
    // A copy's journal is about the new Profile, which does not exist yet; the others are about the old one.
    let journal_profile = match action {
        Action::Copy { to, .. } => to,
        _ => &plan.profile,
    };
    let mut entry = journal::begin_kind(instance, journal_profile, "", journal::KIND_PROFILE)
        .map_err(ModsError::Refused)?;
    let outcome = (|| -> Result<(Option<String>, Option<usize>), String> {
        crate::fail_point("profile")?;
        match action {
            Action::Copy { to, .. } => {
                entry.record_create(instance, &rel_profile(to))?;
                copy_files(instance, &plan.profile, to, &plan.items)?;
                Ok((None, None))
            }
            Action::Delete => {
                trash_staging(instance, &mut entry, &plan.profile)?;
                let to = move_to_trash(
                    instance,
                    &mut entry,
                    &rel_profile(&plan.profile),
                    "profiles",
                    &plan.profile,
                )?;
                Ok((Some(to), None))
            }
            Action::Rename { to } => {
                trash_staging(instance, &mut entry, &plan.profile)?;
                let (old, new) = (rel_profile(&plan.profile), rel_profile(to));
                entry.record_move(instance, &old, &new)?;
                fs::rename(instance.root.join(&old), instance.root.join(&new))
                    .map_err(|e| io_err(&instance.root.join(&new), &e))?;
                crate::fail_point("profile-record")?;
                let record = format!("{new}/{PROFILE_FILE}");
                if let Ok(bytes) = fs::read(instance.root.join(&record))
                    && let Some(renamed) = renamed_record(&bytes, to)
                {
                    entry.record_file(instance, &record)?;
                    journal::write_atomic(&instance.root.join(&record), &renamed)?;
                }
                if !plan.staged {
                    return Ok((None, None));
                }
                crate::fail_point("stage")?;
                entry.record_create(instance, &format!("staging/{to}"))?;
                entry.record_create(instance, &format!("staging/{to}.json"))?;
                nvmod_stage::stage(instance, to, nvmod_stage::Options::default())
                    .map(|s| (None, Some(s.manifest.files)))
                    .map_err(|e| match e {
                        nvmod_stage::StageError::Refused(m)
                        | nvmod_stage::StageError::Failed(m) => m,
                    })
            }
        }
    })();
    match outcome {
        Ok((trashed_to, staged_files)) => {
            journal::complete(instance, &entry).map_err(ModsError::Failed)?;
            report.trashed_to = trashed_to;
            report.staged_files = staged_files;
            report.journal = Some(entry.id);
            Ok(report)
        }
        Err(why) => match journal::undo(instance, &entry) {
            Ok(()) => Err(ModsError::Failed(format!(
                "{why}; the change was rolled back"
            ))),
            Err(undo) => Err(ModsError::Failed(format!(
                "{why}; the rollback failed too ({undo}); run `nvmod rollback`"
            ))),
        },
    }
}
