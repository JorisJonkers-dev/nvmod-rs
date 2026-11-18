// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod import-mo2 --mo2 <dir> --instance <dir> --base <dir> [--profile <name>] [--copy] [--json]`: build a new
//! nvmod Instance from one profile of a Mod Organizer 2 instance. The MO2 instance is only read.

use crate::{FOUND, OK, REFUSED, USAGE, args::Args};
use nvmod_api::import_mo2::{ImportOutput, ImportRequest, Report};
use nvmod_api::{CancelToken, NoProgress, Outcome};
use std::io::Write;
use std::path::Path;

const USAGE_LINE: &str = "usage: nvmod import-mo2 --mo2 <dir> --instance <dir> --base <dir> [--profile <name>] [--copy] [--json]";

#[allow(clippy::too_many_lines)]
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
    let request = ImportRequest {
        mo2: get("mo2").to_path_buf(),
        instance: get("instance").to_path_buf(),
        base: get("base").to_path_buf(),
        profile: args.option("profile").map(str::to_owned),
        copy_only: args.flag("copy"),
    };
    let json = args.flag("json");
    match nvmod_api::import_mo2::import(&request, &mut NoProgress, &CancelToken::new()) {
        Ok(Outcome::Done(ImportOutput::Imported(report))) => {
            print_report(out, &report, json);
            OK
        }
        Ok(Outcome::Done(ImportOutput::Disagreements(findings))) => {
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
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => crate::job_failure("import-mo2", &e, err),
    }
}

fn print_report(out: &mut dyn Write, report: &Report, json: bool) {
    if json {
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(report).unwrap_or_default()
        );
        return;
    }
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
