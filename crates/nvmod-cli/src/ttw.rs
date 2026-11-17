// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod ttw --instance <dir> --fallout3 <dir> [--package <file>] [--installer <path>] [--json]`: Tale of Two
//! Wastelands through an external installer (the Rust installer on Linux, the official one on Windows). nvmod
//! verifies the base game first, runs the installer only through the verified runner of `nvmod-tools` (a pinned
//! or confirmed checksum is required, and re-checked at the start), hands it the Fallout 3 folder (the installer
//! owns Fallout 3 verification), checks what it reported against the `ttw-installer-result` contract, and imports
//! the output as a Mod through the install machinery. It never reimplements the installer.

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, VERIFICATION, args::Args};
use nvmod_core::Severity;
use nvmod_instance::{NvmodInstance, find_ci};
use nvmod_modinstall::{InstallError, Limits, Request};
use nvmod_tools::ttw::{self, Facts};
use nvmod_tools::{Call, Lock, Runner, Store, StoreState, ToolError, Trust, builtin_lock, pin_of};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

const USAGE_LINE: &str = "usage: nvmod ttw --instance <dir> --fallout3 <dir> [--package <file>] [--installer <path>] [--profile <name>] [--json]\n       tool options: [--lock <file>] [--store <dir>] [--timeout-secs <n>]";
const OPTIONS: &[&str] = &[
    "instance",
    "fallout3",
    "package",
    "installer",
    "profile",
    "lock",
    "store",
    "timeout-secs",
];
/// The Mod folder the output becomes.
const MOD_NAME: &str = "Tale of Two Wastelands";
/// The installer takes about 40 minutes on the slowest reported machine; three hours is a stuck run.
const DEFAULT_TIMEOUT_SECS: u64 = 3 * 60 * 60;
/// Where the installer writes, inside the Instance so that the import stays on one file system.
const SCRATCH: &str = ".ttw-scratch";

fn refused(err: &mut dyn Write, why: &str) -> u8 {
    let _ = writeln!(err, "nvmod ttw: refused: {why}");
    REFUSED
}

/// Removes the scratch folder when dropped, so that no run leaves the installer's output behind.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn count_files(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| {
            if e.path().is_dir() {
                count_files(&e.path())
            } else {
                1
            }
        })
        .sum()
}

/// The checks that must pass before anything is started or written.
struct Ready {
    instance: NvmodInstance,
    profile: String,
    fallout3: PathBuf,
    installer: PathBuf,
    sha256: String,
    name: String,
    version: String,
}

#[allow(clippy::too_many_lines)] // a list of refusals, each one a few lines
fn prepare(args: &Args, err: &mut dyn Write) -> Result<Ready, u8> {
    let root = Path::new(args.option("instance").unwrap_or_default());
    let instance = NvmodInstance::open(root)
        .map_err(|e| refused(err, &crate::install_mod::describe_open_error(root, &e)))?;
    let profile = instance
        .resolve_profile(args.option("profile"))
        .map_err(|e| refused(err, &e.to_string()))?;
    let fallout3 = PathBuf::from(args.option("fallout3").unwrap_or_default());
    if !fallout3.is_dir() {
        return Err(refused(
            err,
            &format!(
                "the Fallout 3 folder {} is not a folder",
                fallout3.display()
            ),
        ));
    }
    // The base game is verified here; Fallout 3 is the installer's to verify.
    let base = nvmod_doctor::check_base(&instance.config.base).map_err(|why| refused(err, &why))?;
    let errors: Vec<String> = base
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| format!("  {}: {} ({})", f.subject, f.message, f.code))
        .collect();
    if !errors.is_empty() {
        return Err(refused(
            err,
            &format!(
                "the base game does not verify, so the TTW installer was not started:\n{}",
                errors.join("\n")
            ),
        ));
    }
    if find_ci(&instance.root.join("mods"), MOD_NAME).is_some() {
        return Err(refused(
            err,
            &format!(
                "a Mod folder `{MOD_NAME}` already exists in the Instance; nvmod does not overwrite it"
            ),
        ));
    }
    let lock = match args.option("lock") {
        None => builtin_lock(),
        Some(path) => {
            let text =
                std::fs::read_to_string(path).map_err(|e| refused(err, &format!("{path}: {e}")))?;
            Lock::parse(&text).map_err(|why| refused(err, &why))?
        }
    };
    let entry = lock.tool(ttw::TOOL).ok_or_else(|| {
        refused(
            err,
            &format!(
                "the lock file pins no tool called `{}`; no TTW installer can be run",
                ttw::TOOL
            ),
        )
    })?;
    let asset = entry.asset_here().ok_or_else(|| {
        refused(
            err,
            &format!(
                "{} {} pins no file for {}",
                entry.name,
                entry.version,
                nvmod_tools::platform()
            ),
        )
    })?;
    let trust = Trust::load(&instance.root).map_err(|why| refused(err, &why))?;
    let Some(sha256) = pin_of(entry, asset, Some(&trust))
        .sha256()
        .map(str::to_owned)
    else {
        return Err(refused(
            err,
            &format!(
                "{0} {1} has no pinned checksum and no confirmation: check the download yourself, then run `nvmod tools trust {0} {1} --instance <dir>`",
                entry.name, entry.version
            ),
        ));
    };
    let installer = if let Some(path) = args.option("installer") {
        PathBuf::from(path)
    } else {
        {
            let store = Store::new(
                args.option("store")
                    .map_or_else(Store::default_location, PathBuf::from),
            );
            if store.state(&sha256, &entry.name) == StoreState::Missing {
                return Err(refused(
                    err,
                    &format!(
                        "{} {} is not fetched; run `nvmod tools fetch {}` or give --installer <path>",
                        entry.name, entry.version, entry.name
                    ),
                ));
            }
            store.path(&sha256, &entry.name)
        }
    };
    Ok(Ready {
        instance,
        profile,
        fallout3: std::fs::canonicalize(&fallout3).map_or(fallout3, |p| nvmod_core::plain_path(&p)),
        installer,
        sha256,
        name: entry.name.clone(),
        version: entry.version.clone(),
    })
}

fn tool_failed(err: &mut dyn Write, e: &ToolError) -> u8 {
    match e {
        ToolError::Unverifiable { .. } => {
            let _ = writeln!(
                err,
                "nvmod ttw: verification failed: {e}\nnothing was changed"
            );
            VERIFICATION
        }
        ToolError::Refused { .. } => {
            let _ = writeln!(err, "nvmod ttw: refused: {e}");
            REFUSED
        }
        _ => {
            let _ = writeln!(err, "nvmod ttw: step failed, nothing was changed: {e}");
            ROLLED_BACK
        }
    }
}

#[allow(clippy::too_many_lines)] // one flow: refuse, run, check, import
pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(args, &["json"], OPTIONS) {
        Ok(a)
            if a.positional.is_empty()
                && a.option("instance").is_some()
                && a.option("fallout3").is_some() =>
        {
            a
        }
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_LINE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod ttw: {why}\n{USAGE_LINE}");
            return USAGE;
        }
    };
    let Some(secs) = args
        .option("timeout-secs")
        .map_or(Some(DEFAULT_TIMEOUT_SECS), |v| v.parse::<u64>().ok())
    else {
        let _ = writeln!(err, "nvmod ttw: --timeout-secs needs a number");
        return USAGE;
    };
    let ready = match prepare(&args, err) {
        Ok(r) => r,
        Err(code) => return code,
    };
    let root = match std::fs::canonicalize(&ready.instance.root) {
        Ok(r) => nvmod_core::plain_path(&r),
        Err(e) => return refused(err, &format!("{}: {e}", ready.instance.root.display())),
    };
    let scratch = Scratch(root.join(SCRATCH));
    let _ = std::fs::remove_dir_all(&scratch.0);
    let output_dir = scratch.0.join("out");
    let report_file = scratch.0.join("report.json");
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        let _ = writeln!(err, "nvmod ttw: {}: {e}", output_dir.display());
        return ROLLED_BACK;
    }

    // The installer's own command line: `install --mpi <package> --fo3 <dir> --fnv <dir> --dest <dir>`.
    let base = std::fs::canonicalize(&ready.instance.config.base).map_or_else(
        |_| ready.instance.config.base.clone(),
        |p| nvmod_core::plain_path(&p),
    );
    let mut words = vec!["install".to_owned()];
    if let Some(package) = args.option("package") {
        words.extend(["--mpi".to_owned(), package.to_owned()]);
    }
    words.extend([
        "--fo3".to_owned(),
        ready.fallout3.display().to_string(),
        "--fnv".to_owned(),
        base.display().to_string(),
        "--dest".to_owned(),
        output_dir.display().to_string(),
    ]);
    let runner = Runner {
        timeout: Duration::from_secs(secs),
        env: vec![(
            "NVMOD_TTW_REPORT".to_owned(),
            report_file.display().to_string(),
        )],
        ..Runner::default()
    };
    let mut call = Call::new(&ready.name, &ready.installer, &words);
    call.expected_sha256 = Some(&ready.sha256);
    let exit_code = match runner.run(&call) {
        Ok(output) => output.exit_code,
        Err(e) => return tool_failed(err, &e),
    };
    let report = std::fs::read_to_string(&report_file).ok();
    let facts = Facts {
        installer: (&ready.name, &ready.version),
        exit_code,
        output_dir: &output_dir,
        files_found: count_files(&output_dir),
    };
    let record = match ttw::normalise(&facts, report.as_deref()) {
        Ok(r) => r,
        Err(e) => return tool_failed(err, &e),
    };
    if let Some(why) = ttw::failure(&record) {
        let _ = writeln!(err, "nvmod ttw: step failed, nothing was changed: {why}");
        return ROLLED_BACK;
    }
    if facts.files_found == 0 {
        let _ = writeln!(
            err,
            "nvmod ttw: step failed, nothing was changed: the installer wrote no files"
        );
        return ROLLED_BACK;
    }

    let request = Request {
        instance: &ready.instance,
        profile: &ready.profile,
        archive: &output_dir,
        name: Some(MOD_NAME),
        dry_run: false,
        answers: None,
        extract_tool: None,
        limits: Limits::default(),
    };
    match nvmod_modinstall::install(&request) {
        Ok(report) => {
            if args.flag("json") {
                let doc = serde_json::json!({
                    "status": "installed",
                    "mod": report.r#mod,
                    "profile": report.profile,
                    "files": report.files.len(),
                    "plugins": report.plugins,
                    "journal": report.journal,
                    "result": record,
                });
                let _ = writeln!(
                    out,
                    "{}",
                    serde_json::to_string_pretty(&doc).unwrap_or_default()
                );
            } else {
                let _ = writeln!(
                    out,
                    "nvmod ttw: installed `{}` into profile `{}` ({} files, {} {})",
                    report.r#mod,
                    report.profile,
                    report.files.len(),
                    ready.name,
                    ready.version
                );
                for p in &report.plugins {
                    let _ = writeln!(out, "  activates plugin {p}");
                }
            }
            OK
        }
        Err(InstallError::Refused(why)) => refused(err, &why),
        Err(InstallError::Failed(why)) => {
            let _ = writeln!(err, "nvmod ttw: {why}");
            ROLLED_BACK
        }
        Err(InstallError::Questions(_)) => {
            let _ = writeln!(
                err,
                "nvmod ttw: the output holds a FOMOD that needs answers; nothing was changed"
            );
            ROLLED_BACK
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_is_exit_5() {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        assert_eq!(run(&["--nope".to_owned()], &mut out, &mut err), USAGE);
        assert_eq!(run(&[], &mut out, &mut err), USAGE);
    }
}
