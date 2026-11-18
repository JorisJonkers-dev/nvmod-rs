// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod conflicts` and `nvmod tree`: the file-level conflicts of a profile and its virtual Data folder. Both only
//! read.

use crate::{OK, REFUSED, USAGE, args::Args};
use nvmod_api::{CancelToken, NoProgress, Outcome, conflicts as job};
use nvmod_conflicts::{ConflictError, Report};
use nvmod_instance::NvmodInstance;
use std::io::Write;
use std::path::{Path, PathBuf};

const CONFLICTS_USAGE: &str = "usage: nvmod conflicts --instance <dir> [--profile <name>] [--mod <name>] [--archives] [--json]";
const TREE_USAGE: &str =
    "usage: nvmod tree --instance <dir> [--profile <name>] [--path <prefix>] [--json]";

/// Open the Instance read only and resolve the profile, or print why not.
fn open(command: &str, args: &Args, err: &mut dyn Write) -> Result<(NvmodInstance, String), u8> {
    let root = Path::new(args.option("instance").unwrap_or_default());
    let refused = |why: &str, err: &mut dyn Write| {
        let _ = writeln!(err, "nvmod {command}: refused: {why}");
        REFUSED
    };
    let instance = NvmodInstance::open_read_only(root)
        .map_err(|e| refused(&crate::install_mod::describe_open_error(root, &e), err))?;
    let profile = instance
        .resolve_profile(args.option("profile"))
        .map_err(|e| refused(&e.to_string(), err))?;
    Ok((instance, profile))
}

pub(crate) fn run_conflicts(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(args, &["json", "archives"], &["instance", "profile", "mod"]) {
        Ok(a) if a.positional.is_empty() && a.option("instance").is_some() => a,
        Ok(_) => {
            let _ = writeln!(err, "{CONFLICTS_USAGE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod conflicts: {why}\n{CONFLICTS_USAGE}");
            return USAGE;
        }
    };
    let request = job::ReportRequest {
        instance: PathBuf::from(args.option("instance").unwrap_or_default()),
        profile: args.option("profile").map(str::to_owned),
        mod_name: args.option("mod").map(str::to_owned),
        archives: args.flag("archives"),
    };
    match job::report(&request, &mut NoProgress, &CancelToken::new()) {
        Ok(Outcome::Done(done)) => print_report(&done.report, args.flag("json"), out),
        Ok(Outcome::Cancelled) => {
            let _ = writeln!(err, "nvmod conflicts: cancelled");
            REFUSED
        }
        Err(e) => {
            let _ = writeln!(err, "nvmod conflicts: refused: {e}");
            REFUSED
        }
    }
}

/// Print the report as JSON, or as the text the command prints: one line per Mod that overwrites, is overwritten,
/// is mixed or is redundant, one per contested path, and the archive axes when they were asked for.
fn print_report(report: &Report, json: bool, out: &mut dyn Write) -> u8 {
    if json {
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(report).unwrap_or_default()
        );
        return OK;
    }
    let _ = writeln!(
        out,
        "nvmod conflicts: profile `{}`: {} contested path(s)",
        report.profile,
        report.conflicts.len()
    );
    for m in &report.mods {
        let mut flags = Vec::new();
        for (on, word) in [
            (m.overwrites, "overwrites others"),
            (m.overwritten, "overwritten by others"),
            (m.mixed, "mixed"),
            (m.redundant, "redundant"),
        ] {
            if on {
                flags.push(word);
            }
        }
        if !flags.is_empty() {
            let _ = writeln!(
                out,
                "  {}: {} ({} won, {} lost of {} file(s))",
                m.name,
                flags.join(", "),
                m.wins,
                m.losses,
                m.files
            );
        }
    }
    for c in &report.conflicts {
        let losers: Vec<_> = c.losers.iter().map(|l| l.name.as_str()).collect();
        let _ = writeln!(
            out,
            "  {}: {} beats {}",
            c.path,
            c.winner.name,
            losers.join(", ")
        );
    }
    if let Some(archives) = &report.archives {
        print_archives(archives, out);
    }
    OK
}

fn print_archives(archives: &nvmod_conflicts::archives::ArchiveReport, out: &mut dyn Write) {
    let _ = writeln!(
        out,
        "archives: {} loaded, {} contested path(s) (rules {}; bInvalidateOlderFiles {})",
        archives.archives.len(),
        archives.conflicts.len(),
        archives.basis,
        match archives.invalidate_older_files {
            Some(true) => "on",
            Some(false) => "off",
            None => "not set",
        }
    );
    for a in &archives.archives {
        let _ = match &a.error {
            None => writeln!(
                out,
                "  {} ({}): {} file(s), {}",
                a.name, a.provider, a.files, a.loaded_by
            ),
            Some(why) => writeln!(out, "  {} ({}): {why}", a.name, a.provider),
        };
    }
    let describe = |p: &nvmod_conflicts::archives::Provider| match &p.archive {
        Some(a) => format!("{} ({a})", p.name),
        None => format!("{} (loose)", p.name),
    };
    for c in &archives.conflicts {
        let losers: Vec<_> = c.losers.iter().map(describe).collect();
        let _ = writeln!(
            out,
            "  {}: {} beats {} [{}, unverified]",
            c.path,
            describe(&c.winner),
            losers.join(", "),
            c.rule
        );
    }
}

pub(crate) fn run_tree(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(args, &["json"], &["instance", "profile", "path"]) {
        Ok(a) if a.positional.is_empty() && a.option("instance").is_some() => a,
        Ok(_) => {
            let _ = writeln!(err, "{TREE_USAGE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod tree: {why}\n{TREE_USAGE}");
            return USAGE;
        }
    };
    let (instance, profile) = match open("tree", &args, err) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let tree = match nvmod_conflicts::tree(&instance, &profile) {
        Ok(t) => t.under(args.option("path").unwrap_or_default()),
        Err(ConflictError::Refused(why)) => {
            let _ = writeln!(err, "nvmod tree: refused: {why}");
            return REFUSED;
        }
    };
    if args.flag("json") {
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&tree).unwrap_or_default()
        );
    } else {
        for e in &tree.entries {
            let _ = writeln!(out, "{}  <- {}", e.path, e.provider);
        }
    }
    OK
}
