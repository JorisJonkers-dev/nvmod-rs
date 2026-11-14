// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod setup`: a base copy to a modded, Doctor-clean Instance in one run. It orchestrates the commands that
//! already exist (`init` or `import-mo2`, `install-nv-rs`, `ttw`, `install`, `stage`, `sort --apply`, `doctor`)
//! through the command line seam, so no step has logic of its own here and each keeps its exit code.
//!
//! The run is all or nothing. It only starts on a path that holds no Instance, so the state before the run is
//! "nothing here"; a failing step removes what the run made and exits with the step's code (3 verification
//! failure, 4 step failed). `--json` prints one JSON object per line: a `plan`, the `step` events and a final
//! `finished` event, each with an increasing `seq`, so a wizard can show progress while the run goes.
//!
//! The last two steps matter for the Load order: `install` and `stage` lose the plugin file times, so the run
//! ends with `sort --apply`, which stamps them, and then the Doctor checks that they encode the order.

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, args::Args};
use nvmod_core::Severity;
use std::io::Write;
use std::path::Path;

const USAGE_LINE: &str = "usage: nvmod setup --base <dir> --instance <dir> --masterlist <file> [--import-mo2 <dir>] [--mod <archive>]... [--fallout3 <dir> [--package <file>] [--installer <path>]] [--release | --source <dir>] [--skip-nv-rs] [--dry-run] [--json]";
const FLAGS: &[&str] = &["json", "dry-run", "release", "skip-nv-rs", "copy"];
const OPTIONS: &[&str] = &[
    "base",
    "instance",
    "masterlist",
    "profile",
    "import-mo2",
    "mod",
    "answers",
    "extract-tool",
    "fallout3",
    "package",
    "installer",
    "lock",
    "store",
    "timeout-secs",
    "source",
    "cargo",
    "release-url",
    "asset",
    "repo",
    "gh",
    "curl",
    "retry-delay-ms",
    "catalog",
];

/// One step of the run: its name (the name in the events), what it does in words, and the command line it is.
struct Step {
    name: String,
    what: String,
    args: Vec<String>,
}

fn words(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

/// `--name value` pairs for the options of `args` that are present, to pass on to a step.
fn pass(args: &Args, names: &[&str]) -> Vec<String> {
    names
        .iter()
        .filter_map(|n| args.option(n).map(|v| [format!("--{n}"), v.to_owned()]))
        .flatten()
        .collect()
}

fn step(name: &str, what: &str, line: &[&str], more: Vec<String>) -> Step {
    let mut args = words(line);
    args.extend(more);
    Step {
        name: name.to_owned(),
        what: what.to_owned(),
        args,
    }
}

fn copy_flag(args: &Args) -> Vec<String> {
    if args.flag("copy") {
        words(&["--copy"])
    } else {
        Vec::new()
    }
}

/// The steps that make the Instance and put nv-rs in it.
fn plan_start(args: &Args) -> Vec<Step> {
    let get = |n: &str| args.option(n).unwrap_or_default().to_owned();
    let (base, instance) = (get("base"), get("instance"));
    let mut steps = Vec::new();
    if let Some(mo2) = args.option("import-mo2") {
        let mut more = pass(args, &["profile"]);
        more.extend(copy_flag(args));
        steps.push(step(
            "import-mo2",
            &format!("import the MO2 profile of {mo2} into a new Instance"),
            &[
                "import-mo2",
                "--mo2",
                mo2,
                "--instance",
                &instance,
                "--base",
                &base,
            ],
            more,
        ));
    } else {
        steps.push(step(
            "init",
            "make the Instance skeleton for the base copy",
            &["init", "--base", &base, "--instance", &instance],
            Vec::new(),
        ));
        let named = pass(args, &["profile"])
            .into_iter()
            .map(|w| {
                if w == "--profile" {
                    "--name".to_owned()
                } else {
                    w
                }
            })
            .collect();
        steps.push(step(
            "profile",
            "make the first profile",
            &["profile", "create", "--instance", &instance],
            named,
        ));
    }
    if !args.flag("skip-nv-rs") {
        let mut more = pass(
            args,
            &[
                "source",
                "cargo",
                "release-url",
                "asset",
                "repo",
                "gh",
                "curl",
                "retry-delay-ms",
            ],
        );
        if args.flag("release") {
            more.push("--release".to_owned());
        }
        steps.push(step(
            "nv-rs",
            "build or fetch nv-rs and run `nv-viewer <data folder> --check`",
            &["install-nv-rs", "--instance", &instance],
            more,
        ));
        steps.push(step(
            "target",
            "make nv-rs the target of the Instance",
            &["target", "nv-rs", "--instance", &instance],
            Vec::new(),
        ));
    }
    steps
}

/// TTW, the Mods, and the steps that finish the Load order and check the result.
fn plan_finish(args: &Args) -> Vec<Step> {
    let get = |n: &str| args.option(n).unwrap_or_default().to_owned();
    let (instance, masterlist) = (get("instance"), get("masterlist"));
    let mut steps = Vec::new();
    if let Some(fo3) = args.option("fallout3") {
        steps.push(step(
            "ttw",
            "install Tale of Two Wastelands through the pinned installer",
            &["ttw", "--instance", &instance, "--fallout3", fo3],
            pass(
                args,
                &[
                    "package",
                    "installer",
                    "lock",
                    "store",
                    "timeout-secs",
                    "profile",
                ],
            ),
        ));
    }
    for archive in args.option_all("mod") {
        let file = Path::new(archive)
            .file_name()
            .map_or_else(|| archive.to_owned(), |n| n.to_string_lossy().into_owned());
        steps.push(step(
            &format!("mod:{file}"),
            &format!("install the Mod {file}"),
            &["install", archive, "--instance", &instance],
            pass(args, &["profile", "answers", "extract-tool"]),
        ));
    }
    let mut more = pass(args, &["profile"]);
    more.extend(copy_flag(args));
    steps.push(step(
        "stage",
        "build the Staging tree",
        &["stage", "--instance", &instance],
        more,
    ));
    steps.push(step(
        "sort",
        "compute the Load order with loot-rs, write it and stamp the plugin file times",
        &[
            "sort",
            "--instance",
            &instance,
            "--masterlist",
            &masterlist,
            "--apply",
        ],
        pass(args, &["profile"]),
    ));
    steps.push(step(
        "verify",
        "run the Doctor on the finished Instance and print the launch plan",
        &[
            "verify",
            "--instance",
            &instance,
            "--masterlist",
            &masterlist,
        ],
        pass(args, &["profile", "catalog"]),
    ));
    steps
}

fn plan(args: &Args) -> Vec<Step> {
    let mut steps = plan_start(args);
    steps.extend(plan_finish(args));
    steps
}

/// Writes the events, as text or as JSON lines.
struct Events<'a> {
    out: &'a mut dyn Write,
    json: bool,
    seq: u64,
}

impl Events<'_> {
    fn emit(&mut self, mut value: serde_json::Value, text: &str) {
        self.seq += 1;
        if self.json {
            value["seq"] = self.seq.into();
            let _ = writeln!(self.out, "{value}");
        } else if !text.is_empty() {
            let _ = writeln!(self.out, "{text}");
        }
    }

    fn step(
        &mut self,
        step: &Step,
        index: usize,
        total: usize,
        status: &str,
        extra: &serde_json::Value,
        text: &str,
    ) {
        let mut value = serde_json::json!({
            "event": "step", "step": step.name, "status": status,
            "index": index + 1, "total": total, "description": step.what,
        });
        if let (Some(o), Some(e)) = (value.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        self.emit(value, text);
    }
}

fn refuse(args: &Args, why: &str, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let _ = writeln!(err, "nvmod setup: refused: {why}");
    if args.flag("json") {
        let _ = writeln!(
            out,
            "{}",
            serde_json::json!({ "seq": 1, "event": "finished", "status": "refused", "exit_code": REFUSED, "rolled_back": false, "reason": why })
        );
    }
    REFUSED
}

/// Everything that can be checked before anything is written. Returns the reason for a refusal.
fn preflight(args: &Args) -> Result<(), String> {
    let instance = Path::new(args.option("instance").unwrap_or_default());
    if instance.exists()
        && (!instance.is_dir() || std::fs::read_dir(instance).is_ok_and(|mut d| d.next().is_some()))
    {
        return Err(format!(
            "{} already exists and is not an empty folder; setup only starts from a path that holds nothing",
            instance.display()
        ));
    }
    let base = Path::new(args.option("base").unwrap_or_default());
    let report = nvmod_doctor::check_base(base)?;
    let errors: Vec<String> = report
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| format!("  {}: {} ({})", f.subject, f.message, f.code))
        .collect();
    if !errors.is_empty() {
        return Err(format!(
            "the base copy does not verify:\n{}",
            errors.join("\n")
        ));
    }
    let need = |path: &str, what: &str, dir: bool| {
        let p = Path::new(path);
        if (dir && p.is_dir()) || (!dir && p.is_file()) {
            Ok(())
        } else {
            Err(format!("{what} {path} does not exist"))
        }
    };
    need(
        args.option("masterlist").unwrap_or_default(),
        "the masterlist",
        false,
    )?;
    for archive in args.option_all("mod") {
        if !Path::new(archive).exists() {
            return Err(format!("the archive {archive} does not exist"));
        }
    }
    if let Some(dir) = args.option("fallout3") {
        need(dir, "the Fallout 3 folder", true)?;
    }
    if let Some(dir) = args.option("import-mo2") {
        need(dir, "the MO2 instance folder", true)?;
    }
    for option in ["package", "installer", "lock"] {
        if args.option(option).is_some() && args.option("fallout3").is_none() {
            return Err(format!(
                "--{option} is a TTW option and needs --fallout3 <dir>"
            ));
        }
    }
    Ok(())
}

/// Undo the run: remove what it made. A folder that existed (empty) stays, emptied.
fn roll_back(instance: &Path, pre_existing: bool) -> bool {
    if !pre_existing {
        return std::fs::remove_dir_all(instance).is_ok() || !instance.exists();
    }
    std::fs::read_dir(instance).is_ok_and(|entries| {
        entries.flatten().all(|e| {
            let p = e.path();
            if p.is_dir() {
                std::fs::remove_dir_all(&p).is_ok()
            } else {
                std::fs::remove_file(&p).is_ok()
            }
        })
    })
}

/// The exit code a failed step gives the run: its own code from the table, with a command line problem as a
/// failed step (`verify` already answers a Doctor finding with 3).
fn step_code(code: u8) -> u8 {
    if code == USAGE { ROLLED_BACK } else { code }
}

fn detail(stdout: &[u8], stderr: &[u8]) -> String {
    let (out, err) = (
        String::from_utf8_lossy(stdout),
        String::from_utf8_lossy(stderr),
    );
    let text = if err.trim().is_empty() { out } else { err };
    text.trim().chars().take(4000).collect()
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(args, FLAGS, OPTIONS) {
        Ok(a)
            if a.positional.is_empty()
                && ["base", "instance", "masterlist"]
                    .iter()
                    .all(|n| a.option(n).is_some()) =>
        {
            a
        }
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_LINE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod setup: {why}\n{USAGE_LINE}");
            return USAGE;
        }
    };
    if let Err(why) = preflight(&args) {
        return refuse(&args, &why, out, err);
    }
    let steps = plan(&args);
    let total = steps.len();
    let mut events = Events {
        out,
        json: args.flag("json"),
        seq: 0,
    };
    let dry = args.flag("dry-run");
    events.emit(
        serde_json::json!({
            "event": "plan", "dry_run": dry, "instance": args.option("instance"),
            "steps": steps.iter().map(|s| serde_json::json!({ "step": s.name, "description": s.what })).collect::<Vec<_>>(),
        }),
        &format!("nvmod setup: {} step(s){}", total, if dry { " (dry run: nothing is written)" } else { "" }),
    );
    if dry {
        for (i, s) in steps.iter().enumerate() {
            events.step(
                s,
                i,
                total,
                "planned",
                &serde_json::json!({}),
                &format!("  [{}/{total}] {}: {}", i + 1, s.name, s.what),
            );
        }
        events.emit(
            serde_json::json!({ "event": "finished", "status": "dry-run", "exit_code": OK, "rolled_back": false }),
            "nothing was written; run without --dry-run to apply",
        );
        return OK;
    }
    execute(&args, &steps, &mut events, err)
}

/// Run the steps in order; on the first failure undo the run and finish with that step's code.
fn execute(args: &Args, steps: &[Step], events: &mut Events, err: &mut dyn Write) -> u8 {
    let total = steps.len();
    let instance = Path::new(args.option("instance").unwrap_or_default()).to_owned();
    let pre_existing = instance.is_dir();
    for (i, step) in steps.iter().enumerate() {
        events.step(
            step,
            i,
            total,
            "started",
            &serde_json::json!({}),
            &format!("[{}/{total}] {}: {}", i + 1, step.name, step.what),
        );
        let (mut so, mut se) = (Vec::new(), Vec::new());
        let code = crate::run(&step.args, &mut so, &mut se);
        let shown = detail(&so, &se);
        if code == OK {
            events.step(
                step,
                i,
                total,
                "done",
                &serde_json::json!({ "code": 0, "detail": shown }),
                &if shown.is_empty() {
                    String::new()
                } else {
                    indent(&shown)
                },
            );
            continue;
        }
        let code = step_code(code);
        let undone = roll_back(&instance, pre_existing);
        events.step(
            step,
            i,
            total,
            "failed",
            &serde_json::json!({ "code": code, "detail": shown }),
            &format!(
                "{}\nstep `{}` failed (exit {code})",
                indent(&shown),
                step.name
            ),
        );
        let (final_code, note) = if undone {
            (
                code,
                "rolled back: the Instance was removed and nothing is left behind",
            )
        } else {
            (
                ROLLED_BACK,
                "the rollback could not remove the Instance; remove the folder by hand",
            )
        };
        events.emit(
            serde_json::json!({
                "event": "finished", "status": "failed", "exit_code": final_code,
                "rolled_back": undone, "failed_step": step.name,
            }),
            note,
        );
        let _ = writeln!(err, "nvmod setup: step `{}` failed; {note}", step.name);
        return final_code;
    }
    events.emit(
        serde_json::json!({ "event": "finished", "status": "ok", "exit_code": OK, "rolled_back": false, "instance": instance }),
        &format!("nvmod setup: done; the Instance at {} passes the Doctor", instance.display()),
    );
    OK
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|l| format!("    {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}
