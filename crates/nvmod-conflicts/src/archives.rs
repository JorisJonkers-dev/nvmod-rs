// SPDX-License-Identifier: GPL-3.0-only
//! The archive axes of the conflicts, and the audit of which archives the game loads (read only).
//!
//! Fallout: New Vegas loads the archives named in `SArchiveList` (`[Archive]` of `fallout.ini`, in that order)
//! and, for each active plugin, the archive named like the plugin. The rules below say where each fact comes from:
//!
//! - **Plugin-named archives** (`Foo.esp` loads `Foo.bsa`, and `Foo - Main.bsa` style names): taken from the
//!   nv-rs research note (`docs/research/nv-rs-build-and-install.md`), not read from the original engine. Only the
//!   exact name and the `<plugin> - <suffix>` form are modelled. `unverified`.
//! - **Archive against archive**: the archive loaded first wins (`SArchiveList` order, then the plugins in Load
//!   order). From the same note. `unverified`.
//! - **Loose against archive**: a loose file wins over every archive when `bInvalidateOlderFiles=1`. The same
//!   note says this is the default; MO2 forces the value in `initweaks.ini` (read from MO2's source). With the
//!   value off the engine compares file times, which is not modelled; the report says so. `unverified` for the
//!   engine's side.
//!
//! Only archives of Mods and Overwrite take part: the base copy's own archives are neither audited nor compared.

use crate::{BASE_SOURCE, Kind, Layer, OVERWRITE_SOURCE, index_of};
use nvmod_core::{Finding, Severity};
use nvmod_formats::ini::Ini;
use nvmod_instance::{NvmodInstance, find_ci};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

/// The dummy archive MO2 puts first in `SArchiveList` when archive invalidation is on.
pub const INVALIDATION_BSA: &str = "Fallout - Invalidation.bsa";
/// Why the order of an archive conflict is not certain, repeated in the JSON.
pub const BASIS: &str = "from the nv-rs research note, not read from the original engine";

/// What the INIs and the profile settings say about archives.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settings {
    /// `SArchiveList` in order; `None` when no INI was found (then nothing can be said about it).
    pub archive_list: Option<Vec<String>>,
    /// `bInvalidateOlderFiles` from `[Archive]` of `initweaks.ini`, else of `fallout.ini`.
    pub invalidate_older_files: Option<bool>,
    /// `AutomaticArchiveInvalidation` of the profile's `settings.ini`.
    pub automatic_invalidation: bool,
    /// Whether `fallout.ini` of the profile (or the base copy's default INI) exists.
    pub ini_found: bool,
}

fn read_ini(path: &std::path::Path) -> Option<Ini> {
    let bytes = nvmod_formats::text::read_limited(path).ok()?;
    Some(Ini::parse(&nvmod_formats::text::decode(&bytes)))
}

fn truthy(value: &str) -> bool {
    matches!(value.trim().to_lowercase().as_str(), "1" | "true" | "yes")
}

/// Read the settings of `profile`: its `fallout.ini` (the base copy's `Fallout_default.ini` when the profile has
/// none), `initweaks.ini` and `settings.ini`.
#[must_use]
pub fn read_settings(instance: &NvmodInstance, profile: &str) -> Settings {
    let dir = instance.profile_dir(profile);
    let ini = find_ci(&dir, "fallout.ini")
        .and_then(|p| read_ini(&p))
        .or_else(|| {
            find_ci(&instance.config.base, "fallout_default.ini").and_then(|p| read_ini(&p))
        });
    let tweaks = find_ci(&dir, "initweaks.ini").and_then(|p| read_ini(&p));
    let settings = find_ci(&dir, "settings.ini").and_then(|p| read_ini(&p));
    let archive_list = ini.as_ref().map(|ini| {
        ini.get("Archive", "SArchiveList")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
            .collect()
    });
    let invalidate = |ini: &Option<Ini>| {
        ini.as_ref()
            .and_then(|i| i.get("Archive", "bInvalidateOlderFiles"))
            .map(truthy)
    };
    Settings {
        archive_list,
        invalidate_older_files: invalidate(&tweaks).or_else(|| invalidate(&ini)),
        automatic_invalidation: settings
            .as_ref()
            .and_then(|s| s.get("General", "AutomaticArchiveInvalidation"))
            .is_some_and(truthy),
        ini_found: ini.is_some(),
    }
}

/// The active plugins of `profile` in Load order (empty when the lists cannot be read).
fn active_plugins(instance: &NvmodInstance, profile: &str) -> Vec<String> {
    let dir = instance.profile_dir(profile);
    let read = |file: &str| {
        nvmod_formats::text::read_limited(&dir.join(file))
            .map(|b| nvmod_formats::plugins::parse_plugin_list(&b))
            .ok()
    };
    let active = read("plugins.txt").unwrap_or_default();
    let mut order = read("loadorder.txt").unwrap_or_default();
    for name in &active {
        if !order.iter().any(|o| o.eq_ignore_ascii_case(name)) {
            order.push(name.clone());
        }
    }
    order
        .into_iter()
        .filter(|n| active.iter().any(|a| a.eq_ignore_ascii_case(n)))
        .collect()
}

fn is_plugin(name: &str) -> bool {
    let lower = name.to_lowercase();
    [".esp", ".esm", ".esl"].iter().any(|e| lower.ends_with(e))
}

fn stem(name: &str) -> &str {
    name.rsplit_once('.').map_or(name, |(s, _)| s)
}

/// Whether the archive `archive` is named like the plugin `plugin`: the same name, or `<plugin> - <suffix>`.
fn named_like(archive: &str, plugin: &str) -> bool {
    let (a, p) = (stem(archive).to_lowercase(), stem(plugin).to_lowercase());
    a == p
        || a.strip_prefix(&p)
            .is_some_and(|rest| rest.starts_with(" - "))
}

fn is_archive(path: &str) -> bool {
    !path.contains('/') && path.to_lowercase().ends_with(".bsa")
}

/// One archive of the virtual Data folder.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Found {
    /// The file name as the providing layer spells it.
    name: String,
    /// The layer that provides it.
    layer: usize,
}

/// The root-level archives of the Mod and Overwrite layers, the winning provider for each name (the first layer,
/// highest priority).
fn found_archives(layers: &[Layer]) -> Vec<Found> {
    let mut seen = BTreeSet::new();
    let mut found = Vec::new();
    for (layer, l) in layers.iter().enumerate() {
        if l.kind == Kind::Base {
            continue;
        }
        for file in l.files.iter().filter(|f| is_archive(f)) {
            if seen.insert(file.to_lowercase()) {
                found.push(Found {
                    name: file.clone(),
                    layer,
                });
            }
        }
    }
    found
}

/// What loads an archive.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LoadedBy {
    /// Named in `SArchiveList`, at this position (1 is first).
    List(usize),
    /// Named like an active plugin.
    Plugin(String),
}

impl LoadedBy {
    fn text(&self) -> String {
        match self {
            LoadedBy::List(at) => format!("SArchiveList #{at}"),
            LoadedBy::Plugin(name) => format!("plugin {name}"),
        }
    }
}

/// One archive the game loads, with what was read of its directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArchiveInfo {
    /// The archive's file name.
    pub name: String,
    /// The Mod (or Overwrite) that provides it.
    pub provider: String,
    /// Why it loads: `SArchiveList #n` or `plugin <name>`.
    pub loaded_by: String,
    /// Files in its directory (0 when it could not be read).
    pub files: usize,
    /// Why the directory could not be read.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

fn layer_root(instance: &NvmodInstance, layer: &Layer) -> PathBuf {
    match layer.name.as_str() {
        OVERWRITE_SOURCE => instance.root.join(nvmod_stage::OVERWRITE_DIR),
        BASE_SOURCE => instance.config.base.clone(),
        name => instance.mod_dir(name),
    }
}

/// The archives of the Mods that the game loads, in load order: `SArchiveList` first, then the archives named
/// like the active plugins in Load order.
fn loaded_archives(
    layers: &[Layer],
    settings: &Settings,
    plugins: &[String],
) -> Vec<(Found, LoadedBy)> {
    let found = found_archives(layers);
    let mut loaded: Vec<(Found, LoadedBy)> = Vec::new();
    let mut take = |f: &Found, by: LoadedBy| {
        if !loaded
            .iter()
            .any(|(l, _)| l.name.eq_ignore_ascii_case(&f.name))
        {
            loaded.push((f.clone(), by));
        }
    };
    for (position, name) in settings.archive_list.iter().flatten().enumerate() {
        if let Some(f) = found.iter().find(|f| f.name.eq_ignore_ascii_case(name)) {
            take(f, LoadedBy::List(position + 1));
        }
    }
    for plugin in plugins {
        for f in found.iter().filter(|f| named_like(&f.name, plugin)) {
            take(f, LoadedBy::Plugin(plugin.clone()));
        }
    }
    loaded
}

/// Who provides one side of an archive conflict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Provider {
    /// The Mod (or Overwrite).
    #[serde(rename = "mod")]
    pub name: String,
    /// `loose` or `archive`.
    pub kind: &'static str,
    /// The archive's file name, for an archive.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub archive: Option<String>,
}

/// A path that an archive provides and something else provides too.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArchiveConflict {
    /// The lowercased path.
    pub path: String,
    /// What the engine would use.
    pub winner: Provider,
    /// What it would not use, the likeliest first.
    pub losers: Vec<Provider>,
    /// `loose-over-archive` or `archive-load-order`.
    pub rule: &'static str,
    /// The rule is not read from the engine's source.
    pub unverified: bool,
}

/// The archive part of a conflicts report.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ArchiveReport {
    /// `bInvalidateOlderFiles`, or `None` when no INI says. With it off, loose files do not always win; that is
    /// not modelled and the loose-over-archive rows are then only the likeliest outcome.
    pub invalidate_older_files: Option<bool>,
    /// Where the rules come from.
    pub basis: &'static str,
    /// The archives that load, in load order.
    pub archives: Vec<ArchiveInfo>,
    /// The paths where an archive and another provider collide.
    pub conflicts: Vec<ArchiveConflict>,
}

/// The archive conflicts of `layers` (highest priority first, as [`crate::scan`] returns them).
#[must_use]
pub fn archive_report(instance: &NvmodInstance, profile: &str, layers: &[Layer]) -> ArchiveReport {
    let settings = read_settings(instance, profile);
    let plugins = active_plugins(instance, profile);
    let loaded = loaded_archives(layers, &settings, &plugins);
    let mut infos = Vec::new();
    // path -> archives that hold it, in load order: (provider, archive name).
    let mut held: BTreeMap<String, Vec<(String, String)>> = BTreeMap::new();
    for (f, by) in &loaded {
        let layer = &layers[f.layer];
        let path = layer_root(instance, layer).join(&f.name);
        let (files, error) = match nvmod_bsa::read_file(&path) {
            Ok(dir) => {
                let count = dir.files.len();
                for e in dir.files {
                    held.entry(e.path)
                        .or_default()
                        .push((layer.name.clone(), f.name.clone()));
                }
                (count, None)
            }
            Err(why) => (0, Some(why.to_string())),
        };
        infos.push(ArchiveInfo {
            name: f.name.clone(),
            provider: layer.name.clone(),
            loaded_by: by.text(),
            files,
            error,
        });
    }
    let loose = index_of(
        &layers
            .iter()
            .filter(|l| l.kind != Kind::Base)
            .cloned()
            .collect::<Vec<_>>(),
    );
    let archive = |(m, a): &(String, String)| Provider {
        name: m.clone(),
        kind: "archive",
        archive: Some(a.clone()),
    };
    let mut conflicts = Vec::new();
    for (path, archives) in &held {
        let loose_winner = loose.winner(path);
        if let Some(l) = loose_winner {
            conflicts.push(ArchiveConflict {
                path: path.clone(),
                winner: Provider {
                    name: l.source.clone(),
                    kind: "loose",
                    archive: None,
                },
                losers: archives.iter().map(archive).collect(),
                rule: "loose-over-archive",
                unverified: true,
            });
        } else if let [first, rest @ ..] = archives.as_slice()
            && !rest.is_empty()
        {
            conflicts.push(ArchiveConflict {
                path: path.clone(),
                winner: archive(first),
                losers: rest.iter().map(archive).collect(),
                rule: "archive-load-order",
                unverified: true,
            });
        }
    }
    ArchiveReport {
        invalidate_older_files: settings.invalidate_older_files,
        basis: BASIS,
        archives: infos,
        conflicts,
    }
}

/// Every plugin name of the profile's lists, active or not.
fn listed_plugins(instance: &NvmodInstance, profile: &str) -> Vec<String> {
    let dir = instance.profile_dir(profile);
    let mut names: Vec<String> = Vec::new();
    for file in ["loadorder.txt", "plugins.txt"] {
        if let Ok(b) = nvmod_formats::text::read_limited(&dir.join(file)) {
            names.extend(nvmod_formats::plugins::parse_plugin_list(&b));
        }
    }
    names.retain(|n| is_plugin(n));
    names
}

/// `archive.corrupt-header` for every archive, `archive.not-listed` for those the game does not load.
fn unloaded_and_damaged(
    instance: &NvmodInstance,
    profile: &str,
    layers: &[Layer],
    settings: &Settings,
) -> Vec<Finding> {
    let plugins = listed_plugins(instance, profile);
    let loaded = loaded_archives(layers, settings, &active_plugins(instance, profile));
    let mut findings = Vec::new();
    for f in found_archives(layers) {
        let layer = &layers[f.layer];
        let subject = format!("{}: {}", layer.name, f.name);
        // A damaged header is reported whether or not the archive loads.
        if let Err(why) = nvmod_bsa::read_file(&layer_root(instance, layer).join(&f.name)) {
            findings.push(Finding::new(
                "archive.corrupt-header",
                Severity::Error,
                subject.clone(),
                format!("the archive's directory cannot be read: {why}"),
            ));
        }
        if settings.archive_list.is_none()
            || loaded
                .iter()
                .any(|(l, _)| l.name.eq_ignore_ascii_case(&f.name))
        {
            continue;
        }
        findings.push(match plugins.iter().find(|p| named_like(&f.name, p)) {
            None => Finding::new(
                "archive.not-listed",
                Severity::Warning,
                subject,
                "the game never loads it: it is not named like a plugin and not in SArchiveList".to_owned(),
            ),
            Some(owner) => Finding::new(
                "archive.not-listed",
                Severity::Info,
                subject,
                format!("the game does not load it: its plugin {owner} is not active and it is not in SArchiveList"),
            ),
        });
    }
    findings
}

/// `archive.listed-missing`: an `SArchiveList` entry that nothing provides.
fn listed_missing(instance: &NvmodInstance, layers: &[Layer], settings: &Settings) -> Vec<Finding> {
    let data = find_ci(&instance.config.base, "Data");
    let provided = |name: &str| {
        layers
            .iter()
            .any(|l| l.files.iter().any(|f| f.eq_ignore_ascii_case(name)))
            || data.as_deref().is_some_and(|d| find_ci(d, name).is_some())
    };
    settings
        .archive_list
        .iter()
        .flatten()
        .filter(|name| {
            let made_at_launch =
                settings.automatic_invalidation && name.eq_ignore_ascii_case(INVALIDATION_BSA);
            !made_at_launch && !provided(name)
        })
        .map(|name| {
            Finding::new(
                "archive.listed-missing",
                Severity::Warning,
                name.clone(),
                "is named in SArchiveList but no Mod, Overwrite or the base copy provides it"
                    .to_owned(),
            )
        })
        .collect()
}

/// `archive.invalidation`: the profile asks for invalidation but the INI value or the list entry is missing.
fn invalidation(profile: &str, settings: &Settings) -> Vec<Finding> {
    let mut findings = Vec::new();
    if !settings.automatic_invalidation {
        return findings;
    }
    let mut add = |message: String| {
        findings.push(Finding::new(
            "archive.invalidation",
            Severity::Warning,
            profile,
            message,
        ));
    };
    if settings.invalidate_older_files != Some(true) {
        add("the profile turns archive invalidation on but bInvalidateOlderFiles=1 is not set in [Archive] of its INIs; loose files may lose to older archives".to_owned());
    }
    let entry = settings
        .archive_list
        .iter()
        .flatten()
        .any(|n| n.eq_ignore_ascii_case(INVALIDATION_BSA));
    if !entry {
        add(format!(
            "the profile turns archive invalidation on but SArchiveList has no `{INVALIDATION_BSA}` entry"
        ));
    }
    findings
}

/// The audit: `archive.not-listed`, `archive.listed-missing`, `archive.invalidation` and
/// `archive.corrupt-header` for `profile`. Read only. `layers` come from [`crate::scan`] with the base copy.
#[must_use]
pub fn audit(instance: &NvmodInstance, profile: &str, layers: &[Layer]) -> Vec<Finding> {
    let settings = read_settings(instance, profile);
    let mut findings = unloaded_and_damaged(instance, profile, layers, &settings);
    findings.extend(listed_missing(instance, layers, &settings));
    findings.extend(invalidation(profile, &settings));
    findings
}
