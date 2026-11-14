// SPDX-License-Identifier: GPL-3.0-only
//! The plugin DLLs of the merged view and what is wrong with each.

use crate::OVERWRITE;
use crate::report::PluginReport;
use nvmod_catalog::{Catalog, NvRs};
use nvmod_core::{FileIndex, Finding, Severity};
use nvmod_formats::scan::files_under;
use nvmod_formats::{Mo2Instance, Mo2Profile};
use nvmod_pe::{PeReport, inspect_file};
use std::collections::BTreeSet;

const PLUGIN_DIR: &str = "nvse/plugins";

/// Import names that mean the Visual C++ runtime (or its predecessors) must be in the prefix.
fn is_vc_runtime(import: &str) -> bool {
    let lower = import.to_lowercase();
    lower.strip_suffix(".dll").is_some_and(|stem| {
        ["msvcp", "vcruntime", "concrt", "msvcr"]
            .iter()
            .any(|p| stem.starts_with(p))
    })
}

fn pe_findings(dll: &str, pe: &PeReport, findings: &mut Vec<Finding>) {
    let mut add =
        |code, severity, message: String| findings.push(Finding::new(code, severity, dll, message));
    if !pe.is_x86_32bit() {
        add(
            "pe.not-32-bit",
            Severity::Error,
            format!(
                "{} {:?} image: the game is 32-bit x86 and cannot load it",
                pe.machine_name, pe.format
            ),
        );
    }
    if !pe.is_dll {
        add(
            "pe.not-a-dll",
            Severity::Error,
            "the DLL flag is not set".to_owned(),
        );
    }
    if !pe.nvse.is_plugin {
        add(
            "pe.not-nvse-plugin",
            Severity::Warning,
            "exports none of NVSEPlugin_Query, NVSEPlugin_Load or NVSEPlugin_Version: xNVSE will not treat it as a plugin (a helper library?)".to_owned(),
        );
    }
    let runtimes: Vec<&str> = pe
        .imports
        .iter()
        .map(String::as_str)
        .filter(|i| is_vc_runtime(i))
        .collect();
    if !runtimes.is_empty() {
        add(
            "pe.needs-vc-runtime",
            Severity::Info,
            format!(
                "imports {}: the Visual C++ runtime must be in the Wine prefix",
                runtimes.join(", ")
            ),
        );
    }
    for issue in &pe.issues {
        add("pe.damaged", Severity::Warning, issue.clone());
    }
}

pub(crate) fn check(
    instance: &Mo2Instance,
    profile: &Mo2Profile,
    catalog: &Catalog,
    findings: &mut Vec<Finding>,
) -> Vec<PluginReport> {
    let mut index = FileIndex::new();
    for path in files_under(&instance.overwrite_dir, "nvse") {
        index.insert(OVERWRITE, &path);
    }
    for entry in profile.modlist.enabled_mods() {
        for path in files_under(&instance.mod_dir(&entry.name), "nvse") {
            index.insert(&entry.name, &path);
        }
    }

    let mut reports = Vec::new();
    for (_, providers) in index.in_dir(PLUGIN_DIR, ".dll") {
        let winner = &providers[0];
        let file_name = winner
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&winner.path)
            .to_owned();
        let root = if winner.source == OVERWRITE {
            instance.overwrite_dir.clone()
        } else {
            instance.mod_dir(&winner.source)
        };
        let on_disk = root.join(&winner.path);
        let shadowed: Vec<String> = providers[1..]
            .iter()
            .map(|p| p.source.clone())
            .filter(|s| *s != winner.source)
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        if !shadowed.is_empty() {
            findings.push(Finding::new(
                "plugin.shadowed",
                Severity::Warning,
                file_name.clone(),
                format!(
                    "provided by {} as well; the copy of {} wins and the others are hidden",
                    shadowed.join(", "),
                    winner.source
                ),
            ));
        }
        let pe = match inspect_file(&on_disk) {
            Ok(pe) => {
                pe_findings(&file_name, &pe, findings);
                Some(pe)
            }
            Err(why) => {
                findings.push(Finding::new(
                    "pe.unreadable",
                    Severity::Error,
                    file_name.clone(),
                    why.to_string(),
                ));
                None
            }
        };
        let entry = catalog.by_dll(&file_name);
        if entry.is_none() {
            findings.push(Finding::new(
                "catalog.unknown-plugin",
                Severity::Info,
                file_name.clone(),
                format!("not in the plugin catalogue (from {})", winner.source),
            ));
        }
        reports.push(PluginReport {
            dll: file_name,
            winner: winner.source.clone(),
            shadowed,
            catalog_id: entry.map(|p| p.id.clone()),
            nv_rs: entry.map_or(NvRs::Unknown, |p| p.nv_rs),
            pe,
        });
    }
    catalogue_checks(catalog, &index, &reports, findings);
    reports
}

fn catalogue_checks(
    catalog: &Catalog,
    index: &FileIndex,
    reports: &[PluginReport],
    findings: &mut Vec<Finding>,
) {
    let present: BTreeSet<&str> = reports
        .iter()
        .filter_map(|r| r.catalog_id.as_deref())
        .collect();
    for id in &present {
        let Some(plugin) = catalog.plugin(id) else {
            continue;
        };
        for ini in &plugin.ini {
            if index.winner(&format!("{PLUGIN_DIR}/{ini}")).is_none() {
                findings.push(Finding::new(
                    "plugin.ini-missing",
                    Severity::Info,
                    plugin.dll.clone(),
                    format!("{ini} is not in the merged view; the plugin runs on its defaults"),
                ));
            }
        }
        for required in &plugin.requires {
            if !present.contains(required.as_str()) {
                let name = catalog
                    .plugin(required)
                    .map_or(required.as_str(), |p| p.dll.as_str());
                findings.push(Finding::new(
                    "plugin.missing-requirement",
                    Severity::Error,
                    plugin.dll.clone(),
                    format!("requires {name}, which is not in the merged view"),
                ));
            }
        }
        for other in plugin
            .conflicts
            .iter()
            .filter(|c| present.contains(c.as_str()) && c.as_str() > *id)
        {
            let name = catalog
                .plugin(other)
                .map_or(other.as_str(), |p| p.dll.as_str());
            findings.push(Finding::new(
                "plugin.conflict",
                Severity::Warning,
                plugin.dll.clone(),
                format!("conflicts with {name}, which is also present"),
            ));
        }
        if let Some(replacement) = plugin
            .replaced_by
            .as_deref()
            .and_then(|r| catalog.plugin(r))
        {
            findings.push(Finding::new(
                "plugin.replaced",
                Severity::Info,
                plugin.dll.clone(),
                format!("replaced by {}", replacement.dll),
            ));
        }
    }
}
