// SPDX-License-Identifier: GPL-3.0-only
//! The state file of `nvmod setup`: `.nvmod/setup.json` inside the Instance. It records the command line the run
//! was started with, the plan and the steps that finished, each with a small witness of what the step left
//! behind. It is written after every finished step (to a temporary file and a rename, so a crash never leaves half
//! a file), which lets `nvmod setup --resume` continue from the first unfinished step.
//!
//! The file is a record, not proof: before a finished step is skipped, `verify_witness` checks again that its
//! effects are still in the Instance.

use nvmod_tools::sha256_hex;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The schema version this build writes and reads.
pub(crate) const SCHEMA_VERSION: u32 = 1;
/// Largest file whose content is hashed for a witness; a larger one is witnessed by its size.
const HASH_LIMIT: u64 = 64 * 1024 * 1024;

/// One step of the plan, as recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct PlanEntry {
    pub step: String,
    pub description: String,
}

/// A file a step left behind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct FileWitness {
    pub path: String,
    pub size: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// A Mod folder a step made: its name and the number of files under it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ModWitness {
    pub name: String,
    pub files: u64,
}

/// What a finished step left in the Instance.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Witness {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileWitness>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mods: Vec<ModWitness>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
}

/// A finished step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Completed {
    pub step: String,
    pub witness: Witness,
}

/// The step that failed last, for a wizard to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Failure {
    pub step: String,
    pub code: u8,
    pub detail: String,
}

/// The whole file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct State {
    pub schema_version: u32,
    pub nvmod_version: String,
    /// The command line of the run (without the program, `setup` and the flags that belong to one invocation).
    pub args: Vec<String>,
    pub plan: Vec<PlanEntry>,
    pub completed: Vec<Completed>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure: Option<Failure>,
    #[serde(default)]
    pub complete: bool,
}

/// Why the state file could not be used.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum LoadError {
    /// There is no state file: this is not a setup in progress.
    Missing,
    /// Written by a newer nvmod.
    Newer(u32),
    /// Unreadable or not what it should be.
    Bad(String),
}

pub(crate) fn path(instance: &Path) -> PathBuf {
    instance.join(".nvmod").join("setup.json")
}

/// Read the state of the Instance at `instance`.
pub(crate) fn load(instance: &Path) -> Result<State, LoadError> {
    let file = path(instance);
    let text = match std::fs::read_to_string(&file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(LoadError::Missing),
        Err(e) => return Err(LoadError::Bad(format!("{}: {e}", file.display()))),
    };
    let bad = |e: &dyn std::fmt::Display| LoadError::Bad(format!("{}: {e}", file.display()));
    let value: serde_json::Value = serde_json::from_str(&text).map_err(|e| bad(&e))?;
    match value
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
    {
        Some(v) if v > u64::from(SCHEMA_VERSION) => {
            return Err(LoadError::Newer(u32::try_from(v).unwrap_or(u32::MAX)));
        }
        Some(_) => {}
        None => return Err(bad(&"no schema_version")),
    }
    serde_json::from_value(value).map_err(|e| bad(&e))
}

/// Write the state atomically.
pub(crate) fn save(instance: &Path, state: &State) -> std::io::Result<()> {
    let file = path(instance);
    let dir = file.parent().unwrap_or(instance);
    std::fs::create_dir_all(dir)?;
    let tmp = dir.join("setup.json.tmp");
    let mut text = serde_json::to_string_pretty(state).map_err(std::io::Error::other)?;
    text.push('\n');
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, &file)
}

fn count_files(dir: &Path) -> u64 {
    std::fs::read_dir(dir).map_or(0, |entries| {
        entries
            .flatten()
            .map(|e| {
                let p = e.path();
                if p.is_dir() { count_files(&p) } else { 1 }
            })
            .sum()
    })
}

/// The Mod folders of the Instance with their file counts.
pub(crate) fn mods_snapshot(instance: &Path) -> BTreeMap<String, u64> {
    std::fs::read_dir(instance.join("mods")).map_or_else(
        |_| BTreeMap::new(),
        |entries| {
            entries
                .flatten()
                .filter(|e| e.path().is_dir())
                .map(|e| {
                    (
                        e.file_name().to_string_lossy().into_owned(),
                        count_files(&e.path()),
                    )
                })
                .collect()
        },
    )
}

fn file_witness(instance: &Path, relative: &str) -> Option<FileWitness> {
    let file = instance.join(relative);
    let size = std::fs::metadata(&file).ok()?.len();
    let sha256 = if size <= HASH_LIMIT {
        Some(sha256_hex(&std::fs::read(&file).ok()?))
    } else {
        None
    };
    Some(FileWitness {
        path: relative.to_owned(),
        size,
        sha256,
    })
}

fn viewer(instance: &Path) -> Option<String> {
    std::fs::read_dir(instance.join("nv-rs/bin"))
        .ok()?
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| n.starts_with("nv-viewer"))
        .map(|n| format!("nv-rs/bin/{n}"))
}

fn instance_target(instance: &Path) -> Option<String> {
    let text = std::fs::read_to_string(instance.join("nvmod-instance.json")).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    Some(doc.get("target")?.as_str().unwrap_or("vanilla").to_owned())
}

/// The witness of `step` just after it finished. `mods_before` is `mods_snapshot` from before the step.
pub(crate) fn witness_for(
    step: &str,
    instance: &Path,
    profile: &str,
    mods_before: &BTreeMap<String, u64>,
) -> Witness {
    let mut witness = Witness::default();
    let mut files: Vec<String> = Vec::new();
    match step {
        "nv-rs" => {
            files.push("nv-rs/install.json".to_owned());
            files.extend(viewer(instance));
        }
        "target" => witness.target = instance_target(instance),
        "sort" => files.push(format!("profiles/{profile}/loadorder.txt")),
        _ => {}
    }
    if step == "ttw" || step.starts_with("mod:") {
        witness.mods = mods_snapshot(instance)
            .into_iter()
            .filter(|(name, count)| mods_before.get(name) != Some(count))
            .map(|(name, files)| ModWitness { name, files })
            .collect();
    }
    witness.files = files
        .iter()
        .filter_map(|f| file_witness(instance, f))
        .collect();
    witness
}

/// Check that what `step` left behind is still there.
///
/// # Errors
/// What no longer holds, in words.
pub(crate) fn verify_witness(
    step: &str,
    witness: &Witness,
    instance: &Path,
    profile: &str,
) -> Result<(), String> {
    let exists = |relative: String, dir: bool| {
        let p = instance.join(&relative);
        if (dir && p.is_dir()) || (!dir && p.is_file()) {
            Ok(())
        } else {
            Err(format!("{relative} is missing"))
        }
    };
    match step {
        "init" | "import-mo2" => {
            let text = std::fs::read_to_string(instance.join("nvmod-instance.json"))
                .map_err(|_| "nvmod-instance.json is missing".to_owned())?;
            serde_json::from_str::<serde_json::Value>(&text)
                .map_err(|_| "nvmod-instance.json is not valid JSON".to_owned())?;
            exists("mods".to_owned(), true)?;
        }
        "profile" => {
            for f in ["modlist.txt", "plugins.txt", "loadorder.txt"] {
                exists(format!("profiles/{profile}/{f}"), false)?;
            }
        }
        "target" => {
            if instance_target(instance) != witness.target {
                return Err(format!(
                    "the target of the Instance is no longer {}",
                    witness.target.as_deref().unwrap_or("unset")
                ));
            }
        }
        "stage" => {
            exists(format!("staging/{profile}.json"), false)?;
            exists(format!("staging/{profile}/data"), true)?;
        }
        _ => {}
    }
    if step == "ttw" || step.starts_with("mod:") {
        let modlist =
            std::fs::read_to_string(instance.join(format!("profiles/{profile}/modlist.txt")))
                .unwrap_or_default();
        let now = mods_snapshot(instance);
        for m in &witness.mods {
            match now.get(&m.name) {
                None => return Err(format!("the Mod {} is missing", m.name)),
                Some(n) if *n != m.files => {
                    return Err(format!(
                        "the Mod {} has {n} files, it had {} when the step finished",
                        m.name, m.files
                    ));
                }
                Some(_) => {}
            }
            if !modlist
                .lines()
                .any(|l| l.trim_end() == format!("+{}", m.name))
            {
                return Err(format!("the Mod {} is not enabled in {profile}", m.name));
            }
        }
    }
    for f in &witness.files {
        let now =
            file_witness(instance, &f.path).ok_or_else(|| format!("{} is missing", f.path))?;
        if now.size != f.size || now.sha256 != f.sha256 {
            return Err(format!("{} changed since the step finished", f.path));
        }
    }
    Ok(())
}
