// SPDX-License-Identifier: GPL-3.0-only
//! The read-only NVSE doctor (milestone M1). It answers, without running anything and without writing anything:
//! which NVSE plugins does this profile's merged view contain, which file wins, what does each one declare, and
//! what is wrong around them (game executable, xNVSE files, launch target, `meta.ini` ids, profile lists).
//!
//! Facts the checks rest on that nobody has verified yet are listed in `docs/DECISIONS.md` ("Unverified
//! assumptions"); the constants below name them.

mod base;
mod game;
mod order;
mod plugins;
mod profile;
mod report;

pub use base::{BaseExe, BaseReport, BaseSummary, check_base, check_instance};
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
