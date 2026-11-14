// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod init --base <dir> --instance <dir>`: make an Instance skeleton for a Steam or GOG base copy. The base
//! copy is only read.

use crate::{FOUND, OK, REFUSED, USAGE, args::Args};
use nvmod_instance::{InstanceError, NvmodInstance};
use std::io::Write;
use std::path::Path;

const USAGE_LINE: &str = "usage: nvmod init --base <dir> --instance <dir>";

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(args, &[], &["base", "instance"]) {
        Ok(args)
            if args.positional.is_empty()
                && args.option("base").is_some()
                && args.option("instance").is_some() =>
        {
            args
        }
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_LINE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod init: {why}");
            return USAGE;
        }
    };
    let root = Path::new(args.option("instance").unwrap_or_default());
    let base = Path::new(args.option("base").unwrap_or_default());
    match NvmodInstance::create(root, base) {
        Ok(instance) => {
            let _ = writeln!(
                out,
                "nvmod init: made an Instance at {} for the {} base copy {}",
                instance.root.display(),
                instance.config.storefront,
                instance.config.base.display()
            );
            OK
        }
        Err(InstanceError::Refused(reason)) => {
            let _ = writeln!(err, "nvmod init: refused: {reason}");
            REFUSED
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod init: {why}");
            FOUND
        }
    }
}
