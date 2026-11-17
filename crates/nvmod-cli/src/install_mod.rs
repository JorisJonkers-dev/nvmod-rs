// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod install <archive> --instance <dir> ...` and `nvmod rollback --instance <dir>`.

use crate::{FOUND, OK, REFUSED, USAGE, args::Args};
use nvmod_api::install::{InstallOutput, InstallRequest, Question, Report, RollbackRequest};
use nvmod_api::{CancelToken, NoProgress, Outcome};
use nvmod_instance::{InstanceError, NvmodInstance};
use std::io::Write;
use std::path::{Path, PathBuf};

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

fn print_questions(out: &mut dyn Write, questions: &[Question], json: bool) {
    if json {
        print_json(
            out,
            &serde_json::json!({ "status": "questions", "questions": questions }),
        );
        return;
    }
    let _ = writeln!(
        out,
        "nvmod install: the FOMOD needs answers (nothing was changed); give them with --answers <file>:"
    );
    for q in questions {
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
    let mut max_extract_bytes = None;
    if let Some(text) = args.option("max-extract-bytes") {
        let Ok(n) = text.parse() else {
            let _ = writeln!(err, "nvmod install: --max-extract-bytes wants a number");
            return USAGE;
        };
        max_extract_bytes = Some(n);
    }
    let request = InstallRequest {
        instance: PathBuf::from(args.option("instance").unwrap_or_default()),
        profile: args.option("profile").map(str::to_owned),
        archive: PathBuf::from(&args.positional[0]),
        name: args.option("name").map(str::to_owned),
        dry_run: args.flag("dry-run"),
        answers: args.option("answers").map(PathBuf::from),
        extract_tool: args.option("extract-tool").map(PathBuf::from),
        max_extract_bytes,
    };
    match nvmod_api::install::install(&request, &mut NoProgress, &CancelToken::new()) {
        Ok(Outcome::Done(InstallOutput::Installed(report))) => {
            print_report(out, &report, args.flag("json"));
            OK
        }
        Ok(Outcome::Done(InstallOutput::Questions(questions))) => {
            print_questions(out, &questions, args.flag("json"));
            FOUND
        }
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => crate::job_failure("install", &e, err),
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
    let request = RollbackRequest {
        instance: instance.root.clone(),
    };
    match nvmod_api::install::rollback(&request, &mut NoProgress, &CancelToken::new()) {
        Ok(Outcome::Done(report)) => {
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
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => crate::job_failure("rollback", &e, err),
    }
}
