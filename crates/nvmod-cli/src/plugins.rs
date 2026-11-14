// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod plugins list|enable|disable|move|lock|unlock`: the plugin list of a profile, edited by hand. A write is
//! journaled like a sort, so `nvmod rollback` takes it back; `list` and `--dry-run` write nothing.

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, args::Args};
use nvmod_instance::NvmodInstance;
use nvmod_plugins::{Edit, EditReport, ListReport, Place, PluginsError};
use std::io::Write;
use std::path::Path;

const USAGE_TEXT: &str = "usage: nvmod plugins list --instance <dir> [--profile <name>] [--json]
       nvmod plugins enable <plugin>... [--with-masters] --instance <dir> [--profile <name>] [--dry-run] [--json]
       nvmod plugins disable <plugin>... --instance <dir> [--profile <name>] [--dry-run] [--json]
       nvmod plugins move <plugin> (--top | --bottom | --before <plugin> | --after <plugin>)
                --instance <dir> [--profile <name>] [--dry-run] [--json]
       nvmod plugins lock <plugin> <position> --instance <dir> [--profile <name>] [--dry-run] [--json]
       nvmod plugins unlock <plugin> --instance <dir> [--profile <name>] [--dry-run] [--json]";

fn print_list(out: &mut dyn Write, report: &ListReport) {
    let _ = writeln!(
        out,
        "nvmod plugins: profile `{}`, {} plugin(s)",
        report.profile,
        report.plugins.len()
    );
    for p in &report.plugins {
        let mut notes = Vec::new();
        if p.forced {
            notes.push("forced first".to_owned());
        }
        if let Some(at) = p.locked {
            notes.push(format!("locked at {at}"));
        }
        if !p.installed {
            notes.push("not installed".to_owned());
        }
        if p.masters_missing > 0 {
            let missing: Vec<_> = p
                .masters
                .iter()
                .filter(|m| !m.present)
                .map(|m| m.name.as_str())
                .collect();
            notes.push(format!("missing master(s): {}", missing.join(", ")));
        }
        if let Some(provider) = &p.provider {
            notes.push(format!("from {provider}"));
        }
        let _ = writeln!(
            out,
            "{:>4} {} {}{}",
            p.position,
            if p.active { '*' } else { ' ' },
            p.name,
            if notes.is_empty() {
                String::new()
            } else {
                format!("  ({})", notes.join("; "))
            }
        );
    }
    for l in report.locks.iter().filter(|l| !l.listed) {
        let _ = writeln!(
            out,
            "      lock kept for absent plugin {} (position {})",
            l.name, l.position
        );
    }
}

fn print_edit(out: &mut dyn Write, r: &EditReport) {
    let what = match r.status {
        "dry-run" => "would change (dry run, nothing written)",
        "unchanged" => "nothing to change",
        _ => "changed",
    };
    let _ = writeln!(
        out,
        "nvmod plugins {}: profile `{}`: {what}: {}",
        r.action,
        r.profile,
        if r.changed.is_empty() {
            "none".to_owned()
        } else {
            r.changed.join(", ")
        }
    );
    for o in &r.order {
        let _ = writeln!(
            out,
            "{:>4} {} {}",
            o.position,
            if o.active { '*' } else { ' ' },
            o.name
        );
    }
    if r.journal.is_some() {
        let _ = writeln!(
            out,
            "plugins.txt and loadorder.txt written together; {} plugin file time(s) set; undo with `nvmod rollback`",
            r.stamped
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
    if places.len() == 1 {
        places.pop()
    } else {
        None
    }
}

/// The edit the words ask for, `None` for `list`, or what is wrong with the command line.
fn parse_edit(args: &Args) -> Result<Option<Edit>, String> {
    let names: Vec<String> = args.positional[1..].to_vec();
    let edit = match args.positional[0].as_str() {
        "list" if names.is_empty() => None,
        "enable" if !names.is_empty() => Some(Edit::Enable {
            names,
            with_masters: args.flag("with-masters"),
        }),
        "disable" if !names.is_empty() => Some(Edit::Disable(names)),
        "move" if names.len() == 1 => Some(Edit::Move {
            name: names[0].clone(),
            to: place_of(args).ok_or("give exactly one of --top, --bottom, --before, --after")?,
        }),
        "lock" if names.len() == 2 => match names[1].parse::<usize>() {
            Ok(position) if position >= 1 => Some(Edit::Lock {
                name: names[0].clone(),
                position,
            }),
            _ => return Err("the position is a whole number from 1".to_owned()),
        },
        "unlock" if names.len() == 1 => Some(Edit::Unlock(names[0].clone())),
        _ => return Err("not understood".to_owned()),
    };
    Ok(edit)
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(
        args,
        &["json", "dry-run", "top", "bottom", "with-masters"],
        &["instance", "profile", "before", "after"],
    ) {
        Ok(a) if a.option("instance").is_some() && !a.positional.is_empty() => a,
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_TEXT}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod plugins: {why}\n{USAGE_TEXT}");
            return USAGE;
        }
    };
    let edit = match parse_edit(&args) {
        Ok(edit) => edit,
        Err(why) => {
            let _ = writeln!(
                err,
                "nvmod plugins {}: {why}\n{USAGE_TEXT}",
                args.positional[0]
            );
            return USAGE;
        }
    };
    let refused = |why: &str, err: &mut dyn Write| {
        let _ = writeln!(err, "nvmod plugins: refused: {why}");
        REFUSED
    };
    let root = Path::new(args.option("instance").unwrap_or_default());
    let opened = if edit.is_some() {
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
    let Some(edit) = edit else {
        return match nvmod_plugins::list(&instance, &profile) {
            Ok(report) => {
                if json {
                    let _ = writeln!(
                        out,
                        "{}",
                        serde_json::to_string_pretty(&report).unwrap_or_default()
                    );
                } else {
                    print_list(out, &report);
                }
                OK
            }
            Err(PluginsError::Refused(why)) => refused(&why, err),
            Err(PluginsError::Failed(why)) => {
                let _ = writeln!(err, "nvmod plugins: {why}");
                ROLLED_BACK
            }
        };
    };
    match nvmod_plugins::apply(&instance, &profile, &edit, args.flag("dry-run")) {
        Ok(report) => {
            if json {
                let _ = writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&report).unwrap_or_default()
                );
            } else {
                print_edit(out, &report);
            }
            OK
        }
        Err(PluginsError::Refused(why)) => refused(&why, err),
        Err(PluginsError::Failed(why)) => {
            let _ = writeln!(err, "nvmod plugins: {why}");
            ROLLED_BACK
        }
    }
}
