// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod mods list|enable|disable|reorder`: the Mods of a profile, in priority order (the first is the highest
//! priority). A change is journaled like an install, so `nvmod rollback` takes it back.

use crate::{OK, USAGE, args::Args, mods_life};
use nvmod_api::mods::{Change, ChangeRequest, ListRequest, ModRow, Place, Report};
use nvmod_api::{NoProgress, Outcome};
use std::io::Write;
use std::path::PathBuf;

const USAGE_TEXT: &str = "usage: nvmod mods list --instance <dir> [--profile <name>] [--json]
       nvmod mods enable|disable <mod>... --instance <dir> [--profile <name>] [--dry-run] [--json]
       nvmod mods reorder <mod> (--top | --bottom | --before <mod> | --after <mod> | --position <n>)
                --instance <dir> [--profile <name>] [--dry-run] [--json]
       nvmod mods list ... [--category <id>]; remove|rename|create|clone|hide|unhide|note|separator|category:
                see `nvmod mods remove` with no arguments";

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
    if !report.plugins_deactivated.is_empty() {
        let _ = writeln!(
            out,
            "no longer active (no enabled Mod provides them): {}",
            report.plugins_deactivated.join(", ")
        );
    }
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

// One flat command with its list and change branches.
#[allow(clippy::too_many_lines)]
pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    if matches!(
        args.first().map(String::as_str),
        Some("reinstall" | "reconfigure")
    ) {
        return crate::install_mod::run_reinstall(args, out, err);
    }
    if mods_life::handles(args.first().map(String::as_str)) {
        return mods_life::run(args, out, err);
    }
    let args = match Args::parse(
        args,
        &["json", "dry-run", "top", "bottom"],
        &[
            "instance", "profile", "before", "after", "position", "category",
        ],
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
        "enable" if !names.is_empty() => Some(Change::Enable { mods: names }),
        "disable" if !names.is_empty() => Some(Change::Disable { mods: names }),
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
    let instance = PathBuf::from(args.option("instance").unwrap_or_default());
    let profile = args.option("profile").map(str::to_owned);
    let json = args.flag("json");
    let cancel = crate::process_token();
    let Some(change) = change else {
        let category = match args.option("category").map(str::parse::<u32>) {
            Some(Ok(id)) => Some(id),
            Some(Err(_)) => {
                // The command line opens the Instance before it checks the id, as it always did: a folder that is
                // not an Instance, or a missing profile, is still refused first, and a bad id is then refused with
                // its own words (exit 2) without printing a listing.
                let request = ListRequest {
                    instance,
                    profile,
                    category: None,
                };
                if let Err(e) = nvmod_api::mods::list(&request, &mut NoProgress, &cancel) {
                    return crate::job_failure("mods", &e, err);
                }
                let _ = writeln!(
                    err,
                    "nvmod mods: refused: --category takes a category id (a number)"
                );
                return crate::REFUSED;
            }
            None => None,
        };
        let request = ListRequest {
            instance,
            profile,
            category,
        };
        let listed = match nvmod_api::mods::list(&request, &mut NoProgress, &cancel) {
            Ok(Outcome::Done(listed)) => listed,
            Ok(Outcome::Cancelled) => return crate::REFUSED,
            Err(e) => return crate::job_failure("mods", &e, err),
        };
        let rows: Vec<ModRow> = listed.mods.iter().map(|r| r.row.clone()).collect();
        if json {
            let _ = writeln!(
                out,
                "{}",
                serde_json::to_string_pretty(
                    &serde_json::json!({ "profile": listed.profile, "mods": listed.mods })
                )
                .unwrap_or_default()
            );
        } else {
            let _ = writeln!(
                out,
                "nvmod mods: profile `{}`, {} entr(ies)",
                listed.profile,
                rows.len()
            );
            print_rows(out, &rows);
        }
        return OK;
    };
    let request = ChangeRequest {
        instance,
        profile,
        change,
        dry_run: args.flag("dry-run"),
    };
    match nvmod_api::mods::change(&request, &mut NoProgress, &cancel) {
        Ok(Outcome::Done(report)) => {
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
        Ok(Outcome::Cancelled) => crate::REFUSED,
        Err(e) => crate::job_failure("mods", &e, err),
    }
}
