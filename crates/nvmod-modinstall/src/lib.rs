// SPDX-License-Identifier: GPL-3.0-only
//! Installs a Mod from an archive into an nvmod Instance, and takes the last install back.
//!
//! The steps: extract into a scratch folder (refusing anything that escapes it), work out which files go in (a
//! plain archive as it is; a FOMOD from its `ModuleConfig.xml`, asking only what cannot be inferred), then, with
//! a journal and a backup of the three profile files written first, place the Mod in `mods/`, add it enabled at
//! the highest priority to `modlist.txt`, activate its plugins in `plugins.txt` and `loadorder.txt`, and stage
//! the profile. A failed step rolls everything back; [`rollback`] does the same on request.

mod extract;
mod fomod;
mod journal;
mod lists;

use fomod::{Answers, Env, FileState, Module, Outcome};
use nvmod_instance::{NvmodInstance, find_ci};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

pub use extract::Limits;
pub use fomod::{GroupKind, Inferred, PluginType, Question, QuestionOption};
pub use journal::{RollbackError, RollbackReport, rollback};

/// What to install.
#[derive(Debug, Clone)]
pub struct Request<'a> {
    /// The Instance.
    pub instance: &'a NvmodInstance,
    /// The profile whose lists get the Mod.
    pub profile: &'a str,
    /// The archive.
    pub archive: &'a Path,
    /// The Mod's folder name; the archive's file name without its extension when `None`.
    pub name: Option<&'a str>,
    /// Plan only; write nothing.
    pub dry_run: bool,
    /// A JSON file with answers to the FOMOD's questions.
    pub answers: Option<&'a Path>,
    /// Extract with this tool instead of the built-in readers.
    pub extract_tool: Option<&'a Path>,
    /// Size limits of the extraction.
    pub limits: Limits,
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
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
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

fn plan_plain(root: &Path) -> Plan {
    let content = find_ci(root, "data")
        .filter(|d| d.is_dir())
        .unwrap_or_else(|| root.to_path_buf());
    let mut files = Vec::new();
    walk(&content, "", &mut files);
    files.into_iter().map(|f| (content.join(&f), f)).collect()
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
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let to = if into_folder {
                format!("{destination}{file_name}")
            } else {
                destination
            };
            plan.push((source, clean(&to)?));
        }
    }
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

fn read_answers(path: &Path) -> Result<Answers, String> {
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

fn fail_point(step: &str) -> Result<(), String> {
    if std::env::var("NVMOD_FAIL_AT").is_ok_and(|v| v == step) {
        return Err(format!("injected failure at step `{step}` (NVMOD_FAIL_AT)"));
    }
    Ok(())
}

/// Extract the archive and work out what goes into the Mod.
fn plan_archive(
    request: &Request,
    plugins: Option<&[u8]>,
    order: Option<&[u8]>,
) -> Result<(Scratch, Plan, Vec<Inferred>), InstallError> {
    let answers = match request.answers {
        Some(path) => read_answers(path).map_err(InstallError::Refused)?,
        None => Answers::new(),
    };
    let scratch = Scratch::new().map_err(InstallError::Refused)?;
    let extracted = scratch.0.join("x");
    fs::create_dir_all(&extracted).map_err(|e| InstallError::Refused(e.to_string()))?;
    extract::extract(
        request.archive,
        &extracted,
        request.extract_tool,
        request.limits,
    )
    .map_err(InstallError::Refused)?;
    let root = content_root(&extracted);

    let mut inferred = Vec::new();
    let plan = if let Some(config) = resolve_ci(&root, "fomod/ModuleConfig.xml") {
        let bytes = fs::read(&config).map_err(|e| InstallError::Refused(e.to_string()))?;
        let module = Module::parse(&fomod::decode_xml(&bytes)).map_err(InstallError::Refused)?;
        let env = ListsEnv {
            active: nvmod_formats::plugins::parse_plugin_list(plugins.unwrap_or_default()),
            order: nvmod_formats::plugins::parse_plugin_list(order.unwrap_or_default()),
        };
        match module
            .resolve(&env, &answers)
            .map_err(InstallError::Refused)?
        {
            Outcome::Questions(questions) => return Err(InstallError::Questions(questions)),
            Outcome::Ready(selection) => {
                inferred = selection.inferred;
                plan_fomod(&root, &selection.files).map_err(InstallError::Refused)?
            }
        }
    } else {
        plan_plain(&root)
    };
    Ok((scratch, plan, inferred))
}

/// Install `request.archive`, or plan to.
///
/// # Errors
/// See [`InstallError`].
pub fn install(request: &Request) -> Result<Report, InstallError> {
    let instance = request.instance;
    let name = mod_name(request).map_err(InstallError::Refused)?;
    let dir = instance.profile_dir(request.profile);
    let read = |file: &str| fs::read(dir.join(file)).ok();
    let (modlist, plugins, order) = (
        read("modlist.txt"),
        read("plugins.txt"),
        read("loadorder.txt"),
    );
    if instance.mod_dir(&name).exists() || find_ci(&instance.root.join("mods"), &name).is_some() {
        return refuse(format!(
            "a Mod folder `{name}` already exists in the Instance (choose another name with --name)"
        ));
    }
    let new_modlist =
        lists::add_to_modlist(modlist.as_deref(), &name).map_err(InstallError::Refused)?;

    let (_scratch, plan, inferred) = plan_archive(request, plugins.as_deref(), order.as_deref())?;
    let files = final_paths(&plan);
    if files.is_empty() {
        return refuse("the archive holds no files to install");
    }
    let mut new_plugins: Vec<String> = files
        .iter()
        .filter(|f| !f.contains('/'))
        .filter(|f| has_extension(f, "esp") || has_extension(f, "esm"))
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
        inferred,
        staged_files: None,
        journal: None,
    };
    if request.dry_run {
        return Ok(report);
    }

    let entry = journal::begin(instance, request.profile, &name).map_err(InstallError::Refused)?;
    let outcome = (|| -> Result<usize, String> {
        fail_point("mod")?;
        let mod_dir = instance.mod_dir(&name);
        fs::create_dir_all(&mod_dir).map_err(|e| format!("{}: {e}", mod_dir.display()))?;
        build(&plan, &mod_dir)?;
        fail_point("lists")?;
        journal::write_atomic(&dir.join("modlist.txt"), &new_modlist)?;
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
