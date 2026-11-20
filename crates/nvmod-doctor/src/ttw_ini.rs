// SPDX-License-Identifier: GPL-3.0-only
//! The Doctor's check of the `Fallout.ini` a Tale of Two Wastelands Profile starts the game with. The Profile's
//! plugin list holding `TaleOfTwoWastelands.esm` is the test (there is no flag in the Instance). Three values must
//! differ from the game's own; raul writes them before the game starts, so a file that has other values was edited
//! or was never started by raul. A file that does not exist yet is not a finding: it is made at the first start.

use nvmod_core::{Finding, Severity};
use nvmod_formats::ini::Ini;
use nvmod_instance::{NvmodInstance, find_ci};
use nvmod_loadorder::read_slots;
use std::path::PathBuf;

const MASTER: &str = "TaleOfTwoWastelands.esm";

/// One value the game's INI needs: where it is, what it must be, and what goes wrong without it.
struct Need {
    code: &'static str,
    section: &'static str,
    key: &'static str,
    value: &'static str,
    effect: &'static str,
}

const NEEDS: [Need; 3] = [
    Need {
        code: "ttw-ini.start-quest",
        section: "General",
        key: "SCharGenQuest",
        value: "001FFFF8",
        effect: "a new game starts the New Vegas opening, not the choice of wasteland",
    },
    Need {
        code: "ttw-ini.lip-files",
        section: "BackgroundLoad",
        key: "bBackgroundLoadLipFiles",
        value: "1",
        effect: "Tale of Two Wastelands' scripts show the \"Misconfigured INI settings detected\" box",
    },
    Need {
        code: "ttw-ini.intro-movie",
        section: "General",
        key: "sIntroMovie",
        value: "",
        effect: "the New Vegas intro film plays before the main menu",
    },
];

/// The `Fallout.ini` of the Profile's run folder, as the game reads it.
fn fallout_ini(instance: &NvmodInstance, profile: &str) -> Option<PathBuf> {
    let dir = instance
        .root
        .join("run")
        .join(profile)
        .join("userdata")
        .join("Documents")
        .join("My Games")
        .join("FalloutNV");
    find_ci(&dir, "Fallout.ini")
}

pub(crate) fn check(instance: &NvmodInstance, profile: &str, findings: &mut Vec<Finding>) {
    let Ok(slots) = read_slots(&instance.profile_dir(profile)) else {
        return;
    };
    if !slots
        .iter()
        .any(|s| s.active && s.name.eq_ignore_ascii_case(MASTER))
    {
        return;
    }
    let Some(path) = fallout_ini(instance, profile) else {
        return;
    };
    let Ok(bytes) = std::fs::read(&path) else {
        return;
    };
    let ini = Ini::parse(&String::from_utf8_lossy(&bytes));
    for need in &NEEDS {
        let found = ini.get_first(need.section, need.key);
        if found.is_some_and(|v| v.eq_ignore_ascii_case(need.value)) {
            continue;
        }
        let wanted = if need.value.is_empty() {
            "empty".to_owned()
        } else {
            need.value.to_owned()
        };
        let now = match found {
            None => format!("has no {} line", need.key),
            Some("") => format!("{} is empty", need.key),
            Some(v) => format!("{} is {v}", need.key),
        };
        findings.push(Finding::new(
            need.code,
            Severity::Warning,
            "Fallout.ini",
            format!(
                "this Profile has Tale of Two Wastelands, but its Fallout.ini {now} under [{}]; it must be {wanted}, or {}; raul sets it when the game starts",
                need.section, need.effect
            ),
        ));
    }
}
