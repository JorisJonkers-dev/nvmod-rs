// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod overwrite list|create-mod|move|delete|sync`: what programs wrote that no Mod owns. A change is journaled
//! (kind `lifecycle`), so `nvmod rollback` takes it back, and is refused while the game or Mod Organizer runs.

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, args::Args};
use nvmod_instance::NvmodInstance;
use nvmod_modinstall::lifecycle::{self, Action, Report};
use nvmod_modinstall::mods::ModsError;
use std::io::Write;
use std::path::Path;

const USAGE_TEXT: &str = "usage: nvmod overwrite list [common options]
       nvmod overwrite create-mod <name> [<glob>...] [common options]
       nvmod overwrite move <path> --to <mod> [common options]
       nvmod overwrite delete <path>... [common options]
       nvmod overwrite sync [--to <mod>] [common options]
  common options: --instance <dir> [--profile <name>] [--dry-run] [--json]
  paths are inside Overwrite, `/` separated; a glob uses `*`, `?` and `**`, ignoring case";

fn print_report(out: &mut dyn Write, r: &Report) {
    let what = match r.status {
        "dry-run" => "would change (dry run, nothing written)",
        "unchanged" => "nothing to change",
        _ => "changed",
    };
    let _ = writeln!(
        out,
        "nvmod {}: profile `{}`: {what}: {} file(s)",
        r.action.replacen("overwrite-", "overwrite ", 1),
        r.profile,
        r.files.len()
    );
    if let Some(detail) = &r.detail {
        let _ = writeln!(out, "{detail}");
    }
    for change in &r.changes {
        let who = change
            .provider
            .as_deref()
            .map_or(String::new(), |p| format!(" ({p})"));
        let kind = serde_json::to_value(change.kind)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_default();
        let _ = writeln!(out, "  {kind}: {}{who}", change.path);
    }
    for file in &r.files {
        let _ = writeln!(out, "  {file}");
    }
    if let Some(n) = r.staged_files {
        let _ = writeln!(out, "staged again: {n} file(s)");
    }
    if r.journal.is_some() {
        let _ = writeln!(out, "undo with `nvmod rollback`");
    }
}

const SWEEP_USAGE: &str = "usage: nvmod sweep [--to <mod>] [--protect <glob>] --instance <dir> [--profile <name>] [--dry-run] [--json]
  takes what a program did to the Staging tree back: new and replaced files go to Overwrite (or the Mod --to), the
  tree is built again (which puts every Mod's file back); files written in place and files deleted are reported;
  --protect adds a glob of paths to stage as copies from now on";

/// `nvmod sweep`: see [`SWEEP_USAGE`].
pub(crate) fn run_sweep(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(
        args,
        &["json", "dry-run"],
        &["instance", "profile", "to", "protect"],
    ) {
        Ok(a) if a.option("instance").is_some() && a.positional.is_empty() => a,
        Ok(_) => {
            let _ = writeln!(err, "{SWEEP_USAGE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod sweep: {why}\n{SWEEP_USAGE}");
            return USAGE;
        }
    };
    let refused = |why: &str, err: &mut dyn Write| {
        let _ = writeln!(err, "nvmod sweep: refused: {why}");
        REFUSED
    };
    let root = Path::new(args.option("instance").unwrap_or_default());
    let instance = match NvmodInstance::open(root) {
        Ok(i) => i,
        Err(e) => return refused(&crate::install_mod::describe_open_error(root, &e), err),
    };
    let profile = match instance.resolve_profile(args.option("profile")) {
        Ok(p) => p,
        Err(e) => return refused(&e.to_string(), err),
    };
    let dry_run = args.flag("dry-run");
    if !dry_run
        && let Some(name) = nvmod_backup::game_running(&instance.root)
            .or_else(|| nvmod_backup::process_running(&["ModOrganizer.exe"]))
    {
        return refused(
            &format!("`{name}` is running; close it first (a dry run is allowed)"),
            err,
        );
    }
    let action = Action::Sweep {
        to: args.option("to").map(str::to_owned),
        protect: args.option("protect").map(str::to_owned),
    };
    match lifecycle::apply(&instance, &profile, "overwrite", &action, dry_run) {
        Ok(report) => {
            if args.flag("json") {
                let _ = writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                );
            } else {
                print_report(out, &report);
            }
            OK
        }
        Err(ModsError::Refused(why)) => refused(&why, err),
        Err(ModsError::Failed(why)) => {
            let _ = writeln!(err, "nvmod sweep: {why}");
            ROLLED_BACK
        }
    }
}

// One flat match over the command words reads better than helpers.
#[allow(clippy::too_many_lines)]
pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(args, &["json", "dry-run"], &["instance", "profile", "to"]) {
        Ok(a) if a.option("instance").is_some() && !a.positional.is_empty() => a,
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_TEXT}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod overwrite: {why}\n{USAGE_TEXT}");
            return USAGE;
        }
    };
    let words: Vec<&str> = args.positional.iter().map(String::as_str).collect();
    let to = args.option("to");
    let strings = |w: &[&str]| w.iter().map(|s| (*s).to_owned()).collect::<Vec<String>>();
    let parsed: Option<(String, Action)> = match words.as_slice() {
        ["list"] if to.is_none() => None,
        ["create-mod", name, globs @ ..] if to.is_none() => Some((
            (*name).to_owned(),
            Action::OverwriteCreateMod {
                globs: strings(globs),
            },
        )),
        ["move", path] => to.map(|t| {
            (
                (*path).to_owned(),
                Action::OverwriteMove { to: t.to_owned() },
            )
        }),
        ["delete", first, more @ ..] if to.is_none() => Some((
            (*first).to_owned(),
            Action::OverwriteDelete {
                more: strings(more),
            },
        )),
        ["sync"] => Some((
            "overwrite".to_owned(),
            Action::OverwriteSync {
                to: to.map(str::to_owned),
            },
        )),
        _ => None,
    };
    let is_list = words.as_slice() == ["list"] && to.is_none();
    if parsed.is_none() && !is_list {
        let _ = writeln!(err, "{USAGE_TEXT}");
        return USAGE;
    }
    let refused = |why: &str, err: &mut dyn Write| {
        let _ = writeln!(err, "nvmod overwrite: refused: {why}");
        REFUSED
    };
    let root = Path::new(args.option("instance").unwrap_or_default());
    let instance = match NvmodInstance::open(root) {
        Ok(i) => i,
        Err(e) => return refused(&crate::install_mod::describe_open_error(root, &e), err),
    };
    let profile = match instance.resolve_profile(args.option("profile")) {
        Ok(p) => p,
        Err(e) => return refused(&e.to_string(), err),
    };
    if is_list {
        return match lifecycle::overwrite_list(&instance, &profile) {
            Ok(list) => {
                if args.flag("json") {
                    let _ = writeln!(
                        out,
                        "{}",
                        serde_json::to_string_pretty(&list).unwrap_or_default()
                    );
                } else if list.files.is_empty() {
                    let _ = writeln!(out, "Overwrite is empty");
                } else {
                    let _ = writeln!(out, "Overwrite: {} file(s)", list.files.len());
                    for f in &list.files {
                        let by = if f.providers.is_empty() {
                            String::new()
                        } else {
                            format!("  (also in {})", f.providers.join(", "))
                        };
                        let _ = writeln!(out, "  {}  {} bytes{by}", f.path, f.size);
                    }
                }
                OK
            }
            Err(ModsError::Refused(why)) => refused(&why, err),
            Err(ModsError::Failed(why)) => {
                let _ = writeln!(err, "nvmod overwrite: {why}");
                ROLLED_BACK
            }
        };
    }
    let Some((target, action)) = parsed else {
        return USAGE;
    };
    let dry_run = args.flag("dry-run");
    if !dry_run
        && let Some(name) = nvmod_backup::game_running(&instance.root)
            .or_else(|| nvmod_backup::process_running(&["ModOrganizer.exe"]))
    {
        return refused(
            &format!("`{name}` is running; close it first (a dry run is allowed)"),
            err,
        );
    }
    match lifecycle::apply(&instance, &profile, &target, &action, dry_run) {
        Ok(report) => {
            if args.flag("json") {
                let _ = writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                );
            } else {
                print_report(out, &report);
            }
            OK
        }
        Err(ModsError::Refused(why)) => refused(&why, err),
        Err(ModsError::Failed(why)) => {
            let _ = writeln!(err, "nvmod overwrite: {why}");
            ROLLED_BACK
        }
    }
}
