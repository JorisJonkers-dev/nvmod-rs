// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod import-mo2 --mo2 <dir> --instance <dir> --base <dir> [--profile <name>] [--copy] [--json]`: build a new
//! nvmod Instance from one profile of a Mod Organizer 2 instance. The MO2 instance is only read.

use crate::{FOUND, OK, REFUSED, ROLLED_BACK, USAGE, args::Args};
use nvmod_import::{ImportError, Request};
use std::io::Write;
use std::path::Path;

const USAGE_LINE: &str = "usage: nvmod import-mo2 --mo2 <dir> --instance <dir> --base <dir> [--profile <name>] [--copy] [--json]";

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(
        args,
        &["copy", "json"],
        &["mo2", "instance", "base", "profile"],
    ) {
        Ok(a)
            if a.positional.is_empty()
                && a.option("mo2").is_some()
                && a.option("instance").is_some()
                && a.option("base").is_some() =>
        {
            a
        }
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_LINE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod import-mo2: {why}\n{USAGE_LINE}");
            return USAGE;
        }
    };
    let get = |name: &str| Path::new(args.option(name).unwrap_or_default());
    let request = Request {
        mo2: get("mo2"),
        instance: get("instance"),
        base: get("base"),
        profile: args.option("profile"),
        copy_only: args.flag("copy"),
    };
    let json = args.flag("json");
    match nvmod_import::import(&request) {
        Ok(report) => {
            if json {
                let _ = writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                );
            } else {
                let _ = writeln!(
                    out,
                    "nvmod import-mo2: profile `{}` imported into {}: {} Mod folder(s), {} enabled, {} plugin(s) in the load order ({} hard link(s), {} copied)",
                    report.profile,
                    report.instance,
                    report.mods,
                    report.enabled_mods.len(),
                    report.load_order.len(),
                    report.linked,
                    report.copied
                );
                for f in &report.unmapped {
                    let _ = writeln!(
                        out,
                        "  [{}] {}: {} (not imported: {})",
                        f.severity, f.subject, f.message, f.code
                    );
                }
            }
            OK
        }
        Err(ImportError::Disagree(findings)) => {
            if json {
                let doc = serde_json::json!({ "disagreements": findings });
                let _ = writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&doc).unwrap_or_default()
                );
            } else {
                let _ = writeln!(
                    out,
                    "nvmod import-mo2: the profile files disagree; nothing was written"
                );
                for f in &findings {
                    let _ = writeln!(
                        out,
                        "  [{}] {}: {} ({})",
                        f.severity, f.subject, f.message, f.code
                    );
                }
            }
            FOUND
        }
        Err(ImportError::Refused(reason)) => {
            let _ = writeln!(err, "nvmod import-mo2: refused: {reason}");
            REFUSED
        }
        Err(ImportError::Failed(why)) => {
            let _ = writeln!(err, "nvmod import-mo2: {why}");
            ROLLED_BACK
        }
    }
}
