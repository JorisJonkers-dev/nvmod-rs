// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod doctor --base <dir>` or `nvmod doctor --instance <dir> [--profile <name>] [--game <dir>] [--json]
//! [--fail-on <severity>]`.

use crate::{FOUND, OK, REFUSED, USAGE, args::Args};
use nvmod_catalog::Catalog;
use nvmod_core::Severity;
use nvmod_formats::Instance;
use nvmod_instance::NvmodInstance;
use std::io::Write;
use std::path::Path;

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let parsed = Args::parse(
        args,
        &["json"],
        &["base", "instance", "profile", "game", "fail-on"],
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
                "usage: nvmod doctor (--base <dir> | --instance <dir> [--profile <name>] [--game <dir>]) [--json] [--fail-on <severity>]"
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
    let json = args.flag("json");
    let fail = |message: String, out: &mut dyn Write, err: &mut dyn Write| {
        if json {
            let _ = writeln!(out, "{}", serde_json::json!({ "error": message }));
        } else {
            let _ = writeln!(err, "nvmod doctor: {message}");
        }
        FOUND
    };
    if let Some(base) = args.option("base") {
        return base_doctor(Path::new(base), json, fail_on, out, err);
    }
    let instance_dir = Path::new(args.option("instance").unwrap_or_default());
    if NvmodInstance::is_instance(instance_dir) {
        return match NvmodInstance::open(instance_dir) {
            Ok(instance) => finish(
                nvmod_doctor::check_instance(&instance, args.option("profile")),
                json,
                fail_on,
                out,
                err,
            ),
            Err(why) => fail(why.to_string(), out, err),
        };
    }
    let instance = match Instance::open(instance_dir) {
        Ok(instance) => instance,
        Err(why) => return fail(why.to_string(), out, err),
    };
    let Some(name) = args
        .option("profile")
        .map(str::to_owned)
        .or_else(|| instance.selected_profile.clone())
    else {
        return fail(
            "the instance names no selected profile; pass --profile <name>".to_owned(),
            out,
            err,
        );
    };
    let profile = match instance.load_profile(&name) {
        Ok(profile) => profile,
        Err(why) => return fail(why.to_string(), out, err),
    };
    let report = nvmod_doctor::run(
        &instance,
        &profile,
        args.option("game").map(Path::new),
        &Catalog::builtin(),
    );
    if json {
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    } else {
        let _ = out.write_all(report.to_text().as_bytes());
    }
    if report.worst().is_some_and(|worst| worst >= fail_on) {
        FOUND
    } else {
        OK
    }
}

/// The Doctor on a base copy; a refused storefront exits 2 with the reason.
fn base_doctor(
    base: &Path,
    json: bool,
    fail_on: Severity,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    finish(nvmod_doctor::check_base(base), json, fail_on, out, err)
}

fn finish(
    result: Result<nvmod_doctor::BaseReport, String>,
    json: bool,
    fail_on: Severity,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let report = match result {
        Ok(report) => report,
        Err(reason) => {
            if json {
                let _ = writeln!(
                    out,
                    "{}",
                    serde_json::json!({ "refused": true, "reason": reason })
                );
            } else {
                let _ = writeln!(err, "nvmod doctor: refused: {reason}");
            }
            return REFUSED;
        }
    };
    if json {
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    } else {
        let _ = out.write_all(report.to_text().as_bytes());
    }
    if report.worst().is_some_and(|worst| worst >= fail_on) {
        FOUND
    } else {
        OK
    }
}
