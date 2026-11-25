// SPDX-License-Identifier: GPL-3.0-only
//! File-level conflicts and the virtual Data tree of a profile, as MO2 shows them (read only; nothing is written).
//!
//! The layers are, from the highest priority: Overwrite, the enabled Mods in `modlist.txt` order (the first line
//! wins), and the base copy's `Data` folder. Paths are compared without case through [`FileIndex`]. A Mod's
//! `.mohidden` files and its `meta.ini` are not providers. The base copy takes part in the tree but not in the
//! conflicts (MO2 does not count a Mod that replaces a game file as a conflict).

pub mod archives;

use nvmod_core::FileIndex;
use nvmod_instance::{NvmodInstance, find_ci};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

/// The provider name of the base copy in the tree.
pub const BASE_SOURCE: &str = "the base copy";
/// The provider name of the Overwrite folder.
pub const OVERWRITE_SOURCE: &str = "Overwrite";

/// Why a report was not made (exit code 2 for the command line).
#[derive(Debug, PartialEq, Eq)]
pub enum ConflictError {
    /// A profile, folder or list that cannot be read, or a Mod that is not in the profile.
    Refused(String),
}

/// What kind of layer provides files.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// An enabled Mod.
    Mod,
    /// The Overwrite folder, above every Mod.
    Overwrite,
    /// The base copy's `Data` folder, below every Mod.
    Base,
}

/// The files one layer provides: unique without case, as the layer spells them.
#[derive(Debug, Clone)]
pub struct Layer {
    /// The Mod folder name, [`OVERWRITE_SOURCE`] or [`BASE_SOURCE`].
    pub name: String,
    /// What it is.
    pub kind: Kind,
    /// Relative paths with `/`.
    pub files: Vec<String>,
}

/// One spelling of a path and who provides it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Source {
    /// The layer.
    #[serde(rename = "mod")]
    pub name: String,
    /// The path as that layer spells it.
    pub path: String,
}

/// A path that more than one Mod provides.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Contested {
    /// The lowercased path, as in the Staging tree.
    pub path: String,
    /// Who wins it.
    pub winner: Source,
    /// Who loses it, the highest priority first.
    pub losers: Vec<Source>,
}

/// The conflict flags of one layer, as MO2 computes them.
// The four flags are MO2's own and are printed as four booleans.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModConflict {
    /// The Mod folder name, or `Overwrite`.
    pub name: String,
    /// `mod` or `overwrite`.
    pub kind: Kind,
    /// 1 is the highest priority Mod; absent for Overwrite.
    pub position: Option<usize>,
    /// Files it provides.
    pub files: usize,
    /// Contested paths it wins.
    pub wins: usize,
    /// Paths it provides and loses.
    pub losses: usize,
    /// Wins at least one contested path.
    pub overwrites: bool,
    /// Loses at least one path.
    pub overwritten: bool,
    /// Both of the above.
    pub mixed: bool,
    /// Provides files and loses every one of them.
    pub redundant: bool,
    /// The layers whose files it replaces, highest priority first.
    pub overwrites_mods: Vec<String>,
    /// The layers that replace its files, highest priority first.
    pub overwritten_by_mods: Vec<String>,
}

/// An enabled Mod the conflict report leaves out, and why.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Excluded {
    /// The Mod folder name.
    #[serde(rename = "mod")]
    pub name: String,
    /// The reason, one line.
    pub reason: String,
}

/// The conflicts of a profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Report {
    /// The profile.
    pub profile: String,
    /// Overwrite (when present) and every enabled Mod, highest priority first.
    pub mods: Vec<ModConflict>,
    /// Every contested path once, in path order.
    pub conflicts: Vec<Contested>,
    /// Enabled Mods left out of the report (a generated Mod), one reason each; absent when none was.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub excluded: Vec<Excluded>,
    /// The archive axes (`conflicts --archives`); absent otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archives: Option<archives::ArchiveReport>,
}

/// One path of the virtual Data folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TreeEntry {
    /// The lowercased path, as in the Staging tree.
    pub path: String,
    /// The layer that provides it.
    pub provider: String,
    /// The path as that layer spells it.
    pub source_path: String,
}

/// The virtual Data folder of a profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tree {
    /// The profile.
    pub profile: String,
    /// Every path once, in path order.
    pub entries: Vec<TreeEntry>,
}

fn unique(files: Vec<(String, std::path::PathBuf)>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    files
        .into_iter()
        .filter(|(relative, _)| seen.insert(relative.to_lowercase()))
        .map(|(relative, _)| relative)
        .collect()
}

/// Read the layers of `profile` from disk, highest priority first. The base copy is the last layer when
/// `with_base` is set.
///
/// # Errors
/// `Refused` when the profile or its `modlist.txt` is missing or unreadable, or the base copy has no `Data` folder.
pub fn scan(
    instance: &NvmodInstance,
    profile: &str,
    with_base: bool,
) -> Result<Vec<Layer>, ConflictError> {
    let refuse = ConflictError::Refused;
    if !instance.profile_dir(profile).is_dir() {
        return Err(refuse(format!("the Instance has no profile `{profile}`")));
    }
    let list = instance.profile_dir(profile).join("modlist.txt");
    let bytes = nvmod_formats::text::read_limited(&list)
        .map_err(|e| refuse(format!("{}: {e}", list.display())))?;
    let modlist = nvmod_formats::Modlist::parse(&nvmod_formats::text::decode(&bytes));
    let mut layers = Vec::new();
    let overwrite = instance.root.join(nvmod_stage::OVERWRITE_DIR);
    if overwrite.is_dir() {
        layers.push(Layer {
            name: OVERWRITE_SOURCE.to_owned(),
            kind: Kind::Overwrite,
            files: unique(nvmod_stage::mod_files(&overwrite)),
        });
    }
    for entry in modlist.enabled_mods() {
        let dir = instance.mod_dir(&entry.name);
        if dir.is_dir() {
            layers.push(Layer {
                name: entry.name.clone(),
                kind: Kind::Mod,
                files: unique(nvmod_stage::mod_files(&dir)),
            });
        }
    }
    if with_base {
        let data = find_ci(&instance.config.base, "Data")
            .filter(|p| p.is_dir())
            .ok_or_else(|| {
                refuse(format!(
                    "the base copy {} has no Data folder",
                    instance.config.base.display()
                ))
            })?;
        let mut files = Vec::new();
        collect_base(&data, "", &mut files);
        layers.push(Layer {
            name: BASE_SOURCE.to_owned(),
            kind: Kind::Base,
            files: unique(files),
        });
    }
    Ok(layers)
}

/// The base copy's files: every file, `meta.ini` and `.mohidden` names included (they are game files there).
fn collect_base(dir: &std::path::Path, prefix: &str, out: &mut Vec<(String, std::path::PathBuf)>) {
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            collect_base(&entry.path(), &format!("{name}/"), out);
        } else if kind.is_file() {
            out.push((name, entry.path()));
        }
    }
}

/// Index `layers`, which are given from the highest priority to the lowest.
#[must_use]
pub fn index_of(layers: &[Layer]) -> FileIndex {
    let mut index = FileIndex::new();
    for layer in layers {
        for file in &layer.files {
            index.insert(&layer.name, file);
        }
    }
    index
}

/// Compute the conflicts of `layers` (highest priority first). Base layers are left out.
#[must_use]
pub fn analyze(profile: &str, layers: &[Layer]) -> Report {
    let layers: Vec<Layer> = layers
        .iter()
        .filter(|l| l.kind != Kind::Base)
        .cloned()
        .collect();
    let index = index_of(&layers);
    let mut wins: BTreeMap<&str, usize> = BTreeMap::new();
    let mut losses: BTreeMap<&str, usize> = BTreeMap::new();
    let mut over: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut under: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut conflicts = Vec::new();
    for (key, providers) in index.iter() {
        let [winner, losers @ ..] = providers else {
            continue;
        };
        if losers.is_empty() {
            continue;
        }
        *wins.entry(winner.source.as_str()).or_default() += 1;
        for loser in losers {
            *losses.entry(loser.source.as_str()).or_default() += 1;
            over.entry(winner.source.as_str())
                .or_default()
                .insert(loser.source.as_str());
            under
                .entry(loser.source.as_str())
                .or_default()
                .insert(winner.source.as_str());
        }
        let source = |p: &nvmod_core::Provider| Source {
            name: p.source.clone(),
            path: p.path.clone(),
        };
        conflicts.push(Contested {
            path: key.to_owned(),
            winner: source(winner),
            losers: losers.iter().map(source).collect(),
        });
    }
    // Names in priority order for the lists of who replaces whom.
    let rank: BTreeMap<&str, usize> = layers
        .iter()
        .enumerate()
        .map(|(i, l)| (l.name.as_str(), i))
        .collect();
    let ordered = |set: Option<&BTreeSet<&str>>| -> Vec<String> {
        let mut names: Vec<&str> = set.into_iter().flatten().copied().collect();
        names.sort_by_key(|n| rank.get(n).copied().unwrap_or(usize::MAX));
        names.into_iter().map(str::to_owned).collect()
    };
    let mut position = 0;
    let mods = layers
        .iter()
        .map(|layer| {
            let name = layer.name.as_str();
            let (w, l) = (
                wins.get(name).copied().unwrap_or(0),
                losses.get(name).copied().unwrap_or(0),
            );
            let files = layer.files.len();
            if layer.kind == Kind::Mod {
                position += 1;
            }
            ModConflict {
                name: layer.name.clone(),
                kind: layer.kind,
                position: (layer.kind == Kind::Mod).then_some(position),
                files,
                wins: w,
                losses: l,
                overwrites: w > 0,
                overwritten: l > 0,
                mixed: w > 0 && l > 0,
                redundant: files > 0 && l == files,
                overwrites_mods: ordered(over.get(name)),
                overwritten_by_mods: ordered(under.get(name)),
            }
        })
        .collect();
    Report {
        profile: profile.to_owned(),
        mods,
        conflicts,
        excluded: Vec::new(),
        archives: None,
    }
}

/// Take the generated Mods out of `layers` (highest priority first) and say why. A generated Mod (the Toolbox's
/// LOD Mod) has thousands of files at the top priority and would drown the conflict view.
fn without_generated(instance: &NvmodInstance, layers: Vec<Layer>) -> (Vec<Layer>, Vec<Excluded>) {
    let mut excluded = Vec::new();
    let kept = layers
        .into_iter()
        .filter(|layer| {
            let generated = layer.kind == Kind::Mod
                && nvmod_formats::metaini::is_generated_mod_dir(&instance.mod_dir(&layer.name));
            if generated {
                excluded.push(Excluded {
                    name: layer.name.clone(),
                    reason: format!(
                        "`{}` is a generated Mod ({} file(s)); its paths are not listed as conflicts",
                        layer.name,
                        layer.files.len()
                    ),
                });
            }
            !generated
        })
        .collect();
    (kept, excluded)
}

/// The virtual Data folder of `layers` (highest priority first, the base copy last).
#[must_use]
pub fn tree_of(profile: &str, layers: &[Layer]) -> Tree {
    let index = index_of(layers);
    let entries = index
        .iter()
        .filter_map(|(key, providers)| {
            providers.first().map(|p| TreeEntry {
                path: key.to_owned(),
                provider: p.source.clone(),
                source_path: p.path.clone(),
            })
        })
        .collect();
    Tree {
        profile: profile.to_owned(),
        entries,
    }
}

/// The file a provider of the tree stands for: in Overwrite, in the Mod's folder or in the base copy's `Data`.
#[must_use]
pub fn provider_file(
    instance: &NvmodInstance,
    provider: &nvmod_core::Provider,
) -> Option<std::path::PathBuf> {
    let root = match provider.source.as_str() {
        BASE_SOURCE => find_ci(&instance.config.base, "Data")?,
        OVERWRITE_SOURCE => instance.root.join(nvmod_stage::OVERWRITE_DIR),
        name => instance.mod_dir(name),
    };
    Some(root.join(&provider.path))
}

/// The conflicts of `profile`, read from disk.
///
/// # Errors
/// See [`scan`].
pub fn conflicts(instance: &NvmodInstance, profile: &str) -> Result<Report, ConflictError> {
    let (layers, excluded) = without_generated(instance, scan(instance, profile, false)?);
    let mut report = analyze(profile, &layers);
    report.excluded = excluded;
    Ok(report)
}

/// The conflicts of `profile` with the archive axes: which archives load, and where an archive and a loose file or
/// another archive collide.
///
/// # Errors
/// See [`scan`].
pub fn conflicts_with_archives(
    instance: &NvmodInstance,
    profile: &str,
) -> Result<Report, ConflictError> {
    let (layers, excluded) = without_generated(instance, scan(instance, profile, false)?);
    let mut report = analyze(profile, &layers);
    report.excluded = excluded;
    report.archives = Some(archives::archive_report(instance, profile, &layers));
    Ok(report)
}

/// The archive audit of `profile` (see [`archives::audit`]).
///
/// # Errors
/// See [`scan`].
pub fn audit_archives(
    instance: &NvmodInstance,
    profile: &str,
) -> Result<Vec<nvmod_core::Finding>, ConflictError> {
    Ok(archives::audit(
        instance,
        profile,
        &scan(instance, profile, true)?,
    ))
}

/// The virtual Data folder of `profile`, read from disk.
///
/// # Errors
/// See [`scan`].
pub fn tree(instance: &NvmodInstance, profile: &str) -> Result<Tree, ConflictError> {
    Ok(tree_of(profile, &scan(instance, profile, true)?))
}

impl Report {
    /// Keep only what involves the layer `name`: its own row and the contested paths it is in.
    ///
    /// # Errors
    /// `Refused` when `name` is not a layer of the report.
    pub fn only(mut self, name: &str) -> Result<Report, ConflictError> {
        if let Some(left_out) = self.excluded.iter().find(|e| e.name == name) {
            return Err(ConflictError::Refused(left_out.reason.clone()));
        }
        if !self.mods.iter().any(|m| m.name == name) {
            return Err(ConflictError::Refused(format!(
                "the profile `{}` has no enabled Mod `{name}`",
                self.profile
            )));
        }
        self.mods.retain(|m| m.name == name);
        self.conflicts
            .retain(|c| c.winner.name == name || c.losers.iter().any(|l| l.name == name));
        Ok(self)
    }
}

impl Tree {
    /// Keep the entries at or under `prefix` (compared without case, `\` or `/`, whole folder names).
    #[must_use]
    pub fn under(mut self, prefix: &str) -> Tree {
        let prefix = nvmod_core::normalize_path(prefix);
        let prefix = prefix.trim_end_matches('/');
        if !prefix.is_empty() {
            let folder = format!("{prefix}/");
            self.entries
                .retain(|e| e.path == prefix || e.path.starts_with(&folder));
        }
        self
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::assert_is_empty)]
    use super::*;
    use std::time::Instant;

    fn layer(name: &str, files: &[&str]) -> Layer {
        Layer {
            name: name.to_owned(),
            kind: Kind::Mod,
            files: files.iter().map(|f| (*f).to_owned()).collect(),
        }
    }

    #[test]
    fn winners_losers_and_flags() {
        let layers = [
            layer("top", &["a.nif", "Meshes/b.nif"]),
            layer("mid", &["meshes/B.nif", "c.nif", "d.nif"]),
            layer("low", &["C.nif", "d.nif", "e.nif"]),
            layer("alone", &["x.nif"]),
        ];
        let r = analyze("P", &layers);
        let paths: Vec<_> = r.conflicts.iter().map(|c| c.path.as_str()).collect();
        assert_eq!(paths, ["c.nif", "d.nif", "meshes/b.nif"]);
        let b = &r.conflicts[2];
        assert_eq!(
            (b.winner.name.as_str(), b.winner.path.as_str()),
            ("top", "Meshes/b.nif")
        );
        assert_eq!(b.losers[0].path, "meshes/B.nif");
        let flag = |n: &str| r.mods.iter().find(|m| m.name == n).unwrap();
        assert!(flag("top").overwrites && !flag("top").overwritten && !flag("top").mixed);
        assert!(flag("mid").mixed);
        assert!(!flag("low").redundant);
        assert!(!flag("alone").overwrites && !flag("alone").overwritten);
        assert_eq!(flag("mid").overwrites_mods, ["low"]);
        assert_eq!(flag("mid").overwritten_by_mods, ["top"]);
        assert_eq!(flag("alone").position, Some(4));
    }

    #[test]
    fn a_mod_that_loses_everything_is_redundant() {
        let r = analyze("P", &[layer("a", &["x", "y"]), layer("b", &["X", "Y"])]);
        assert!(r.mods[1].redundant && !r.mods[0].redundant);
        let empty = analyze("P", &[layer("e", &[])]);
        assert!(!empty.mods[0].redundant);
    }

    #[test]
    fn the_base_copy_is_in_the_tree_not_in_the_conflicts() {
        let base = Layer {
            name: BASE_SOURCE.to_owned(),
            kind: Kind::Base,
            files: vec!["Meshes/b.nif".to_owned()],
        };
        let layers = [layer("a", &["meshes/b.nif"]), base];
        assert!(analyze("P", &layers).conflicts.is_empty());
        let t = tree_of("P", &layers);
        assert_eq!(t.entries[0].provider, "a");
    }

    #[test]
    fn tree_prefix_keeps_whole_folders() {
        let t = tree_of(
            "P",
            &[layer(
                "a",
                &["meshes/x.nif", "meshes2/y.nif", "Meshes/sub/z.nif"],
            )],
        );
        let kept: Vec<_> = t
            .under("Meshes\\")
            .entries
            .into_iter()
            .map(|e| e.path)
            .collect();
        assert_eq!(kept, ["meshes/sub/z.nif", "meshes/x.nif"]);
    }

    #[test]
    fn three_hundred_mods_over_fifty_thousand_paths_are_fast() {
        let layers: Vec<Layer> = (0..300)
            .map(|m| {
                let files: Vec<String> = (0..50_000)
                    .filter(|p| (p * 7 + m) % 10 == 0)
                    .map(|p| format!("Meshes/Folder{}/File{p}.nif", p % 97))
                    .collect();
                Layer {
                    name: format!("mod{m}"),
                    kind: Kind::Mod,
                    files,
                }
            })
            .collect();
        // Only the analysis is timed: building the 1.5 million fixture paths in a debug build took most of the 10 s on
        // the macOS runner and is not what the bound is about.
        let start = Instant::now();
        let r = analyze("P", &layers);
        let took = start.elapsed();
        assert!(!r.conflicts.is_empty());
        assert!(took.as_secs() < 10, "took {took:?}");
    }
}
