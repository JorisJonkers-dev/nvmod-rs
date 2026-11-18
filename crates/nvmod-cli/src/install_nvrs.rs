// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod install-nv-rs`: build nv-rs from source, or fetch and verify the fork's release, then run the
//! installation check. The tools it runs (`cargo`, `gh`, `curl`) can each be named, so tests inject fakes.

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, VERIFICATION, args::Args};
use nvmod_api::install_nvrs::{
    DEFAULT_ASSET, DEFAULT_RELEASE_URL, DEFAULT_REPO, InstallOutput, InstallRequest, Method,
};
use nvmod_api::{CancelToken, ErrorKind, JobError, NoProgress, Outcome};
use std::io::Write;
use std::path::PathBuf;

const USAGE_LINE: &str = "usage: nvmod install-nv-rs --instance <dir> [--source <dir>] [--release]";
const OPTIONS: &[&str] = &[
    "instance",
    "source",
    "cargo",
    "release-url",
    "asset",
    "repo",
    "gh",
    "curl",
    "retry-delay-ms",
];

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(args, &["release"], OPTIONS) {
        Ok(args) if args.positional.is_empty() && args.option("instance").is_some() => args,
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_LINE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod install-nv-rs: {why}");
            return USAGE;
        }
    };
    let path = |name: &str| args.option(name).map(PathBuf::from);
    let Ok(retry_delay_ms) = args
        .option("retry-delay-ms")
        .map_or(Ok(1000), str::parse::<u64>)
    else {
        let _ = writeln!(err, "nvmod install-nv-rs: --retry-delay-ms needs a number");
        return USAGE;
    };
    let request = InstallRequest {
        instance: path("instance").unwrap_or_default(),
        source: path("source"),
        release: args.flag("release"),
        cargo: path("cargo").or_else(|| std::env::var_os("NVMOD_CARGO").map(PathBuf::from)),
        release_url: args
            .option("release-url")
            .unwrap_or(DEFAULT_RELEASE_URL)
            .to_owned(),
        asset: args.option("asset").unwrap_or(DEFAULT_ASSET).to_owned(),
        repo: args.option("repo").unwrap_or(DEFAULT_REPO).to_owned(),
        gh: path("gh").unwrap_or_else(|| PathBuf::from("gh")),
        curl: path("curl").unwrap_or_else(|| PathBuf::from("curl")),
        retry_delay_ms,
    };
    match nvmod_api::install_nvrs::install(&request, &mut NoProgress, &CancelToken::new()) {
        Ok(Outcome::Done(done)) => {
            print_done(out, &done);
            OK
        }
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => print_failure(&e, err),
    }
}

/// Print what was installed and the check that passed, as this command always has.
fn print_done(out: &mut dyn Write, done: &InstallOutput) {
    let how = match done.method {
        Method::Source => "built from source",
        Method::Release => "fetched from the release and verified",
    };
    let _ = writeln!(
        out,
        "nvmod install-nv-rs: nv-rs {how}: {}",
        done.binary.display()
    );
    let _ = writeln!(
        out,
        "installation check passed: {}",
        done.check.command.join(" ")
    );
    let shown = done.check.output.trim_end();
    if !shown.is_empty() {
        let _ = writeln!(out, "{shown}");
    }
}

/// Print a failed job the way this command always has, and return its exit code.
fn print_failure(e: &JobError, err: &mut dyn Write) -> u8 {
    match e.kind {
        ErrorKind::Refused => {
            let _ = writeln!(err, "nvmod install-nv-rs: refused: {}", e.summary);
            REFUSED
        }
        ErrorKind::Corrupt => {
            let _ = writeln!(
                err,
                "nvmod install-nv-rs: verification failed: {}\nnothing was installed",
                e.summary
            );
            VERIFICATION
        }
        _ => {
            let _ = writeln!(
                err,
                "nvmod install-nv-rs: failed and was rolled back: {}",
                e.summary
            );
            ROLLED_BACK
        }
    }
}
