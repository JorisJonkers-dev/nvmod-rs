// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod install <archive> --instance <dir> ...` and `nvmod rollback --instance <dir>`.

use crate::{FOUND, OK, REFUSED, ROLLED_BACK, USAGE, args::Args};
use nvmod_instance::{InstanceError, NvmodInstance};
use nvmod_modinstall::{InstallError, Limits, Report, Request, RollbackError};
use std::io::Write;
use std::path::Path;

const INSTALL_USAGE: &str = "usage: nvmod install <archive> --instance <dir> [--profile <name>] [--name <mod name>] [--dry-run] [--answers <file>] [--extract-tool <path>] [--max-extract-bytes <n>] [--json]";
const ROLLBACK_USAGE: &str = "usage: nvmod rollback --instance <dir> [--json]";

fn open(args: &Args, name: &str, err: &mut dyn Write) -> Result<NvmodInstance, u8> {
    let root = Path::new(args.option("instance").unwrap_or_default());
    // Rollback must not migrate: it may be undoing a migration.
    let opened = if name == "rollback" {
        NvmodInstance::open_read_only(root)
    } else {
        NvmodInstance::open(root)
    };
    opened.map_err(|e| {
        let _ = writeln!(
            err,
            "nvmod {name}: refused: {}",
            describe_open_error(root, &e)
        );
        REFUSED
    })
}

/// Why an Instance did not open, in words: a newer schema says so, anything else says the folder is not an Instance.
pub(crate) fn describe_open_error(root: &Path, e: &InstanceError) -> String {
    match e {
        InstanceError::Refused(why) => why.clone(),
        _ => format!("{} is not an nvmod Instance ({e})", root.display()),
    }
}

fn print_json(out: &mut dyn Write, value: &impl serde::Serialize) {
    let _ = writeln!(
        out,
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

fn print_report(out: &mut dyn Write, report: &Report, json: bool) {
    if json {
        print_json(out, report);
        return;
    }
    let verb = if report.dry_run {
        "would install"
    } else {
        "installed"
    };
    let _ = writeln!(
        out,
        "nvmod install: {verb} `{}` into profile `{}` ({} files)",
        report.r#mod,
        report.profile,
        report.files.len()
    );
    if report.dry_run {
        for f in &report.files {
            let _ = writeln!(out, "  + {f}");
        }
    }
    for p in &report.plugins {
        let _ = writeln!(out, "  activates plugin {p}");
    }
    for i in &report.inferred {
        let _ = writeln!(
            out,
            "  chose {} for {} ({})",
            i.selected.join(", "),
            i.id,
            i.reason
        );
    }
}

pub(crate) fn run_install(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let parsed = Args::parse(
        args,
        &["dry-run", "json"],
        &[
            "instance",
            "profile",
            "name",
            "answers",
            "extract-tool",
            "max-extract-bytes",
        ],
    );
    let args = match parsed {
        Ok(a) if a.positional.len() == 1 && a.option("instance").is_some() => a,
        Ok(_) => {
            let _ = writeln!(err, "{INSTALL_USAGE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod install: {why}");
            return USAGE;
        }
    };
    let mut limits = Limits::default();
    if let Some(text) = args.option("max-extract-bytes") {
        let Ok(n) = text.parse() else {
            let _ = writeln!(err, "nvmod install: --max-extract-bytes wants a number");
            return USAGE;
        };
        limits.max_bytes = n;
    }
    let instance = match open(&args, "install", err) {
        Ok(i) => i,
        Err(code) => return code,
    };
    let profile = match instance.resolve_profile(args.option("profile")) {
        Ok(p) => p,
        Err(e) => {
            let _ = writeln!(err, "nvmod install: refused: {e}");
            return REFUSED;
        }
    };
    let request = Request {
        instance: &instance,
        profile: &profile,
        archive: Path::new(&args.positional[0]),
        name: args.option("name"),
        dry_run: args.flag("dry-run"),
        answers: args.option("answers").map(Path::new),
        extract_tool: args.option("extract-tool").map(Path::new),
        limits,
    };
    match nvmod_modinstall::install(&request) {
        Ok(report) => {
            print_report(out, &report, args.flag("json"));
            OK
        }
        Err(InstallError::Questions(questions)) => {
            if args.flag("json") {
                print_json(
                    out,
                    &serde_json::json!({ "status": "questions", "questions": questions }),
                );
            } else {
                let _ = writeln!(
                    out,
                    "nvmod install: the FOMOD needs answers (nothing was changed); give them with --answers <file>:"
                );
                for q in &questions {
                    let _ = writeln!(
                        out,
                        "  {} ({}, choose {} to {})",
                        q.id,
                        q.kind.as_str(),
                        q.min,
                        q.max
                    );
                    for o in &q.options {
                        let _ = writeln!(out, "    - {}: {}", o.name, o.description);
                    }
                }
            }
            FOUND
        }
        Err(InstallError::Refused(why)) => {
            let _ = writeln!(err, "nvmod install: refused: {why}");
            REFUSED
        }
        Err(InstallError::Failed(why)) => {
            let _ = writeln!(err, "nvmod install: {why}");
            ROLLED_BACK
        }
    }
}

pub(crate) fn run_rollback(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(args, &["json"], &["instance"]) {
        Ok(a) if a.positional.is_empty() && a.option("instance").is_some() => a,
        Ok(_) => {
            let _ = writeln!(err, "{ROLLBACK_USAGE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod rollback: {why}");
            return USAGE;
        }
    };
    let instance = match open(&args, "rollback", err) {
        Ok(i) => i,
        Err(code) => return code,
    };
    match nvmod_modinstall::rollback(&instance) {
        Ok(report) => {
            if args.flag("json") {
                print_json(out, &report);
            } else if report.kind == "migrate" {
                let _ = writeln!(
                    out,
                    "nvmod rollback: restored the Instance file as it was before its schema migration"
                );
            } else if report.kind == "mods" {
                let _ = writeln!(
                    out,
                    "nvmod rollback: undid the Mods change of profile `{}`",
                    report.profile
                );
            } else if report.kind == "sort" {
                let _ = writeln!(
                    out,
                    "nvmod rollback: undid the Load order sort of profile `{}`",
                    report.profile
                );
            } else {
                let _ = writeln!(
                    out,
                    "nvmod rollback: removed `{}` and restored profile `{}`",
                    report.r#mod, report.profile
                );
            }
            OK
        }
        Err(RollbackError::Refused(why)) => {
            let _ = writeln!(err, "nvmod rollback: refused: {why}");
            REFUSED
        }
        Err(RollbackError::Failed(why)) => {
            let _ = writeln!(err, "nvmod rollback: {why}");
            ROLLED_BACK
        }
    }
}
