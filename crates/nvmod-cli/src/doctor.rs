// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod doctor --base <dir>` or `nvmod doctor --instance <dir> [--profile <name>] [--game <dir>] [--json]
//! [--fail-on <severity>]`.

use crate::{FOUND, OK, REFUSED, USAGE, args::Args};
use nvmod_api::doctor::{CheckOutput, CheckRequest, Target, is_open_refusal};
use nvmod_api::{ErrorKind, JobError, NoProgress, Outcome};
use nvmod_core::Severity;
use std::io::Write;
use std::path::PathBuf;

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let parsed = Args::parse(
        args,
        &["json"],
        &[
            "base",
            "instance",
            "profile",
            "game",
            "fail-on",
            "masterlist",
            "catalog",
        ],
    );
    let args = match parsed {
        Ok(args)
            if args.positional.is_empty()
                && (args.option("instance").is_some() != args.option("base").is_some()) =>
        {
            args
        }
        Ok(_) => {
            let _ = writeln!(
                err,
                "usage: nvmod doctor (--base <dir> | --instance <dir> [--profile <name>] [--game <dir>] [--masterlist <file>] [--catalog <file>]) [--json] [--fail-on <severity>]"
            );
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod doctor: {why}");
            return USAGE;
        }
    };
    let fail_on = match args
        .option("fail-on")
        .map_or(Ok(Severity::Error), str::parse)
    {
        Ok(severity) => severity,
        Err(why) => {
            let _ = writeln!(err, "nvmod doctor: --fail-on: {why}");
            return USAGE;
        }
    };
    let target = match args.option("base") {
        Some(base) => Target::Base {
            folder: PathBuf::from(base),
        },
        None => Target::Instance {
            folder: PathBuf::from(args.option("instance").unwrap_or_default()),
            profile: args.option("profile").map(str::to_owned),
            game: args.option("game").map(PathBuf::from),
            masterlist: args.option("masterlist").map(PathBuf::from),
            catalog: args.option("catalog").map(PathBuf::from),
        },
    };
    let request = CheckRequest { target, fail_on };
    match nvmod_api::doctor::check(&request, &mut NoProgress, &crate::process_token()) {
        Ok(Outcome::Done(done)) => print_report(&done, args.flag("json"), out),
        Ok(Outcome::Cancelled) => {
            let _ = writeln!(err, "nvmod doctor: cancelled");
            REFUSED
        }
        Err(e) => print_failure(&e, args.flag("json"), out, err),
    }
}

/// Print the report as JSON or as text, and exit 1 when it has a finding at or above `--fail-on`.
fn print_report(done: &CheckOutput, json: bool, out: &mut dyn Write) -> u8 {
    if json {
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&done.report).unwrap_or_default()
        );
    } else {
        let _ = out.write_all(done.report.to_text().as_bytes());
    }
    if done.found { FOUND } else { OK }
}

/// Print a refusal or a failure in the shape the command has always used: a refusal is `{"refused": true,
/// "reason": ...}` with `--json` and a line on the error stream otherwise, except an Instance whose schema is newer,
/// which is always a line on the error stream; a failure is `{"error": ...}` or a line, and exits 1.
fn print_failure(e: &JobError, json: bool, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match e.kind {
        ErrorKind::Refused if is_open_refusal(e) => {
            let _ = writeln!(err, "nvmod doctor: refused: {}", e.summary);
            REFUSED
        }
        ErrorKind::Refused => {
            if json {
                let _ = writeln!(
                    out,
                    "{}",
                    serde_json::json!({ "refused": true, "reason": e.summary })
                );
            } else {
                let _ = writeln!(err, "nvmod doctor: refused: {}", e.summary);
            }
            REFUSED
        }
        _ => {
            if json {
                let _ = writeln!(out, "{}", serde_json::json!({ "error": e.summary }));
            } else {
                let _ = writeln!(err, "nvmod doctor: {}", e.summary);
            }
            FOUND
        }
    }
}
