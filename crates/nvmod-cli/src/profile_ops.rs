// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod profile list|copy|rename|delete|select|set`: manage the Profiles of an Instance.

use crate::{OK, REFUSED, USAGE, args::Args};
use nvmod_api::profile_ops::{
    Change, ChangeRequest, ListRequest, SelectRequest, SetRequest, list, select, set,
};
use nvmod_api::{NoProgress, Outcome};
use std::io::Write;
use std::path::PathBuf;

const USAGE_LINE: &str = "usage: nvmod profile list --instance <dir> [--json]
       nvmod profile copy <from> <to> --instance <dir> [--with-saves] [--dry-run] [--json]
       nvmod profile rename <from> <to> --instance <dir> [--dry-run] [--json]
       nvmod profile delete <name> --instance <dir> [--dry-run] [--json]
       nvmod profile select <name> --instance <dir> [--json]
       nvmod profile set <LocalSaves|LocalSettings|AutomaticArchiveInvalidation> <true|false> --instance <dir> [--profile <name>] [--dry-run] [--json]";

/// The sub-commands of this module.
pub(crate) const COMMANDS: [&str; 6] = ["list", "copy", "rename", "delete", "select", "set"];

fn print_json(out: &mut dyn Write, value: &impl serde::Serialize) {
    let _ = writeln!(
        out,
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

fn value_of(text: &str) -> Option<bool> {
    match text.to_ascii_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let parsed = match Args::parse(
        args,
        &["json", "dry-run", "with-saves"],
        &["instance", "profile"],
    ) {
        Ok(a) => a,
        Err(why) => return usage(&why, err),
    };
    let Some(instance) = parsed.option("instance").map(PathBuf::from) else {
        return usage("--instance is required", err);
    };
    let words: Vec<&str> = parsed.positional.iter().map(String::as_str).collect();
    let json = parsed.flag("json");
    let dry_run = parsed.flag("dry-run");
    match words.as_slice() {
        ["list"] => show_list(instance, json, out, err),
        [action @ ("copy" | "rename"), from, to] => {
            let change = if *action == "copy" {
                Change::Copy {
                    to: (*to).to_owned(),
                    with_saves: parsed.flag("with-saves"),
                }
            } else {
                Change::Rename {
                    to: (*to).to_owned(),
                }
            };
            change_profile(instance, from, change, dry_run, json, out, err)
        }
        ["delete", name] => change_profile(instance, name, Change::Delete, dry_run, json, out, err),
        ["select", name] => select_profile(instance, name, json, out, err),
        ["set", key, value] => {
            let request = match set_request(instance, &parsed, key, value) {
                Ok(request) => request,
                Err(why) => return usage(&why, err),
            };
            set_profile(&request, json, out, err)
        }
        _ => usage("", err),
    }
}

fn usage(why: &str, err: &mut dyn Write) -> u8 {
    if !why.is_empty() {
        let _ = writeln!(err, "nvmod profile: {why}");
    }
    let _ = writeln!(err, "{USAGE_LINE}");
    USAGE
}

fn show_list(instance: PathBuf, json: bool, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match list(
        &ListRequest { instance },
        &mut NoProgress,
        &crate::process_token(),
    ) {
        Ok(Outcome::Done(report)) => {
            if json {
                print_json(out, &report);
            } else {
                for p in &report.profiles {
                    let _ = writeln!(
                        out,
                        "{} {}  mods {}/{}  plugins {}/{}  local-saves {}  local-settings {}  archive-invalidation {}",
                        if p.selected { "*" } else { " " },
                        p.name,
                        p.mods_enabled,
                        p.mods_total,
                        p.plugins_active,
                        p.plugins_total,
                        p.local_saves,
                        p.local_settings,
                        p.archive_invalidation,
                    );
                }
            }
            OK
        }
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => crate::job_failure("profile", &e, err),
    }
}

fn select_profile(
    instance: PathBuf,
    name: &str,
    json: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let request = SelectRequest {
        instance,
        profile: name.to_owned(),
    };
    match select(&request, &mut NoProgress, &crate::process_token()) {
        Ok(Outcome::Done(done)) => {
            if json {
                print_json(out, &done);
            } else if done.changed {
                let _ = writeln!(out, "nvmod profile: selected `{}`", done.selected);
            } else {
                let _ = writeln!(
                    out,
                    "nvmod profile: `{}` is already selected",
                    done.selected
                );
            }
            OK
        }
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => crate::job_failure("profile", &e, err),
    }
}

/// The request of `profile set <key> <value>`, or what is wrong with the words (exit 5).
fn set_request(
    instance: PathBuf,
    parsed: &Args,
    key: &str,
    value: &str,
) -> Result<SetRequest, String> {
    if nvmod_profile::settings::known_key(key).is_none() {
        return Err(format!(
            "`{key}` is not a profile setting (one of {})",
            nvmod_profile::settings::KEYS.join(", ")
        ));
    }
    let value = value_of(value).ok_or_else(|| format!("`{value}` is not true or false"))?;
    Ok(SetRequest {
        instance,
        profile: parsed.option("profile").map(str::to_owned),
        key: key.to_owned(),
        value,
        dry_run: parsed.flag("dry-run"),
    })
}

fn set_profile(request: &SetRequest, json: bool, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match set(request, &mut NoProgress, &crate::process_token()) {
        Ok(Outcome::Done(done)) => {
            if json {
                print_json(out, &done);
            } else {
                let verb = match (done.dry_run, done.changed) {
                    (true, true) => "would set",
                    (false, true) => "set",
                    (_, false) => "already",
                };
                let _ = writeln!(
                    out,
                    "nvmod profile: {verb} {} = {} in `{}`",
                    done.key, done.value, done.profile
                );
            }
            OK
        }
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => crate::job_failure("profile", &e, err),
    }
}

fn change_profile(
    instance: PathBuf,
    profile: &str,
    change: Change,
    dry_run: bool,
    json: bool,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let request = ChangeRequest {
        instance,
        profile: profile.to_owned(),
        change,
        dry_run,
    };
    match nvmod_api::profile_ops::change(&request, &mut NoProgress, &crate::process_token()) {
        Ok(Outcome::Done(report)) => {
            if json {
                print_json(out, &report);
            } else {
                let verb = match (report.dry_run, report.action) {
                    (true, a) => format!("would {a}"),
                    (false, "copy") => "copied".to_owned(),
                    (false, "rename") => "renamed".to_owned(),
                    (false, _) => "deleted".to_owned(),
                };
                let _ = write!(out, "nvmod profile: {verb} `{}`", report.profile);
                if let Some(new) = &report.new_profile {
                    let _ = write!(out, " as `{new}`");
                }
                if let Some(to) = &report.trashed_to {
                    let _ = write!(out, " (moved to {to})");
                }
                let _ = writeln!(out);
                if let Some(id) = &report.journal {
                    let _ = writeln!(
                        out,
                        "nvmod profile: `nvmod rollback` undoes it (journal {id})"
                    );
                }
            }
            OK
        }
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => crate::job_failure("profile", &e, err),
    }
}
