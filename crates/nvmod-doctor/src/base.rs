// SPDX-License-Identifier: GPL-3.0-only
//! The Doctor on a base copy of the game: the storefront, the executable and the `Data` folder. Read only.
//! A 4GB-patched executable is accepted, as nv-rs accepts it (its check hashes only data sections,
//! docs/DECISIONS.md); an executable that is not the supported build is reported as unknown.

use crate::SUPPORTED_GAME_VERSION;
use nvmod_catalog::Catalog;
use nvmod_core::{Finding, Severity};
use nvmod_instance::{NvmodInstance, Storefront, find_ci};
use nvmod_pe::{Version4, inspect_file};
use serde::Serialize;
use std::path::Path;

const EXE: &str = "FalloutNV.exe";

/// The executable of the base copy.
#[derive(Debug, Clone, Serialize)]
pub struct BaseExe {
    /// File version from the version resource.
    pub file_version: Option<Version4>,
    /// Large-address-aware flag (a 4GB-patched executable has it).
    pub large_address_aware: bool,
}

/// Counts of findings.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct BaseSummary {
    /// Findings of severity info.
    pub info: usize,
    /// Findings of severity warning.
    pub warnings: usize,
    /// Findings of severity error.
    pub errors: usize,
}

/// The result of the Doctor on a base copy.
#[derive(Debug, Clone, Serialize)]
pub struct BaseReport {
    /// Version of this JSON layout.
    pub schema_version: u32,
    /// The base folder.
    pub base: String,
    /// The storefront the copy came from.
    pub storefront: Storefront,
    /// The executable, when found and readable.
    pub exe: Option<BaseExe>,
    /// What was found.
    pub findings: Vec<Finding>,
    /// Counts.
    pub summary: BaseSummary,
}

impl BaseReport {
    fn recount(&mut self) {
        let count = |s| self.findings.iter().filter(|f| f.severity == s).count();
        self.summary = BaseSummary {
            info: count(Severity::Info),
            warnings: count(Severity::Warning),
            errors: count(Severity::Error),
        };
    }

    /// The most serious finding, if any.
    #[must_use]
    pub fn worst(&self) -> Option<Severity> {
        self.findings.iter().map(|f| f.severity).max()
    }

    /// The report as text for people.
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut lines = vec![format!(
            "nvmod doctor: base copy {} ({})",
            self.base, self.storefront
        )];
        if let Some(exe) = &self.exe {
            lines.push(format!(
                "executable: {} {} (4GB/large address aware: {})",
                EXE,
                exe.file_version
                    .map_or_else(|| "unknown version".to_owned(), |v| v.to_string()),
                if exe.large_address_aware { "yes" } else { "no" }
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

/// Check the base copy in `dir`.
///
/// # Errors
/// The reason, when the base is not a folder or is a storefront nvmod refuses (Epic, unknown).
pub fn check_base(dir: &Path) -> Result<BaseReport, String> {
    if !dir.is_dir() {
        return Err(format!("{} is not a folder", dir.display()));
    }
    let storefront = Storefront::detect(dir);
    if let Some(reason) = storefront.refusal() {
        return Err(reason.to_owned());
    }
    let mut findings = Vec::new();
    let exe = check_exe(dir, &mut findings);
    if find_ci(dir, "Data").is_none_or(|p| !p.is_dir()) {
        findings.push(Finding::new(
            "base.data-missing",
            Severity::Error,
            "Data",
            "the base copy has no Data folder",
        ));
    }
    let mut report = BaseReport {
        schema_version: crate::SCHEMA_VERSION,
        base: dir.display().to_string(),
        storefront,
        exe,
        findings,
        summary: BaseSummary::default(),
    };
    report.recount();
    Ok(report)
}

/// Check an nvmod Instance: its base copy and, when it has profiles, the Staging tree of `profile` (or of the
/// only profile), its Load order (missing masters; the order loot-rs would give, from `masterlist` when one is
/// given, else from the plugin headers only; file times) and its NVSE plugins against `catalog`.
///
/// # Errors
/// The reason, when the base copy is refused or `profile` names no profile of the Instance.
pub fn check_instance(
    instance: &NvmodInstance,
    profile: Option<&str>,
    masterlist: Option<&Path>,
    catalog: &Catalog,
) -> Result<BaseReport, String> {
    let mut report = check_base(&instance.config.base)?;
    let profile = match (profile, instance.profiles().is_empty()) {
        (_, true) if profile.is_none() => return Ok(report),
        _ => instance
            .resolve_profile(profile)
            .map_err(|why| why.to_string())?,
    };
    report
        .findings
        .extend(nvmod_stage::doctor_findings(instance, &profile));
    let game = report.exe.as_ref().and_then(|e| e.file_version);
    crate::order::check(
        instance,
        &profile,
        &crate::order::Inputs {
            masterlist,
            game,
            catalog,
        },
        &mut report.findings,
    );
    report.recount();
    Ok(report)
}

fn check_exe(dir: &Path, findings: &mut Vec<Finding>) -> Option<BaseExe> {
    let Some(path) = find_ci(dir, EXE) else {
        findings.push(Finding::new(
            "base.exe-missing",
            Severity::Error,
            EXE,
            "not found in the base copy",
        ));
        return None;
    };
    let report = match inspect_file(&path) {
        Ok(report) => report,
        Err(why) => {
            findings.push(Finding::new(
                "base.exe-unreadable",
                Severity::Error,
                EXE,
                why.to_string(),
            ));
            return None;
        }
    };
    let known = report.is_x86_32bit() && report.file_version == Some(SUPPORTED_GAME_VERSION);
    if !known {
        findings.push(Finding::new(
            "base.exe-unknown",
            Severity::Error,
            EXE,
            format!(
                "not the build nv-rs supports ({SUPPORTED_GAME_VERSION}, 32-bit x86): found version {}",
                report
                    .file_version
                    .map_or_else(|| "none".to_owned(), |v| v.to_string())
            ),
        ));
    } else if report.large_address_aware {
        findings.push(Finding::new(
            "base.exe-4gb-patched",
            Severity::Info,
            EXE,
            "4GB-patched (large address aware); nv-rs accepts it",
        ));
    }
    Some(BaseExe {
        file_version: report.file_version,
        large_address_aware: report.large_address_aware,
    })
}
