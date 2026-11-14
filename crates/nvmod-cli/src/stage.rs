// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod stage --instance <dir> [--profile <name>] [--copy] [--json]`: build the Staging tree of a profile.

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, args::Args};
use nvmod_instance::NvmodInstance;
use nvmod_stage::{Options, StageError};
use std::io::Write;
use std::path::Path;

const USAGE_LINE: &str = "usage: nvmod stage --instance <dir> [--profile <name>] [--copy] [--json]";

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(args, &["copy", "json"], &["instance", "profile"]) {
        Ok(args) if args.positional.is_empty() && args.option("instance").is_some() => args,
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_LINE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod stage: {why}");
            return USAGE;
        }
    };
    let root = Path::new(args.option("instance").unwrap_or_default());
    if !NvmodInstance::is_instance(root) {
        let _ = writeln!(
            err,
            "nvmod stage: refused: {} is not an nvmod Instance (made by `nvmod init`)",
            root.display()
        );
        return REFUSED;
    }
    let result = NvmodInstance::open(root)
        .map_err(|e| StageError::Refused(e.to_string()))
        .and_then(|instance| {
            let profile = instance
                .resolve_profile(args.option("profile"))
                .map_err(|e| StageError::Refused(e.to_string()))?;
            nvmod_stage::stage(
                &instance,
                &profile,
                Options {
                    copy_only: args.flag("copy"),
                },
            )
        });
    match result {
        Ok(report) => {
            if args.flag("json") {
                let _ = writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                );
            } else {
                let m = &report.manifest;
                let _ = writeln!(
                    out,
                    "nvmod stage: profile `{}`: {} files in {} ({} hard link(s), {} copied, {} replaced by a later Mod)",
                    m.profile, m.files, report.tree, m.linked, m.copied, m.overridden
                );
                for c in &report.collisions {
                    let _ = writeln!(
                        out,
                        "  collision in {}: {} (kept {})",
                        c.mod_name,
                        c.paths.join(", "),
                        c.kept
                    );
                }
                for f in &report.findings {
                    let _ = writeln!(
                        out,
                        "  [{}] {}: {} ({})",
                        f.severity, f.subject, f.message, f.code
                    );
                }
            }
            OK
        }
        Err(StageError::Refused(reason)) => {
            let _ = writeln!(err, "nvmod stage: refused: {reason}");
            REFUSED
        }
        Err(StageError::Failed(why)) => {
            let _ = writeln!(err, "nvmod stage: {why}");
            ROLLED_BACK
        }
    }
}
