// SPDX-License-Identifier: GPL-3.0-only
//! Installs a Mod from an archive into an nvmod Instance, and takes the last install back.
//!
//! The steps: extract into a scratch folder (refusing anything that escapes it), work out which files go in (a
//! plain archive as it is; a FOMOD from its `ModuleConfig.xml`, asking only what cannot be inferred), then, with
//! a journal and a backup of the three profile files written first, place the Mod in `mods/`, add it enabled at
//! the highest priority to `modlist.txt`, activate its plugins in `plugins.txt` and `loadorder.txt`, and stage
//! the profile. A failed step rolls everything back; [`rollback`] does the same on request.

pub mod extract;
pub mod fomod;
pub use nvmod_journal as journal;
pub mod lifecycle;
mod lists;
pub mod mods;
pub mod profiles;
pub mod saved;
pub mod session;

use fomod::{Answers, Env, FileState};
use nvmod_formats::{EntryKind, ModState, Modlist};
use nvmod_instance::{NvmodInstance, find_ci};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub use extract::{LimitOverrides, Limits};
pub use fomod::{Decided, GroupEvent, OptionEvent, StepEvent};
pub use fomod::{GroupKind, Inferred, PluginType, Question, QuestionOption};
pub use journal::{RollbackError, RollbackReport, rollback};
pub use saved::{ANSWERS_PATH, AnswersFile, ReinstallRequest, open_saved, reconfigure, reinstall};
pub use session::{FomodEvent, PlanEvent, Session, answers_schema, events_schema};

/// What to do when a Mod folder of the name already exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OnConflict {
    /// Refuse (exit code 2); nothing is written.
    #[default]
    Refuse,
    /// Replace the folder's content with the new Mod. The Mod keeps its place in `modlist.txt` (priority and
    /// enabled state), its Plugin entries, and its `meta.ini` (notes, categories, anything else in it); the
    /// old folder goes to the journal's trash, so a rollback brings it back.
    Replace,
    /// Put the new files into the existing folder, over files of the same path; other files stay. The Mod
    /// keeps its place and state as with [`OnConflict::Replace`].
    Merge,
    /// Install under the first free name `<name> 2`, `<name> 3`, ...
    Rename,
}

impl OnConflict {
    /// The word used on the command line and in reports: `refuse`, `replace`, `merge` or `rename`.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            OnConflict::Refuse => "refuse",
            OnConflict::Replace => "replace",
            OnConflict::Merge => "merge",
            OnConflict::Rename => "rename",
        }
    }

    /// The reverse of [`OnConflict::as_str`].
    #[must_use]
    pub fn parse(word: &str) -> Option<OnConflict> {
        [
            OnConflict::Refuse,
            OnConflict::Replace,
            OnConflict::Merge,
            OnConflict::Rename,
        ]
        .into_iter()
        .find(|c| c.as_str() == word)
    }
}

/// What to install.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    /// The Instance.
    pub instance: &'a NvmodInstance,
    /// The profile whose lists get the Mod.
    pub profile: &'a str,
    /// The archive, or a folder to install as it is.
    pub archive: &'a Path,
    /// The Mod's folder name; the archive's file name without its extension when `None`.
    pub name: Option<&'a str>,
    /// Plan only; write nothing.
    pub dry_run: bool,
    /// A JSON file with answers to the FOMOD's questions.
    pub answers: Option<&'a Path>,
    /// Answers given in memory; they replace the file's answer for the same group.
    pub answers_inline: Option<&'a Answers>,
    /// Size limits of the extraction.
    pub limits: Limits,
    /// What to do when the Mod's folder already exists.
    pub on_conflict: OnConflict,
}

/// Why nothing was installed.
#[derive(Debug)]
pub enum InstallError {
    /// Refused before anything was written (exit code 2).
    Refused(String),
    /// The FOMOD needs answers (exit code 1); nothing was written.
    Questions(Vec<Question>),
    /// A step failed and the Instance was put back as it was (exit code 4).
    Failed(String),
}

/// The result of an install or a dry run.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// `installed` or `dry-run`.
    pub status: &'static str,
    /// True for a dry run.
    pub dry_run: bool,
    /// The Mod's folder name.
    pub r#mod: String,
    /// The profile.
    pub profile: String,
    /// Files of the Mod, relative paths with `/`, sorted.
    pub files: Vec<String>,
    /// Plugins newly activated.
    pub plugins: Vec<String>,
    /// FOMOD choices made without asking.
    pub inferred: Vec<Inferred>,
    /// `replaced`, `merged` or `renamed` when a Mod of the name existed (`mod` is then the folder that was
    /// written), else `None`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conflict: Option<&'static str>,
    /// Files in the Staging tree afterwards (`None` for a dry run).
    pub staged_files: Option<usize>,
    /// The journal entry `nvmod rollback` uses (`None` for a dry run).
    pub journal: Option<String>,
}

fn refuse<T>(why: impl Into<String>) -> Result<T, InstallError> {
    Err(InstallError::Refused(why.into()))
}

/// Whether the file name ends in `.ext`, ignoring case.
fn has_extension(name: &str, ext: &str) -> bool {
    std::path::Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

/// A scratch folder, removed when dropped.
#[derive(Debug)]
struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Result<Scratch, String> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let dir =
            std::env::temp_dir().join(format!("nvmod-install-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        Ok(Scratch(dir))
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The Mod folder name: `name`, else the archive's file name without its extension (and without `.tar`).
fn mod_name(request: &Request) -> Result<String, String> {
    let raw = request.name.map_or_else(
        || {
            let file = request
                .archive
                .file_name()
                .map_or_default(|n| n.to_string_lossy().into_owned());
            let stem = file.rsplit_once('.').map_or(file.as_str(), |(s, _)| s);
            stem.strip_suffix(".tar").unwrap_or(stem).to_owned()
        },
        str::to_owned,
    );
    let name = raw.trim();
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
        || name.ends_with("_separator")
        || name.starts_with(['+', '-', '*', '#'])
    {
        return Err(format!(
            "`{name}` cannot be the name of a Mod folder; give one with --name"
        ));
    }
    Ok(name.to_owned())
}

const GAME_FOLDERS: [&str; 14] = [
    "data",
    "meshes",
    "textures",
    "sound",
    "nvse",
    "scripts",
    "menus",
    "music",
    "video",
    "lodsettings",
    "shaders",
    "strings",
    "interface",
    "config",
];

/// The folder that holds the Mod's content: a single wrapper folder is stepped into, and so is a `Data` folder.
fn content_root(extracted: &Path) -> PathBuf {
    let mut root = extracted.to_path_buf();
    loop {
        let entries: Vec<_> = fs::read_dir(&root)
            .into_iter()
            .flatten()
            .flatten()
            .collect();
        let [only] = entries.as_slice() else { break };
        let name = only.file_name().to_string_lossy().to_ascii_lowercase();
        if only.path().is_dir() && !GAME_FOLDERS.contains(&name.as_str()) && name != "fomod" {
            root = only.path();
        } else {
            break;
        }
    }
    root
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).into_iter().flatten().flatten() {
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        if entry.path().is_dir() {
            walk(&entry.path(), &format!("{name}/"), out);
        } else {
            out.push(name);
        }
    }
}

/// What goes where: pairs of (source file, relative destination in the Mod), in install order.
type Plan = Vec<(PathBuf, String)>;

/// `relative` below `base`, matching each component without regard to case.
fn resolve_ci(base: &Path, relative: &str) -> Option<PathBuf> {
    let rel = extract::safe_relative(relative).ok()?;
    let mut path = base.to_path_buf();
    for part in &rel {
        path = find_ci(&path, &part.to_string_lossy())?;
    }
    Some(path)
}

fn plan_plain(root: &Path) -> Result<Plan, String> {
    let content = find_ci(root, "data")
        .filter(|d| d.is_dir())
        .unwrap_or_else(|| root.to_path_buf());
    let mut files = Vec::new();
    walk(&content, "", &mut files);
    let plan: Plan = files.into_iter().map(|f| (content.join(&f), f)).collect();
    check_reserved(&plan)?;
    Ok(plan)
}

fn plan_fomod(root: &Path, items: &[fomod::FileItem]) -> Result<Plan, String> {
    let mut sorted: Vec<_> = items.iter().enumerate().collect();
    sorted.sort_by_key(|(i, item)| (item.priority, *i));
    let mut plan = Plan::new();
    for (_, item) in sorted {
        let source = resolve_ci(root, &item.source).ok_or_else(|| {
            format!(
                "the FOMOD installs `{}`, which the archive does not hold",
                item.source
            )
        })?;
        let destination = item.destination.replace('\\', "/");
        let into_folder = destination.is_empty() || destination.ends_with('/');
        if item.folder {
            if !source.is_dir() {
                return Err(format!(
                    "the FOMOD installs `{}` as a folder, but it is a file",
                    item.source
                ));
            }
            let mut inner = Vec::new();
            walk(&source, "", &mut inner);
            for file in inner {
                let to = format!(
                    "{destination}{}{file}",
                    if destination.is_empty() || into_folder {
                        ""
                    } else {
                        "/"
                    }
                );
                plan.push((source.join(&file), clean(&to)?));
            }
        } else {
            if !source.is_file() {
                return Err(format!(
                    "the FOMOD installs `{}` as a file, but it is a folder",
                    item.source
                ));
            }
            let file_name = source
                .file_name()
                .map_or_default(|n| n.to_string_lossy().into_owned());
            let to = if into_folder {
                format!("{destination}{file_name}")
            } else {
                destination
            };
            plan.push((source, clean(&to)?));
        }
    }
    check_reserved(&plan)?;
    Ok(plan)
}

fn clean(path: &str) -> Result<String, String> {
    let rel = extract::safe_relative(path)?;
    Ok(rel
        .iter()
        .map(|p| p.to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

/// Copy the plan into `dest` (a later entry replaces an earlier one; a folder that exists with other case is
/// reused). Returns the final relative paths.
fn build(plan: &Plan, dest: &Path) -> Result<(), String> {
    for (source, relative) in plan {
        if nvmod_core::cancel::requested() {
            return Err(nvmod_core::cancel::INTERRUPTED.to_owned());
        }
        let mut target = dest.to_path_buf();
        let parts: Vec<_> = relative.split('/').collect();
        for (i, part) in parts.iter().enumerate() {
            if let Some(existing) = find_ci(&target, part) {
                if i + 1 < parts.len() || !existing.is_file() {
                    target = existing;
                    continue;
                }
                fs::remove_file(&existing).map_err(|e| format!("{}: {e}", existing.display()))?;
            }
            target = target.join(part);
            if i + 1 < parts.len() {
                fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
            }
        }
        fs::create_dir_all(dest).map_err(|e| e.to_string())?;
        fs::copy(source, &target).map_err(|e| format!("{}: {e}", target.display()))?;
    }
    Ok(())
}

/// The distinct final paths of a plan, case-folded, with the spelling of the last writer; sorted.
fn final_paths(plan: &Plan) -> Vec<String> {
    let mut by_key = std::collections::BTreeMap::new();
    for (_, relative) in plan {
        by_key.insert(relative.to_ascii_lowercase(), relative.clone());
    }
    let mut paths: Vec<String> = by_key.into_values().collect();
    paths.sort();
    paths
}

#[derive(Debug)]
struct ListsEnv {
    active: Vec<String>,
    order: Vec<String>,
}

impl Env for ListsEnv {
    fn file_state(&self, file: &str) -> Option<FileState> {
        let lower = file.to_ascii_lowercase();
        let name = lower.rsplit(['/', '\\']).next().unwrap_or(&lower);
        if !["esp", "esm", "esl"].iter().any(|e| has_extension(name, e)) {
            return None;
        }
        Some(if lists::lists(&self.active, name) {
            FileState::Active
        } else if lists::lists(&self.order, name) {
            FileState::Inactive
        } else {
            FileState::Missing
        })
    }
}

/// Read an answers file: `{"answers": {"<question id>": ["<option>", ...]}}`, or the bare map.
///
/// # Errors
/// The file cannot be read or has another shape.
pub fn read_answers(path: &Path) -> Result<Answers, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| format!("{}: not valid JSON: {e}", path.display()))?;
    let map = value.get("answers").unwrap_or(&value);
    serde_json::from_value(map.clone()).map_err(|e| {
        format!(
            r#"{}: expected {{"answers": {{"<question id>": ["<option>", ...]}}}}: {e}"#,
            path.display()
        )
    })
}

/// The answers of a request: the file's, with the in-memory ones laid over them.
fn gather_answers(request: &Request) -> Result<Answers, String> {
    let mut answers = request
        .answers
        .map(read_answers)
        .transpose()?
        .unwrap_or_default();
    for (id, picked) in request.answers_inline.into_iter().flatten() {
        answers.insert(id.clone(), picked.clone());
    }
    Ok(answers)
}

/// Where a step may fail: an injected failure (`NVMOD_FAIL_AT`) or a termination request.
/// The `gameName` of a `meta.ini` nvmod writes: the Nexus game domain of Fallout: New Vegas.
const META_GAME_NAME: &str = "newvegas";

/// Write the Mod's `meta.ini`: `gameName`, `modid` and `version` when the archive's file name is in Nexus style
/// (`<name>-<modid>-<version>-<timestamp>`), and `installationFile` as the file name only. A `meta.ini` that the
/// archive carried is edited in place, so its other lines stay as they were; a folder install has no archive
/// name and writes only `gameName`.
fn write_meta(mod_dir: &Path, archive: &Path) -> Result<(), String> {
    let path = mod_dir.join("meta.ini");
    let mut meta = nvmod_formats::metaini::MetaIni::parse(&fs::read(&path).unwrap_or_default());
    meta.set("General", "gameName", META_GAME_NAME);
    if archive.is_file() {
        let file = archive
            .file_name()
            .map_or_default(|n| n.to_string_lossy().into_owned());
        meta.set_installation_file(&file);
        if let Some(nexus) = nvmod_formats::metaini::parse_nexus_name(&file) {
            meta.set_modid(nexus.modid);
            meta.set_version(&nexus.version);
        }
    }
    journal::write_atomic(&path, &meta.to_bytes())
}

fn fail_point(step: &str) -> Result<(), String> {
    nvmod_core::cancel::step_point(step)
}

/// Refuse a plan that writes into `.nvmod` at the top of the Mod: that folder holds nvmod's own records.
fn check_reserved(plan: &Plan) -> Result<(), String> {
    if plan.iter().any(|(_, relative)| {
        let lower = relative.to_ascii_lowercase();
        lower == ".nvmod" || lower.starts_with(".nvmod/")
    }) {
        return Err(
            "the archive installs into `.nvmod`, which nvmod keeps for its own records of a Mod"
                .to_owned(),
        );
    }
    Ok(())
}

/// The entry of `modlist.txt` named `name`, as the list spells it, and whether it is enabled.
fn listed_entry(modlist: Option<&[u8]>, name: &str) -> Result<Option<(String, bool)>, String> {
    let text = modlist.map_or_default(nvmod_formats::text::decode);
    let parsed = Modlist::parse(&text);
    let Some(entry) = parsed
        .entries
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case(name))
    else {
        return Ok(None);
    };
    if entry.kind == EntryKind::Separator || entry.state == ModState::Foreign {
        return Err(format!(
            "`{}` in modlist.txt is not a Mod (a separator or an unmanaged entry)",
            entry.name
        ));
    }
    Ok(Some((entry.name.clone(), entry.state == ModState::Enabled)))
}

/// The first of `<name> 2`, `<name> 3`, ... that is neither a folder under `mods/` nor listed.
fn free_name(mods: &Path, modlist: Option<&[u8]>, name: &str) -> String {
    let text = modlist.map_or_default(nvmod_formats::text::decode);
    let parsed = Modlist::parse(&text);
    let mut n = 2_u32;
    loop {
        let candidate = format!("{name} {n}");
        if find_ci(mods, &candidate).is_none()
            && !parsed
                .entries
                .iter()
                .any(|e| e.name.eq_ignore_ascii_case(&candidate))
        {
            return candidate;
        }
        n += 1;
    }
}

/// The files of a Mod folder as `/` paths (without `meta.ini`, hidden markers' suffix kept, and `.nvmod`).
fn existing_files(mod_dir: &Path) -> Vec<String> {
    let mut files = Vec::new();
    walk(mod_dir, "", &mut files);
    files.retain(|f| !nvmod_stage::is_mod_metadata(f));
    files
}

/// Install `request.archive`, or plan to.
///
/// When a Mod folder of the name exists, `request.on_conflict` decides: refuse (the default), replace, merge or
/// rename (see [`OnConflict`]).
///
/// # Errors
/// See [`InstallError`].
// One flat sequence: checks, plan, lists, then the journaled writes.
#[allow(clippy::too_many_lines)]
pub fn install(request: &Request) -> Result<Report, InstallError> {
    let instance = request.instance;
    let mut name = mod_name(request).map_err(InstallError::Refused)?;
    let dir = instance.profile_dir(request.profile);
    let read = |file: &str| fs::read(dir.join(file)).ok();
    let (modlist, plugins, order) = (
        read("modlist.txt"),
        read("plugins.txt"),
        read("loadorder.txt"),
    );
    let mods = instance.root.join("mods");
    let folder = find_ci(&mods, &name)
        .filter(|p| p.is_dir())
        .and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()));
    let listed = listed_entry(modlist.as_deref(), &name).map_err(InstallError::Refused)?;
    if folder.is_none() && instance.mod_dir(&name).exists() {
        return refuse(format!(
            "`mods/{name}` exists and is not a folder; remove it or choose another name with --name"
        ));
    }
    let (mut conflict, mut keep_line, mut enabled) = (None, false, true);
    if folder.is_some() || listed.is_some() {
        match request.on_conflict {
            OnConflict::Refuse if folder.is_some() => {
                return refuse(format!(
                    "a Mod folder `{name}` already exists in the Instance (choose another name with --name, or --on-conflict replace|merge|rename)"
                ));
            }
            OnConflict::Refuse => {}
            OnConflict::Rename => {
                name = free_name(&mods, modlist.as_deref(), &name);
                conflict = Some("renamed");
            }
            OnConflict::Replace | OnConflict::Merge => {
                conflict = Some(if request.on_conflict == OnConflict::Merge {
                    "merged"
                } else {
                    "replaced"
                });
                if let Some((listed_name, on)) = &listed {
                    keep_line = true;
                    enabled = *on;
                    name.clone_from(listed_name);
                }
                if let Some(on_disk) = &folder {
                    name.clone_from(on_disk);
                }
            }
        }
    }
    let existing_folder = folder.filter(|_| conflict.is_some() && conflict != Some("renamed"));
    let new_modlist = if keep_line {
        None
    } else {
        Some(lists::add_to_modlist(modlist.as_deref(), &name).map_err(InstallError::Refused)?)
    };

    let answers = gather_answers(request).map_err(InstallError::Refused)?;
    let session = session::Session::open_with(
        request.archive,
        request.limits,
        plugins.as_deref(),
        order.as_deref(),
    )
    .map_err(InstallError::Refused)?;
    let resolved = session.resolve(&answers)?;
    let plan = resolved.plan;
    let mut files = final_paths(&plan);
    if files.is_empty() {
        return refuse("the archive holds no files to install");
    }
    if conflict == Some("merged") {
        // The Mod afterwards: what the folder holds with the new files laid over it.
        let mut by_key: std::collections::BTreeMap<String, String> =
            existing_files(&instance.mod_dir(&name))
                .into_iter()
                .map(|f| (f.to_ascii_lowercase(), f))
                .collect();
        for f in files {
            by_key.insert(f.to_ascii_lowercase(), f);
        }
        files = by_key.into_values().collect();
        files.sort();
    }
    let mut new_plugins: Vec<String> = files
        .iter()
        .filter(|f| !f.contains('/'))
        .filter(|f| has_extension(f, "esp") || has_extension(f, "esm"))
        .filter(|_| enabled)
        .cloned()
        .collect();
    new_plugins.sort_by_key(|p| (!has_extension(p, "esm"), p.to_ascii_lowercase()));
    let new_lists = lists::add_plugins(plugins.as_deref(), order.as_deref(), &new_plugins)
        .map_err(InstallError::Refused)?;
    let activated: Vec<String> = new_plugins
        .iter()
        .filter(|p| {
            !nvmod_formats::plugins::parse_plugin_list(order.as_deref().unwrap_or_default())
                .iter()
                .chain(&nvmod_formats::plugins::parse_plugin_list(
                    plugins.as_deref().unwrap_or_default(),
                ))
                .any(|e| e.eq_ignore_ascii_case(p))
        })
        .cloned()
        .collect();

    let mut report = Report {
        status: if request.dry_run {
            "dry-run"
        } else {
            "installed"
        },
        dry_run: request.dry_run,
        r#mod: name.clone(),
        profile: request.profile.to_owned(),
        files,
        plugins: activated,
        inferred: resolved.inferred,
        conflict,
        staged_files: None,
        journal: None,
    };
    if request.dry_run {
        return Ok(report);
    }

    let mut entry = if existing_folder.is_some() {
        journal::begin_kind(instance, request.profile, &name, journal::KIND_LIFECYCLE)
    } else {
        journal::begin(instance, request.profile, &name)
    }
    .map_err(InstallError::Refused)?;
    let outcome = (|| -> Result<usize, String> {
        fail_point("mod")?;
        let mod_dir = instance.mod_dir(&name);
        let old_meta = existing_folder
            .as_ref()
            .and_then(|_| fs::read(mod_dir.join("meta.ini")).ok());
        if let Some(on_disk) = &existing_folder {
            // The old folder goes to the journal's trash, so a rollback puts it back whole.
            let (from, to) = (
                format!("mods/{on_disk}"),
                format!(".nvmod/trash/{}/{on_disk}", entry.id),
            );
            entry.record_move(instance, &from, &to)?;
            let trashed = instance.root.join(&to);
            if let Some(parent) = trashed.parent() {
                fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
            }
            fs::rename(instance.root.join(&from), &trashed)
                .map_err(|e| format!("{}: {e}", trashed.display()))?;
            entry.record_create(instance, &from)?;
            fs::create_dir_all(&mod_dir).map_err(|e| format!("{}: {e}", mod_dir.display()))?;
            if conflict == Some("merged") {
                lifecycle::copy_tree(&trashed, &mod_dir, false)?;
            }
        } else {
            fs::create_dir_all(&mod_dir).map_err(|e| format!("{}: {e}", mod_dir.display()))?;
        }
        build(&plan, &mod_dir)?;
        if let Some(meta) = old_meta {
            // Notes, categories and the rest of the Mod's meta.ini outlive the new archive's.
            journal::write_atomic(&mod_dir.join("meta.ini"), &meta)?;
        }
        write_meta(&mod_dir, request.archive)?;
        match &resolved.fomod {
            Some((used, module)) => saved::write(
                &mod_dir,
                &AnswersFile {
                    schema_version: saved::ANSWERS_SCHEMA_VERSION,
                    module: module.clone(),
                    archive: saved::absolute(request.archive),
                    answers: used.clone(),
                },
            )?,
            None => {
                let _ = fs::remove_file(mod_dir.join(ANSWERS_PATH));
            }
        }
        fail_point("lists")?;
        if let Some(new) = &new_modlist {
            journal::write_atomic(&dir.join("modlist.txt"), new)?;
        }
        if let Some((p, o)) = &new_lists {
            journal::write_atomic(&dir.join("plugins.txt"), p)?;
            journal::write_atomic(&dir.join("loadorder.txt"), o)?;
        }
        fail_point("stage")?;
        let staged = nvmod_stage::stage(instance, request.profile, nvmod_stage::Options::default())
            .map_err(|e| match e {
                nvmod_stage::StageError::Refused(m) | nvmod_stage::StageError::Failed(m) => m,
            })?;
        Ok(staged.manifest.files)
    })();
    match outcome {
        Ok(staged) => {
            journal::complete(instance, &entry).map_err(InstallError::Failed)?;
            report.staged_files = Some(staged);
            report.journal = Some(entry.id);
            Ok(report)
        }
        Err(why) => match journal::undo(instance, &entry) {
            Ok(()) => Err(InstallError::Failed(format!(
                "{why}; the install was rolled back"
            ))),
            Err(undo) => Err(InstallError::Failed(format!(
                "{why}; the rollback failed too ({undo}); run `nvmod rollback`"
            ))),
        },
    }
}
