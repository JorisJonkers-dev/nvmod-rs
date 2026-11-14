// SPDX-License-Identifier: GPL-3.0-only
//! Import of one Mod Organizer 2 profile into a new nvmod Instance.
//!
//! The MO2 instance is only read. The nvmod Instance is built in a new folder: the profile's `modlist.txt`,
//! `plugins.txt` and `loadorder.txt` are copied together, byte for byte (so `plugins.txt` keeps its Windows code
//! page), and every Mod folder the profile lists is mirrored with hard links, or copies where a link cannot be
//! made. When the three profile files disagree the import is refused and nothing is written.

use nvmod_core::{Finding, PROFILE_SCHEMA_VERSION, Profile, ProfileMod, Severity};
use nvmod_formats::{EntryKind, Mo2Error, Mo2Instance, Mo2Profile, ModState};
use nvmod_instance::{InstanceError as NvmodError, NvmodInstance, PROFILE_FILE};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

/// What to import.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    /// The MO2 instance folder (read only).
    pub mo2: &'a Path,
    /// The folder of the new nvmod Instance (must not exist, must not lie inside the MO2 instance).
    pub instance: &'a Path,
    /// The base copy of the game the new Instance points at.
    pub base: &'a Path,
    /// The profile to import; MO2's selected profile (or the only one) when `None`.
    pub profile: Option<&'a str>,
    /// Copy every file instead of hard-linking.
    pub copy_only: bool,
}

/// Why the import did not produce an Instance.
#[derive(Debug, PartialEq, Eq)]
pub enum ImportError {
    /// Refused before anything was written (exit code 2).
    Refused(String),
    /// The profile files contradict each other (exit code 1); nothing was written.
    Disagree(Vec<Finding>),
    /// A step failed; the new Instance was removed again (exit code 4).
    Failed(String),
}

/// The result of an import.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// The new Instance's folder.
    pub instance: String,
    /// The profile imported.
    pub profile: String,
    /// Enabled Mods in `modlist.txt`, highest priority first.
    pub enabled_mods: Vec<String>,
    /// Active plugins in `plugins.txt` order.
    pub plugins: Vec<String>,
    /// All plugins in `loadorder.txt` order.
    pub load_order: Vec<String>,
    /// Mod folders mirrored.
    pub mods: usize,
    /// Files hard-linked.
    pub linked: usize,
    /// Files copied.
    pub copied: usize,
    /// What could not be mapped or was left out, one finding each.
    pub unmapped: Vec<Finding>,
}

fn refuse(why: &str) -> ImportError {
    ImportError::Refused(why.to_owned())
}

fn map_mo2(e: &Mo2Error) -> ImportError {
    refuse(&e.to_string())
}

/// The Mod name that holds `plugin` at its top level, per Mod, case-insensitively.
fn holds_plugin(mo2: &Mo2Instance, name: &str, plugin: &str) -> bool {
    fs::read_dir(mo2.mod_dir(name)).is_ok_and(|dir| {
        dir.flatten()
            .any(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(plugin))
    })
}

/// Where the files of a profile contradict each other, one finding per contradiction. Empty when they agree.
///
/// The contradictions: a list that is missing, an active plugin that `loadorder.txt` does not list, active plugins
/// in another order than in `loadorder.txt` (one finding for each plugin that stands in another place), and an
/// active plugin that only a disabled Mod provides.
#[must_use]
pub fn disagreements(mo2: &Mo2Instance, profile: &Mo2Profile) -> Vec<Finding> {
    let mut out = Vec::new();
    let (Some(active), Some(order)) = (&profile.plugins, &profile.load_order) else {
        for (file, list) in [
            ("plugins.txt", &profile.plugins),
            ("loadorder.txt", &profile.load_order),
        ] {
            if list.is_none() {
                out.push(Finding::new(
                    "import.list-missing",
                    Severity::Error,
                    file,
                    format!("the MO2 profile has no {file}, so the plugin lists cannot be checked against each other"),
                ));
            }
        }
        return out;
    };
    let key = |name: &str| name.to_ascii_lowercase();
    let in_order: HashSet<String> = order.iter().map(|n| key(n)).collect();
    for name in active {
        if !in_order.contains(&key(name)) {
            out.push(Finding::new(
                "import.plugin-not-in-loadorder",
                Severity::Error,
                name,
                "plugins.txt activates this plugin, but loadorder.txt does not list it",
            ));
        }
    }
    let active_listed: Vec<&String> = active
        .iter()
        .filter(|n| in_order.contains(&key(n)))
        .collect();
    let active_keys: HashSet<String> = active_listed.iter().map(|n| key(n)).collect();
    let by_order: Vec<&String> = order
        .iter()
        .filter(|n| active_keys.contains(&key(n)))
        .collect();
    for (mine, theirs) in active_listed.iter().zip(&by_order) {
        if key(mine) != key(theirs) {
            out.push(Finding::new(
                "import.order-differs",
                Severity::Error,
                *mine,
                format!(
                    "plugins.txt and loadorder.txt order the active plugins differently here (`{mine}` against `{theirs}`)"
                ),
            ));
        }
    }
    let enabled: Vec<&str> = profile
        .modlist
        .entries
        .iter()
        .filter(|e| e.kind == EntryKind::Mod && e.state == ModState::Enabled)
        .map(|e| e.name.as_str())
        .collect();
    let disabled: Vec<&str> = profile
        .modlist
        .entries
        .iter()
        .filter(|e| e.kind == EntryKind::Mod && e.state == ModState::Disabled)
        .map(|e| e.name.as_str())
        .collect();
    for name in active {
        let from_enabled = enabled.iter().any(|m| holds_plugin(mo2, m, name));
        if !from_enabled && disabled.iter().any(|m| holds_plugin(mo2, m, name)) {
            out.push(Finding::new(
                "import.plugin-in-disabled-mod",
                Severity::Error,
                name,
                "plugins.txt activates this plugin, but only a disabled Mod provides it",
            ));
        }
    }
    out
}

fn mirror(
    from: &Path,
    to: &Path,
    copy_only: bool,
    counts: &mut (usize, usize),
) -> std::io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let (src, dst) = (entry.path(), to.join(entry.file_name()));
        let kind = entry.file_type()?;
        if kind.is_dir() {
            mirror(&src, &dst, copy_only, counts)?;
        } else if kind.is_file() {
            if !copy_only && fs::hard_link(&src, &dst).is_ok() {
                counts.0 += 1;
            } else {
                fs::copy(&src, &dst)?;
                counts.1 += 1;
            }
        }
        // Symbolic links and special files are not followed: they could lead outside the Mod.
    }
    Ok(())
}

fn io(path: &Path, e: &std::io::Error) -> String {
    format!("{}: {e}", path.display())
}

fn is_inside(inner: &Path, outer: &Path) -> bool {
    let abs = |p: &Path| {
        let mut head = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
        let mut tail = Vec::new();
        loop {
            if let Ok(real) = head.canonicalize() {
                return tail.iter().rev().fold(real, |acc, n| acc.join(n));
            }
            match (head.parent().map(Path::to_path_buf), head.file_name()) {
                (Some(parent), Some(name)) => {
                    tail.push(name.to_owned());
                    head = parent;
                }
                _ => return head,
            }
        }
    };
    abs(inner).starts_with(abs(outer))
}

fn pick_profile(mo2: &Mo2Instance, wanted: Option<&str>) -> Result<String, ImportError> {
    if let Some(name) = wanted {
        return Ok(name.to_owned());
    }
    let names = mo2.profile_names().map_err(|e| map_mo2(&e))?;
    match (&mo2.selected_profile, names.as_slice()) {
        (Some(sel), _) if names.contains(sel) => Ok(sel.clone()),
        (_, [only]) => Ok(only.clone()),
        (_, []) => Err(refuse("the MO2 instance has no profile")),
        _ => Err(refuse(&format!(
            "the MO2 instance has several profiles ({}); name one with --profile",
            names.join(", ")
        ))),
    }
}

/// Import the MO2 profile of `request` into a new nvmod Instance.
///
/// # Errors
/// `Refused` for a folder that is not an MO2 instance, a profile that is missing or ambiguous, an Instance folder
/// that exists or lies inside the MO2 instance, or an unusable base copy. `Disagree` when the profile files
/// contradict each other. `Failed` when writing fails; the new folder is removed again.
pub fn import(request: &Request<'_>) -> Result<Report, ImportError> {
    let mo2 = Mo2Instance::open(request.mo2).map_err(|e| map_mo2(&e))?;
    let name = pick_profile(&mo2, request.profile)?;
    let profile = mo2.load_profile(&name).map_err(|e| map_mo2(&e))?;
    let bad = disagreements(&mo2, &profile);
    if !bad.is_empty() {
        return Err(ImportError::Disagree(bad));
    }
    if is_inside(request.instance, request.mo2) {
        return Err(refuse(
            "the new Instance would lie inside the MO2 instance; choose another folder",
        ));
    }
    let instance = NvmodInstance::create(request.instance, request.base).map_err(|e| match e {
        NvmodError::Refused(why) => ImportError::Refused(why),
        other => ImportError::Failed(other.to_string()),
    })?;
    build(&mo2, &profile, &instance, request.copy_only).map_err(|why| {
        let _ = fs::remove_dir_all(&instance.root);
        ImportError::Failed(why)
    })
}

/// Write the Profile record `profile.json` next to the copied files: the Mods and the Load order as nvmod's
/// data model holds them.
fn write_record(profile: &Mo2Profile, dest: &Path) -> Result<(), String> {
    let record = Profile {
        schema_version: PROFILE_SCHEMA_VERSION,
        name: profile.name.clone(),
        mods: profile
            .modlist
            .entries
            .iter()
            .filter(|e| e.kind == EntryKind::Mod && e.state != ModState::Foreign)
            .map(|e| ProfileMod {
                name: e.name.clone(),
                enabled: e.state == ModState::Enabled,
            })
            .collect(),
        load_order: profile.load_order.clone().unwrap_or_default(),
        active_plugins: profile.plugins.clone().unwrap_or_default(),
    };
    let text = serde_json::to_string_pretty(&record).map_err(|e| e.to_string())?;
    let path = dest.join(PROFILE_FILE);
    fs::write(&path, text + "\n").map_err(|e| io(&path, &e))
}

fn build(
    mo2: &Mo2Instance,
    profile: &Mo2Profile,
    instance: &NvmodInstance,
    copy_only: bool,
) -> Result<Report, String> {
    let mut unmapped = Vec::new();
    let dest = instance.profile_dir(&profile.name);
    fs::create_dir_all(&dest).map_err(|e| io(&dest, &e))?;
    for file in ["modlist.txt", "plugins.txt", "loadorder.txt"] {
        let from = profile.dir.join(file);
        fs::copy(&from, dest.join(file)).map_err(|e| io(&from, &e))?;
    }
    write_record(profile, &dest)?;
    let others = fs::read_dir(&profile.dir).map_err(|e| io(&profile.dir, &e))?;
    let mut left: Vec<String> = others
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !["modlist.txt", "plugins.txt", "loadorder.txt"].contains(&n.as_str()))
        .collect();
    left.sort();
    for file in left {
        unmapped.push(Finding::new(
            "import.profile-file-not-imported",
            Severity::Info,
            file,
            "a profile file nvmod does not use yet; it stays in the MO2 instance",
        ));
    }
    let mut counts = (0, 0);
    let mut mods = 0;
    for entry in &profile.modlist.entries {
        nvmod_core::cancel::step_point("import-mod")?;
        if entry.state == ModState::Foreign {
            unmapped.push(Finding::new(
                "import.foreign-entry",
                Severity::Info,
                &entry.name,
                "an unmanaged MO2 entry (such as a DLC); it has no folder and is not imported",
            ));
            continue;
        }
        let from = mo2.mod_dir(&entry.name);
        if !from.is_dir() {
            unmapped.push(Finding::new(
                "import.mod-folder-missing",
                Severity::Warning,
                &entry.name,
                "listed in modlist.txt but has no folder in the MO2 mods folder; not imported",
            ));
            continue;
        }
        let to = instance.mod_dir(&entry.name);
        mirror(&from, &to, copy_only, &mut counts).map_err(|e| io(&to, &e))?;
        mods += 1;
    }
    if fs::read_dir(&mo2.overwrite_dir).is_ok_and(|mut d| d.next().is_some()) {
        unmapped.push(Finding::new(
            "import.overwrite-not-imported",
            Severity::Warning,
            "overwrite",
            "the MO2 overwrite folder holds files (generated output); move them into a Mod first if they matter",
        ));
    }
    if counts.1 > 0 && !copy_only {
        unmapped.push(Finding::new(
            "import.copied",
            Severity::Info,
            "mods",
            format!(
                "{} file(s) were copied because a hard link could not be made (another drive?)",
                counts.1
            ),
        ));
    }
    for issue in &mo2.issues {
        unmapped.push(Finding::new(
            "import.unmapped-path",
            Severity::Warning,
            "ModOrganizer.ini",
            issue.clone(),
        ));
    }
    Ok(Report {
        instance: instance.root.display().to_string(),
        profile: profile.name.clone(),
        enabled_mods: profile
            .modlist
            .entries
            .iter()
            .filter(|e| e.kind == EntryKind::Mod && e.state == ModState::Enabled)
            .map(|e| e.name.clone())
            .collect(),
        plugins: profile.plugins.clone().unwrap_or_default(),
        load_order: profile.load_order.clone().unwrap_or_default(),
        mods,
        linked: counts.0,
        copied: counts.1,
        unmapped,
    })
}
