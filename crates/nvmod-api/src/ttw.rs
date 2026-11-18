// SPDX-License-Identifier: GPL-3.0-only
//! Tale of Two Wastelands through an external installer: the base game is verified first, the pinned installer runs
//! only through the verified runner of `nvmod-tools` (a pinned or confirmed checksum is required, and re-checked at
//! the start), what it reported is checked against the `ttw-installer-result` contract, and its output is imported
//! as a Mod through the install machinery. The installer is never reimplemented here.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
use nvmod_core::Severity;
use nvmod_instance::{NvmodInstance, find_ci};
pub use nvmod_modinstall::Report;
use nvmod_modinstall::{InstallError, Limits, Request as ModInstallRequest};
use nvmod_tools::ttw::{self, Facts};
use nvmod_tools::{Call, Lock, Runner, Store, StoreState, ToolError, Trust, builtin_lock, pin_of};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

const INSTALL: &str = "ttw.install";
/// The Mod folder the installer's output becomes.
const MOD_NAME: &str = "Tale of Two Wastelands";
/// The installer takes about 40 minutes on the slowest reported machine; three hours is a stuck run.
const DEFAULT_TIMEOUT_SECS: u64 = 3 * 60 * 60;
/// Where the installer writes, inside the Instance so that the import stays on one file system.
const SCRATCH: &str = ".ttw-scratch";

/// What [`install`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile the Mod goes into, `None` for the only one there is.
    #[serde(default)]
    pub profile: Option<String>,
    /// The Fallout 3 folder the installer reads; the installer verifies it.
    pub fallout3: PathBuf,
    /// The TTW package the installer builds from.
    #[serde(default)]
    pub package: Option<PathBuf>,
    /// The installer program; `None` takes the pinned one from the tool store.
    #[serde(default)]
    pub installer: Option<PathBuf>,
    /// The lock file to read; `None` reads the one nvmod ships.
    #[serde(default)]
    pub lock: Option<PathBuf>,
    /// The tool store; `None` uses the default location.
    #[serde(default)]
    pub store: Option<PathBuf>,
    /// How long the installer may run, in seconds; `None` for three hours.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

/// The result of [`install`].
#[derive(Debug, Clone, Serialize)]
pub struct InstallOutput {
    /// The profile the Mod went into.
    pub profile: String,
    /// The install report.
    pub report: Report,
    /// The installer's name, as the lock pins it.
    pub installer: String,
    /// The installer's version, as the lock pins it.
    pub version: String,
    /// The normalised `ttw-installer-result` record the run was checked against.
    pub result: Value,
}

/// The checks that must pass before anything is started or written.
struct Ready {
    /// The Fallout 3 folder, as the installer is given it.
    fallout3: PathBuf,
    /// The installer program.
    installer: PathBuf,
    /// The SHA-256 the installer must have.
    sha256: String,
    /// The installer's name, as the lock pins it.
    name: String,
    /// The installer's version, as the lock pins it.
    version: String,
}

/// The installer's scratch folder, removed when dropped, so that no run leaves its output behind.
struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A refusal: nothing was written.
fn refused(why: String) -> JobError {
    JobError::new(ErrorKind::Refused, INSTALL, why)
}

/// A failed step: nothing was changed, and `why` says what went wrong.
fn step_failed(why: &str) -> JobError {
    JobError::new(
        ErrorKind::Failed,
        INSTALL,
        format!("step failed, nothing was changed: {why}"),
    )
}

/// A failed tool call as a job error: a checksum that is not the pinned one is a verification failure, a newer
/// contract version a refusal, and everything else a failed step.
fn tool_error(e: &ToolError) -> JobError {
    let summary = e.to_string();
    match e {
        ToolError::Unverifiable { .. } => JobError::new(ErrorKind::Corrupt, INSTALL, summary),
        ToolError::Refused { .. } => JobError::new(ErrorKind::Refused, INSTALL, summary),
        ToolError::Failed { .. } | ToolError::Malformed { .. } => step_failed(&summary),
    }
}

/// `path` canonicalized and written plainly, or as it is when it cannot be canonicalized.
fn plain_canonical(path: &Path) -> PathBuf {
    match std::fs::canonicalize(path) {
        Ok(found) => nvmod_core::plain_path(&found),
        Err(_) => path.to_path_buf(),
    }
}

/// How many files are under `dir`, at any depth.
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

/// Run the pinned TTW installer on `request.fallout3` and import what it wrote as the Mod
/// `Tale of Two Wastelands` of `request.profile`.
///
/// # Errors
/// [`ErrorKind::Refused`] before anything was written: the folder is not an Instance, the profile cannot be
/// chosen, the Fallout 3 folder is not a folder, the base game does not verify, the Mod folder already exists, the
/// lock pins no installer or no checksum, or the installer is not fetched. [`ErrorKind::Corrupt`] when the
/// installer is not the pinned file. [`ErrorKind::Failed`] when the installer failed, its report breaks the
/// contract, it wrote no files, or the import failed and was rolled back.
pub fn install(
    request: &InstallRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<InstallOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, true, INSTALL)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), INSTALL)?;
    progress.report(&Event::step("check-base", "ttw.install.checking-base"));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let ready = prepare(request, &instance)?;
    progress.report(&Event::step(
        "run-installer",
        "ttw.install.running-installer",
    ));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let root = std::fs::canonicalize(&instance.root)
        .map_err(|e| refused(format!("{}: {e}", instance.root.display())))?;
    let root = nvmod_core::plain_path(&root);
    let scratch = Scratch(root.join(SCRATCH));
    let _ = std::fs::remove_dir_all(&scratch.0);
    let output_dir = scratch.0.join("out");
    let report_file = scratch.0.join("report.json");
    if let Err(e) = std::fs::create_dir_all(&output_dir) {
        return Err(JobError::new(
            ErrorKind::Failed,
            INSTALL,
            format!("{}: {e}", output_dir.display()),
        ));
    }
    let record = run_installer(request, &instance, &ready, &output_dir, &report_file)?;
    progress.report(&Event::step("import", "ttw.install.importing"));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let mod_request = ModInstallRequest {
        instance: &instance,
        profile: &profile,
        archive: &output_dir,
        name: Some(MOD_NAME),
        dry_run: false,
        answers: None,
        extract_tool: None,
        limits: Limits::default(),
    };
    match nvmod_modinstall::install(&mod_request) {
        Ok(report) => Ok(Outcome::Done(InstallOutput {
            profile,
            report,
            installer: ready.name,
            version: ready.version,
            result: record,
        })),
        Err(InstallError::Refused(why)) => Err(refused(why)),
        Err(InstallError::Failed(why)) => Err(JobError::new(ErrorKind::Failed, INSTALL, why)),
        Err(InstallError::Questions(_)) => Err(JobError::new(
            ErrorKind::Failed,
            INSTALL,
            "the output holds a FOMOD that needs answers; nothing was changed",
        )),
    }
}

/// The checks that must pass before anything is started or written: the Fallout 3 folder, the base game, the Mod
/// folder, the lock, the checksum and the installer program.
fn prepare(request: &InstallRequest, instance: &NvmodInstance) -> Result<Ready, JobError> {
    if !request.fallout3.is_dir() {
        return Err(refused(format!(
            "the Fallout 3 folder {} is not a folder",
            request.fallout3.display()
        )));
    }
    // The base game is verified here; Fallout 3 is the installer's to verify.
    let base = nvmod_doctor::check_base(&instance.config.base).map_err(refused)?;
    let errors: Vec<String> = base
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| format!("  {}: {} ({})", f.subject, f.message, f.code))
        .collect();
    if !errors.is_empty() {
        return Err(refused(format!(
            "the base game does not verify, so the TTW installer was not started:\n{}",
            errors.join("\n")
        )));
    }
    if find_ci(&instance.root.join("mods"), MOD_NAME).is_some() {
        return Err(refused(format!(
            "a Mod folder `{MOD_NAME}` already exists in the Instance; nvmod does not overwrite it"
        )));
    }
    let lock = match &request.lock {
        None => builtin_lock(),
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|e| refused(format!("{}: {e}", path.display())))?;
            Lock::parse(&text).map_err(refused)?
        }
    };
    let entry = lock.tool(ttw::TOOL).ok_or_else(|| {
        refused(format!(
            "the lock file pins no tool called `{}`; no TTW installer can be run",
            ttw::TOOL
        ))
    })?;
    let asset = entry.asset_here().ok_or_else(|| {
        refused(format!(
            "{} {} pins no file for {}",
            entry.name,
            entry.version,
            nvmod_tools::platform()
        ))
    })?;
    let trust = Trust::load(&instance.root).map_err(refused)?;
    let pin = pin_of(entry, asset, Some(&trust));
    let Some(sha256) = pin.sha256().map(str::to_owned) else {
        return Err(refused(format!(
            "{0} {1} has no pinned checksum and no confirmation: check the download yourself, then run `nvmod tools trust {0} {1} --instance <dir>`",
            entry.name, entry.version
        )));
    };
    let installer = if let Some(path) = &request.installer {
        path.clone()
    } else {
        let root = request
            .store
            .clone()
            .unwrap_or_else(Store::default_location);
        let store = Store::new(root);
        if store.state(&sha256, &entry.name) == StoreState::Missing {
            return Err(refused(format!(
                "{} {} is not fetched; run `nvmod tools fetch {}` or give --installer <path>",
                entry.name, entry.version, entry.name
            )));
        }
        store.path(&sha256, &entry.name)
    };
    Ok(Ready {
        fallout3: plain_canonical(&request.fallout3),
        installer,
        sha256,
        name: entry.name.clone(),
        version: entry.version.clone(),
    })
}

/// Run the installer into `output_dir` and return the record its run was checked against.
fn run_installer(
    request: &InstallRequest,
    instance: &NvmodInstance,
    ready: &Ready,
    output_dir: &Path,
    report_file: &Path,
) -> Result<Value, JobError> {
    // The installer's own command line: `install --mpi <package> --fo3 <dir> --fnv <dir> --dest <dir>`.
    let base = plain_canonical(&instance.config.base);
    let mut words = vec!["install".to_owned()];
    if let Some(package) = &request.package {
        words.extend(["--mpi".to_owned(), package.display().to_string()]);
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
        timeout: Duration::from_secs(request.timeout_secs.unwrap_or(DEFAULT_TIMEOUT_SECS)),
        env: vec![(
            "NVMOD_TTW_REPORT".to_owned(),
            report_file.display().to_string(),
        )],
        ..Runner::default()
    };
    let mut call = Call::new(&ready.name, &ready.installer, &words);
    call.expected_sha256 = Some(&ready.sha256);
    let exit_code = runner.run(&call).map_err(|e| tool_error(&e))?.exit_code;
    let report = std::fs::read_to_string(report_file).ok();
    let facts = Facts {
        installer: (&ready.name, &ready.version),
        exit_code,
        output_dir,
        files_found: count_files(output_dir),
    };
    let checked = ttw::normalise(&facts, report.as_deref());
    let record = checked.map_err(|e| tool_error(&e))?;
    if let Some(why) = ttw::failure(&record) {
        return Err(step_failed(&why));
    }
    if facts.files_found == 0 {
        return Err(step_failed("the installer wrote no files"));
    }
    Ok(record)
}
