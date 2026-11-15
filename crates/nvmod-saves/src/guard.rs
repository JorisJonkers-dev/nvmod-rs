// SPDX-License-Identifier: GPL-3.0-only
//! The check around a run. [`Guard::before`] takes the checksums of every save folder that matters, and
//! [`Guard::after`] compares them: a change in the folder the profile saves to is expected (new and overwritten
//! saves), a removed save there is not, and any change in another folder (the shared `Saves` folder while the
//! profile saves locally, or another profile's `saves/`) is not. There is no switch to turn it off: `nvmod run`
//! calls it for every launch.

use crate::{Location, hash_file, locate};
use nvmod_instance::NvmodInstance;
use nvmod_profile::Target;
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

type Sums = BTreeMap<String, (u64, String)>;

/// The size and SHA-256 of every regular file directly in `dir` (links and folders are skipped).
#[must_use]
pub fn checksums(dir: &Path) -> Sums {
    let mut out = Sums::new();
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.file_type().is_ok_and(|t| t.is_file())
            && let Ok(sum) = hash_file(&entry.path())
        {
            out.insert(entry.file_name().to_string_lossy().into_owned(), sum);
        }
    }
    out
}

#[derive(Debug, Clone)]
struct Watched {
    label: String,
    dir: PathBuf,
    active: bool,
    before: Sums,
}

/// The checksums taken before a run.
#[derive(Debug, Clone)]
pub struct Guard {
    profile: String,
    watched: Vec<Watched>,
}

/// One save file that changed during the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    /// The folder, as `active`, `shared` or `profile <name>`.
    pub folder: String,
    /// The file name.
    pub name: String,
    /// `added`, `modified` or `removed`.
    pub kind: &'static str,
    /// Whether the profile expected it.
    pub expected: bool,
}

/// What changed across the run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GuardReport {
    /// Every change.
    pub changes: Vec<Change>,
    /// One line per change that the profile did not expect.
    pub warnings: Vec<String>,
}

impl Guard {
    /// Take the checksums for a run of `profile`: the folder it saves to, the shared `Saves` folder when it
    /// saves locally, and the `saves/` of every other profile.
    #[must_use]
    pub fn before(instance: &NvmodInstance, profile: &str, target: Option<&Target>) -> Guard {
        let mut watched: Vec<Watched> = Vec::new();
        let mut add = |label: String, dir: PathBuf, active: bool| {
            if !watched.iter().any(|w| w.dir == dir) {
                watched.push(Watched {
                    before: checksums(&dir),
                    label,
                    dir,
                    active,
                });
            }
        };
        let active: Option<Location> = locate(instance, profile, target).ok();
        if let Some(a) = &active {
            add("active".to_owned(), a.dir.clone(), true);
            if a.local
                && let Some(t) = target
            {
                add("shared".to_owned(), t.my_games.join("Saves"), false);
            }
        }
        for other in instance.profiles() {
            if other != profile {
                add(
                    format!("profile {other}"),
                    instance.profile_dir(&other).join("saves"),
                    false,
                );
            }
        }
        Guard {
            profile: profile.to_owned(),
            watched,
        }
    }

    /// Compare with the folders as they are now.
    #[must_use]
    pub fn after(&self) -> GuardReport {
        let mut changes = Vec::new();
        for w in &self.watched {
            let now = checksums(&w.dir);
            let mut note = |name: &str, kind: &'static str| {
                changes.push(Change {
                    folder: w.label.clone(),
                    name: name.to_owned(),
                    kind,
                    expected: w.active && kind != "removed",
                });
            };
            for (name, sum) in &now {
                match w.before.get(name) {
                    None => note(name, "added"),
                    Some(old) if old != sum => note(name, "modified"),
                    Some(_) => {}
                }
            }
            for name in w.before.keys().filter(|n| !now.contains_key(*n)) {
                note(name, "removed");
            }
        }
        let warnings = changes
            .iter()
            .filter(|c| !c.expected)
            .map(|c| {
                format!(
                    "save file `{}` ({}) was {} during the run, which profile `{}` did not expect",
                    c.name,
                    c.folder,
                    match c.kind {
                        "added" => "added",
                        "modified" => "changed",
                        _ => "removed",
                    },
                    self.profile
                )
            })
            .collect();
        GuardReport { changes, warnings }
    }
}
