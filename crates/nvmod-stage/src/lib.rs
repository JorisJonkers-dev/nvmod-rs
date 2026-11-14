// SPDX-License-Identifier: GPL-3.0-only
//! The Staging tree: the merged view of the game's `Data` files and the active Mods, as a tree of lowercased paths
//! made of hard links (copies where links cannot be made). Built beside the Instance's mods, never inside the base
//! copy; the game executable is not part of it (nvmod never copies or shares it).
//!
//! Layout: `staging/<profile>/data/...` is the tree, `staging/<profile>.json` its manifest.

mod plan;

use nvmod_core::{Finding, Severity};
use nvmod_instance::{NvmodInstance, find_ci};
use plan::Plan;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub use plan::Collision;

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
    Ok(plan)
}

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
