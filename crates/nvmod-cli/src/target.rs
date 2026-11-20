// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod target <vanilla|nv-rs> --instance <dir>`: choose what the Instance runs on. The Doctor reads it: the
//! vanilla target needs xNVSE's DLL in the base copy, the nv-rs target has it built in.

use crate::{OK, REFUSED, USAGE, args::Args};
use nvmod_api::target::{SetRequest, Target};
use nvmod_api::{NoProgress, Outcome};
use std::io::Write;
use std::path::PathBuf;

const USAGE_LINE: &str = "usage: nvmod target <vanilla|nv-rs> --instance <dir>";

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(args, &[], &["instance"]) {
        Ok(a) if a.positional.len() == 1 && a.option("instance").is_some() => a,
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_LINE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod target: {why}\n{USAGE_LINE}");
            return USAGE;
        }
    };
    let (target, name) = match args.positional[0].as_str() {
        "vanilla" => (Target::Vanilla, "vanilla"),
        "nv-rs" => (Target::NvRs, "nv-rs"),
        other => {
            let _ = writeln!(err, "nvmod target: `{other}` is not a target\n{USAGE_LINE}");
            return USAGE;
        }
    };
    let request = SetRequest {
        instance: PathBuf::from(args.option("instance").unwrap_or_default()),
        target,
    };
    match nvmod_api::target::set(&request, &mut NoProgress, &crate::process_token()) {
        Ok(Outcome::Done(_)) => {
            let _ = writeln!(out, "nvmod target: the Instance now runs on {name}");
            OK
        }
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => crate::job_failure("target", &e, err),
    }
}
