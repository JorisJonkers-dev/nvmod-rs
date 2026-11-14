// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod setup`: a base copy to a modded, Doctor-clean Instance in one run. It orchestrates the commands that
//! already exist (`init` or `import-mo2`, `install-nv-rs`, `ttw`, `install`, `stage`, `sort --apply`, `doctor`)
//! through the command line seam, so no step has logic of its own here and each keeps its exit code.
//!
//! By default the run is all or nothing. It only starts on a path that holds no Instance, so the state before the
//! run is "nothing here"; a failing step removes what the run made and exits with the step's code (3 verification
//! failure, 4 step failed). With `--keep-partial` a failing step leaves the Instance where it is instead (the
//! commands roll back their own work, so only the failed step is undone) and `--resume` continues from the first
//! unfinished step, using the state file the run keeps in the Instance (`setup_state`). `--restart --yes` removes
//! a partial Instance. `--json` prints one JSON object per line: a `plan`, the `step` events (and, after a
//! resume, `resumed` and `skipped` events) and a final `finished` event, each with an increasing `seq`, so a
//! wizard can show progress while the run goes.
//!
//! The last two steps matter for the Load order: `install` and `stage` lose the plugin file times, so the run
//! ends with `sort --apply`, which stamps them, and then the Doctor checks that they encode the order.

use crate::setup_state::{self as state, Completed, Failure, LoadError, PlanEntry, State};
use crate::{OK, REFUSED, ROLLED_BACK, USAGE, VERIFICATION, args::Args};
use nvmod_core::Severity;
use std::io::Write;
use std::path::Path;

const USAGE_LINE: &str = "usage: nvmod setup --base <dir> --instance <dir> --masterlist <file> [--import-mo2 <dir>] [--mod <archive>]... [--fallout3 <dir> [--package <file>] [--installer <path>]] [--release | --source <dir>] [--skip-nv-rs] [--keep-partial] [--dry-run] [--json]\n       nvmod setup --resume --instance <dir> [--json]\n       nvmod setup --restart --instance <dir> --yes";
const FLAGS: &[&str] = &[
    "json",
    "dry-run",
    "release",
    "skip-nv-rs",
    "copy",
    "keep-partial",
    "resume",
    "restart",
    "yes",
];
/// The options whose value is a path: made absolute when a run starts, so a resume from another folder works.
const PATH_OPTIONS: &[&str] = &[
    "base",
    "instance",
    "masterlist",
    "import-mo2",
    "mod",
    "answers",
    "extract-tool",
    "fallout3",
    "package",
    "installer",
    "lock",
    "store",
    "source",
    "catalog",
];
/// The flags that belong to one invocation and are not part of the recorded command line.
const INVOCATION_FLAGS: &[&str] = &["--json", "--dry-run", "--keep-partial"];
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
        let file = archive_name(archive);
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

fn archive_name(archive: &str) -> String {
    Path::new(archive)
        .file_name()
        .map_or_else(|| archive.to_owned(), |n| n.to_string_lossy().into_owned())
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

    fn plan(&mut self, args: &Args, steps: &[Step], dry: bool, resume: bool) {
        let total = steps.len();
        self.emit(
            serde_json::json!({
                "event": "plan", "dry_run": dry, "resume": resume, "instance": args.option("instance"),
                "steps": steps.iter().map(|s| serde_json::json!({ "step": s.name, "description": s.what })).collect::<Vec<_>>(),
            }),
            &format!(
                "nvmod setup: {total} step(s){}",
                if dry { " (dry run: nothing is written)" } else { "" }
            ),
        );
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

/// Stop before running anything: print why (and, with `--json`, a `finished` event) and return `code`.
fn stop(
    json: bool,
    code: u8,
    status: &str,
    why: &str,
    extra: &serde_json::Value,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let _ = writeln!(err, "nvmod setup: {status}: {why}");
    if json {
        let mut event = serde_json::json!({
            "seq": 1, "event": "finished", "status": status, "exit_code": code,
            "rolled_back": false, "reason": why,
        });
        if let (Some(o), Some(e)) = (event.as_object_mut(), extra.as_object()) {
            o.extend(e.clone());
        }
        let _ = writeln!(out, "{event}");
    }
    code
}

fn refuse(json: bool, why: &str, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    stop(
        json,
        REFUSED,
        "refused",
        why,
        &serde_json::json!({}),
        out,
        err,
    )
}

/// Everything that can be checked before anything is written. Returns the reason for a refusal. `done` names the
/// steps a resume will skip, whose inputs are not needed again; `fresh` is a run that starts a new Instance.
fn preflight(args: &Args, done: &[String], fresh: bool) -> Result<(), String> {
    let instance = Path::new(args.option("instance").unwrap_or_default());
    if fresh
        && instance.exists()
        && (!instance.is_dir() || std::fs::read_dir(instance).is_ok_and(|mut d| d.next().is_some()))
    {
        let hint = if state::path(instance).is_file() {
            "; it holds an unfinished setup: continue it with --resume or remove it with --restart --yes"
        } else {
            ""
        };
        return Err(format!(
            "{} already exists and is not an empty folder; setup only starts from a path that holds nothing{hint}",
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
    let finished = |name: &str| done.iter().any(|d| d == name);
    need(
        args.option("masterlist").unwrap_or_default(),
        "the masterlist",
        false,
    )?;
    for archive in args.option_all("mod") {
        if !finished(&format!("mod:{}", archive_name(archive))) && !Path::new(archive).exists() {
            return Err(format!("the archive {archive} does not exist"));
        }
    }
    if let Some(dir) = args.option("fallout3")
        && !finished("ttw")
    {
        need(dir, "the Fallout 3 folder", true)?;
    }
    if let Some(dir) = args.option("import-mo2")
        && !finished("import-mo2")
    {
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

/// `args` with every path-valued option made absolute, so the recorded command line means the same from any folder.
fn absolutize(args: &[String]) -> Vec<String> {
    let mut out = Vec::with_capacity(args.len());
    let mut words = args.iter();
    while let Some(word) = words.next() {
        out.push(word.clone());
        let is_path = word
            .strip_prefix("--")
            .is_some_and(|n| PATH_OPTIONS.contains(&n));
        if is_path && let Some(value) = words.next() {
            {
                out.push(
                    std::path::absolute(value)
                        .map_or_else(|_| value.clone(), |p| p.to_string_lossy().into_owned()),
                );
            }
        }
    }
    out
}

fn same_path(a: &Path, b: &Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(x), Ok(y)) => x == y,
        _ => a == b,
    }
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let parsed = match Args::parse(args, FLAGS, OPTIONS) {
        Ok(a) => a,
        Err(why) => {
            let _ = writeln!(err, "nvmod setup: {why}\n{USAGE_LINE}");
            return USAGE;
        }
    };
    if parsed.flag("resume") || parsed.flag("restart") {
        return lifecycle(&parsed, out, err);
    }
    let complete = parsed.positional.is_empty()
        && !parsed.flag("yes")
        && ["base", "instance", "masterlist"]
            .iter()
            .all(|n| parsed.option(n).is_some());
    if !complete {
        let _ = writeln!(err, "{USAGE_LINE}");
        return USAGE;
    }
    let absolute = absolutize(args);
    let Ok(args) = Args::parse(&absolute, FLAGS, OPTIONS) else {
        return USAGE;
    };
    let json = args.flag("json");
    if let Err(why) = preflight(&args, &[], true) {
        return refuse(json, &why, out, err);
    }
    let steps = plan(&args);
    let total = steps.len();
    let mut events = Events { out, json, seq: 0 };
    let dry = args.flag("dry-run");
    events.plan(&args, &steps, dry, false);
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
    let recorded: Vec<String> = absolute
        .iter()
        .filter(|a| !INVOCATION_FLAGS.contains(&a.as_str()))
        .cloned()
        .collect();
    let mut run = Run {
        keep: args.flag("keep-partial"),
        state: State {
            schema_version: state::SCHEMA_VERSION,
            nvmod_version: env!("CARGO_PKG_VERSION").to_owned(),
            args: recorded,
            plan: plan_entries(&steps),
            completed: Vec::new(),
            failure: None,
            complete: false,
        },
        done: 0,
        resumed: false,
    };
    execute(&args, &steps, &mut events, err, &mut run)
}

fn plan_entries(steps: &[Step]) -> Vec<PlanEntry> {
    steps
        .iter()
        .map(|s| PlanEntry {
            step: s.name.clone(),
            description: s.what.clone(),
        })
        .collect()
}

/// `--resume` and `--restart`: they take the Instance and nothing else (`--restart` also `--yes`).
fn lifecycle(args: &Args, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let restart = args.flag("restart");
    let stray_option = OPTIONS
        .iter()
        .any(|o| *o != "instance" && args.option(o).is_some());
    let stray_flag = ["dry-run", "release", "skip-nv-rs", "copy", "keep-partial"]
        .iter()
        .any(|f| args.flag(f));
    let misplaced = (args.flag("resume") && restart) || (args.flag("yes") && !restart);
    if stray_option
        || stray_flag
        || misplaced
        || !args.positional.is_empty()
        || args.option("instance").is_none()
    {
        let _ = writeln!(err, "{USAGE_LINE}");
        return USAGE;
    }
    let instance = std::path::absolute(args.option("instance").unwrap_or_default())
        .unwrap_or_else(|_| Path::new(args.option("instance").unwrap_or_default()).to_owned());
    let json = args.flag("json");
    let state = match state::load(&instance) {
        Ok(s) => s,
        Err(LoadError::Missing) => {
            let why = format!(
                "{} holds no unfinished setup (there is no .nvmod/setup.json)",
                instance.display()
            );
            return refuse(json, &why, out, err);
        }
        Err(LoadError::Newer(v)) => {
            let why = format!(
                "the setup state of {} is schema version {v}, newer than the {} this nvmod reads",
                instance.display(),
                state::SCHEMA_VERSION
            );
            return refuse(json, &why, out, err);
        }
        Err(LoadError::Bad(why)) => return refuse(json, &why, out, err),
    };
    if restart {
        restart_setup(args, &instance, &state, out, err)
    } else {
        resume(args, &instance, state, out, err)
    }
}

/// Remove a partial Instance. Only one that holds an unfinished setup, and only with `--yes`.
fn restart_setup(
    args: &Args,
    instance: &Path,
    state: &State,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let json = args.flag("json");
    if state.complete {
        let why = format!(
            "{} is a finished Instance, not a partial one; setup will not remove it",
            instance.display()
        );
        return refuse(json, &why, out, err);
    }
    if !args.flag("yes") {
        let why = format!(
            "--restart removes {} and everything in it; add --yes to confirm",
            instance.display()
        );
        return refuse(json, &why, out, err);
    }
    if std::fs::remove_dir_all(instance).is_err() && instance.exists() {
        return stop(
            json,
            ROLLED_BACK,
            "failed",
            &format!(
                "cannot remove {}; remove the folder by hand",
                instance.display()
            ),
            &serde_json::json!({}),
            out,
            err,
        );
    }
    let mut events = Events { out, json, seq: 0 };
    events.emit(
        serde_json::json!({ "event": "finished", "status": "restarted", "exit_code": OK, "rolled_back": false, "removed": instance }),
        &format!(
            "nvmod setup: removed {}; start again with a new `nvmod setup`",
            instance.display()
        ),
    );
    OK
}

/// Continue a recorded run: re-check the finished steps, skip them, run the rest.
fn resume(
    args: &Args,
    instance: &Path,
    state: State,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let json = args.flag("json");
    let Ok(recorded) = Args::parse(&state.args, FLAGS, OPTIONS) else {
        return refuse(json, "the recorded command line cannot be read", out, err);
    };
    let there = recorded.option("instance").unwrap_or_default();
    if !same_path(Path::new(there), instance) {
        let why = format!(
            "the setup was started for {there}, not {}",
            instance.display()
        );
        return refuse(json, &why, out, err);
    }
    let finished: Vec<String> = state.completed.iter().map(|c| c.step.clone()).collect();
    if let Err(why) = preflight(&recorded, &finished, false) {
        return refuse(json, &why, out, err);
    }
    let steps = plan(&recorded);
    let names: Vec<&str> = steps.iter().map(|s| s.name.as_str()).collect();
    let was: Vec<&str> = state.plan.iter().map(|p| p.step.as_str()).collect();
    let in_order = finished
        .iter()
        .zip(&names)
        .all(|(done, planned)| done == planned);
    if names != was || !in_order || finished.len() > names.len() {
        let why =
            "the recorded plan does not match this nvmod's plan for the recorded command line";
        return refuse(json, why, out, err);
    }
    let profile = recorded.option("profile").unwrap_or("Default");
    for done in &state.completed {
        if let Err(why) = state::verify_witness(&done.step, &done.witness, instance, profile) {
            let text = format!(
                "cannot resume: step `{}` no longer holds ({why}); nothing was run. Start again with `nvmod setup --restart --instance {} --yes`",
                done.step,
                instance.display()
            );
            return stop(
                json,
                VERIFICATION,
                "failed",
                &text,
                &serde_json::json!({ "failed_step": done.step, "resumable": false }),
                out,
                err,
            );
        }
    }
    let mut events = Events { out, json, seq: 0 };
    events.plan(&recorded, &steps, false, true);
    let mut run = Run {
        keep: true,
        done: state.completed.len(),
        state,
        resumed: true,
    };
    execute(&recorded, &steps, &mut events, err, &mut run)
}

/// What one invocation does about the state file.
struct Run {
    /// A failing step leaves the Instance resumable instead of removing it.
    keep: bool,
    state: State,
    /// Steps already finished and verified: they are skipped.
    done: usize,
    resumed: bool,
}

/// The `resumed` event of a continued run.
fn emit_resumed(events: &mut Events, steps: &[Step], run: &Run, instance: &Path) {
    let total = steps.len();
    events.emit(
        serde_json::json!({
            "event": "resumed", "instance": instance, "total": total,
            "completed": run.state.completed.iter().map(|c| c.step.clone()).collect::<Vec<_>>(),
            "next": steps.get(run.done).map(|s| s.name.clone()),
        }),
        &format!(
            "nvmod setup: resuming at step {} of {total}",
            run.done.min(total - 1) + 1
        ),
    );
}

/// The `skipped` event of a step that finished earlier and still holds.
fn emit_skipped(events: &mut Events, step: &Step, i: usize, total: usize) {
    events.emit(
        serde_json::json!({
            "event": "skipped", "step": step.name, "index": i + 1, "total": total,
            "description": step.what, "reason": "finished earlier and still holds",
        }),
        &format!(
            "[{}/{total}] {}: finished earlier, skipped",
            i + 1,
            step.name
        ),
    );
}

/// Run the steps in order. A failure with `keep` set (and something finished) leaves the Instance resumable;
/// otherwise it undoes the run. Either way the run finishes with that step's code.
fn execute(
    args: &Args,
    steps: &[Step],
    events: &mut Events,
    err: &mut dyn Write,
    run: &mut Run,
) -> u8 {
    let total = steps.len();
    let instance = Path::new(args.option("instance").unwrap_or_default()).to_owned();
    let profile = args.option("profile").unwrap_or("Default").to_owned();
    let pre_existing = instance.is_dir();
    if run.resumed {
        emit_resumed(events, steps, run, &instance);
    }
    for (i, step) in steps.iter().enumerate() {
        if i < run.done {
            emit_skipped(events, step, i, total);
            continue;
        }
        if nvmod_core::cancel::requested() {
            // Stopped between two steps: nothing of this step ran; the run ends like a failed step.
            return fail(
                &Failed {
                    step,
                    index: i,
                    total,
                    code: ROLLED_BACK,
                    shown: nvmod_core::cancel::INTERRUPTED,
                },
                &instance,
                pre_existing,
                events,
                err,
                run,
            );
        }
        events.step(
            step,
            i,
            total,
            "started",
            &serde_json::json!({}),
            &format!("[{}/{total}] {}: {}", i + 1, step.name, step.what),
        );
        let mods_before = state::mods_snapshot(&instance);
        let (mut so, mut se) = (Vec::new(), Vec::new());
        let mut code = crate::run(&step.args, &mut so, &mut se);
        let mut shown = detail(&so, &se);
        if code == OK && step.name != "verify" {
            let witness = state::witness_for(&step.name, &instance, &profile, &mods_before);
            run.state.completed.push(Completed {
                step: step.name.clone(),
                witness,
            });
            run.state.failure = None;
            if let Err(e) = state::save(&instance, &run.state) {
                run.state.completed.pop();
                code = ROLLED_BACK;
                shown = format!("cannot write the setup state file: {e}");
            }
        }
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
        let code = if nvmod_core::cancel::requested() {
            ROLLED_BACK
        } else {
            step_code(code)
        };
        return fail(
            &Failed {
                step,
                index: i,
                total,
                code,
                shown: &shown,
            },
            &instance,
            pre_existing,
            events,
            err,
            run,
        );
    }
    run.state.complete = true;
    run.state.failure = None;
    let _ = state::save(&instance, &run.state);
    events.emit(
        serde_json::json!({ "event": "finished", "status": "ok", "exit_code": OK, "rolled_back": false, "resumable": false, "instance": instance }),
        &format!("nvmod setup: done; the Instance at {} passes the Doctor", instance.display()),
    );
    OK
}

/// A step that failed.
struct Failed<'a> {
    step: &'a Step,
    index: usize,
    total: usize,
    code: u8,
    shown: &'a str,
}

/// Finish a run whose step failed: keep the Instance resumable, or undo the run.
fn fail(
    f: &Failed,
    instance: &Path,
    pre_existing: bool,
    events: &mut Events,
    err: &mut dyn Write,
    run: &mut Run,
) -> u8 {
    let (step, code) = (f.step, f.code);
    let resumable = run.keep && !run.state.completed.is_empty();
    let undone = !resumable && roll_back(instance, pre_existing);
    if resumable {
        run.state.failure = Some(Failure {
            step: step.name.clone(),
            code,
            detail: f.shown.to_owned(),
        });
        let _ = state::save(instance, &run.state);
    }
    events.step(
        step,
        f.index,
        f.total,
        "failed",
        &serde_json::json!({ "code": code, "detail": f.shown }),
        &format!(
            "{}\nstep `{}` failed (exit {code})",
            indent(f.shown),
            step.name
        ),
    );
    let (final_code, note) = if resumable {
        (
            code,
            format!(
                "the Instance is kept: continue with `nvmod setup --resume --instance {0}` or remove it with `nvmod setup --restart --instance {0} --yes`",
                instance.display()
            ),
        )
    } else if undone {
        (
            code,
            "rolled back: the Instance was removed and nothing is left behind".to_owned(),
        )
    } else {
        (
            ROLLED_BACK,
            "the rollback could not remove the Instance; remove the folder by hand".to_owned(),
        )
    };
    events.emit(
        serde_json::json!({
            "event": "finished", "status": "failed", "exit_code": final_code,
            "rolled_back": undone, "resumable": resumable, "failed_step": step.name,
            "outcome": if resumable { "kept-partial" } else if undone { "rolled-back" } else { "rollback-failed" },
            "interrupted": nvmod_core::cancel::requested(),
        }),
        &note,
    );
    let _ = writeln!(err, "nvmod setup: step `{}` failed; {note}", step.name);
    final_code
}

fn indent(text: &str) -> String {
    text.lines()
        .map(|l| format!("    {l}"))
        .collect::<Vec<_>>()
        .join("\n")
}
