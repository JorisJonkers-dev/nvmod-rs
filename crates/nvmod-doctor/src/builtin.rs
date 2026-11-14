// SPDX-License-Identifier: GPL-3.0-only
//! What provides each catalogue entry on the Instance's target (ticket 29). On the vanilla target the script
//! extender is an external DLL the Doctor requires. On nv-rs it is built in: nothing is required, the coverage is
//! read at run time from the installed nv-rs's `capabilities.json` (never from the catalogue), a plugin DLL that is
//! built in is redundant, one the catalogue marks absent cannot load, and one that is external is reported unknown,
//! because nothing records that nv-rs loads third-party NVSE plugins.

use nvmod_catalog::{Catalog, Kind, Plugin};
use nvmod_core::{Finding, Severity, Target};
use nvmod_instance::{NvmodInstance, find_ci};
use nvmod_pe::{Version4, inspect_file};
use nvmod_tools::{Capabilities, CapabilitiesError};
use serde::Serialize;

/// How far the installed nv-rs's `capabilities.json` could be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilitiesStatus {
    /// Read, valid, and it lists every extender the catalogue says is built in.
    Used,
    /// There is no file.
    Missing,
    /// There is a file that cannot be used (not JSON, breaks the contract, newer schema).
    Invalid,
    /// Valid, but it lacks an extender the catalogue says is built in (an nv-rs older than the catalogue expects).
    Stale,
}

/// The `capabilities.json` the report used.
#[derive(Debug, Clone, Serialize)]
pub struct CapabilitiesUse {
    /// Where it was looked for.
    pub path: String,
    /// What came of it.
    pub status: CapabilitiesStatus,
    /// Why it was not used, when it was not.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// The coverage of one built-in extender, from `capabilities.json`.
#[derive(Debug, Clone, Serialize)]
pub struct ExtenderCoverage {
    /// The extender's id in `capabilities.json`.
    pub id: String,
    /// The catalogue entry that is built in through it.
    pub plugin: String,
    /// What the catalogue says (`builtin-full` or `builtin-partial`).
    pub kind: Kind,
    /// Commands implemented.
    pub implemented: u64,
    /// Commands that exist.
    pub total: u64,
    /// Commands not implemented.
    pub missing: u64,
}

/// The nv-rs part of a report.
#[derive(Debug, Clone, Serialize)]
pub struct NvRsReport {
    /// The capabilities file the coverage came from, or why there is none.
    pub capabilities: CapabilitiesUse,
    /// The coverage of each built-in extender the file lists.
    pub extenders: Vec<ExtenderCoverage>,
}

fn is_present(plugin: &Plugin, dlls: &[String]) -> bool {
    dlls.iter().any(|d| d.eq_ignore_ascii_case(&plugin.dll))
}

/// The vanilla target: the extender's DLL is in the base copy, at the version the catalogue records.
fn vanilla(instance: &NvmodInstance, catalog: &Catalog, findings: &mut Vec<Finding>) {
    for plugin in catalog
        .plugins
        .iter()
        .filter(|p| p.role.as_deref() == Some("extender"))
    {
        let Some(path) = find_ci(&instance.config.base, &plugin.dll) else {
            findings.push(Finding::new(
                "extender.missing",
                Severity::Error,
                plugin.dll.clone(),
                format!(
                    "{} is an external DLL on the vanilla target and {} is not in the base copy",
                    plugin.name, plugin.dll
                ),
            ));
            continue;
        };
        let wanted = plugin.version.as_deref().and_then(Version4::parse);
        let actual = inspect_file(&path).ok().and_then(|pe| pe.file_version);
        match (wanted, actual) {
            (None, _) => findings.push(Finding::new(
                "extender.version-unknown",
                Severity::Info,
                plugin.dll.clone(),
                format!("the catalogue does not record which version of {} it expects: unknown", plugin.name),
            )),
            (Some(wanted), Some(actual)) if actual != wanted => findings.push(Finding::new(
                "extender.version-mismatch",
                Severity::Warning,
                plugin.dll.clone(),
                format!("the catalogue expects {} {wanted}; the base copy has {actual}", plugin.name),
            )),
            (Some(_), Some(_)) => {}
            (Some(wanted), None) => findings.push(Finding::new(
                "extender.version-unknown",
                Severity::Info,
                plugin.dll.clone(),
                format!("the catalogue expects {} {wanted}, but the version of the DLL in the base copy cannot be read", plugin.name),
            )),
        }
    }
}

fn load(
    instance: &NvmodInstance,
    catalog: &Catalog,
    findings: &mut Vec<Finding>,
) -> (CapabilitiesUse, Option<Capabilities>) {
    let path = instance.nv_rs_capabilities();
    let shown = path.display().to_string();
    let mut warn = |code: &'static str, message: String| {
        findings.push(Finding::new(code, Severity::Warning, "nv-rs", message));
    };
    let loaded = match Capabilities::load(&path) {
        Ok(caps) => caps,
        Err(CapabilitiesError::Missing) => {
            warn(
                "capabilities.missing",
                format!(
                    "{shown} does not exist, so the installed nv-rs's coverage is unknown: no coverage was used (run `nvmod install-nv-rs`, or install a newer nv-rs)"
                ),
            );
            let reason = Some("the file does not exist".to_owned());
            return (
                CapabilitiesUse {
                    path: shown,
                    status: CapabilitiesStatus::Missing,
                    reason,
                },
                None,
            );
        }
        Err(CapabilitiesError::Invalid(why)) => {
            warn(
                "capabilities.invalid",
                format!("{shown} cannot be used ({why}): no coverage was used"),
            );
            return (
                CapabilitiesUse {
                    path: shown,
                    status: CapabilitiesStatus::Invalid,
                    reason: Some(why),
                },
                None,
            );
        }
    };
    let lacking: Vec<&str> = catalog
        .plugins
        .iter()
        .filter(|p| p.provided_by.nv_rs.kind.is_builtin())
        .filter_map(|p| p.provided_by.nv_rs.extender.as_deref())
        .filter(|id| loaded.extender(id).is_none())
        .collect();
    if lacking.is_empty() {
        return (
            CapabilitiesUse {
                path: shown,
                status: CapabilitiesStatus::Used,
                reason: None,
            },
            Some(loaded),
        );
    }
    let why = format!(
        "it does not list {}, which the catalogue says nv-rs builds in; it is from an nv-rs older than this catalogue expects",
        lacking.join(", ")
    );
    warn(
        "capabilities.stale",
        format!("{shown}: {why}; the coverage of the extenders it does list was used"),
    );
    (
        CapabilitiesUse {
            path: shown,
            status: CapabilitiesStatus::Stale,
            reason: Some(why),
        },
        Some(loaded),
    )
}

fn coverage_findings(
    plugin: &Plugin,
    coverage: &nvmod_tools::Coverage,
    path: &str,
    dll_present: bool,
    findings: &mut Vec<Finding>,
) {
    let line = format!(
        "built in on nv-rs, {} of {} commands implemented",
        coverage.implemented, coverage.total
    );
    let (severity, message) = if coverage.missing() == 0 {
        (Severity::Info, format!("{line} (coverage from {path})"))
    } else {
        let severity = if dll_present || plugin.role.as_deref() == Some("extender") {
            Severity::Warning
        } else {
            Severity::Info
        };
        let note = if plugin.provided_by.nv_rs.kind == Kind::BuiltinFull {
            "; the catalogue lists it as fully built in, the file says otherwise"
        } else {
            ""
        };
        (
            severity,
            format!(
                "{line}, {} missing{note} (coverage from {path}); which commands a Mod's scripts use is not checked",
                coverage.missing()
            ),
        )
    };
    findings.push(Finding::new(
        "extender.coverage",
        severity,
        plugin.name.clone(),
        message,
    ));
}

fn nv_rs(
    instance: &NvmodInstance,
    catalog: &Catalog,
    dlls: &[String],
    findings: &mut Vec<Finding>,
) -> NvRsReport {
    let (used, caps) = load(instance, catalog, findings);
    let mut extenders = Vec::new();
    for plugin in &catalog.plugins {
        let provided = &plugin.provided_by.nv_rs;
        let present = is_present(plugin, dlls);
        if provided.kind.is_builtin() {
            let coverage = provided
                .extender
                .as_deref()
                .and_then(|id| caps.as_ref().and_then(|c| c.extender(id)));
            if let Some(coverage) = coverage {
                coverage_findings(plugin, coverage, &used.path, present, findings);
                extenders.push(ExtenderCoverage {
                    id: coverage.id.clone(),
                    plugin: plugin.name.clone(),
                    kind: provided.kind,
                    implemented: coverage.implemented,
                    total: coverage.total,
                    missing: coverage.missing(),
                });
            }
        }
        if plugin.role.as_deref() == Some("extender") {
            continue;
        }
        if !present {
            continue;
        }
        let dll = plugin.dll.clone();
        match provided.kind {
            Kind::BuiltinFull | Kind::BuiltinPartial => findings.push(Finding::new(
                "plugin.redundant-on-nv-rs",
                Severity::Info,
                dll,
                format!("{} is built in on nv-rs, so this DLL is not needed there", plugin.name),
            )),
            Kind::Absent => findings.push(Finding::new(
                "plugin.unable-to-load-on-nv-rs",
                Severity::Error,
                dll,
                format!("{} cannot load on nv-rs: the catalogue marks it absent there", plugin.name),
            )),
            Kind::External => findings.push(Finding::new(
                "plugin.nv-rs-unknown",
                Severity::Info,
                dll,
                format!("{} is an external DLL; nothing records that nv-rs loads it, so whether it works on nv-rs is unknown", plugin.name),
            )),
        }
    }
    NvRsReport {
        capabilities: used,
        extenders,
    }
}

/// The per-target checks. Returns the nv-rs part of the report on the nv-rs target.
pub(crate) fn check(
    instance: &NvmodInstance,
    catalog: &Catalog,
    dlls: &[String],
    findings: &mut Vec<Finding>,
) -> Option<NvRsReport> {
    match instance.config.target {
        Target::Vanilla => {
            vanilla(instance, catalog, findings);
            None
        }
        Target::NvRs => Some(nv_rs(instance, catalog, dlls, findings)),
    }
}
