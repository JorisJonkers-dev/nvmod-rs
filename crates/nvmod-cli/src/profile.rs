// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod profile create --instance <dir> [--name <name>]`: make an empty profile (default name `Default`).

use crate::{OK, REFUSED, USAGE, args::Args};
use nvmod_api::profile::CreateRequest;
use nvmod_api::{NoProgress, Outcome};
use std::io::Write;
use std::path::Path;

const USAGE_LINE: &str = "usage: nvmod profile create --instance <dir> [--name <name>]\n       nvmod profile apply|finish --instance <dir> ... (see --help)";

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    if matches!(args.first().map(String::as_str), Some("apply" | "finish")) {
        return crate::profile_apply::run(args, out, err);
    }
    let args = match Args::parse(args, &[], &["instance", "name"]) {
        Ok(a) if a.positional == ["create"] && a.option("instance").is_some() => a,
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_LINE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod profile: {why}\n{USAGE_LINE}");
            return USAGE;
        }
    };
    let request = CreateRequest {
        instance: Path::new(args.option("instance").unwrap_or_default()).to_path_buf(),
        name: args.option("name").map(str::to_owned),
    };
    match nvmod_api::profile::create(&request, &mut NoProgress, &crate::process_token()) {
        Ok(Outcome::Done(made)) => {
            let _ = writeln!(out, "nvmod profile: made the profile `{}`", made.profile);
            OK
        }
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => crate::job_failure("profile", &e, err),
    }
}
