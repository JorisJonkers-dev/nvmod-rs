// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod profile create --instance <dir> [--name <name>]`: make an empty profile (default name `Default`).

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, args::Args};
use nvmod_instance::{InstanceError, NvmodInstance};
use std::io::Write;
use std::path::Path;

const USAGE_LINE: &str = "usage: nvmod profile create --instance <dir> [--name <name>]";

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
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
    let root = Path::new(args.option("instance").unwrap_or_default());
    let name = args.option("name").unwrap_or("Default");
    let made = NvmodInstance::open(root)
        .map_err(|e| InstanceError::Refused(crate::install_mod::describe_open_error(root, &e)))
        .and_then(|instance| instance.create_profile(name));
    match made {
        Ok(()) => {
            let _ = writeln!(out, "nvmod profile: made the profile `{name}`");
            OK
        }
        Err(InstanceError::Io(why)) => {
            let _ = writeln!(err, "nvmod profile: {why}");
            ROLLED_BACK
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod profile: refused: {why}");
            REFUSED
        }
    }
}
