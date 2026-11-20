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
use nvmod_api::verify::{VerifyOutput, VerifyRequest};
use nvmod_api::{NoProgress, Outcome};
use nvmod_core::Severity;
use std::io::Write;
use std::path::PathBuf;

const USAGE_LINE: &str = "usage: nvmod verify --instance <dir> [--profile <name>] [--masterlist <file>] [--catalog <file>] [--proton <program>] [--fail-on <severity>] [--json]";

/// Print the verification: the Doctor's report, the state of nv-rs and the launch plan, as text or as one JSON
/// document.
fn print_verification(out: &mut dyn Write, done: &VerifyOutput, json: bool) {
    if json {
        let doc = serde_json::json!({
            "schema_version": 1,
            "verified": done.verified,
            "doctor": done.doctor,
            "nv_rs": done.nv_rs,
            "launch_plan": done.launch_plan,
        });
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&doc).unwrap_or_default()
        );
        return;
    }
    let _ = out.write_all(done.doctor.to_text().as_bytes());
    let _ = writeln!(out, "nv-rs: {}", done.nv_rs.message);
    let _ = out.write_all(done.launch_plan.to_text().as_bytes());
    let _ = writeln!(
        out,
        "nvmod verify: {}",
        if done.verified {
            "verified"
        } else {
            "verification failed"
        }
    );
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
    let request = VerifyRequest {
        instance: PathBuf::from(args.option("instance").unwrap_or_default()),
        profile: args.option("profile").map(str::to_owned),
        masterlist: args.option("masterlist").map(PathBuf::from),
        catalog: args.option("catalog").map(PathBuf::from),
        proton: args.option("proton").map(str::to_owned),
        fail_on,
    };
    match nvmod_api::verify::verify(&request, &mut NoProgress, &crate::process_token()) {
        Ok(Outcome::Done(done)) => {
            print_verification(out, &done, args.flag("json"));
            if done.verified { OK } else { VERIFICATION }
        }
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => crate::job_failure("verify", &e, err),
    }
}
