// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod exec list|add|edit|remove|import`: the executables registry of an Instance (nv-linux #148).

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, args::Args};
use nvmod_core::Executable;
use nvmod_instance::NvmodInstance;
use nvmod_run::Patch;
use std::io::Write;
use std::path::Path;

const USAGE_LINE: &str = "usage: nvmod exec list --instance <dir> [--json]\n       nvmod exec add --instance <dir> --title <t> --binary <path> [--arguments <a>] [--working-dir <dir>] [--steam-app-id <n>] [--hidden]\n       nvmod exec edit <title> --instance <dir> [--new-title <t>] [--binary <path>] [--arguments <a>] [--working-dir <dir>] [--steam-app-id <n>] [--hidden | --visible]\n       nvmod exec remove <title> --instance <dir>\n       nvmod exec import --mo2 <dir> --instance <dir> [--json]";

fn text_of(e: &Executable) -> String {
    use std::fmt::Write as _;
    let mut s = format!(
        "{}{}: {}",
        e.title,
        if e.hidden { " (hidden)" } else { "" },
        e.binary
    );
    if !e.arguments.is_empty() {
        let _ = write!(s, " {}", e.arguments);
    }
    if let Some(w) = &e.working_directory {
        let _ = write!(s, "  [in {w}]");
    }
    if let Some(id) = e.steam_app_id {
        let _ = write!(s, "  [app id {id}]");
    }
    s
}

type Change = (Option<String>, Vec<nvmod_core::Finding>);

/// Apply the subcommand to `list`; `Err` is the exit code after the message was written.
fn change(
    sub: &str,
    a: &Args,
    app_id: Option<u32>,
    list: &mut Vec<Executable>,
    err: &mut dyn Write,
) -> Result<Change, u8> {
    let refuse = |why: &str, err: &mut dyn Write| {
        let _ = writeln!(err, "nvmod exec {sub}: refused: {why}");
        REFUSED
    };
    let mut findings = Vec::new();
    let mut changed = None;
    match (sub, a.positional.as_slice()) {
        ("list", []) => {}
        ("add", []) => {
            let (Some(title), Some(binary)) = (a.option("title"), a.option("binary")) else {
                let _ = writeln!(err, "{USAGE_LINE}");
                return Err(USAGE);
            };
            let entry = Executable {
                title: title.to_owned(),
                binary: binary.to_owned(),
                arguments: a.option("arguments").unwrap_or_default().to_owned(),
                working_directory: a
                    .option("working-dir")
                    .filter(|w| !w.is_empty())
                    .map(str::to_owned),
                steam_app_id: app_id.filter(|id| *id != 0),
                hidden: a.flag("hidden"),
            };
            if let Err(why) = nvmod_run::add(list, entry) {
                return Err(refuse(&why, err));
            }
            changed = Some(format!("added `{title}`"));
        }
        ("edit", [title]) => {
            let hidden = match (a.flag("hidden"), a.flag("visible")) {
                (true, true) => {
                    let _ = writeln!(
                        err,
                        "nvmod exec edit: --hidden and --visible exclude each other"
                    );
                    return Err(USAGE);
                }
                (true, false) => Some(true),
                (false, true) => Some(false),
                (false, false) => None,
            };
            let patch = Patch {
                title: a.option("new-title").map(str::to_owned),
                binary: a.option("binary").map(str::to_owned),
                arguments: a.option("arguments").map(str::to_owned),
                working_directory: a.option("working-dir").map(str::to_owned),
                steam_app_id: app_id,
                hidden,
            };
            if let Err(why) = nvmod_run::edit(list, title, &patch) {
                return Err(refuse(&why, err));
            }
            changed = Some(format!("edited `{title}`"));
        }
        ("remove", [title]) => {
            if let Err(why) = nvmod_run::remove(list, title) {
                return Err(refuse(&why, err));
            }
            changed = Some(format!("removed `{title}`"));
        }
        ("import", []) => {
            let Some(mo2) = a.option("mo2") else {
                let _ = writeln!(err, "{USAGE_LINE}");
                return Err(USAGE);
            };
            match nvmod_run::import_mo2(Path::new(mo2), list) {
                Ok(done) => {
                    findings = done.findings;
                    changed = Some(format!("imported {} entr(ies) from MO2", done.added.len()));
                }
                Err(why) => return Err(refuse(&why, err)),
            }
        }
        _ => {
            let _ = writeln!(err, "{USAGE_LINE}");
            return Err(USAGE);
        }
    }
    Ok((changed, findings))
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let Some(sub) = args.first().map(String::as_str) else {
        let _ = writeln!(err, "{USAGE_LINE}");
        return USAGE;
    };
    let parsed = Args::parse(
        &args[1..],
        &["json", "hidden", "visible"],
        &[
            "instance",
            "title",
            "new-title",
            "binary",
            "arguments",
            "working-dir",
            "steam-app-id",
            "mo2",
        ],
    );
    let a = match parsed {
        Ok(a) if a.option("instance").is_some() => a,
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_LINE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod exec: {why}\n{USAGE_LINE}");
            return USAGE;
        }
    };
    let root = Path::new(a.option("instance").unwrap_or_default());
    let mut instance = match NvmodInstance::open_read_only(root) {
        Ok(i) => i,
        Err(e) => {
            let _ = writeln!(
                err,
                "nvmod exec: refused: {}",
                crate::install_mod::describe_open_error(root, &e)
            );
            return REFUSED;
        }
    };
    let app_id = match a.option("steam-app-id").map(str::parse::<u32>) {
        Some(Err(_)) => {
            let _ = writeln!(err, "nvmod exec: --steam-app-id needs a number");
            return USAGE;
        }
        Some(Ok(id)) => Some(id),
        None => None,
    };
    let mut list = instance.config.executables.clone();
    let (changed, findings) = match change(sub, &a, app_id, &mut list, err) {
        Ok(done) => done,
        Err(code) => return code,
    };
    if changed.is_some()
        && list != instance.config.executables
        && let Err(e) = instance.set_executables(list.clone())
    {
        let _ = writeln!(err, "nvmod exec {sub}: {e}");
        return ROLLED_BACK;
    }
    if a.flag("json") {
        let doc = serde_json::json!({
            "schema_version": 1,
            "executables": list,
            "builtin": [nvmod_run::ENTRY_GAME, nvmod_run::ENTRY_NV_RS],
            "findings": findings,
        });
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&doc).unwrap_or_default()
        );
    } else {
        if let Some(what) = &changed {
            let _ = writeln!(out, "nvmod exec: {what}");
        }
        if sub == "list" {
            let _ = writeln!(out, "built in: game, nv-rs");
            for e in &list {
                let _ = writeln!(out, "{}", text_of(e));
            }
        }
        for f in &findings {
            let _ = writeln!(
                out,
                "  [{}] {}: {} ({})",
                f.severity, f.subject, f.message, f.code
            );
        }
    }
    OK
}
