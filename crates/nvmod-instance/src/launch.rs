// SPDX-License-Identifier: GPL-3.0-only
//! The launch plan of the vanilla plus xNVSE path: what a person or a launcher would run to start the game from
//! an Instance's base copy, per platform. It is a description; nothing here starts a program.
//!
//! On Linux the Windows game runs under Proton, so the entry invokes Proton. On Windows the entry invokes the
//! game executable directly. xNVSE starts through `nvse_loader.exe` when the base copy has it; otherwise the plan
//! falls back to the vanilla `FalloutNV.exe` and says that no NVSE plugin will load. Native nv-rs launch is not
//! part of this plan (docs/DECISIONS.md).

use crate::{NvmodInstance, find_ci};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;

/// The Proton program named in the plan when none is given.
pub const DEFAULT_PROTON: &str = "proton";
/// The xNVSE loader.
pub const NVSE_LOADER: &str = "nvse_loader.exe";
/// The game executable.
pub const GAME_EXE: &str = "FalloutNV.exe";

/// One way to start the game.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchEntry {
    /// `linux` or `windows`.
    pub platform: String,
    /// The program to run.
    pub program: String,
    /// Its arguments.
    pub args: Vec<String>,
    /// Environment variables to set.
    pub env: BTreeMap<String, String>,
    /// The folder to run in.
    pub working_dir: String,
}

/// How the game is started from an Instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LaunchPlan {
    /// The name of the path: `vanilla+xnvse`.
    pub path: String,
    /// Whether the plan starts xNVSE (`nvse_loader.exe` exists in the base copy).
    pub extender: bool,
    /// What the plan does, and what it cannot do.
    pub note: String,
    /// One entry per platform.
    pub entries: Vec<LaunchEntry>,
    /// Where native nv-rs launch stands.
    pub nv_rs: String,
}

/// The plan for `instance`. `proton` is the Proton program to name on Linux ([`DEFAULT_PROTON`] when `None`).
#[must_use]
pub fn launch_plan(instance: &NvmodInstance, proton: Option<&str>) -> LaunchPlan {
    let base = &instance.config.base;
    let loader = find_ci(base, NVSE_LOADER);
    let extender = loader.is_some();
    let exe = loader
        .or_else(|| find_ci(base, GAME_EXE))
        .unwrap_or_else(|| base.join(GAME_EXE));
    let show = |p: &Path| p.display().to_string();
    let dir = show(base);
    let env = BTreeMap::from([(
        "STEAM_COMPAT_DATA_PATH".to_owned(),
        show(&instance.root.join("compatdata")),
    )]);
    let note = if extender {
        "vanilla plus xNVSE: the game starts through nvse_loader.exe, so NVSE plugins load"
            .to_owned()
    } else {
        format!(
            "vanilla: the base copy has no {NVSE_LOADER}, so the game starts without xNVSE and no NVSE plugin loads"
        )
    };
    LaunchPlan {
        path: "vanilla+xnvse".to_owned(),
        extender,
        note,
        entries: vec![
            LaunchEntry {
                platform: "linux".to_owned(),
                program: proton.unwrap_or(DEFAULT_PROTON).to_owned(),
                args: vec!["run".to_owned(), show(&exe)],
                env,
                working_dir: dir.clone(),
            },
            LaunchEntry {
                platform: "windows".to_owned(),
                program: show(&exe),
                args: Vec::new(),
                env: BTreeMap::new(),
                working_dir: dir,
            },
        ],
        nv_rs: "native nv-rs launch is not part of this milestone; on Linux it ends at an installed, Doctor-clean Instance".to_owned(),
    }
}

impl LaunchPlan {
    /// The plan as text for people.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut lines = vec![format!("launch plan ({}): {}", self.path, self.note)];
        for e in &self.entries {
            let mut parts: Vec<String> = e.env.iter().map(|(k, v)| format!("{k}={v}")).collect();
            parts.push(e.program.clone());
            parts.extend(e.args.iter().cloned());
            lines.push(format!("  {}: {}", e.platform, parts.join(" ")));
        }
        lines.push(format!("  nv-rs: {}", self.nv_rs));
        lines.join("\n") + "\n"
    }
}
