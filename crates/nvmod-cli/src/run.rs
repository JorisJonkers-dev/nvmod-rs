// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod run <title|path> --instance <dir> [--profile <name>] [--proton <program>] [--dry-run] [--wait]
//! [--restage] [--no-stage] [--no-profile] [--json] [-- <args>...]`: start the game or a tool through the staged view
//! (nv-linux #148).
//!
//! Order: plan (read only; `--dry-run` stops here), refuse while Mod Organizer 2 runs, take the Instance lock,
//! build or rebuild the Staging tree when it is missing or older than its inputs, start the program. A detached
//! run (the default) hands the lock to the program, so the tree stays safe until it ends.

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, args::Args};
use nvmod_instance::NvmodInstance;
use nvmod_run::{InstanceLock, Launcher, LockError, Platform, ProcessLauncher, Request};
use nvmod_stage::{Options, StageError};
use std::io::Write;
use std::path::{Path, PathBuf};

const USAGE_LINE: &str = "usage: nvmod run <title|path|game|nv-rs> --instance <dir> [--profile <name>] [--proton <program>] [--dry-run] [--wait] [--restage] [--no-stage] [--no-profile] [--json] [-- <args>...]";

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    run_with(args, out, err, &ProcessLauncher)
}

fn steam_client() -> Option<PathBuf> {
    std::env::var_os("STEAM_COMPAT_CLIENT_INSTALL_PATH")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join(".steam/steam"))
                .filter(|p| p.is_dir())
        })
}

fn print_plan(plan: &nvmod_run::RunPlan, json: bool, out: &mut dyn Write) {
    if json {
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&plan.to_json()).unwrap_or_default()
        );
    } else {
        let _ = out.write_all(plan.to_text().as_bytes());
    }
}

/// [`run`] with the launcher chosen by the caller.
pub(crate) fn run_with(
    args: &[String],
    out: &mut dyn Write,
    err: &mut dyn Write,
    launcher: &dyn Launcher,
) -> u8 {
    let (own, extra) = match args.iter().position(|a| a == "--") {
        Some(i) => (&args[..i], &args[i + 1..]),
        None => (args, &args[args.len()..]),
    };
    let a = match Args::parse(
        own,
        &[
            "dry-run",
            "wait",
            "restage",
            "no-stage",
            "no-profile",
            "json",
        ],
        &["instance", "profile", "proton"],
    ) {
        Ok(a) if a.positional.len() == 1 && a.option("instance").is_some() => a,
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_LINE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod run: {why}\n{USAGE_LINE}");
            return USAGE;
        }
    };
    let refuse = |why: &str, err: &mut dyn Write| {
        let _ = writeln!(err, "nvmod run: refused: {why}");
        REFUSED
    };
    let root = Path::new(a.option("instance").unwrap_or_default());
    let instance = match NvmodInstance::open_read_only(root) {
        Ok(i) => i,
        Err(e) => return refuse(&crate::install_mod::describe_open_error(root, &e), err),
    };
    let profile = match instance.resolve_profile(a.option("profile")) {
        Ok(p) => p,
        Err(e) => return refuse(&e.to_string(), err),
    };
    let request = Request {
        instance: &instance,
        profile: &profile,
        target: &a.positional[0],
        args: extra,
        proton: a.option("proton"),
        platform: Platform::host(),
        steam_client: steam_client(),
    };
    let mut plan = match nvmod_run::plan(&request) {
        Ok(p) => p,
        Err(why) => return refuse(&why, err),
    };
    plan.staging =
        nvmod_run::plan_staging(&instance, &profile, a.flag("restage"), a.flag("no-stage"));
    let dry = a.flag("dry-run");
    if dry {
        if !a.flag("no-profile") {
            plan.notes.push(profile_note(&instance, &profile));
        }
        print_plan(&plan, a.flag("json"), out);
        return OK;
    }

    execute(&a, &instance, &profile, plan, launcher, out, err)
}

/// The Wine prefix of the Instance (the `compatdata` folder the launch plan gives Proton) as a profile target:
/// `None` where there is none yet, and always on Windows, whose real folders have no backend here.
fn prefix_target(instance: &NvmodInstance) -> Result<nvmod_profile::Target, String> {
    if cfg!(windows) {
        return Err(
            "applying a profile to the Windows folders is not done by `nvmod run` yet".to_owned(),
        );
    }
    nvmod_profile::Target::from_prefix(&instance.root.join("compatdata"))
}

/// What a dry run says about applying the profile to the prefix (nothing is written).
fn profile_note(instance: &NvmodInstance, profile: &str) -> String {
    match prefix_target(instance) {
        Err(why) => format!("profile: not applied to a prefix ({why})"),
        Ok(target) => match nvmod_profile::apply(instance, profile, &target, true) {
            Ok(r) => format!(
                "profile: `{profile}` is placed in the prefix before the start ({} change(s) planned)",
                r.actions.len()
            ),
            Err(nvmod_profile::Error::Refused(why) | nvmod_profile::Error::Failed(why)) => {
                format!("profile: cannot be applied: {why}")
            }
        },
    }
}

/// Under the lock: stage when the tree is stale, then place the profile in the prefix. `Err` is the exit code
/// after the message was written; `Ok` carries the prefix target when the profile was placed.
fn prepare(
    a: &Args,
    instance: &NvmodInstance,
    profile: &str,
    plan: &mut nvmod_run::RunPlan,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> Result<Option<nvmod_profile::Target>, u8> {
    let refuse = |why: &str, err: &mut dyn Write| {
        let _ = writeln!(err, "nvmod run: refused: {why}");
        REFUSED
    };
    // Plan again under the lock: whoever held it may have changed the tree.
    plan.staging =
        nvmod_run::plan_staging(instance, profile, a.flag("restage"), a.flag("no-stage"));
    if plan.staging.builds() {
        match nvmod_stage::stage(instance, profile, Options { copy_only: false }) {
            Ok(report) => {
                if !a.flag("json") {
                    let _ = writeln!(
                        out,
                        "nvmod run: staged profile `{profile}`: {} files in {}",
                        report.manifest.files, report.tree
                    );
                }
            }
            Err(StageError::Refused(why)) => return Err(refuse(&why, err)),
            Err(StageError::Failed(why)) => {
                let _ = writeln!(err, "nvmod run: staging failed: {why}");
                return Err(ROLLED_BACK);
            }
        }
    }
    let target = if a.flag("no-profile") {
        None
    } else {
        match prefix_target(instance) {
            Ok(t) => Some(t),
            Err(why) => {
                plan.notes
                    .push(format!("profile: not applied to a prefix ({why})"));
                None
            }
        }
    };
    if let Some(target) = &target {
        match nvmod_profile::apply(instance, profile, target, false) {
            Ok(r) => plan.notes.push(format!(
                "profile: `{profile}` {} in the prefix ({} change(s))",
                r.status,
                r.actions.len()
            )),
            Err(nvmod_profile::Error::Refused(why)) => return Err(refuse(&why, err)),
            Err(nvmod_profile::Error::Failed(why)) => {
                let _ = writeln!(
                    err,
                    "nvmod run: applying the profile failed: {why}; rolled back"
                );
                return Err(ROLLED_BACK);
            }
        }
    }
    Ok(target)
}

/// The writing half of a run: checks, the lock, staging and the start.
fn execute(
    a: &Args,
    instance: &NvmodInstance,
    profile: &str,
    mut plan: nvmod_run::RunPlan,
    launcher: &dyn Launcher,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let root = instance.root.as_path();
    let refuse = |why: &str, err: &mut dyn Write| {
        let _ = writeln!(err, "nvmod run: refused: {why}");
        REFUSED
    };
    if let Some(name) = nvmod_run::mo2_running() {
        return refuse(
            &format!(
                "Mod Organizer 2 is running (process `{name}`); close it first, two managers must not share an Instance"
            ),
            err,
        );
    }
    let mut lock = match InstanceLock::acquire(root, &format!("run {}", plan.entry)) {
        Ok(l) => l,
        Err(LockError::Held(h)) => {
            return refuse(&format!("the Instance is in use by {h}"), err);
        }
        Err(LockError::Io(why)) => {
            return refuse(&format!("cannot take the Instance lock: {why}"), err);
        }
    };
    if let Some(old) = &lock.took_over {
        let _ = writeln!(
            err,
            "nvmod run: info: took over the lock of {old}, which is no longer running"
        );
    }
    let target = match prepare(a, instance, profile, &mut plan, out, err) {
        Ok(t) => t,
        Err(code) => return code,
    };
    print_plan(&plan, a.flag("json"), out);
    let wait = a.flag("wait");
    let running = match launcher.start(&plan.command, wait) {
        Ok(r) => r,
        Err(why) => {
            let _ = writeln!(err, "nvmod run: {why}");
            return ROLLED_BACK;
        }
    };
    let pid = running.pid();
    if wait {
        let waited = running.wait();
        if let Some(target) = &target
            && let Err(nvmod_profile::Error::Refused(why) | nvmod_profile::Error::Failed(why)) =
                nvmod_profile::finish(instance, target, false)
        {
            let _ = writeln!(
                err,
                "nvmod run: warning: collecting what the program changed failed: {why}"
            );
        }
        return match waited {
            Ok(code) => u8::try_from(code.clamp(0, 255)).unwrap_or(1),
            Err(why) => {
                let _ = writeln!(err, "nvmod run: {why}");
                ROLLED_BACK
            }
        };
    }
    if let Err(why) = lock.hand_over(pid, &format!("run {} (program)", plan.entry)) {
        let _ = writeln!(err, "nvmod run: warning: {why}");
    }
    if !a.flag("json") {
        let _ = writeln!(
            out,
            "nvmod run: started `{}` (pid {pid}); the Instance stays locked until it ends; run `nvmod profile finish` afterwards to collect what it changed",
            plan.entry
        );
    }
    OK
}
