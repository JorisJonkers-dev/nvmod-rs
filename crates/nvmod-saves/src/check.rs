// SPDX-License-Identifier: GPL-3.0-only
//! `saves check`: the warning of MO2's Saves tab, a save made with a plugin the profile does not have active.

use crate::{Error, Location, save_files};
use nvmod_formats::plugins::parse_plugin_list;
use nvmod_formats::savegame;
use nvmod_instance::{NvmodInstance, find_ci};
use serde::Serialize;

/// The result for one save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SaveCheck {
    /// The file name.
    pub name: String,
    /// `ok`, `missing-plugins` or `unreadable`.
    pub status: &'static str,
    /// The plugins of the save that the profile has not active (in the save's order).
    pub missing: Vec<String>,
    /// Why the header could not be read.
    pub problem: Option<String>,
}

/// What `check` found.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CheckReport {
    /// The profile.
    pub profile: String,
    /// The save folder that was checked.
    pub dir: String,
    /// Number of active plugins of the profile.
    pub active: usize,
    /// Every save.
    pub saves: Vec<SaveCheck>,
    /// True when every save is `ok`.
    pub clean: bool,
}

/// Check every save of `location` against the active plugins of `profile` (`plugins.txt`; `FalloutNV.esm` is
/// always active). Plugin names are compared without regard to case.
///
/// # Errors
/// `Refused` when the profile has no `plugins.txt`.
pub fn check(
    instance: &NvmodInstance,
    profile: &str,
    location: &Location,
) -> Result<CheckReport, Error> {
    let dir = instance.profile_dir(profile);
    let path = find_ci(&dir, "plugins.txt")
        .ok_or_else(|| Error::Refused(format!("profile `{profile}` has no plugins.txt")))?;
    let bytes = nvmod_formats::text::read_limited(&path)
        .map_err(|e| Error::Refused(format!("{}: {e}", path.display())))?;
    let mut active: Vec<String> = parse_plugin_list(&bytes)
        .into_iter()
        .map(|n| n.to_ascii_lowercase())
        .collect();
    active.push("falloutnv.esm".to_owned());
    let saves: Vec<SaveCheck> = save_files(&location.dir)
        .into_iter()
        .map(|(name, file)| match savegame::read_header(&file) {
            Err(e) => SaveCheck {
                name,
                status: "unreadable",
                missing: Vec::new(),
                problem: Some(e.to_string()),
            },
            Ok(h) => {
                let missing: Vec<String> = h
                    .plugins
                    .into_iter()
                    .filter(|p| !active.contains(&p.to_ascii_lowercase()))
                    .collect();
                SaveCheck {
                    name,
                    status: if missing.is_empty() {
                        "ok"
                    } else {
                        "missing-plugins"
                    },
                    missing,
                    problem: None,
                }
            }
        })
        .collect();
    Ok(CheckReport {
        profile: profile.to_owned(),
        dir: location.dir.display().to_string(),
        active: active.len() - 1,
        clean: saves.iter().all(|s| s.status == "ok"),
        saves,
    })
}
