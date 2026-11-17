// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod saves list|backup|restore|transfer|check|checksum`: saves are never damaged (see `nvmod-saves`).

use crate::{FOUND, OK, REFUSED, USAGE, args::Args};
use std::io::Write;
use std::path::PathBuf;

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

struct Cx {
    json: bool,
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
    if sub == "list" {
        return list(&args, out, err);
    }
    if sub == "check" {
        return check(&args, out, err);
    }
    if sub == "checksum" {
        return checksum(&args, out, err);
    }
    let cx = Cx {
        json: args.flag("json"),
    };
    if sub == "transfer" {
        return transfer(&cx, &args, out, err);
    }
    match sub {
        "backup" => backup(&cx, &args, out, err),
        _ => restore(&cx, &args, out, err),
    }
}

/// `saves list`, through the library API.
fn list(args: &Args, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let request = nvmod_api::saves::ListRequest {
        instance: PathBuf::from(args.option("instance").unwrap_or_default()),
        profile: args.option("profile").map(str::to_owned),
        prefix: args.option("prefix").map(PathBuf::from),
    };
    let listed = match nvmod_api::saves::list(
        &request,
        &mut nvmod_api::NoProgress,
        &nvmod_api::CancelToken::new(),
    ) {
        Ok(nvmod_api::Outcome::Done(listed)) => listed,
        Ok(nvmod_api::Outcome::Cancelled) => return REFUSED,
        Err(e) => return crate::job_failure("saves", &e, err),
    };
    let (profile, saves) = (&listed.profile, &listed.saves);
    if args.flag("json") {
        print_json(
            out,
            &serde_json::json!({
                "schema_version": 1, "profile": profile, "dir": listed.dir.display().to_string(), "saves": saves,
            }),
        );
    } else {
        let _ = writeln!(
            out,
            "nvmod saves: profile `{profile}`, {} save(s) in {}",
            saves.len(),
            listed.dir.display()
        );
        for s in saves {
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

fn check(args: &Args, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let request = nvmod_api::saves::CheckRequest {
        instance: PathBuf::from(args.option("instance").unwrap_or_default()),
        profile: args.option("profile").map(str::to_owned),
        prefix: args.option("prefix").map(PathBuf::from),
    };
    let checked = match nvmod_api::saves::check(
        &request,
        &mut nvmod_api::NoProgress,
        &nvmod_api::CancelToken::new(),
    ) {
        Ok(nvmod_api::Outcome::Done(checked)) => checked,
        Ok(nvmod_api::Outcome::Cancelled) => return REFUSED,
        Err(e) => return crate::job_failure("saves", &e, err),
    };
    let profile = &checked.profile;
    let report = &checked.report;
    if args.flag("json") {
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

fn checksum(args: &Args, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let request = nvmod_api::saves::ChecksumRequest {
        instance: PathBuf::from(args.option("instance").unwrap_or_default()),
        profile: args.option("profile").map(str::to_owned),
        prefix: args.option("prefix").map(PathBuf::from),
    };
    let summed = match nvmod_api::saves::checksum(
        &request,
        &mut nvmod_api::NoProgress,
        &nvmod_api::CancelToken::new(),
    ) {
        Ok(nvmod_api::Outcome::Done(summed)) => summed,
        Ok(nvmod_api::Outcome::Cancelled) => return REFUSED,
        Err(e) => return crate::job_failure("saves", &e, err),
    };
    let profile = &summed.profile;
    let dir = &summed.dir;
    let sums = &summed.files;
    if args.flag("json") {
        let files: Vec<serde_json::Value> = sums
            .iter()
            .map(|(name, (size, sha))| serde_json::json!({ "name": name, "size": size, "sha256": sha }))
            .collect();
        print_json(
            out,
            &serde_json::json!({
                "schema_version": 1, "profile": profile, "dir": dir.display().to_string(), "files": files,
            }),
        );
    } else {
        for (name, (size, sha)) in sums {
            let _ = writeln!(out, "{sha}  {size:>10}  {name}");
        }
    }
    OK
}

fn backup(cx: &Cx, args: &Args, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let request = nvmod_api::saves::BackupRequest {
        instance: PathBuf::from(args.option("instance").unwrap_or_default()),
        profile: args.option("profile").map(str::to_owned),
        prefix: args.option("prefix").map(PathBuf::from),
    };
    let backed = match nvmod_api::saves::backup(
        &request,
        &mut nvmod_api::NoProgress,
        &nvmod_api::CancelToken::new(),
    ) {
        Ok(nvmod_api::Outcome::Done(backed)) => backed,
        Ok(nvmod_api::Outcome::Cancelled) => return REFUSED,
        Err(e) => return crate::job_failure("saves", &e, err),
    };
    let profile = &backed.profile;
    let r = &backed.backup;
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

fn restore(cx: &Cx, args: &Args, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let id = args.positional[1].clone();
    let request = nvmod_api::saves::RestoreRequest {
        instance: PathBuf::from(args.option("instance").unwrap_or_default()),
        profile: args.option("profile").map(str::to_owned),
        prefix: args.option("prefix").map(PathBuf::from),
        id,
    };
    let restored = match nvmod_api::saves::restore(
        &request,
        &mut nvmod_api::NoProgress,
        &nvmod_api::CancelToken::new(),
    ) {
        Ok(nvmod_api::Outcome::Done(restored)) => restored,
        Ok(nvmod_api::Outcome::Cancelled) => return REFUSED,
        Err(e) => return crate::job_failure("saves", &e, err),
    };
    let profile = &restored.profile;
    let r = &restored.report;
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

fn transfer(cx: &Cx, args: &Args, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let request = nvmod_api::saves::TransferRequest {
        instance: PathBuf::from(args.option("instance").unwrap_or_default()),
        from: args.positional[1].clone(),
        to: args.positional[2].clone(),
        prefix: args.option("prefix").map(PathBuf::from),
        force: args.flag("force"),
    };
    let transferred = match nvmod_api::saves::transfer(
        &request,
        &mut nvmod_api::NoProgress,
        &nvmod_api::CancelToken::new(),
    ) {
        Ok(nvmod_api::Outcome::Done(transferred)) => transferred,
        Ok(nvmod_api::Outcome::Cancelled) => return REFUSED,
        Err(e) => return crate::job_failure("saves", &e, err),
    };
    let r = &transferred.report;
    let (from, to) = (args.positional[1].as_str(), args.positional[2].as_str());
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
