// SPDX-License-Identifier: GPL-3.0-only
//! Per-category readiness on the nv-rs target: of the things the profile's load order uses, how many nv-rs
//! provides. Four categories come from the plugin DLLs of the Staging tree, grouped by the catalogue's `category`
//! (script-extender plugins, render injectors, patch DLLs, LOD); a fifth counts the enabled mods that bring no DLL
//! at all (data only: plugins, meshes, textures, sounds), which need nothing from nv-rs beyond the engine. The
//! command figure sums `implemented` and `total` over every extender in `capabilities.json`, so it rises when
//! nv-rs lists a new command. Numbers only: nothing here names a mod.

use nvmod_catalog::{Catalog, Category, Kind};
use nvmod_formats::{Modlist, text::decode};
use nvmod_instance::NvmodInstance;
use nvmod_tools::Capabilities;
use serde::Serialize;
use std::path::Path;

/// The label of DLLs the catalogue does not know or does not classify.
pub const UNCLASSIFIED: &str = "unclassified";

/// One category of plugin DLLs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CategoryReadiness {
    /// `script-extender`, `render`, `patch`, `lod` or `unclassified`.
    pub category: String,
    /// DLLs of this category in the Staging tree.
    pub present: usize,
    /// Built into nv-rs with no command missing (or built in fully when no coverage was read).
    pub built_in: usize,
    /// Built into nv-rs with commands still missing, or built in partly with no coverage to say how far.
    pub partial: usize,
    /// External DLLs: nothing records that nv-rs loads them.
    pub external: usize,
    /// DLLs the catalogue marks as unable to load on nv-rs.
    pub absent: usize,
    /// `built_in` as a whole percent of `present`; absent when there are none.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ready_percent: Option<u32>,
}

/// Enabled mods split by whether they bring a DLL.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct DataOnly {
    /// Enabled mods whose folder holds no DLL: data only, nothing for nv-rs to host.
    pub data_only: usize,
    /// Enabled mods that hold at least one DLL.
    pub with_dlls: usize,
    /// Enabled mods whose folder is missing.
    pub missing: usize,
}

/// Script commands of every extender `capabilities.json` lists, summed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandReadiness {
    /// Commands implemented.
    pub implemented: u64,
    /// Commands that exist.
    pub total: u64,
    /// `implemented` as a whole percent of `total`; absent when `total` is zero.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percent: Option<u32>,
}

/// The readiness part of the nv-rs report (`schema_version` 2 of the Doctor's JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Readiness {
    /// Plugin DLL categories, in report order; `unclassified` only when it is not empty.
    pub categories: Vec<CategoryReadiness>,
    /// The mods without DLLs.
    pub mods: DataOnly,
    /// Commands built, when `capabilities.json` was read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub commands: Option<CommandReadiness>,
}

fn percent(part: u64, whole: u64) -> Option<u32> {
    (whole > 0).then(|| u32::try_from(part * 100 / whole).unwrap_or(100))
}

/// Count the enabled mods of `profile` by whether their folder holds a DLL (walked to a bounded depth; links are
/// not followed).
#[must_use]
pub fn count_mods(instance: &NvmodInstance, profile: &str) -> DataOnly {
    let bytes =
        std::fs::read(instance.profile_dir(profile).join("modlist.txt")).unwrap_or_default();
    let list = Modlist::parse(&decode(&bytes));
    let mut counts = DataOnly::default();
    for entry in list.enabled_mods() {
        let dir = instance.mod_dir(&entry.name);
        if !dir.is_dir() {
            counts.missing += 1;
        } else if has_dll(&dir, 0) {
            counts.with_dlls += 1;
        } else {
            counts.data_only += 1;
        }
    }
    counts
}

fn has_dll(dir: &Path, depth: usize) -> bool {
    const MAX_DEPTH: usize = 8;
    let Ok(entries) = std::fs::read_dir(dir) else {
        return false;
    };
    for e in entries.flatten() {
        let Ok(kind) = e.file_type() else { continue };
        if kind.is_dir() {
            if depth < MAX_DEPTH && has_dll(&e.path(), depth + 1) {
                return true;
            }
        } else if kind.is_file()
            && e.file_name()
                .to_string_lossy()
                .to_lowercase()
                .ends_with(".dll")
        {
            return true;
        }
    }
    false
}

/// Build the report from the staged DLL names, the catalogue, the coverage (when it was read) and the mod counts.
#[must_use]
pub fn build(
    catalog: &Catalog,
    dlls: &[String],
    caps: Option<&Capabilities>,
    mods: DataOnly,
) -> Readiness {
    let mut order: Vec<Option<Category>> = Category::ALL.iter().copied().map(Some).collect();
    order.push(None);
    let mut categories = Vec::new();
    for want in order {
        let mut row = CategoryReadiness {
            category: want.map_or(UNCLASSIFIED, Category::label).to_owned(),
            present: 0,
            built_in: 0,
            partial: 0,
            external: 0,
            absent: 0,
            ready_percent: None,
        };
        for dll in dlls {
            let entry = catalog.by_dll(dll);
            if entry.and_then(|p| p.category) != want {
                continue;
            }
            row.present += 1;
            let provided = entry.map(|p| &p.provided_by.nv_rs);
            match provided.map_or(Kind::External, |p| p.kind) {
                Kind::External => row.external += 1,
                Kind::Absent => row.absent += 1,
                kind => {
                    let coverage = provided
                        .and_then(|p| p.extender.as_deref())
                        .and_then(|id| caps.and_then(|c| c.extender(id)));
                    let complete = match coverage {
                        Some(c) => c.missing() == 0,
                        None => kind == Kind::BuiltinFull,
                    };
                    if complete {
                        row.built_in += 1;
                    } else {
                        row.partial += 1;
                    }
                }
            }
        }
        row.ready_percent = percent(row.built_in as u64, row.present as u64);
        if row.present > 0 || want.is_some() {
            categories.push(row);
        }
    }
    let commands = caps.map(|c| {
        let implemented = c.extenders.iter().map(|e| e.implemented).sum();
        let total = c.extenders.iter().map(|e| e.total).sum();
        CommandReadiness {
            implemented,
            total,
            percent: percent(implemented, total),
        }
    });
    Readiness {
        categories,
        mods,
        commands,
    }
}

impl Readiness {
    /// The report as lines of text for people.
    #[must_use]
    pub fn to_lines(&self) -> Vec<String> {
        let mut lines = vec!["nv-rs readiness:".to_owned()];
        for c in &self.categories {
            let pct = c
                .ready_percent
                .map_or(String::new(), |p| format!(", {p}% ready"));
            lines.push(format!(
                "  {}: {} present: {} built in, {} partial, {} external (unknown), {} absent{pct}",
                c.category, c.present, c.built_in, c.partial, c.external, c.absent
            ));
        }
        lines.push(format!(
            "  data-only mods: {} (mods with DLLs: {}, folder missing: {})",
            self.mods.data_only, self.mods.with_dlls, self.mods.missing
        ));
        if let Some(c) = &self.commands {
            let pct = c.percent.map_or(String::new(), |p| format!(" ({p}%)"));
            lines.push(format!(
                "  script commands: {} of {} implemented{pct}",
                c.implemented, c.total
            ));
        }
        lines
    }
}
