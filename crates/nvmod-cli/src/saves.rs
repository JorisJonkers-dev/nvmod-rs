// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod saves list|backup|restore|transfer|check|checksum`: saves are never damaged (see `nvmod-saves`).

use crate::{FOUND, OK, REFUSED, ROLLED_BACK, USAGE, VERIFICATION, args::Args};
use nvmod_backup::clock;
use nvmod_instance::NvmodInstance;
use nvmod_profile::Target;
use nvmod_saves::{Error, Location};
use std::io::Write;
use std::path::{Path, PathBuf};

const USAGE_TEXT: &str = "usage: nvmod saves list|backup|check|checksum --instance <dir> [--profile <name>] [--prefix <dir>] [--json]
       nvmod saves restore <id|latest> --instance <dir> [--profile <name>] [--prefix <dir>] [--json]
       nvmod saves transfer <from> <to> --instance <dir> [--prefix <dir>] [--force] [--json]";

fn print_json(out: &mut dyn Write, value: &serde_json::Value) {
    let _ = writeln!(
        out,
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

fn failure(error: &Error, err: &mut dyn Write) -> u8 {
    match error {
        Error::Refused(why) => {
            let _ = writeln!(err, "nvmod saves: refused: {why}");
            REFUSED
        }
        Error::Corrupt(why) => {
            let _ = writeln!(
                err,
                "nvmod saves: verification failed: {why}; nothing was changed"
            );
            VERIFICATION
        }
        Error::Failed(why) => {
            let _ = writeln!(err, "nvmod saves: {why}");
            ROLLED_BACK
        }
    }
}

struct Cx<'a> {
    instance: &'a NvmodInstance,
    target: Result<Target, String>,
    json: bool,
}

impl Cx<'_> {
    fn locate(&self, profile: &str) -> Result<Location, Error> {
        nvmod_saves::locate(self.instance, profile, self.target.as_ref().ok()).map_err(|e| {
            match (e, &self.target) {
                (Error::Refused(why), Err(prefix)) => Error::Refused(format!("{why}; {prefix}")),
                (e, _) => e,
            }
        })
    }
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(args, &["json", "force"], &["instance", "profile", "prefix"]) {
        Ok(a) if a.option("instance").is_some() && !a.positional.is_empty() => a,
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_TEXT}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod saves: {why}\n{USAGE_TEXT}");
            return USAGE;
        }
    };
    let sub = args.positional[0].as_str();
    let shape_ok = match sub {
        "list" | "backup" | "check" | "checksum" => args.positional.len() == 1,
        "restore" => args.positional.len() == 2,
        "transfer" => args.positional.len() == 3,
        _ => false,
    };
    if !shape_ok || (args.flag("force") && sub != "transfer") {
        let _ = writeln!(err, "{USAGE_TEXT}");
        return USAGE;
    }
    let root = Path::new(args.option("instance").unwrap_or_default());
    let opened = if matches!(sub, "backup" | "restore" | "transfer") {
        NvmodInstance::open(root)
    } else {
        NvmodInstance::open_read_only(root)
    };
    let instance = match opened {
        Ok(i) => i,
        Err(e) => {
            let _ = writeln!(
                err,
                "nvmod saves: refused: {}",
                crate::install_mod::describe_open_error(root, &e)
            );
            return REFUSED;
        }
    };
    let prefix: PathBuf = args
        .option("prefix")
        .map_or_else(|| instance.root.join("compatdata"), PathBuf::from);
    let cx = Cx {
        instance: &instance,
        target: Target::from_prefix(&prefix),
        json: args.flag("json"),
    };
    if sub == "transfer" {
        return transfer(&cx, &args, out, err);
    }
    let profile = match instance.resolve_profile(args.option("profile")) {
        Ok(p) => p,
        Err(e) => {
            let _ = writeln!(err, "nvmod saves: refused: {e}");
            return REFUSED;
        }
    };
    let location = match cx.locate(&profile) {
        Ok(l) => l,
        Err(e) => return failure(&e, err),
    };
    match sub {
        "list" => list(&cx, &profile, &location, out),
        "check" => check(&cx, &profile, &location, out, err),
        "checksum" => checksum(&cx, &profile, &location, out),
        "backup" => backup(&cx, &profile, &location, out, err),
        _ => restore(&cx, &profile, &location, &args.positional[1], out, err),
    }
}

fn list(cx: &Cx, profile: &str, location: &Location, out: &mut dyn Write) -> u8 {
    let saves = nvmod_saves::list(&location.dir);
    if cx.json {
        print_json(
            out,
            &serde_json::json!({
                "schema_version": 1, "profile": profile, "dir": location.dir.display().to_string(), "saves": saves,
            }),
        );
    } else {
        let _ = writeln!(
            out,
            "nvmod saves: profile `{profile}`, {} save(s) in {}",
            saves.len(),
            location.dir.display()
        );
        for s in &saves {
            let detail = s.problem.as_ref().map_or_else(
                || format!("{} plugin(s)", s.plugins.unwrap_or(0)),
                |p| format!("unreadable: {p}"),
            );
            let cosave = if s.cosave { "" } else { ", no co-save" };
            let _ = writeln!(out, "  {}  {} byte(s), {detail}{cosave}", s.name, s.size);
        }
    }
    OK
}

fn check(
    cx: &Cx,
    profile: &str,
    location: &Location,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    match nvmod_saves::check(cx.instance, profile, location) {
        Ok(report) => {
            if cx.json {
                print_json(
                    out,
                    &serde_json::json!({ "schema_version": 1, "report": report }),
                );
            } else {
                let _ = writeln!(
                    out,
                    "nvmod saves: profile `{profile}`, {} save(s) checked against {} active plugin(s)",
                    report.saves.len(),
                    report.active
                );
                for s in report.saves.iter().filter(|s| s.status != "ok") {
                    match s.status {
                        "unreadable" => {
                            let _ = writeln!(
                                out,
                                "  {}: unreadable: {}",
                                s.name,
                                s.problem.as_deref().unwrap_or("")
                            );
                        }
                        _ => {
                            let _ = writeln!(
                                out,
                                "  {}: needs plugins the profile has not active: {}",
                                s.name,
                                s.missing.join(", ")
                            );
                        }
                    }
                }
            }
            if report.clean { OK } else { FOUND }
        }
        Err(e) => failure(&e, err),
    }
}

fn checksum(cx: &Cx, profile: &str, location: &Location, out: &mut dyn Write) -> u8 {
    let sums = nvmod_saves::checksums(&location.dir);
    if cx.json {
        let files: Vec<serde_json::Value> = sums
            .iter()
            .map(|(name, (size, sha))| serde_json::json!({ "name": name, "size": size, "sha256": sha }))
            .collect();
        print_json(
            out,
            &serde_json::json!({
                "schema_version": 1, "profile": profile, "dir": location.dir.display().to_string(), "files": files,
            }),
        );
    } else {
        for (name, (size, sha)) in &sums {
            let _ = writeln!(out, "{sha}  {size:>10}  {name}");
        }
    }
    OK
}

fn backup(
    cx: &Cx,
    profile: &str,
    location: &Location,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    match nvmod_saves::backup(cx.instance, profile, location, clock::now()) {
        Ok(r) => {
            if cx.json {
                print_json(
                    out,
                    &serde_json::json!({ "schema_version": 1, "status": "backed-up", "profile": profile, "backup": r }),
                );
            } else {
                let _ = writeln!(
                    out,
                    "nvmod saves: backup `{}` of profile `{profile}` ({} file(s), {} byte(s))",
                    r.id, r.files, r.bytes
                );
            }
            OK
        }
        Err(e) => failure(&e, err),
    }
}

fn restore(
    cx: &Cx,
    profile: &str,
    location: &Location,
    id: &str,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    match nvmod_saves::restore(cx.instance, profile, location, id, clock::now()) {
        Ok(r) => {
            if cx.json {
                print_json(
                    out,
                    &serde_json::json!({ "schema_version": 1, "status": "restored", "profile": profile, "report": r }),
                );
            } else {
                let _ = writeln!(
                    out,
                    "nvmod saves: restored `{}` into profile `{profile}` ({} file(s))",
                    r.restored, r.files
                );
                if let Some(pre) = &r.pre_restore {
                    let _ = writeln!(out, "the folder as it was is the backup `{pre}`");
                }
                if !r.kept_extra.is_empty() {
                    let _ = writeln!(
                        out,
                        "left alone (not in the backup): {}",
                        r.kept_extra.join(", ")
                    );
                }
            }
            OK
        }
        Err(e) => failure(&e, err),
    }
}

fn transfer(cx: &Cx, args: &Args, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let (from, to) = (args.positional[1].as_str(), args.positional[2].as_str());
    let both = (|| -> Result<(Location, Location), Error> {
        for name in [from, to] {
            if let Err(e) = cx.instance.resolve_profile(Some(name)) {
                return Err(Error::Refused(e.to_string()));
            }
        }
        Ok((cx.locate(from)?, cx.locate(to)?))
    })();
    let (a, b) = match both {
        Ok(pair) => pair,
        Err(e) => return failure(&e, err),
    };
    match nvmod_saves::transfer((from, &a), (to, &b), args.flag("force")) {
        Ok(r) => {
            if cx.json {
                print_json(
                    out,
                    &serde_json::json!({ "schema_version": 1, "status": "transferred", "report": r }),
                );
            } else {
                let _ = writeln!(
                    out,
                    "nvmod saves: copied {} file(s) from `{from}` to `{to}` ({} already there)",
                    r.copied.len(),
                    r.already_there.len()
                );
            }
            OK
        }
        Err(e) => failure(&e, err),
    }
}
