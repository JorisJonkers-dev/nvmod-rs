// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod doctor --base <dir>` or `nvmod doctor --instance <dir> [--profile <name>] [--game <dir>] [--json]
//! [--fail-on <severity>]`.

use crate::{FOUND, OK, REFUSED, USAGE, args::Args};
use nvmod_catalog::Catalog;
use nvmod_core::Severity;
use nvmod_formats::Mo2Instance;
use nvmod_instance::{InstanceError, NvmodInstance};
use std::io::Write;
use std::path::Path;

/// The Doctor is read only: an older Instance file is read as migrated, but not rewritten.
fn open_and_check(
    dir: &Path,
    args: &Args,
    fail_on: Severity,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let json = args.flag("json");
    match NvmodInstance::open_read_only(dir) {
        Ok(instance) => nvmod_instance_doctor(&instance, args, json, fail_on, out, err),
        Err(InstanceError::Refused(why)) => {
            let _ = writeln!(err, "nvmod doctor: refused: {why}");
            REFUSED
        }
        Err(why) => {
            if json {
                let _ = writeln!(out, "{}", serde_json::json!({ "error": why.to_string() }));
            } else {
                let _ = writeln!(err, "nvmod doctor: {why}");
            }
            FOUND
        }
    }
}

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
        return open_and_check(instance_dir, &args, fail_on, out, err);
    }
    let instance = match Mo2Instance::open(instance_dir) {
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

/// The Doctor on an nvmod Instance, with the catalogue of `--catalog` when one is given.
fn nvmod_instance_doctor(
    instance: &NvmodInstance,
    args: &Args,
    json: bool,
    fail_on: Severity,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let catalog = match args.option("catalog") {
        None => Catalog::builtin(),
        Some(file) => match std::fs::read_to_string(file)
            .map_err(|e| format!("{file}: {e}"))
            .and_then(|text| Catalog::parse(&text).map_err(|e| e.to_string()))
        {
            Ok(catalog) => catalog,
            Err(reason) => return finish(Err(reason), json, fail_on, out, err),
        },
    };
    finish(
        nvmod_doctor::check_instance(
            instance,
            args.option("profile"),
            args.option("masterlist").map(Path::new),
            &catalog,
        ),
        json,
        fail_on,
        out,
        err,
    )
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
