// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod catalog [--json]`: list the plugin catalogue.

use crate::{OK, USAGE, args::Args};
use nvmod_catalog::Catalog;
use serde::Serialize;
use std::io::Write;

#[derive(Serialize)]
struct Output<'a> {
    schema: u32,
    plugins: &'a [nvmod_catalog::Plugin],
    mods: &'a [nvmod_catalog::KnownMod],
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(args, &["json"], &[]) {
        Ok(args) if args.positional.is_empty() => args,
        Ok(_) => {
            let _ = writeln!(err, "usage: nvmod catalog [--json]");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod catalog: {why}");
            return USAGE;
        }
    };
    let catalog = Catalog::builtin();
    if args.flag("json") {
        let output = Output {
            schema: nvmod_catalog::SCHEMA,
            plugins: &catalog.plugins,
            mods: &catalog.mods,
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
                "{:<32} {}{nexus} ({verified} {})",
                p.id, p.dll, p.provenance.verified_on
            );
        }
    }
    OK
}
