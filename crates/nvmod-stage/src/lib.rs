// SPDX-License-Identifier: GPL-3.0-only
//! The Staging tree: the merged view of the game's `Data` files and the active Mods, as a tree of lowercased paths
//! made of hard links (copies where links cannot be made). Built beside the Instance's mods, never inside the base
//! copy; the game executable is not part of it (nvmod never copies or shares it).
//!
//! Layout: `staging/<profile>/data/...` is the tree, `staging/<profile>.json` its manifest.

mod plan;
pub mod stamp;

use nvmod_core::{Finding, Severity};
use nvmod_formats::plugins::parse_plugin_list;
use nvmod_formats::text::read_limited;
use nvmod_instance::{NvmodInstance, find_ci};
use plan::Plan;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub use plan::{Collision, is_mod_metadata, mod_files};

/// One plugin of a Load order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    /// The file name as the lists spell it.
    pub name: String,
    /// Listed in `plugins.txt`.
    pub active: bool,
}

/// The plugins of `profile_dir` in `loadorder.txt` order (or `plugins.txt` order when there is no
/// `loadorder.txt`), each marked active when `plugins.txt` lists it. Plugins only in `plugins.txt` follow at the end.
///
/// # Errors
/// The reason, when the profile has neither list or a list cannot be read.
pub fn read_slots(profile_dir: &Path) -> Result<Vec<Slot>, String> {
    let read = |file: &str| -> Result<Option<Vec<String>>, String> {
        let path = profile_dir.join(file);
        if !path.is_file() {
            return Ok(None);
        }
        read_limited(&path)
            .map(|bytes| Some(parse_plugin_list(&bytes)))
            .map_err(|e| format!("{}: {e}", path.display()))
    };
    let active = read("plugins.txt")?;
    let order = read("loadorder.txt")?;
    if active.is_none() && order.is_none() {
        return Err(format!(
            "{} has neither plugins.txt nor loadorder.txt",
            profile_dir.display()
        ));
    }
    let active = active.unwrap_or_default();
    let is_active = |name: &str| active.iter().any(|a| a.eq_ignore_ascii_case(name));
    let mut slots: Vec<Slot> = Vec::new();
    for name in order.unwrap_or_default().into_iter().chain(active.clone()) {
        if !slots.iter().any(|s| s.name.eq_ignore_ascii_case(&name)) {
            let active = is_active(&name);
            slots.push(Slot { name, active });
        }
    }
    Ok(slots)
}

/// The file of `name` in the `Data` folder of a Staging tree (paths are lowercase there), ignoring case.
#[must_use]
pub fn staged_file(data: &Path, name: &str) -> Option<PathBuf> {
    let direct = data.join(name.to_lowercase());
    if direct.is_file() {
        Some(direct)
    } else {
        find_ci(data, name).filter(|p| p.is_file())
    }
}

/// The Load order of `profile` when it has a list, for the file times of a new tree.
fn load_order(instance: &NvmodInstance, profile: &str) -> Result<Vec<Slot>, StageError> {
    let dir = instance.profile_dir(profile);
    if !dir.join("plugins.txt").is_file() && !dir.join("loadorder.txt").is_file() {
        return Ok(Vec::new());
    }
    read_slots(&dir).map_err(StageError::Refused)
}

/// Whether the plugin file times of the built tree of `profile` do not follow its `loadorder.txt`: what a game
/// that orders by file time would read as another order. `false` when there is no tree or no list.
#[must_use]
pub fn times_stale(instance: &NvmodInstance, profile: &str) -> bool {
    let data = instance.staging_dir(profile).join("data");
    data.is_dir()
        && load_order(instance, profile).is_ok_and(|order| stamp::times_stale(&data, &order))
}

/// How to build the tree.
#[derive(Debug, Clone, Copy, Default)]
pub struct Options {
    /// Never try a hard link; copy every file.
    pub copy_only: bool,
}

/// Why staging did not produce a tree.
#[derive(Debug, PartialEq, Eq)]
pub enum StageError {
    /// Refused before anything was written (exit code 2).
    Refused(String),
    /// A step failed; what was started has been removed and the old tree is untouched (exit code 4).
    Failed(String),
}

/// What was built, also stored as the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    /// Version of this file's layout.
    pub schema: u32,
    /// The profile staged.
    pub profile: String,
    /// Files in the tree.
    pub files: usize,
    /// Of those, hard links.
    pub linked: usize,
    /// Of those, copies.
    pub copied: usize,
    /// Files that a later Mod replaced.
    pub overridden: usize,
}

/// The result of one staging run.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// What was built.
    #[serde(flatten)]
    pub manifest: Manifest,
    /// The folder of the tree.
    pub tree: String,
    /// Mods with files that collide after lowercasing.
    pub collisions: Vec<Collision>,
    /// Things worth knowing, such as a listed Mod without a folder.
    pub findings: Vec<Finding>,
}

const SCHEMA: u32 = 1;

fn manifest_path(instance: &NvmodInstance, profile: &str) -> PathBuf {
    instance.staging_dir(profile).with_extension("json")
}

/// The manifest of the last staging run of `profile`, when there is a readable one.
#[must_use]
pub fn read_manifest(instance: &NvmodInstance, profile: &str) -> Option<Manifest> {
    let text = fs::read_to_string(manifest_path(instance, profile)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Read the plan: the base `Data` files, then the active Mods from the lowest priority to the highest.
fn plan_of(instance: &NvmodInstance, profile: &str) -> Result<Plan, StageError> {
    let refuse = |m: String| StageError::Refused(m);
    let list = instance.profile_dir(profile).join("modlist.txt");
    let bytes = nvmod_formats::text::read_limited(&list)
        .map_err(|e| refuse(format!("{}: {e}", list.display())))?;
    let modlist = nvmod_formats::Modlist::parse(&nvmod_formats::text::decode(&bytes));
    let data = find_ci(&instance.config.base, "Data")
        .filter(|p| p.is_dir())
        .ok_or_else(|| {
            refuse(format!(
                "the base copy {} has no Data folder",
                instance.config.base.display()
            ))
        })?;
    let mut plan = Plan::default();
    plan.add_dir("the base copy", &data, true);
    // modlist.txt lists the highest priority first; the Load order is the reverse, and the later Mod wins.
    let enabled: Vec<_> = modlist.enabled_mods().collect();
    for entry in enabled.iter().rev() {
        let dir = instance.mod_dir(&entry.name);
        if dir.is_dir() {
            plan.add_dir(&entry.name, &dir, false);
        } else {
            plan.findings.push(Finding::new(
                "stage.mod-folder-missing",
                Severity::Warning,
                entry.name.clone(),
                "listed in modlist.txt but has no folder in mods/; it is left out of the Staging tree",
            ));
        }
    }
    // Overwrite, the folder that captures what programs write, sits above every Mod.
    let overwrite = instance.root.join(OVERWRITE_DIR);
    if overwrite.is_dir() {
        plan.add_dir("Overwrite", &overwrite, false);
    }
    Ok(plan)
}

/// Folder of the Instance that holds Overwrite, the highest layer.
pub const OVERWRITE_DIR: &str = "overwrite";

/// Mods of `profile` whose files collide after lowercasing.
///
/// # Errors
/// `Refused` when the profile's `modlist.txt` or the base `Data` folder cannot be read.
pub fn collisions(instance: &NvmodInstance, profile: &str) -> Result<Vec<Collision>, StageError> {
    Ok(plan_of(instance, profile)?.collisions)
}

/// The Doctor's findings about the Staging tree of `profile`.
#[must_use]
pub fn doctor_findings(instance: &NvmodInstance, profile: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    match read_manifest(instance, profile) {
        None => findings.push(Finding::new(
            "stage.missing",
            Severity::Info,
            profile,
            "no Staging tree has been built for this profile; run `nvmod stage`",
        )),
        Some(m) if m.copied > 0 => findings.push(Finding::new(
            "stage.degraded",
            Severity::Warning,
            profile,
            format!(
                "{} of {} files in the Staging tree are copies, not hard links: it takes more space and does not follow edits of the mods",
                m.copied, m.files
            ),
        )),
        Some(_) => {}
    }
    for c in collisions(instance, profile).unwrap_or_default() {
        findings.push(Finding::new(
            "stage.case-collision",
            Severity::Warning,
            c.mod_name.clone(),
            format!(
                "files differ only in case ({}); the Staging tree keeps `{}`",
                c.paths.join(", "),
                c.kept
            ),
        ));
    }
    findings
}

/// Build the Staging tree of `profile`, replacing the old one only when the new one is complete.
///
/// # Errors
/// `Refused` when the profile or the base `Data` folder cannot be read; `Failed` when a file cannot be linked or
/// copied (the partial tree is removed and the old tree stays).
pub fn stage(
    instance: &NvmodInstance,
    profile: &str,
    options: Options,
) -> Result<Report, StageError> {
    if !instance.profile_dir(profile).is_dir() {
        return Err(StageError::Refused(format!(
            "the Instance has no profile `{profile}`"
        )));
    }
    let plan = plan_of(instance, profile)?;
    let final_dir = instance.staging_dir(profile);
    let partial = final_dir.with_extension("partial");
    let failed = |why: String| {
        StageError::Failed(format!(
            "{why}; rolled back, the old Staging tree is untouched"
        ))
    };
    let _ = fs::remove_dir_all(&partial);
    let (linked, copied) = match build(&plan, &partial.join("data"), options) {
        Ok(counts) => counts,
        Err(why) => {
            let _ = fs::remove_dir_all(&partial);
            return Err(failed(why));
        }
    };
    // The game orders plugins by file time: the new tree gets the times of the Load order, as `sort --apply`
    // gives them, so building the tree again never undoes a sort.
    let stamped = load_order(instance, profile).and_then(|order| {
        stamp::apply(&stamp::plan(&partial.join("data"), &order)).map_err(StageError::Failed)
    });
    if let Err(e) = stamped {
        let _ = fs::remove_dir_all(&partial);
        return Err(match e {
            StageError::Failed(why) => failed(why),
            refused @ StageError::Refused(_) => refused,
        });
    }
    let manifest = Manifest {
        schema: SCHEMA,
        profile: profile.to_owned(),
        files: plan.files.len(),
        linked,
        copied,
        overridden: plan.overridden,
    };
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| failed(e.to_string()))?;
    if let Err(why) = swap(
        &partial,
        &final_dir,
        &manifest_path(instance, profile),
        &text,
    ) {
        let _ = fs::remove_dir_all(&partial);
        return Err(failed(why));
    }
    Ok(Report {
        manifest,
        tree: final_dir.join("data").display().to_string(),
        collisions: plan.collisions,
        findings: plan.findings,
    })
}

fn swap(partial: &Path, final_dir: &Path, manifest: &Path, text: &str) -> Result<(), String> {
    let io = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
    if final_dir.exists() {
        fs::remove_dir_all(final_dir).map_err(|e| io(final_dir, e))?;
    }
    fs::rename(partial, final_dir).map_err(|e| io(final_dir, e))?;
    fs::write(manifest, text.to_owned() + "\n").map_err(|e| io(manifest, e))
}

/// Link or copy every file of the plan into `dest`. Returns (linked, copied).
fn build(plan: &Plan, dest: &Path, options: Options) -> Result<(usize, usize), String> {
    let (mut linked, mut copied) = (0, 0);
    fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    for (key, path) in &plan.files {
        let target = dest.join(key);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        if !options.copy_only && fs::hard_link(path, &target).is_ok() {
            linked += 1;
        } else {
            fs::copy(path, &target).map_err(|e| format!("{}: {e}", path.display()))?;
            copied += 1;
        }
    }
    Ok((linked, copied))
}
