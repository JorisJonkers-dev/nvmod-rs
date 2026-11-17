// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod mods remove|rename|create|clone|hide|unhide|note`, `mods separator add|rename|remove` and
//! `mods category set`: the life of a Mod folder and its separators. A change is journaled (kind `lifecycle`), so
//! `nvmod rollback` takes it back, and is refused while the game or Mod Organizer runs.

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, args::Args};
use nvmod_instance::NvmodInstance;
use nvmod_modinstall::lifecycle::{self, Action, Report};
use nvmod_modinstall::mods::{ModsError, Place};
use std::io::Write;
use std::path::Path;

pub(crate) const USAGE_TEXT: &str = "usage: nvmod mods remove <mod> [--force] [common options]
       nvmod mods rename <mod> <new name> [common options]
       nvmod mods create <name> [--top | --bottom | --before <mod> | --after <mod> | --position <n>] [common options]
       nvmod mods clone <mod> [--name <name>] [--hardlink] [common options]
       nvmod mods hide|unhide <mod> <path inside the Mod> [common options]
       nvmod mods note <mod> <text> [common options]
       nvmod mods separator add <label> (--before <mod> | --after <mod>) [common options]
       nvmod mods separator rename <label> <new label> [common options]
       nvmod mods separator remove <label> [common options]
       nvmod mods category set <mod> [<id>...] [common options]
  common options: --instance <dir> [--profile <name>] [--dry-run] [--json]";

/// Whether `sub` (the word after `nvmod mods`) is a command of this module.
pub(crate) fn handles(sub: Option<&str>) -> bool {
    matches!(
        sub,
        Some(
            "remove"
                | "rename"
                | "create"
                | "clone"
                | "hide"
                | "unhide"
                | "note"
                | "separator"
                | "category"
        )
    )
}

fn place_of(args: &Args, required: bool) -> Result<Option<Place>, ()> {
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
        places.push(Place::Position(n.parse().map_err(|_| ())?));
    }
    match places.len() {
        0 if !required => Ok(None),
        1 => Ok(places.pop()),
        _ => Err(()),
    }
}

fn print_report(out: &mut dyn Write, r: &Report) {
    let what = match r.status {
        "dry-run" => "would change (dry run, nothing written)",
        "unchanged" => "nothing to change",
        _ => "changed",
    };
    let subject = r
        .new_name
        .as_ref()
        .map_or_else(|| r.name.clone(), |new| format!("{} -> {new}", r.name));
    let _ = writeln!(
        out,
        "nvmod mods {}: profile `{}`: {what}: {subject}",
        r.action, r.profile
    );
    if !r.plugins_dropped.is_empty() {
        let _ = writeln!(out, "plugins dropped: {}", r.plugins_dropped.join(", "));
    }
    if !r.dependants.is_empty() {
        let _ = writeln!(
            out,
            "warning: active plugins that need it (removed anyway with --force): {}",
            r.dependants.join(", ")
        );
    }
    if let Some(n) = r.staged_files {
        let _ = writeln!(out, "staged again: {n} file(s)");
    }
    if r.journal.is_some() {
        let _ = writeln!(out, "undo with `nvmod rollback`");
    }
}

/// The active plugins of other Mods that need a plugin `target` provides.
fn dependants_of(instance: &NvmodInstance, profile: &str, target: &str) -> Vec<String> {
    let Ok(list) = nvmod_plugins::list(instance, profile) else {
        return Vec::new();
    };
    let theirs: Vec<&str> = list
        .plugins
        .iter()
        .filter(|p| {
            p.provider
                .as_deref()
                .is_some_and(|m| m.eq_ignore_ascii_case(target))
        })
        .map(|p| p.name.as_str())
        .collect();
    list.plugins
        .iter()
        .filter(|p| p.active && !theirs.contains(&p.name.as_str()))
        .filter(|p| {
            p.masters
                .iter()
                .any(|m| theirs.iter().any(|t| t.eq_ignore_ascii_case(&m.name)))
        })
        .map(|p| p.name.clone())
        .collect()
}

fn known_categories(instance: &NvmodInstance) -> Vec<u32> {
    std::fs::read(instance.root.join("categories.dat")).map_or_default(|b| {
        nvmod_formats::categories::Categories::parse(&b)
            .entries
            .iter()
            .map(|c| c.id)
            .collect()
    })
}

// One flat match over the command words reads better than ten helpers.
#[allow(clippy::too_many_lines)]
pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(
        args,
        &["json", "dry-run", "top", "bottom", "force", "hardlink"],
        &["instance", "profile", "before", "after", "position", "name"],
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
    let usage = |err: &mut dyn Write| {
        let _ = writeln!(err, "{USAGE_TEXT}");
        USAGE
    };
    let words: Vec<&str> = args.positional.iter().map(String::as_str).collect();
    let place = |required| place_of(&args, required);
    let parsed: Option<(String, Action)> = match words.as_slice() {
        ["remove", m] => Some((
            (*m).to_owned(),
            Action::Remove {
                force: args.flag("force"),
                dependants: Vec::new(),
            },
        )),
        ["rename", m, to] => Some((
            (*m).to_owned(),
            Action::Rename {
                to: (*to).to_owned(),
            },
        )),
        ["create", name] => place(false)
            .ok()
            .map(|p| ((*name).to_owned(), Action::Create { place: p })),
        ["clone", m] => Some((
            (*m).to_owned(),
            Action::Clone {
                name: args.option("name").map(str::to_owned),
                hardlink: args.flag("hardlink"),
            },
        )),
        ["hide", m, path] => Some((
            (*m).to_owned(),
            Action::Hide {
                path: (*path).to_owned(),
            },
        )),
        ["unhide", m, path] => Some((
            (*m).to_owned(),
            Action::Unhide {
                path: (*path).to_owned(),
            },
        )),
        ["note", m, text] => Some((
            (*m).to_owned(),
            Action::Note {
                text: (*text).to_owned(),
            },
        )),
        ["separator", "add", label] => place(true)
            .ok()
            .flatten()
            .filter(|p| matches!(p, Place::Before(_) | Place::After(_)))
            .map(|p| ((*label).to_owned(), Action::SeparatorAdd { place: p })),
        ["separator", "rename", label, to] => Some((
            (*label).to_owned(),
            Action::SeparatorRename {
                to: (*to).to_owned(),
            },
        )),
        ["separator", "remove", label] => Some(((*label).to_owned(), Action::SeparatorRemove)),
        ["category", "set", m, ids @ ..] => ids
            .iter()
            .map(|i| i.parse::<u32>().ok())
            .collect::<Option<Vec<u32>>>()
            .map(|ids| {
                (
                    (*m).to_owned(),
                    Action::Categories {
                        ids,
                        known: Vec::new(),
                    },
                )
            }),
        _ => None,
    };
    let Some((target, mut action)) = parsed else {
        return usage(err);
    };
    let refused = |why: &str, err: &mut dyn Write| {
        let _ = writeln!(err, "nvmod mods: refused: {why}");
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
        && let Some(name) = nvmod_backup::game_running()
            .or_else(|| nvmod_backup::process_running(&["ModOrganizer.exe"]))
    {
        return refused(
            &format!("`{name}` is running; close it first (a dry run is allowed)"),
            err,
        );
    }
    match &mut action {
        Action::Remove { dependants, .. } => {
            *dependants = dependants_of(&instance, &profile, &target);
        }
        Action::Categories { known, .. } => *known = known_categories(&instance),
        _ => {}
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
            let _ = writeln!(err, "nvmod mods: {why}");
            ROLLED_BACK
        }
    }
}
