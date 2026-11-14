// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod verify --instance <dir> [--profile <name>] [--masterlist <file>] [--catalog <file>] [--proton <program>]
//! [--fail-on <severity>] [--json]`: confirm an install. It runs the Doctor against the Instance, checks that the
//! installed nv-rs is still the file that was recorded and verified when it was installed, and prints the launch
//! plan of the vanilla plus xNVSE path. Read only. On Linux the milestone ends here: native nv-rs launch is out of
//! scope.
//!
//! Exit codes follow the table: 0 verified, 2 refused (not an Instance), 3 verification failure (the Doctor found
//! a problem at or above `--fail-on`, or nv-rs differs from its record), 5 usage.

use crate::{OK, REFUSED, USAGE, VERIFICATION, args::Args};
use nvmod_catalog::Catalog;
use nvmod_core::Severity;
use nvmod_instance::{NvmodInstance, launch_plan};
use nvmod_tools::sha256_hex;
use std::io::Write;
use std::path::Path;

const USAGE_LINE: &str = "usage: nvmod verify --instance <dir> [--profile <name>] [--masterlist <file>] [--catalog <file>] [--proton <program>] [--fail-on <severity>] [--json]";

/// The state of the installed nv-rs against its install record.
struct NvRs {
    installed: bool,
    verified: bool,
    message: String,
}

fn check_nv_rs(root: &Path) -> NvRs {
    let record = root.join("nv-rs/install.json");
    let Ok(text) = std::fs::read_to_string(&record) else {
        return NvRs {
            installed: false,
            verified: true,
            message: "nv-rs is not installed".to_owned(),
        };
    };
    let recorded = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v["sha256"].as_str().map(str::to_owned));
    let bin = root
        .join("nv-rs/bin")
        .join(format!("nv-viewer{}", std::env::consts::EXE_SUFFIX));
    let actual = std::fs::read(&bin).ok().map(|b| sha256_hex(&b));
    let (verified, message) = match (recorded, actual) {
        (Some(r), Some(a)) if r == a => (true, "nv-rs matches its install record".to_owned()),
        (Some(_), Some(_)) => (
            false,
            "nv-rs differs from the checksum recorded when it was installed".to_owned(),
        ),
        (None, _) => (false, "the nv-rs install record has no checksum".to_owned()),
        (_, None) => (false, "the nv-rs program is missing".to_owned()),
    };
    NvRs {
        installed: true,
        verified,
        message,
    }
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(
        args,
        &["json"],
        &[
            "instance",
            "profile",
            "masterlist",
            "catalog",
            "proton",
            "fail-on",
        ],
    ) {
        Ok(a) if a.positional.is_empty() && a.option("instance").is_some() => a,
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_LINE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod verify: {why}\n{USAGE_LINE}");
            return USAGE;
        }
    };
    let fail_on = match args
        .option("fail-on")
        .map_or(Ok(Severity::Error), str::parse)
    {
        Ok(s) => s,
        Err(why) => {
            let _ = writeln!(err, "nvmod verify: --fail-on: {why}");
            return USAGE;
        }
    };
    let refused = |why: &str, err: &mut dyn Write| {
        let _ = writeln!(err, "nvmod verify: refused: {why}");
        REFUSED
    };
    let root = Path::new(args.option("instance").unwrap_or_default());
    let instance = match NvmodInstance::open_read_only(root) {
        Ok(i) => i,
        Err(e) => return refused(&crate::install_mod::describe_open_error(root, &e), err),
    };
    let catalog = match args.option("catalog") {
        None => Catalog::builtin(),
        Some(file) => match std::fs::read_to_string(file)
            .map_err(|e| format!("{file}: {e}"))
            .and_then(|t| Catalog::parse(&t).map_err(|e| e.to_string()))
        {
            Ok(c) => c,
            Err(why) => return refused(&why, err),
        },
    };
    let report = match nvmod_doctor::check_instance(
        &instance,
        args.option("profile"),
        args.option("masterlist").map(Path::new),
        &catalog,
    ) {
        Ok(r) => r,
        Err(why) => return refused(&why, err),
    };
    let nv_rs = check_nv_rs(&instance.root);
    let plan = launch_plan(&instance, args.option("proton"));
    let doctor_ok = report.worst().is_none_or(|w| w < fail_on);
    let verified = doctor_ok && nv_rs.verified;
    if args.flag("json") {
        let doc = serde_json::json!({
            "schema_version": 1,
            "verified": verified,
            "doctor": report,
            "nv_rs": { "installed": nv_rs.installed, "verified": nv_rs.verified, "message": nv_rs.message },
            "launch_plan": plan,
        });
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&doc).unwrap_or_default()
        );
    } else {
        let _ = out.write_all(report.to_text().as_bytes());
        let _ = writeln!(out, "nv-rs: {}", nv_rs.message);
        let _ = out.write_all(plan.to_text().as_bytes());
        let _ = writeln!(
            out,
            "nvmod verify: {}",
            if verified {
                "verified"
            } else {
                "verification failed"
            }
        );
    }
    if verified { OK } else { VERIFICATION }
}
