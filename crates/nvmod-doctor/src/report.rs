// SPDX-License-Identifier: GPL-3.0-only
//! The report and its JSON shape.

use nvmod_catalog::Kind;
use nvmod_core::{Finding, Severity};
use nvmod_pe::{PeReport, Version4};
use serde::Serialize;

/// The game executable.
#[derive(Debug, Clone, Serialize)]
pub struct ExeReport {
    /// File name as found in the game folder.
    pub file: String,
    /// Version resource file version.
    pub file_version: Option<Version4>,
    /// Large-address-aware flag (a 4GB-patched executable has it).
    pub large_address_aware: bool,
    /// The executable imports `nvse_steam_loader.dll`, so xNVSE loads without `nvse_loader.exe`.
    pub imports_nvse_steam_loader: bool,
    /// `FalloutNV_backup.exe` exists next to it.
    pub backup_present: bool,
}

/// The game folder.
#[derive(Debug, Clone, Default, Serialize)]
pub struct GameReport {
    /// The folder, when known and present.
    pub path: Option<String>,
    /// The executable, when found and readable.
    pub exe: Option<ExeReport>,
    /// File version of `nvse_1_4.dll`, when present.
    pub xnvse_version: Option<Version4>,
    /// `nvse_loader.exe` exists.
    pub nvse_loader_present: bool,
}

/// One plugin DLL of the merged view.
#[derive(Debug, Clone, Serialize)]
pub struct PluginReport {
    /// The DLL's file name as the winning mod spells it.
    pub dll: String,
    /// The mod whose file wins.
    pub winner: String,
    /// Mods whose copy is hidden by the winner.
    pub shadowed: Vec<String>,
    /// Catalogue id, when the catalogue knows the DLL.
    pub catalog_id: Option<String>,
    /// `user` when the entry comes from the player's overlay; left out for a shipped entry or an unknown DLL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub catalog_source: Option<&'static str>,
    /// What provides it on nv-rs (`external` when the catalogue does not say, which on nv-rs means unknown).
    pub nv_rs: Kind,
    /// What the PE inspector found, `None` when the file could not be read as a PE file.
    pub pe: Option<PeReport>,
}

/// Counts for a quick look.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    /// Plugin DLLs in the merged view.
    pub plugins: usize,
    /// Of those, in the catalogue.
    pub catalogued: usize,
    /// Findings of severity info.
    pub info: usize,
    /// Findings of severity warning.
    pub warnings: usize,
    /// Findings of severity error.
    pub errors: usize,
}

impl Summary {
    /// Count `findings` and `plugins`.
    #[must_use]
    pub fn of(findings: &[Finding], plugins: &[PluginReport]) -> Summary {
        let count = |s| findings.iter().filter(|f| f.severity == s).count();
        Summary {
            plugins: plugins.len(),
            catalogued: plugins.iter().filter(|p| p.catalog_id.is_some()).count(),
            info: count(Severity::Info),
            warnings: count(Severity::Warning),
            errors: count(Severity::Error),
        }
    }
}

/// The result of one doctor run.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// Version of this JSON layout.
    pub schema_version: u32,
    /// The instance folder.
    pub instance: String,
    /// The profile checked.
    pub profile: String,
    /// The game folder facts.
    pub game: GameReport,
    /// The plugins of the merged view.
    pub plugins: Vec<PluginReport>,
    /// What was found, in the order the checks ran.
    pub findings: Vec<Finding>,
    /// Counts.
    pub summary: Summary,
}

impl Report {
    /// The most serious finding, if any.
    #[must_use]
    pub fn worst(&self) -> Option<Severity> {
        self.findings.iter().map(|f| f.severity).max()
    }

    /// The report as text for people.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut lines = vec![format!(
            "nvmod doctor: profile `{}` of {}",
            self.profile, self.instance
        )];
        if let Some(exe) = &self.game.exe {
            lines.push(format!(
                "game: {} {} (4GB/large address aware: {}), xNVSE {}",
                exe.file,
                exe.file_version
                    .map_or_else(|| "unknown version".to_owned(), |v| v.to_string()),
                if exe.large_address_aware { "yes" } else { "no" },
                self.game
                    .xnvse_version
                    .map_or_else(|| "not found".to_owned(), |v| v.to_string()),
            ));
        }
        lines.push(format!(
            "plugins ({}, {} in the catalogue):",
            self.summary.plugins, self.summary.catalogued
        ));
        for p in &self.plugins {
            let state = match (&p.pe, &p.catalog_id) {
                (None, _) => "unreadable".to_owned(),
                (Some(pe), id) => format!(
                    "{}{}",
                    if pe.nvse.is_plugin {
                        "nvse plugin"
                    } else {
                        "not an nvse plugin"
                    },
                    id.as_ref()
                        .map_or(", not catalogued".to_owned(), |i| format!(
                            ", catalogue `{i}`"
                        ))
                ),
            };
            lines.push(format!(
                "  {} <- {} ({state}; nv-rs: {})",
                p.dll,
                p.winner,
                p.nv_rs.label()
            ));
        }
        lines.push(format!(
            "findings: {} error(s), {} warning(s), {} info",
            self.summary.errors, self.summary.warnings, self.summary.info
        ));
        for f in &self.findings {
            lines.push(format!(
                "  [{}] {}: {} ({})",
                f.severity, f.subject, f.message, f.code
            ));
        }
        lines.join("\n") + "\n"
    }
}
