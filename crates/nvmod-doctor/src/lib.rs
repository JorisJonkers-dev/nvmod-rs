// SPDX-License-Identifier: GPL-3.0-only
//! The read-only NVSE doctor (milestone M1). It answers, without running anything and without writing anything:
//! which NVSE plugins does this profile's merged view contain, which file wins, what does each one declare, and
//! what is wrong around them (game executable, xNVSE files, launch target, `meta.ini` ids, profile lists).
//!
//! Facts the checks rest on that nobody has verified yet are listed in `docs/DECISIONS.md` ("Unverified
//! assumptions"); the constants below name them.

mod base;
mod builtin;
mod game;
mod order;
mod plugins;
mod profile;
mod readiness;
mod report;
mod ttw_ini;

pub use base::{BaseExe, BaseReport, BaseSummary, check_base, check_instance};
pub use builtin::{CapabilitiesStatus, CapabilitiesUse, ExtenderCoverage, NvRsReport};
pub use readiness::{CategoryReadiness, CommandReadiness, DataOnly, Readiness};
pub use report::{ExeReport, GameReport, PluginReport, Report, Summary};

use nvmod_catalog::Catalog;
use nvmod_core::Finding;
use nvmod_formats::{Mo2Instance, Mo2Profile};
use nvmod_pe::Version4;
use std::path::Path;

/// The game executable version the xNVSE of this plan targets (plan, section 6.1, item 2). Unverified.
pub const SUPPORTED_GAME_VERSION: Version4 = Version4([1, 4, 0, 525]);
/// The name under which files of the overwrite folder are indexed.
pub const OVERWRITE: &str = "<overwrite>";
/// The version of the report's JSON layout.
pub const SCHEMA_VERSION: u32 = 1;

/// What the player's catalogue overlay needs to say: files and entries that were ignored, and (for `dlls` present)
/// entries that are the player's own, not the shipped catalogue's.
pub(crate) fn overlay_findings(catalog: &Catalog, dlls: &[String]) -> Vec<Finding> {
    let mut found: Vec<Finding> = catalog
        .notes
        .iter()
        .map(|n| {
            Finding::new(
                n.code,
                nvmod_core::Severity::Warning,
                n.subject.clone(),
                n.message.clone(),
            )
        })
        .collect();
    for dll in dlls {
        if let Some(p) = catalog.by_dll(dll).filter(|p| !p.source.is_shipped()) {
            found.push(Finding::new(
                "catalog.user-entry",
                nvmod_core::Severity::Info,
                dll.clone(),
                format!(
                    "recognised from your own catalogue overlay (source = {}), id {}",
                    p.source.label(),
                    p.id
                ),
            ));
        }
    }
    found
}

/// Run every check.
#[must_use]
pub fn run(
    instance: &Mo2Instance,
    profile: &Mo2Profile,
    game_dir: Option<&Path>,
    catalog: &Catalog,
) -> Report {
    let mut findings: Vec<Finding> = Vec::new();
    let game = game::check(instance, game_dir, &mut findings);
    findings.extend(profile::check(instance, profile, catalog));
    let plugins = plugins::check(instance, profile, catalog, &mut findings);
    findings.extend(game::launch_target(instance, &game));
    let dlls: Vec<String> = plugins.iter().map(|p| p.dll.clone()).collect();
    findings.extend(overlay_findings(catalog, &dlls));
    let summary = Summary::of(&findings, &plugins);
    Report {
        schema_version: SCHEMA_VERSION,
        instance: instance.root.display().to_string(),
        profile: profile.name.clone(),
        game,
        plugins,
        findings,
        summary,
    }
}
