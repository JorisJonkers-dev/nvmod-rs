// SPDX-License-Identifier: GPL-3.0-only
//! Checks of the profile and its mods that need no plugin file: the lists, the folders, the Nexus ids.

use nvmod_catalog::Catalog;
use nvmod_core::{Finding, Severity};
use nvmod_formats::{Instance, Profile};

pub(crate) fn check(instance: &Instance, profile: &Profile, catalog: &Catalog) -> Vec<Finding> {
    let mut findings = Vec::new();
    for issue in &instance.issues {
        findings.push(Finding::new(
            "instance.path-unresolved",
            Severity::Warning,
            "ModOrganizer.ini",
            issue.clone(),
        ));
    }
    let (missing, unlisted) = instance.reconcile(profile);
    for name in missing {
        findings.push(Finding::new(
            "profile.mod-folder-missing",
            Severity::Warning,
            name,
            "listed in modlist.txt but has no folder in mods/; MO2 drops the line on its next start",
        ));
    }
    for name in unlisted {
        findings.push(Finding::new(
            "profile.mod-unlisted",
            Severity::Info,
            name,
            "has a folder in mods/ but is not in modlist.txt; MO2 adds it at the highest priority on its next start",
        ));
    }
    if profile.fallout_ini_read_only {
        findings.push(Finding::new(
            "profile.fallout-ini-read-only",
            Severity::Warning,
            "fallout.ini",
            "the profile's fallout.ini is read-only; MO2 stalls on a hidden dialog",
        ));
    }
    for (file, list) in [
        ("plugins.txt", &profile.plugins),
        ("loadorder.txt", &profile.load_order),
    ] {
        if list.is_none() {
            findings.push(Finding::new(
                "profile.list-missing",
                Severity::Warning,
                file,
                "the profile has no such file; every plugin counts as inactive until MO2 writes it",
            ));
        }
    }
    for entry in profile.modlist.enabled_mods() {
        let expected = catalog
            .known_mod(&entry.name)
            .map(|m| m.nexus_mod_id)
            .or_else(|| {
                catalog
                    .plugins
                    .iter()
                    .find(|p| p.name == entry.name)
                    .and_then(|p| p.nexus_mod_id)
            });
        let (Some(expected), Some(meta)) = (expected, instance.mod_meta(&entry.name)) else {
            continue;
        };
        match meta.modid {
            Some(id) if id != expected => findings.push(Finding::new(
                "meta.modid-mismatch",
                Severity::Warning,
                entry.name.clone(),
                format!("meta.ini says Nexus id {id}, the catalogue says {expected}; MO2 cannot check this mod for updates"),
            )),
            None => findings.push(Finding::new(
                "meta.modid-missing",
                Severity::Info,
                entry.name.clone(),
                format!("meta.ini has no Nexus id; the catalogue says {expected}"),
            )),
            Some(_) => {}
        }
    }
    findings
}
