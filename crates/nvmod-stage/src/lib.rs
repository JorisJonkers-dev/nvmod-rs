// SPDX-License-Identifier: GPL-3.0-only
//! The Staging tree: the merged view of the game's `Data` files and the active Mods, as a tree of lowercased paths
//! made of hard links (copies where links cannot be made). Built beside the Instance's mods, never inside the base
//! copy; the game executable is not part of it (nvmod never copies or shares it).
//!
//! Layout: `staging/<profile>/data/...` is the tree, `staging/<profile>.json` its manifest.

pub mod glob;
mod incremental;
mod plan;
mod records;
pub mod stamp;
pub mod sweep;

use nvmod_core::{Finding, Severity};
use nvmod_formats::plugins::parse_plugin_list;
use nvmod_formats::text::read_limited;
use nvmod_instance::{NvmodInstance, find_ci};
use plan::Plan;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

pub use plan::{Collision, is_mod_metadata, mod_files};
pub use records::Record;
pub use sweep::PROTECT_FILE;

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
    /// Build the whole tree again. Without it a run changes only what differs from the last one, when the
    /// manifest of the last run allows it (same layout, same way of building, a record for every file).
    pub full: bool,
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
    /// What each file of the tree was when it was built, by lowercased path (schema 2). Only the file on disk has
    /// it: the [`Report`] of a run leaves it empty.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub records: BTreeMap<String, Record>,
    /// Built with `copy_only`.
    #[serde(default)]
    pub copy_only: bool,
    /// The protected globs the tree was built with.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub protect: Vec<String>,
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
    /// `full` (the whole tree was built) or `incremental` (only what differed was changed).
    pub mode: &'static str,
    /// File operations done: every file for a full build, the links, unlinks and replacements of an
    /// incremental run.
    pub operations: usize,
}

const SCHEMA: u32 = 2;

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
    plan_without(instance, profile, &[])
}

/// The plan of `profile` with the enabled Mods named in `without` (any case) left out.
fn plan_without(
    instance: &NvmodInstance,
    profile: &str,
    without: &[String],
) -> Result<Plan, StageError> {
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
    // A generated Mod (the Toolbox's far view, made for this Load order) wins over every other Mod wherever it
    // sits in the list: its far-view files belong together, and another Mod's file at one of its paths (a texture
    // pack's far-view atlas) would break the rest of it.
    let enabled: Vec<_> = modlist
        .enabled_mods()
        .filter(|entry| !without.iter().any(|w| w.eq_ignore_ascii_case(&entry.name)))
        .collect();
    let (generated, ordinary): (Vec<&nvmod_formats::ModlistEntry>, Vec<_>) =
        enabled.iter().rev().partition(|entry| {
            nvmod_formats::metaini::is_generated_mod_dir(&instance.mod_dir(&entry.name))
        });
    for entry in ordinary.into_iter().chain(generated.iter().copied()) {
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
    // A packed generated Mod keeps its files in an archive, and a loose file beats an archive in the game: loose
    // files of the base copy and of other Mods at the archive's paths are left out, so the archive's file is the
    // one the game reads.
    for entry in &generated {
        plan.give_way_to_archives(&entry.name, &instance.mod_dir(&entry.name));
    }
    // Overwrite, the folder that captures what programs write, sits above every Mod.
    let overwrite = instance.root.join(OVERWRITE_DIR);
    if overwrite.is_dir() {
        plan.add_dir("Overwrite", &overwrite, false);
    }
    Ok(plan)
}

/// What [`stage_without`] built.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WithoutReport {
    /// The profile.
    pub profile: String,
    /// The `Data` folder of the tree that was built.
    pub tree: PathBuf,
    /// The Mods that were left out, as `modlist.txt` spells them.
    pub left_out: Vec<String>,
    /// Files in the tree.
    pub files: usize,
    /// The active plugins whose file is in the tree, in Load order: the list a game on this tree loads. A plugin only
    /// a left-out Mod provided is not in it.
    pub plugins: Vec<String>,
}

/// Build the Staging tree of `profile` without the enabled Mods `without` into `dest` (the tree is `dest/data`),
/// for a picture or a test of the profile as it would be without them. The profile, its lists and its own Staging
/// tree are not touched; `dest` is replaced only when the new tree is complete.
///
/// # Errors
/// `Refused` when the profile cannot be read, `without` names a Mod the profile does not enable, or `dest` is the
/// profile's own Staging tree or lies inside the Instance's `mods` or base copy; `Failed` when a file cannot be
/// linked or copied (the partial tree is removed and an old tree at `dest` stays).
pub fn stage_without(
    instance: &NvmodInstance,
    profile: &str,
    without: &[String],
    dest: &Path,
    options: Options,
) -> Result<WithoutReport, StageError> {
    let refuse = |m: String| Err(StageError::Refused(m));
    if !instance.profile_dir(profile).is_dir() {
        return refuse(format!("the Instance has no profile `{profile}`"));
    }
    // `a` is `b` or lies inside it, links followed where the path exists.
    let inside = |a: &Path, b: &Path| {
        let b = fs::canonicalize(b).unwrap_or_else(|_| b.to_path_buf());
        let mut probe = a.to_path_buf();
        loop {
            if fs::canonicalize(&probe).unwrap_or_else(|_| probe.clone()) == b {
                return true;
            }
            if !probe.pop() {
                return false;
            }
        }
    };
    let own = instance.staging_dir(profile);
    let base = &instance.config.base;
    if inside(dest, &own) || inside(dest, &instance.root.join("mods")) || inside(dest, base) {
        return refuse(format!(
            "{} is the profile's own Staging tree or inside the Instance's mods or base copy; pick another folder",
            dest.display()
        ));
    }
    // The folder is replaced whole: it must not hold the Instance or the base copy.
    if inside(&instance.root, dest) || inside(base, dest) {
        return refuse(format!(
            "{} holds the Instance or its base copy and would be replaced whole; pick an empty or new folder",
            dest.display()
        ));
    }
    let list = instance.profile_dir(profile).join("modlist.txt");
    let bytes =
        read_limited(&list).map_err(|e| StageError::Refused(format!("{}: {e}", list.display())))?;
    let modlist = nvmod_formats::Modlist::parse(&nvmod_formats::text::decode(&bytes));
    let mut left_out = Vec::new();
    for name in without {
        match modlist
            .enabled_mods()
            .find(|e| e.name.eq_ignore_ascii_case(name))
        {
            Some(entry) => left_out.push(entry.name.clone()),
            None => {
                return refuse(format!(
                    "profile `{profile}` does not enable a Mod `{name}`"
                ));
            }
        }
    }
    let plan = plan_without(instance, profile, without)?;
    let protect = sweep::protected_globs(instance, profile);
    let partial = dest.with_extension("partial");
    let _ = fs::remove_dir_all(&partial);
    let failed = |why: String| {
        let _ = fs::remove_dir_all(&partial);
        StageError::Failed(format!(
            "{why}; rolled back, {} is untouched",
            dest.display()
        ))
    };
    build(&plan, &partial.join("data"), options, &protect).map_err(failed)?;
    let order: Vec<Slot> = load_order(instance, profile)?
        .into_iter()
        .filter(|slot| staged_file(&partial.join("data"), &slot.name).is_some())
        .collect();
    stamp::apply(&stamp::plan(&partial.join("data"), &order)).map_err(failed)?;
    swap(&partial, dest).map_err(failed)?;
    Ok(WithoutReport {
        profile: profile.to_owned(),
        tree: dest.join("data"),
        left_out,
        files: plan.files.len(),
        plugins: order
            .into_iter()
            .filter(|s| s.active)
            .map(|s| s.name)
            .collect(),
    })
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
    if incremental::journal_path(instance, profile).is_file() {
        findings.push(Finding::new(
            "stage.interrupted",
            Severity::Warning,
            profile,
            "a staging run stopped half way, so the Staging tree may be part old and part new; run `nvmod stage`, which puts it right",
        ));
    }
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
    if let Ok(changes) = sweep::scan(instance, profile)
        && !changes.is_empty()
    {
        findings.push(Finding::new(
            "stage.unswept",
            Severity::Warning,
            profile,
            format!(
                "{} path(s) of the Staging tree differ from what was built (a program wrote, replaced or removed them); run `nvmod sweep` to take the output into Overwrite and put the Mods' files back",
                changes.len()
            ),
        ));
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
    let failed = |why: String| {
        StageError::Failed(format!(
            "{why}; rolled back, the old Staging tree is untouched"
        ))
    };
    // A run that was killed half way left a journal: put the tree back first.
    incremental::recover(instance, profile).map_err(|why| {
        StageError::Failed(format!(
            "the last run did not finish and could not be undone: {why}"
        ))
    })?;
    let protect = sweep::protected_globs(instance, profile);
    let old = (!options.full)
        .then(|| {
            incremental::usable(
                instance,
                profile,
                read_manifest(instance, profile),
                options.copy_only,
                &protect,
            )
        })
        .flatten();
    let order = load_order(instance, profile)?;
    let (copied, operations, mode) = match old {
        Some(old) => {
            let applied = incremental::run(instance, profile, &plan, &old, &order, &protect)
                .map_err(failed)?;
            (applied.copied, applied.operations, "incremental")
        }
        None => (
            build_full(instance, profile, &plan, options, &protect, &order).map_err(
                |e| match e {
                    StageError::Failed(why) => failed(why),
                    refused @ StageError::Refused(_) => refused,
                },
            )?,
            plan.files.len(),
            "full",
        ),
    };
    let final_dir = instance.staging_dir(profile);
    let records = plan
        .owners
        .iter()
        .filter_map(|(key, (provider, source))| {
            let from = plan.files.get(key)?;
            Record::of(
                &final_dir.join("data").join(key),
                from,
                provider,
                source,
                !copied.contains(key),
            )
            .map(|record| (key.clone(), record))
        })
        .collect();
    let mut manifest = Manifest {
        schema: SCHEMA,
        profile: profile.to_owned(),
        files: plan.files.len(),
        linked: plan.files.len() - copied.len(),
        copied: copied.len(),
        overridden: plan.overridden,
        records,
        copy_only: options.copy_only,
        protect,
    };
    let text = serde_json::to_string_pretty(&manifest).map_err(|e| failed(e.to_string()))?;
    write_atomic(&manifest_path(instance, profile), &(text + "\n")).map_err(failed)?;
    manifest.records.clear();
    Ok(Report {
        manifest,
        tree: final_dir.join("data").display().to_string(),
        collisions: plan.collisions,
        findings: plan.findings,
        mode,
        operations,
    })
}

/// Build the whole tree beside the old one and swap it in. Returns the keys that are copies.
fn build_full(
    instance: &NvmodInstance,
    profile: &str,
    plan: &Plan,
    options: Options,
    protect: &[String],
    order: &[Slot],
) -> Result<std::collections::BTreeSet<String>, StageError> {
    let final_dir = instance.staging_dir(profile);
    let partial = final_dir.with_extension("partial");
    let _ = fs::remove_dir_all(&partial);
    let cleanup = |why: String| {
        let _ = fs::remove_dir_all(&partial);
        StageError::Failed(why)
    };
    let copied = build(plan, &partial.join("data"), options, protect).map_err(cleanup)?;
    // The game orders plugins by file time: the new tree gets the times of the Load order, as `sort --apply`
    // gives them, so building the tree again never undoes a sort.
    stamp::apply(&stamp::plan(&partial.join("data"), order)).map_err(cleanup)?;
    swap(&partial, &final_dir).map_err(cleanup)?;
    Ok(copied)
}

fn write_atomic(path: &Path, text: &str) -> Result<(), String> {
    let io = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, text).map_err(|e| io(&tmp, e))?;
    fs::rename(&tmp, path).map_err(|e| io(path, e))
}

fn swap(partial: &Path, final_dir: &Path) -> Result<(), String> {
    let io = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
    if final_dir.exists() {
        fs::remove_dir_all(final_dir).map_err(|e| io(final_dir, e))?;
    }
    fs::rename(partial, final_dir).map_err(|e| io(final_dir, e))
}

/// Link or copy every file of the plan into `dest`. Returns the keys that are copies.
fn build(
    plan: &Plan,
    dest: &Path,
    options: Options,
    protect: &[String],
) -> Result<std::collections::BTreeSet<String>, String> {
    let mut copied = std::collections::BTreeSet::new();
    fs::create_dir_all(dest).map_err(|e| format!("{}: {e}", dest.display()))?;
    for (key, path) in &plan.files {
        let target = dest.join(key);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
        }
        let protected = protect.iter().any(|g| glob::selects(g, key));
        if options.copy_only || protected || fs::hard_link(path, &target).is_err() {
            fs::copy(path, &target).map_err(|e| format!("{}: {e}", path.display()))?;
            copied.insert(key.clone());
        }
    }
    Ok(copied)
}
