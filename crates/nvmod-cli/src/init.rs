// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod init --base <dir> --instance <dir>`: make an Instance skeleton for a Steam or GOG base copy. The base
//! copy is only read.

use crate::{FOUND, OK, REFUSED, USAGE, args::Args};
use std::io::Write;
use std::path::PathBuf;

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
    let request = nvmod_api::init::CreateRequest {
        instance: PathBuf::from(args.option("instance").unwrap_or_default()),
        base: PathBuf::from(args.option("base").unwrap_or_default()),
    };
    match nvmod_api::init::create(
        &request,
        &mut nvmod_api::NoProgress,
        &nvmod_api::CancelToken::new(),
    ) {
        Ok(nvmod_api::Outcome::Done(made)) => {
            let _ = writeln!(
                out,
                "nvmod init: made an Instance at {} for the {} base copy {}",
                made.instance.display(),
                made.storefront,
                made.base.display()
            );
            OK
        }
        Ok(nvmod_api::Outcome::Cancelled) => REFUSED,
        Err(e) if e.kind == nvmod_api::ErrorKind::Refused => {
            let _ = writeln!(err, "nvmod init: refused: {}", e.summary);
            REFUSED
        }
        Err(e) => {
            let _ = writeln!(err, "nvmod init: {}", e.summary);
            FOUND
        }
    }
}
