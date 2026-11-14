// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod install-nv-rs`: build nv-rs from source, or fetch and verify the fork's release, then run the play-patch
//! step. The tools it runs (`cargo`, `gh`, `curl`, the play-patch step) can each be named, so tests inject fakes.

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, VERIFICATION, args::Args};
use nvmod_install::{
    DEFAULT_ASSET, DEFAULT_RELEASE_URL, DEFAULT_REPO, InstallError, Method, Options, install,
};
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

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
    "play-patch",
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
    let Ok(delay) = args
        .option("retry-delay-ms")
        .map_or(Ok(1000), str::parse::<u64>)
    else {
        let _ = writeln!(err, "nvmod install-nv-rs: --retry-delay-ms needs a number");
        return USAGE;
    };
    let opts = Options {
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
        play_patch: path("play-patch"),
        retry_delay: Duration::from_millis(delay),
    };
    match install(&opts) {
        Ok(done) => {
            let how = match done.method {
                Method::Source => "built from source",
                Method::Release => "fetched from the release and verified",
            };
            let _ = writeln!(
                out,
                "nvmod install-nv-rs: nv-rs {how}: {}",
                done.binary.display()
            );
            match done.play_patch {
                Some(step) => {
                    let _ = writeln!(out, "play-patch step run: {}", step.display());
                }
                None => {
                    let _ = writeln!(
                        out,
                        "no play-patch step was given or shipped, so none was run"
                    );
                }
            }
            OK
        }
        Err(InstallError::Refused(why)) => {
            let _ = writeln!(err, "nvmod install-nv-rs: refused: {why}");
            REFUSED
        }
        Err(InstallError::Verification(why)) => {
            let _ = writeln!(
                err,
                "nvmod install-nv-rs: verification failed: {why}\nnothing was installed"
            );
            VERIFICATION
        }
        Err(InstallError::Failed(why)) => {
            let _ = writeln!(
                err,
                "nvmod install-nv-rs: step failed and was rolled back: {why}"
            );
            ROLLED_BACK
        }
    }
}
