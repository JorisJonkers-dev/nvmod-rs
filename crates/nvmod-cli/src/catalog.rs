// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod catalog [--catalog <file>] [--instance <dir>] [--json]`: list the plugin catalogue with what provides each
//! entry per target. With an Instance, the coverage of its installed nv-rs is read from `capabilities.json`.

use crate::{OK, REFUSED, USAGE, args::Args};
use nvmod_catalog::{Catalog, Provided};
use nvmod_instance::NvmodInstance;
use nvmod_tools::Capabilities;
use serde::Serialize;
use std::io::Write;

#[derive(Serialize)]
struct Output<'a> {
    schema: u32,
    plugins: &'a [nvmod_catalog::Plugin],
    mods: &'a [nvmod_catalog::KnownMod],
    #[serde(skip_serializing_if = "Option::is_none")]
    capabilities: Option<CapabilitiesOutput>,
}

#[derive(Serialize)]
struct CapabilitiesOutput {
    path: String,
    status: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    extenders: Vec<ExtenderOutput>,
}

#[derive(Serialize)]
struct ExtenderOutput {
    id: String,
    name: String,
    implemented: u64,
    total: u64,
    missing: u64,
}

fn read_capabilities(instance: &str) -> Result<CapabilitiesOutput, String> {
    let instance =
        NvmodInstance::open_read_only(std::path::Path::new(instance)).map_err(|e| e.to_string())?;
    let path = instance.nv_rs_capabilities();
    let shown = path.display().to_string();
    Ok(match Capabilities::load(&path) {
        Ok(caps) => CapabilitiesOutput {
            path: shown,
            status: "used",
            reason: None,
            extenders: caps
                .extenders
                .iter()
                .map(|e| ExtenderOutput {
                    id: e.id.clone(),
                    name: e.name.clone(),
                    implemented: e.implemented,
                    total: e.total,
                    missing: e.missing(),
                })
                .collect(),
        },
        Err(why) => CapabilitiesOutput {
            path: shown,
            status: if why == nvmod_tools::CapabilitiesError::Missing {
                "missing"
            } else {
                "invalid"
            },
            reason: Some(why.to_string()),
            extenders: Vec::new(),
        },
    })
}

fn provided(p: &Provided) -> String {
    match &p.extender {
        Some(id) => format!("{} ({id})", p.kind.label()),
        None => p.kind.label().to_owned(),
    }
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(args, &["json"], &["catalog", "instance"]) {
        Ok(args) if args.positional.is_empty() => args,
        Ok(_) => {
            let _ = writeln!(
                err,
                "usage: nvmod catalog [--catalog <file>] [--instance <dir>] [--json]"
            );
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod catalog: {why}");
            return USAGE;
        }
    };
    let catalog = match args.option("catalog") {
        None => Catalog::builtin(),
        Some(file) => match std::fs::read_to_string(file)
            .map_err(|e| format!("{file}: {e}"))
            .and_then(|text| Catalog::parse(&text).map_err(|e| e.to_string()))
        {
            Ok(catalog) => catalog,
            Err(why) => {
                let _ = writeln!(err, "nvmod catalog: {why}");
                return REFUSED;
            }
        },
    };
    let capabilities = match args.option("instance").map(read_capabilities) {
        None => None,
        Some(Ok(caps)) => Some(caps),
        Some(Err(why)) => {
            let _ = writeln!(err, "nvmod catalog: {why}");
            return REFUSED;
        }
    };
    if args.flag("json") {
        let output = Output {
            schema: nvmod_catalog::SCHEMA,
            plugins: &catalog.plugins,
            mods: &catalog.mods,
            capabilities,
        };
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&output).unwrap_or_default()
        );
    } else {
        for p in &catalog.plugins {
            let nexus = p
                .nexus_mod_id
                .map_or_else(String::new, |id| format!(" nexus {id}"));
            let verified = if p.provenance.verified {
                "observed"
            } else {
                "unverified"
            };
            let _ = writeln!(
                out,
                "{:<32} {}{nexus} ({verified} {}) vanilla: {}; nv-rs: {}",
                p.id,
                p.dll,
                p.provenance.verified_on,
                provided(&p.provided_by.vanilla),
                provided(&p.provided_by.nv_rs)
            );
        }
        if let Some(caps) = &capabilities {
            let _ = writeln!(out, "installed nv-rs, {} ({}):", caps.path, caps.status);
            if let Some(why) = &caps.reason {
                let _ = writeln!(out, "  no coverage: {why}");
            }
            for e in &caps.extenders {
                let _ = writeln!(
                    out,
                    "  {} ({}): {} of {} commands implemented, {} missing (capabilities.json)",
                    e.name, e.id, e.implemented, e.total, e.missing
                );
            }
        }
    }
    OK
}
