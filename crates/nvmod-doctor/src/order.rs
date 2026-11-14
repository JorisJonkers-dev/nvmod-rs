// SPDX-License-Identifier: GPL-3.0-only
//! The Doctor's checks of an nvmod Instance's Load order and of its NVSE plugins against the catalogue: missing
//! masters, a Load order that differs from what loot-rs would sort, plugin file times that do not encode the order,
//! and a catalogue requirement (game or xNVSE version) the installation does not meet. Read only.

use nvmod_catalog::Catalog;
use nvmod_core::{Finding, Severity};
use nvmod_instance::{NvmodInstance, find_ci};
use nvmod_loadorder::{Error, Slot, empty_masterlist, load_masterlist, propose, read_slots};
use nvmod_pe::{Version4, inspect_file};
use std::path::Path;

fn position(order: &[Slot], name: &str) -> Option<usize> {
    order.iter().position(|s| s.name.eq_ignore_ascii_case(name))
}

/// A plugin that is active needs every master of its header active and earlier in the Load order.
fn missing_masters(order: &[Slot], data: &Path, findings: &mut Vec<Finding>) {
    for (index, slot) in order.iter().enumerate().filter(|(_, s)| s.active) {
        let Some(path) = nvmod_loadorder::staged_file(data, &slot.name) else {
            continue;
        };
        let masters = match nvmod_loadorder::masters_of(&path) {
            Ok(masters) => masters,
            Err(why) => {
                findings.push(Finding::new(
                    "loadorder.plugin-unreadable",
                    Severity::Warning,
                    slot.name.clone(),
                    format!("its header cannot be read, so its masters are not checked: {why}"),
                ));
                continue;
            }
        };
        for master in masters {
            let problem = match position(order, &master) {
                None => "is not in the Load order",
                Some(at) if !order[at].active => "is in the Load order but not active",
                Some(at) if at > index => "loads after it",
                Some(_) => continue,
            };
            findings.push(Finding::new(
                "loadorder.missing-master",
                Severity::Error,
                slot.name.clone(),
                format!("its master {master} {problem}"),
            ));
        }
    }
}

fn differs_from_loot(
    instance: &NvmodInstance,
    profile: &str,
    masterlist: Option<&Path>,
    findings: &mut Vec<Finding>,
) {
    let list = match masterlist.map(load_masterlist) {
        None => empty_masterlist(),
        Some(Ok(list)) => list,
        Some(Err(Error::Refused(why) | Error::Failed(why))) => {
            findings.push(Finding::new(
                "loadorder.masterlist-unreadable",
                Severity::Warning,
                profile,
                format!("the Load order was not compared with loot-rs: {why}"),
            ));
            return;
        }
    };
    let proposal = match propose(instance, profile, &list) {
        Ok(proposal) => proposal,
        Err(Error::Refused(why) | Error::Failed(why)) => {
            findings.push(Finding::new(
                "loadorder.sort-failed",
                Severity::Warning,
                profile,
                why,
            ));
            return;
        }
    };
    if !proposal.reordered() {
        return;
    }
    let first = proposal
        .current
        .iter()
        .zip(&proposal.proposed)
        .position(|(a, b)| !a.name.eq_ignore_ascii_case(&b.name))
        .unwrap_or(0);
    let (now, wanted) = (&proposal.current[first], &proposal.proposed[first]);
    let (severity, basis) = if masterlist.is_some() {
        (Severity::Warning, "the masterlist and the plugin headers")
    } else {
        (
            Severity::Info,
            "the plugin headers only (no --masterlist given)",
        )
    };
    findings.push(Finding::new(
        "loadorder.differs-from-loot",
        severity,
        profile,
        format!(
            "loot-rs, using {basis}, would sort this Load order differently: position {} would hold {} instead of {}; `nvmod sort` shows the whole proposal",
            first + 1,
            wanted.name,
            now.name
        ),
    ));
}

/// The plugin files of the Staging tree must carry times that increase in Load order (nv-rs orders by file time).
fn times(order: &[Slot], data: &Path, profile: &str, findings: &mut Vec<Finding>) {
    let mut previous: Option<(&str, std::time::SystemTime)> = None;
    for slot in order {
        let Some(path) = nvmod_loadorder::staged_file(data, &slot.name) else {
            continue;
        };
        let Ok(time) = std::fs::metadata(&path).and_then(|m| m.modified()) else {
            continue;
        };
        if let Some((before, before_time)) = previous
            && time <= before_time
        {
            findings.push(Finding::new(
                "loadorder.times-out-of-order",
                Severity::Warning,
                profile,
                format!(
                    "the file time of {} is not later than that of {before}, so nv-rs would not load it after {before}; run `nvmod sort --apply`",
                    slot.name
                ),
            ));
            return;
        }
        previous = Some((&slot.name, time));
    }
}

fn version_of(path: &Path) -> Option<Version4> {
    inspect_file(path).ok().and_then(|pe| pe.file_version)
}

/// The file names of the NVSE plugin DLLs in the Staging tree's `Data`, sorted.
pub(crate) fn staged_dlls(data: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(data.join("nvse").join("plugins")) else {
        return Vec::new();
    };
    let mut dlls: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.to_lowercase().ends_with(".dll"))
        .collect();
    dlls.sort();
    dlls
}

/// NVSE plugin DLLs of the Staging tree against the catalogue's required game and xNVSE versions. A field the
/// catalogue does not record is reported as unknown; nothing is guessed.
fn nvse_plugins(
    instance: &NvmodInstance,
    data: &Path,
    game: Option<Version4>,
    catalog: &Catalog,
    findings: &mut Vec<Finding>,
) {
    let dlls = staged_dlls(data);
    let xnvse = find_ci(&instance.config.base, "nvse_1_4.dll")
        .as_deref()
        .and_then(version_of);
    for dll in dlls {
        let Some(plugin) = catalog.by_dll(&dll) else {
            continue;
        };
        let mut unknown = Vec::new();
        match plugin.game_version.as_deref().and_then(Version4::parse) {
            None => unknown.push("the game version it requires"),
            Some(wanted) => match game {
                Some(actual) if actual != wanted => findings.push(Finding::new(
                    "plugin.game-version-mismatch",
                    Severity::Warning,
                    plugin.dll.clone(),
                    format!("the catalogue says it requires game version {wanted}; the base copy's executable is {actual}"),
                )),
                Some(_) => {}
                None => findings.push(Finding::new(
                    "plugin.game-version-unknown",
                    Severity::Info,
                    plugin.dll.clone(),
                    format!("the catalogue says it requires game version {wanted}, but the base copy's executable version is not known"),
                )),
            },
        }
        match plugin.min_xnvse.as_deref().and_then(Version4::parse) {
            None => unknown.push("the lowest xNVSE it requires"),
            Some(wanted) => match xnvse {
                Some(actual) if actual < wanted => findings.push(Finding::new(
                    "plugin.xnvse-version-mismatch",
                    Severity::Warning,
                    plugin.dll.clone(),
                    format!("the catalogue says it requires xNVSE {wanted} or later; the xNVSE of the base copy is {actual}"),
                )),
                Some(_) => {}
                None => findings.push(Finding::new(
                    "plugin.xnvse-version-unknown",
                    Severity::Info,
                    plugin.dll.clone(),
                    format!("the catalogue says it requires xNVSE {wanted} or later, but no readable nvse_1_4.dll is in the base copy"),
                )),
            },
        }
        if !unknown.is_empty() {
            findings.push(Finding::new(
                "plugin.requirements-unknown",
                Severity::Info,
                plugin.dll.clone(),
                format!(
                    "the catalogue does not record {}: unknown",
                    unknown.join(" or ")
                ),
            ));
        }
    }
}

/// What the checks compare the Instance with.
pub(crate) struct Inputs<'a> {
    pub masterlist: Option<&'a Path>,
    pub game: Option<Version4>,
    pub catalog: &'a Catalog,
}

/// Every Load order and NVSE plugin check for `profile`. Needs the Staging tree; without one the Doctor's
/// `stage.missing` finding already says so and these checks have nothing to read.
pub(crate) fn check(
    instance: &NvmodInstance,
    profile: &str,
    inputs: &Inputs,
    findings: &mut Vec<Finding>,
) {
    let Inputs {
        masterlist,
        game,
        catalog,
    } = *inputs;
    let Ok(data) = nvmod_loadorder::staging_data(instance, profile) else {
        return;
    };
    nvse_plugins(instance, &data, game, catalog, findings);
    let order = match read_slots(&instance.profile_dir(profile)) {
        Ok(order) => order,
        Err(Error::Refused(why) | Error::Failed(why)) => {
            findings.push(Finding::new(
                "loadorder.lists-missing",
                Severity::Info,
                profile,
                why,
            ));
            return;
        }
    };
    missing_masters(&order, &data, findings);
    differs_from_loot(instance, profile, masterlist, findings);
    times(&order, &data, profile, findings);
}
