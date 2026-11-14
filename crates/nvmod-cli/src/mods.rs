// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod mods list|enable|disable|reorder`: the Mods of a profile, in priority order (the first is the highest
//! priority). A change is journaled like an install, so `nvmod rollback` takes it back.

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, args::Args};
use nvmod_instance::NvmodInstance;
use nvmod_modinstall::mods::{self, Change, ModRow, ModsError, Place, Report};
use std::io::Write;
use std::path::Path;

const USAGE_TEXT: &str = "usage: nvmod mods list --instance <dir> [--profile <name>] [--json]
       nvmod mods enable|disable <mod>... --instance <dir> [--profile <name>] [--dry-run] [--json]
       nvmod mods reorder <mod> (--top | --bottom | --before <mod> | --after <mod> | --position <n>)
                --instance <dir> [--profile <name>] [--dry-run] [--json]";

fn print_rows(out: &mut dyn Write, rows: &[ModRow]) {
    for r in rows {
        let mark = match r.state {
            "enabled" => '+',
            "disabled" => '-',
            _ => '*',
        };
        let note = match (r.kind, r.installed) {
            ("separator", _) => " (separator)",
            (_, false) => " (folder missing)",
            _ => "",
        };
        let _ = writeln!(out, "{:>4} {mark} {}{note}", r.position, r.name);
    }
}

fn print_report(out: &mut dyn Write, report: &Report) {
    let what = match report.status {
        "dry-run" => "would change (dry run, nothing written)",
        "unchanged" => "nothing to change",
        _ => "changed",
    };
    let _ = writeln!(
        out,
        "nvmod mods {}: profile `{}`: {what}: {}",
        report.action,
        report.profile,
        if report.changed.is_empty() {
            "none".to_owned()
        } else {
            report.changed.join(", ")
        }
    );
    print_rows(out, &report.mods);
    if let Some(n) = report.staged_files {
        let _ = writeln!(out, "staged again: {n} file(s)");
    }
    if report.journal.is_some() {
        let _ = writeln!(
            out,
            "undo with `nvmod rollback`; run `nvmod sort --apply` to bring the Load order in line"
        );
    }
}

fn place_of(args: &Args) -> Option<Place> {
    let mut places = Vec::new();
    if args.flag("top") {
        places.push(Place::Top);
    }
    if args.flag("bottom") {
        places.push(Place::Bottom);
    }
    if let Some(n) = args.option("before") {
        places.push(Place::Before(n.to_owned()));
    }
    if let Some(n) = args.option("after") {
        places.push(Place::After(n.to_owned()));
    }
    if let Some(n) = args.option("position") {
        places.push(Place::Position(n.parse().ok()?));
    }
    if places.len() == 1 {
        places.pop()
    } else {
        None
    }
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(
        args,
        &["json", "dry-run", "top", "bottom"],
        &["instance", "profile", "before", "after", "position"],
    ) {
        Ok(a) if a.option("instance").is_some() && !a.positional.is_empty() => a,
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_TEXT}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod mods: {why}\n{USAGE_TEXT}");
            return USAGE;
        }
    };
    let sub = args.positional[0].as_str();
    let names: Vec<String> = args.positional[1..].to_vec();
    let change = match sub {
        "list" if names.is_empty() => None,
        "enable" if !names.is_empty() => Some(Change::Enable(names)),
        "disable" if !names.is_empty() => Some(Change::Disable(names)),
        "reorder" if names.len() == 1 => {
            let Some(to) = place_of(&args) else {
                let _ = writeln!(
                    err,
                    "nvmod mods reorder: give exactly one of --top, --bottom, --before, --after, --position <n>\n{USAGE_TEXT}"
                );
                return USAGE;
            };
            Some(Change::Move {
                name: names[0].clone(),
                to,
            })
        }
        _ => {
            let _ = writeln!(err, "{USAGE_TEXT}");
            return USAGE;
        }
    };
    let refused = |why: &str, err: &mut dyn Write| {
        let _ = writeln!(err, "nvmod mods: refused: {why}");
        REFUSED
    };
    let root = Path::new(args.option("instance").unwrap_or_default());
    let opened = if change.is_some() {
        NvmodInstance::open(root)
    } else {
        NvmodInstance::open_read_only(root)
    };
    let instance = match opened {
        Ok(i) => i,
        Err(e) => return refused(&crate::install_mod::describe_open_error(root, &e), err),
    };
    let profile = match instance.resolve_profile(args.option("profile")) {
        Ok(p) => p,
        Err(e) => return refused(&e.to_string(), err),
    };
    let json = args.flag("json");
    let Some(change) = change else {
        let rows = mods::list(&instance, &profile);
        if json {
            let _ = writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({ "profile": profile, "mods": rows })
                )
                .unwrap_or_default()
            );
        } else {
            let _ = writeln!(
                out,
                "nvmod mods: profile `{profile}`, {} entr(ies)",
                rows.len()
            );
            print_rows(out, &rows);
        }
        return OK;
    };
    match mods::apply(&instance, &profile, &change, args.flag("dry-run")) {
        Ok(report) => {
            if json {
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
            let _ = writeln!(err, "nvmod mods: {why}");
            ROLLED_BACK
        }
    }
}
