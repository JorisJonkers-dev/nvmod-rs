// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod profile apply|finish`: place a profile in a Wine prefix before a launch, and collect what the game
//! changed there afterwards (see `nvmod-profile`).

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, args::Args};
use nvmod_instance::NvmodInstance;
use nvmod_profile::{Error, Report, Target};
use std::io::Write;
use std::path::{Path, PathBuf};

const USAGE_TEXT: &str = "usage: nvmod profile apply --instance <dir> [--profile <name>] [--prefix <dir>] [--dry-run] [--json]
       nvmod profile finish --instance <dir> [--prefix <dir>] [--dry-run] [--json]";

fn print(out: &mut dyn Write, report: &Report, json: bool) {
    if json {
        let mut value = serde_json::to_value(report).unwrap_or_default();
        value["schema_version"] = 1.into();
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
        return;
    }
    let what = match report.status {
        "dry-run" => "would change (dry run, nothing written)",
        "unchanged" => "already in place",
        "collected" => "collected what the game changed",
        _ => "applied",
    };
    let _ = writeln!(
        out,
        "nvmod profile: {}: {what}",
        report.profile.as_deref().unwrap_or("no profile placed")
    );
    for a in &report.actions {
        let _ = writeln!(out, "  {:<7} {}  ({})", a.action, a.path, a.note);
    }
    for note in &report.notes {
        let _ = writeln!(out, "  note: {note}");
    }
    if let Some(n) = report.staged_files {
        let _ = writeln!(out, "staged again: {n} file(s)");
    }
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(
        args,
        &["json", "dry-run"],
        &["instance", "profile", "prefix"],
    ) {
        Ok(a) if a.option("instance").is_some() && a.positional.len() == 1 => a,
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_TEXT}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod profile: {why}\n{USAGE_TEXT}");
            return USAGE;
        }
    };
    let apply = args.positional[0] == "apply";
    if !apply && args.positional[0] != "finish" {
        let _ = writeln!(err, "{USAGE_TEXT}");
        return USAGE;
    }
    let refused = |why: &str, err: &mut dyn Write| {
        let _ = writeln!(err, "nvmod profile: refused: {why}");
        REFUSED
    };
    let root = Path::new(args.option("instance").unwrap_or_default());
    let instance = match NvmodInstance::open(root) {
        Ok(i) => i,
        Err(e) => return refused(&crate::install_mod::describe_open_error(root, &e), err),
    };
    // Without --prefix: the prefix the launch plan gives Proton (STEAM_COMPAT_DATA_PATH).
    let prefix: PathBuf = args
        .option("prefix")
        .map_or_else(|| instance.root.join("compatdata"), PathBuf::from);
    let target = match Target::from_prefix(&prefix) {
        Ok(t) => t,
        Err(why) => return refused(&why, err),
    };
    let dry = args.flag("dry-run");
    let result = if apply {
        match instance.resolve_profile(args.option("profile")) {
            Ok(profile) => nvmod_profile::apply(&instance, &profile, &target, dry),
            Err(e) => return refused(&e.to_string(), err),
        }
    } else {
        nvmod_profile::finish(&instance, &target, dry)
    };
    match result {
        Ok(report) => {
            print(out, &report, args.flag("json"));
            OK
        }
        Err(Error::Refused(why)) => refused(&why, err),
        Err(Error::Failed(why)) => {
            let _ = writeln!(err, "nvmod profile: {why}; rolled back");
            ROLLED_BACK
        }
    }
}
