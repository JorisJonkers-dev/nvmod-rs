// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod stage --instance <dir> [--profile <name>] [--copy] [--json]`: build the Staging tree of a profile.

use crate::{OK, REFUSED, USAGE, args::Args};
use nvmod_api::stage::StageRequest;
use nvmod_api::{NoProgress, Outcome};
use std::io::Write;
use std::path::PathBuf;

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
    let request = StageRequest {
        instance: PathBuf::from(args.option("instance").unwrap_or_default()),
        profile: args.option("profile").map(str::to_owned),
        copy: args.flag("copy"),
    };
    match nvmod_api::stage::stage(&request, &mut NoProgress, &crate::process_token()) {
        Ok(Outcome::Done(staged)) => {
            let report = &staged.report;
            if args.flag("json") {
                let _ = writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(report).unwrap_or_default()
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
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => crate::job_failure("stage", &e, err),
    }
}
