// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod downloads list|show|install|hide|unhide|forget`: wrappers over the jobs of `nvmod_api::downloads`.

use crate::{FOUND, OK, REFUSED, USAGE, args::Args};
use nvmod_api::downloads::{
    Action, Archive, ChangeRequest, InstallRequest, ListRequest, MatchedBy, ShowRequest, State,
    downloads_dir,
};
use nvmod_api::install::{InstallOutput as Installed, Question, Report};
use nvmod_api::{CancelToken, NoProgress, Outcome};
use std::io::Write;
use std::path::{Path, PathBuf};

const USAGE_TEXT: &str = "usage:
  nvmod downloads list --instance <dir> [--downloads <dir>] [--unmatched] [--hash] [--json]
  nvmod downloads show <archive> --instance <dir> [--downloads <dir>] [--hash] [--json]
  nvmod downloads install <archive> --instance <dir> [--downloads <dir>] [--profile <name>] [--name <mod name>]
                [--dry-run] [--answers <file>] [--max-extract-bytes <n>] [--max-extract-entries <n>]
                [--max-extract-entry-bytes <n>] [--max-extract-ratio <n>] [--max-extract-depth <n>]
                [--max-extract-operations <n>] [--max-extract-seconds <n>] [--json]
  nvmod downloads hide|unhide|forget <archive> --instance <dir> [--downloads <dir>] [--dry-run] [--json]";

fn print_json(out: &mut dyn Write, value: &impl serde::Serialize) {
    let _ = writeln!(
        out,
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

fn state_word(state: State) -> &'static str {
    match state {
        State::Installed => "installed",
        State::Uninstalled => "uninstalled",
        State::Hidden => "hidden",
        State::Paused => "paused",
    }
}

fn archive_line(a: &Archive) -> String {
    let id = a
        .nexus_mod_id
        .map_or_else(|| "-".to_owned(), |id| id.to_string());
    let matched = a.matched_mod.as_ref().map_or_else(String::new, |m| {
        let how = match m.by {
            MatchedBy::InstallationFile => "",
            MatchedBy::ModId => " (same Nexus mod)",
        };
        format!("  -> {}{how}", m.name)
    });
    format!("{:<11} {:>8}  {}{matched}", state_word(a.state), id, a.file)
}

/// The file name of `word`: itself when it is a name, else the name of a file that sits in the downloads folder.
fn archive_name(word: &str, instance: &Path, downloads: Option<&Path>) -> Result<String, String> {
    let path = Path::new(word);
    if path.parent().is_none_or(|p| p.as_os_str().is_empty()) {
        return Ok(word.to_owned());
    }
    let (dir, _) = downloads_dir(instance, downloads);
    let same = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    };
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) if same(parent, &dir) => Ok(name.to_string_lossy().into_owned()),
        _ => Err(format!(
            "{word} is not in the downloads folder {}; give the file name of a download",
            dir.display()
        )),
    }
}

struct Common {
    args: Args,
    instance: PathBuf,
    downloads: Option<PathBuf>,
}

fn parse(
    verb: &str,
    args: &[String],
    flags: &[&str],
    options: &[&str],
    positional: usize,
    err: &mut dyn Write,
) -> Result<Common, u8> {
    let mut all_options = vec!["instance", "downloads"];
    all_options.extend_from_slice(options);
    match Args::parse(args, flags, &all_options) {
        Ok(a) if a.positional.len() == positional && a.option("instance").is_some() => Ok(Common {
            instance: PathBuf::from(a.option("instance").unwrap_or_default()),
            downloads: a.option("downloads").map(PathBuf::from),
            args: a,
        }),
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_TEXT}");
            Err(USAGE)
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod downloads {verb}: {why}");
            Err(USAGE)
        }
    }
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let rest = args.get(1..).unwrap_or_default();
    match args.first().map(String::as_str) {
        Some("list") => run_list(rest, out, err),
        Some("show") => run_show(rest, out, err),
        Some("install") => run_install(rest, out, err),
        Some(verb @ ("hide" | "unhide" | "forget")) => run_change(verb, rest, out, err),
        _ => {
            let _ = writeln!(err, "{USAGE_TEXT}");
            USAGE
        }
    }
}

fn run_list(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let c = match parse("list", args, &["json", "unmatched", "hash"], &[], 0, err) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let request = ListRequest {
        instance: c.instance,
        downloads: c.downloads,
        unmatched: c.args.flag("unmatched"),
        hash: c.args.flag("hash"),
    };
    match nvmod_api::downloads::list(&request, &mut NoProgress, &CancelToken::new()) {
        Ok(Outcome::Done(report)) => {
            if c.args.flag("json") {
                print_json(out, &report);
            } else {
                let _ = writeln!(
                    out,
                    "{} downloads in {}",
                    report.archives.len(),
                    report.downloads_dir
                );
                for a in &report.archives {
                    let _ = writeln!(out, "{}", archive_line(a));
                    if let (Some(md5), Some(sha)) = (&a.md5, &a.sha256) {
                        let _ = writeln!(out, "              md5 {md5}  sha256 {sha}");
                    }
                }
                if report.unmatched_only {
                    let _ = writeln!(
                        out,
                        "{} Mods without an archive in the folder",
                        report.mods_without_archive.len()
                    );
                    for m in &report.mods_without_archive {
                        let _ = writeln!(
                            out,
                            "  {}  ({})",
                            m.name,
                            m.installation_file
                                .as_deref()
                                .unwrap_or("no installationFile")
                        );
                    }
                }
            }
            OK
        }
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => crate::job_failure("downloads list", &e, err),
    }
}

fn run_show(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let c = match parse("show", args, &["json", "hash"], &[], 1, err) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let archive = match archive_name(&c.args.positional[0], &c.instance, c.downloads.as_deref()) {
        Ok(name) => name,
        Err(why) => {
            let _ = writeln!(err, "nvmod downloads show: refused: {why}");
            return REFUSED;
        }
    };
    let request = ShowRequest {
        instance: c.instance,
        downloads: c.downloads,
        archive,
        hash: c.args.flag("hash"),
    };
    match nvmod_api::downloads::show(&request, &mut NoProgress, &CancelToken::new()) {
        Ok(Outcome::Done(report)) => {
            if c.args.flag("json") {
                print_json(out, &report);
            } else {
                let a = &report.archive;
                let _ = writeln!(out, "{}", archive_line(a));
                let line = |out: &mut dyn Write, key: &str, value: Option<String>| {
                    if let Some(v) = value {
                        let _ = writeln!(out, "  {key}: {v}");
                    }
                };
                line(out, "size", Some(a.size.to_string()));
                line(
                    out,
                    "meta file",
                    Some(
                        if a.has_meta {
                            "yes"
                        } else {
                            "no (provenance unknown)"
                        }
                        .to_owned(),
                    ),
                );
                line(out, "game", a.game_name.clone());
                line(out, "nexus mod id", a.nexus_mod_id.map(|n| n.to_string()));
                line(out, "nexus file id", a.nexus_file_id.map(|n| n.to_string()));
                line(out, "name", a.name.clone());
                line(out, "mod name", a.mod_name.clone());
                line(out, "version", a.version.clone());
                line(out, "newest version", a.newest_version.clone());
                line(out, "md5", a.md5.clone());
                line(out, "sha256", a.sha256.clone());
            }
            OK
        }
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => crate::job_failure("downloads show", &e, err),
    }
}

fn print_install(out: &mut dyn Write, report: &Report, json_meta: Option<&serde_json::Value>) {
    let verb = if report.dry_run {
        "would install"
    } else {
        "installed"
    };
    let _ = writeln!(
        out,
        "nvmod downloads install: {verb} `{}` into profile `{}` ({} files)",
        report.r#mod,
        report.profile,
        report.files.len()
    );
    if report.dry_run {
        for f in &report.files {
            let _ = writeln!(out, "  + {f}");
        }
    }
    for p in &report.plugins {
        let _ = writeln!(out, "  activates plugin {p}");
    }
    if let Some(meta) = json_meta {
        if let Some(why) = meta.get("warning").and_then(serde_json::Value::as_str) {
            let _ = writeln!(out, "  warning: the .meta file was not updated: {why}");
        } else {
            let _ = writeln!(out, "  marked installed in the .meta file");
        }
    }
}

fn print_questions(out: &mut dyn Write, questions: &[Question]) {
    let _ = writeln!(
        out,
        "nvmod downloads install: the FOMOD needs answers (nothing was changed); give them with --answers <file>:"
    );
    for q in questions {
        let _ = writeln!(
            out,
            "  {} ({}, choose {} to {})",
            q.id,
            q.kind.as_str(),
            q.min,
            q.max
        );
    }
}

fn run_install(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let c = match parse(
        "install",
        args,
        &["json", "dry-run"],
        &[
            "profile",
            "name",
            "answers",
            "max-extract-bytes",
            "max-extract-entries",
            "max-extract-entry-bytes",
            "max-extract-ratio",
            "max-extract-depth",
            "max-extract-operations",
            "max-extract-seconds",
        ],
        1,
        err,
    ) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let extract_limits = match crate::install_mod::extract_limits(&c.args) {
        Ok(limits) => limits,
        Err(flag) => {
            let _ = writeln!(err, "nvmod downloads install: --{flag} wants a number");
            return USAGE;
        }
    };
    let archive = match archive_name(&c.args.positional[0], &c.instance, c.downloads.as_deref()) {
        Ok(name) => name,
        Err(why) => {
            let _ = writeln!(err, "nvmod downloads install: refused: {why}");
            return REFUSED;
        }
    };
    let request = InstallRequest {
        instance: c.instance,
        downloads: c.downloads,
        archive,
        profile: c.args.option("profile").map(str::to_owned),
        name: c.args.option("name").map(str::to_owned),
        dry_run: c.args.flag("dry-run"),
        answers: c.args.option("answers").map(PathBuf::from),
        placement: nvmod_api::install::Placement::Top,
        max_extract_bytes: extract_limits.max_bytes,
        extract_limits,
    };
    match nvmod_api::downloads::install(&request, &mut NoProgress, &CancelToken::new()) {
        Ok(Outcome::Done(output)) => {
            let json = c.args.flag("json");
            match &output.install {
                Installed::Installed(report) => {
                    if json {
                        print_json(
                            out,
                            &serde_json::json!({ "install": report, "meta": output.meta }),
                        );
                    } else {
                        let meta = serde_json::to_value(&output.meta).ok();
                        print_install(out, report, meta.as_ref().filter(|m| !m.is_null()));
                    }
                    OK
                }
                Installed::Questions(questions) => {
                    if json {
                        print_json(
                            out,
                            &serde_json::json!({ "status": "questions", "questions": questions }),
                        );
                    } else {
                        print_questions(out, questions);
                    }
                    FOUND
                }
            }
        }
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => crate::job_failure("downloads install", &e, err),
    }
}

fn run_change(verb: &str, args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let c = match parse(verb, args, &["json", "dry-run"], &[], 1, err) {
        Ok(c) => c,
        Err(code) => return code,
    };
    let name = format!("downloads {verb}");
    let archive = match archive_name(&c.args.positional[0], &c.instance, c.downloads.as_deref()) {
        Ok(name) => name,
        Err(why) => {
            let _ = writeln!(err, "nvmod {name}: refused: {why}");
            return REFUSED;
        }
    };
    let action = match verb {
        "hide" => Action::Hide,
        "unhide" => Action::Unhide,
        _ => Action::Forget,
    };
    let request = ChangeRequest {
        instance: c.instance,
        downloads: c.downloads,
        archive,
        action,
        dry_run: c.args.flag("dry-run"),
    };
    match nvmod_api::downloads::change(&request, &mut NoProgress, &CancelToken::new()) {
        Ok(Outcome::Done(output)) => {
            if c.args.flag("json") {
                print_json(out, &output);
            } else {
                let what = match (action, output.changed, output.dry_run) {
                    (Action::Hide, true, false) => "hid",
                    (Action::Hide, true, true) => "would hide",
                    (Action::Unhide, true, false) => "showed",
                    (Action::Unhide, true, true) => "would show",
                    (Action::Forget, _, false) => "moved to the trash:",
                    (Action::Forget, _, true) => "would move to the trash:",
                    (Action::Hide, false, _) => "already hidden:",
                    (Action::Unhide, false, _) => "was not hidden:",
                };
                let _ = writeln!(out, "nvmod {name}: {what} {}", output.archive);
                if let Some(to) = &output.trashed_to {
                    let _ = writeln!(out, "  now in {to}");
                }
            }
            OK
        }
        Ok(Outcome::Cancelled) => REFUSED,
        Err(e) => crate::job_failure(&name, &e, err),
    }
}
