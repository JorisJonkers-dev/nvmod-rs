// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod backup create|list|restore|prune`: snapshots of a profile's lists and INIs, and the automatic snapshot
//! that the writing commands take first (see `nvmod-backup`).

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, VERIFICATION, args::Args};
use nvmod_backup::{Config, Error, Snapshot, clock};
use nvmod_instance::NvmodInstance;
use std::io::Write;
use std::path::Path;

const USAGE_TEXT: &str = "usage: nvmod backup create --instance <dir> [--profile <name>] [--with-saves] [--json]
       nvmod backup list --instance <dir> [--profile <name>] [--json]
       nvmod backup restore <id|latest> --instance <dir> [--profile <name>] [--json]
       nvmod backup prune --instance <dir> [--profile <name>] [--keep <n>] [--days <n>] [--dry-run] [--json]";

fn print_json(out: &mut dyn Write, value: &serde_json::Value) {
    let _ = writeln!(
        out,
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

fn print_snapshots(out: &mut dyn Write, snapshots: &[Snapshot]) {
    for s in snapshots {
        let note = s
            .problem
            .as_ref()
            .map_or_else(String::new, |p| format!("  (unreadable: {p})"));
        let _ = writeln!(
            out,
            "  {}  {:<5} {} file(s), {} byte(s){}{note}",
            s.id,
            s.source,
            s.files,
            s.bytes,
            if s.with_saves { ", with saves" } else { "" }
        );
    }
}

/// The exit code and message of a failed backup step.
fn failure(error: &Error, err: &mut dyn Write) -> u8 {
    match error {
        Error::Refused(why) => {
            let _ = writeln!(err, "nvmod backup: refused: {why}");
            REFUSED
        }
        Error::Corrupt(why) => {
            let _ = writeln!(
                err,
                "nvmod backup: verification failed: {why}; nothing was changed"
            );
            VERIFICATION
        }
        Error::Failed(why) => {
            let _ = writeln!(err, "nvmod backup: {why}");
            ROLLED_BACK
        }
    }
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(
        args,
        &["json", "with-saves", "dry-run"],
        &["instance", "profile", "keep", "days"],
    ) {
        Ok(a) if a.option("instance").is_some() && !a.positional.is_empty() => a,
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_TEXT}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod backup: {why}\n{USAGE_TEXT}");
            return USAGE;
        }
    };
    let sub = args.positional[0].as_str();
    let wanted = args.positional.len();
    let shape_ok = match sub {
        "create" | "list" | "prune" => wanted == 1,
        "restore" => wanted == 2,
        _ => false,
    };
    let number = |name: &str| args.option(name).map(str::parse::<usize>);
    let (keep, days) = (number("keep"), number("days"));
    if !shape_ok || matches!(keep, Some(Err(_))) || matches!(days, Some(Err(_))) {
        let _ = writeln!(err, "{USAGE_TEXT}");
        return USAGE;
    }
    let root = Path::new(args.option("instance").unwrap_or_default());
    let opened = if sub == "list" {
        NvmodInstance::open_read_only(root)
    } else {
        NvmodInstance::open(root)
    };
    let instance = match opened {
        Ok(i) => i,
        Err(e) => {
            let _ = writeln!(
                err,
                "nvmod backup: refused: {}",
                crate::install_mod::describe_open_error(root, &e)
            );
            return REFUSED;
        }
    };
    let profile = match instance.resolve_profile(args.option("profile")) {
        Ok(p) => p,
        Err(e) => {
            let _ = writeln!(err, "nvmod backup: refused: {e}");
            return REFUSED;
        }
    };
    let cx = Cx {
        instance: &instance,
        profile: &profile,
        json: args.flag("json"),
        now: clock::now(),
    };
    match sub {
        "create" => create(&cx, args.flag("with-saves"), out, err),
        "list" => list(&cx, out),
        "restore" => restore(&cx, &args.positional[1], out, err),
        _ => {
            let mut config: Config = nvmod_backup::load_config(&instance);
            if let Some(Ok(n)) = keep {
                config.keep_newest = n;
            }
            if let Some(Ok(n)) = days {
                config.daily_days = u32::try_from(n).unwrap_or(u32::MAX);
            }
            prune(&cx, &config, args.flag("dry-run"), out, err)
        }
    }
}

struct Cx<'a> {
    instance: &'a NvmodInstance,
    profile: &'a str,
    json: bool,
    now: i64,
}

fn create(cx: &Cx, with_saves: bool, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match nvmod_backup::create(cx.instance, cx.profile, cx.now, with_saves) {
        Ok(m) => {
            if cx.json {
                print_json(
                    out,
                    &serde_json::json!({
                        "schema_version": 1, "status": "created", "profile": cx.profile, "snapshot": m,
                    }),
                );
            } else {
                let _ = writeln!(
                    out,
                    "nvmod backup: snapshot `{}` of profile `{}` ({} file(s))",
                    m.id,
                    cx.profile,
                    m.files.len()
                );
            }
            OK
        }
        Err(e) => failure(&e, err),
    }
}

fn list(cx: &Cx, out: &mut dyn Write) -> u8 {
    let snapshots = nvmod_backup::list(cx.instance, cx.profile);
    if cx.json {
        print_json(
            out,
            &serde_json::json!({
                "schema_version": 1, "profile": cx.profile, "snapshots": snapshots,
            }),
        );
    } else {
        let _ = writeln!(
            out,
            "nvmod backup: profile `{}`, {} snapshot(s)",
            cx.profile,
            snapshots.len()
        );
        print_snapshots(out, &snapshots);
    }
    OK
}

fn restore(cx: &Cx, id: &str, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    if let Some(name) = nvmod_backup::game_running() {
        let _ = writeln!(
            err,
            "nvmod backup: refused: the game is running (`{name}`); close it first"
        );
        return REFUSED;
    }
    match nvmod_backup::restore(cx.instance, cx.profile, id, cx.now) {
        Ok(r) => {
            if cx.json {
                print_json(
                    out,
                    &serde_json::json!({
                        "schema_version": 1, "status": "restored", "profile": cx.profile, "report": r,
                    }),
                );
            } else {
                let _ = writeln!(
                    out,
                    "nvmod backup: restored `{}` into profile `{}` ({} file(s))",
                    r.restored, cx.profile, r.files
                );
                let _ = writeln!(
                    out,
                    "the state before is the snapshot `{}`; restore it to undo",
                    r.pre_restore
                );
            }
            OK
        }
        Err(e) => failure(&e, err),
    }
}

fn prune(cx: &Cx, config: &Config, dry_run: bool, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    match nvmod_backup::prune(cx.instance, cx.profile, config, cx.now, dry_run) {
        Ok(r) => {
            if cx.json {
                print_json(
                    out,
                    &serde_json::json!({
                        "schema_version": 1, "profile": cx.profile, "dry_run": r.dry_run,
                        "kept": r.kept, "removed": r.removed,
                    }),
                );
            } else {
                let what = if r.dry_run { "would remove" } else { "removed" };
                let _ = writeln!(
                    out,
                    "nvmod backup: {what} {} snapshot(s), kept {}",
                    r.removed.len(),
                    r.kept.len()
                );
            }
            OK
        }
        Err(e) => failure(&e, err),
    }
}

/// Whether `args` is a command that changes a list of a profile: `install`, `mods enable|disable|reorder`,
/// `plugins enable|disable|move|lock|unlock` and `sort --apply`, none of them as a dry run.
fn changes_lists(args: &[String]) -> bool {
    if args.iter().any(|a| a == "--dry-run") {
        return false;
    }
    match args.first().map(String::as_str) {
        Some("install") => true,
        Some("mods") => matches!(
            args.get(1).map(String::as_str),
            Some("enable" | "disable" | "reorder")
        ),
        Some("plugins") => matches!(
            args.get(1).map(String::as_str),
            Some("enable" | "disable" | "move" | "lock" | "unlock")
        ),
        Some("sort") => args.iter().any(|a| a == "--apply" || a == "--keep-order"),
        Some("profile") => matches!(args.get(1).map(String::as_str), Some("apply")),
        _ => false,
    }
}

fn value_of<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// Run `command`, after an automatic snapshot of the profile when `args` is a writing command and the newest
/// snapshot is older than the configured interval (default one hour). A snapshot that cannot be taken is a
/// warning, not a refusal. When the command fails it leaves the Instance as it was, the automatic snapshot
/// included.
pub(crate) fn around(
    args: &[String],
    err: &mut dyn Write,
    command: impl FnOnce(&mut dyn Write) -> u8,
) -> u8 {
    if !changes_lists(args) {
        return command(err);
    }
    let Some(root) = value_of(args, "--instance") else {
        return command(err);
    };
    let Ok(instance) = NvmodInstance::open_read_only(Path::new(root)) else {
        return command(err);
    };
    let Ok(profile) = instance.resolve_profile(value_of(args, "--profile")) else {
        return command(err);
    };
    let config = nvmod_backup::load_config(&instance);
    let before = nvmod_backup::presence(&instance);
    let taken = match nvmod_backup::auto(&instance, &profile, clock::now(), &config) {
        Ok(taken) => taken,
        Err(Error::Refused(why) | Error::Corrupt(why) | Error::Failed(why)) => {
            let _ = writeln!(
                err,
                "nvmod: warning: no automatic snapshot of profile `{profile}`: {why}"
            );
            None
        }
    };
    let code = command(err);
    if code != OK
        && let Some(snapshot) = taken
    {
        nvmod_backup::discard(&instance, &profile, &snapshot.id, before);
    }
    code
}
