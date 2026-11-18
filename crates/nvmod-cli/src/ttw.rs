// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod ttw --instance <dir> --fallout3 <dir> [--package <file>] [--installer <path>] [--json]`: Tale of Two
//! Wastelands through an external installer (the Rust installer on Linux, the official one on Windows). The job of
//! `nvmod-api` verifies the base game, runs the installer only through the verified runner of `nvmod-tools`,
//! checks what it reported against the `ttw-installer-result` contract, and imports the output as a Mod. This is
//! the thin wrapper: it reads the command line, calls the job and prints what the job says.

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, VERIFICATION, args::Args};
use nvmod_api::ttw::{InstallOutput, InstallRequest};
use nvmod_api::{CancelToken, ErrorKind, JobError, NoProgress, Outcome};
use std::io::Write;
use std::path::PathBuf;

const USAGE_LINE: &str = "usage: nvmod ttw --instance <dir> --fallout3 <dir> [--package <file>] [--installer <path>] [--profile <name>] [--json]\n       tool options: [--lock <file>] [--store <dir>] [--timeout-secs <n>]";
const OPTIONS: &[&str] = &[
    "instance",
    "fallout3",
    "package",
    "installer",
    "profile",
    "lock",
    "store",
    "timeout-secs",
];

/// Print a failed job the way this command always has, and return its exit code: a refusal is 2, a verification
/// failure 3, a failed step 4.
fn failure(err: &mut dyn Write, e: &JobError) -> u8 {
    match e.kind {
        ErrorKind::Refused => {
            let _ = writeln!(err, "nvmod ttw: refused: {}", e.summary);
            REFUSED
        }
        ErrorKind::Corrupt => {
            let _ = writeln!(
                err,
                "nvmod ttw: verification failed: {}\nnothing was changed",
                e.summary
            );
            VERIFICATION
        }
        _ => {
            let _ = writeln!(err, "nvmod ttw: {}", e.summary);
            ROLLED_BACK
        }
    }
}

/// Print what the job did: the JSON document with `--json`, the sentence and the activated plugins otherwise.
fn print(out: &mut dyn Write, output: &InstallOutput, json: bool) {
    if json {
        let doc = serde_json::json!({
            "status": "installed",
            "mod": output.report.r#mod,
            "profile": output.report.profile,
            "files": output.report.files.len(),
            "plugins": output.report.plugins,
            "journal": output.report.journal,
            "result": output.result,
        });
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&doc).unwrap_or_default()
        );
        return;
    }
    let _ = writeln!(
        out,
        "nvmod ttw: installed `{}` into profile `{}` ({} files, {} {})",
        output.report.r#mod,
        output.report.profile,
        output.report.files.len(),
        output.installer,
        output.version
    );
    for p in &output.report.plugins {
        let _ = writeln!(out, "  activates plugin {p}");
    }
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(args, &["json"], OPTIONS) {
        Ok(a)
            if a.positional.is_empty()
                && a.option("instance").is_some()
                && a.option("fallout3").is_some() =>
        {
            a
        }
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_LINE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod ttw: {why}\n{USAGE_LINE}");
            return USAGE;
        }
    };
    let timeout_secs = if let Some(text) = args.option("timeout-secs") {
        if let Ok(secs) = text.parse::<u64>() {
            Some(secs)
        } else {
            let _ = writeln!(err, "nvmod ttw: --timeout-secs needs a number");
            return USAGE;
        }
    } else {
        None
    };
    let request = InstallRequest {
        instance: PathBuf::from(args.option("instance").unwrap_or_default()),
        profile: args.option("profile").map(str::to_owned),
        fallout3: PathBuf::from(args.option("fallout3").unwrap_or_default()),
        package: args.option("package").map(PathBuf::from),
        installer: args.option("installer").map(PathBuf::from),
        lock: args.option("lock").map(PathBuf::from),
        store: args.option("store").map(PathBuf::from),
        timeout_secs,
    };
    match nvmod_api::ttw::install(&request, &mut NoProgress, &CancelToken::new()) {
        Ok(Outcome::Done(output)) => {
            print(out, &output, args.flag("json"));
            OK
        }
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => failure(err, &e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_is_exit_5() {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(run(&["--nope".to_owned()], &mut out, &mut err), USAGE);
        assert_eq!(run(&[], &mut out, &mut err), USAGE);
    }
}
