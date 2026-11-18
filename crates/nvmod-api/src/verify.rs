// SPDX-License-Identifier: GPL-3.0-only
//! Confirming an install: the Doctor on the Instance, the installed nv-rs against the file its install record
//! names, and the launch plan of the vanilla plus xNVSE path. Read only: nothing is written.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
use nvmod_catalog::Catalog;
pub use nvmod_core::Severity;
use nvmod_doctor::BaseReport;
use nvmod_instance::{LaunchPlan, launch_plan};
use nvmod_tools::sha256_hex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const VERIFY: &str = "verify.verify";

/// The severity a finding must reach to fail the verification when the request names none.
fn default_fail_on() -> Severity {
    Severity::Error
}

/// Read a severity written as `info`, `warning` or `error`.
fn severity<'de, D>(deserializer: D) -> Result<Severity, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let text = String::deserialize(deserializer)?;
    text.parse().map_err(serde::de::Error::custom)
}

/// What [`verify`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    #[serde(default)]
    pub profile: Option<String>,
    /// The masterlist the Load order is checked against; `None` checks the plugin headers only.
    #[serde(default)]
    pub masterlist: Option<PathBuf>,
    /// The catalogue the NVSE plugins are checked against; `None` for the one shipped with nvmod.
    #[serde(default)]
    pub catalog: Option<PathBuf>,
    /// The Proton program the Linux entry of the launch plan names; `None` for `proton`.
    #[serde(default)]
    pub proton: Option<String>,
    /// The severity at or above which a finding of the Doctor fails the verification.
    #[serde(default = "default_fail_on", deserialize_with = "severity")]
    pub fail_on: Severity,
}

/// The state of the installed nv-rs against its install record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct NvRs {
    /// Whether the Instance has an nv-rs install record.
    pub installed: bool,
    /// Whether the program is the file the record names.
    pub verified: bool,
    /// What was found, in words.
    pub message: String,
}

/// The result of [`verify`].
#[derive(Debug, Clone, Serialize)]
pub struct VerifyOutput {
    /// Whether the install is confirmed: the Doctor found nothing at or above the request's `fail_on` and nv-rs
    /// is the file its record names.
    pub verified: bool,
    /// The Doctor's report on the Instance.
    pub doctor: BaseReport,
    /// The installed nv-rs against its install record.
    pub nv_rs: NvRs,
    /// The launch plan of the vanilla plus xNVSE path.
    pub launch_plan: LaunchPlan,
}

/// Confirm the install of `request.instance`: run the Doctor, check the installed nv-rs against its install
/// record and build the launch plan of the vanilla plus xNVSE path. Nothing is written.
///
/// A verification that fails is a result, not an error: [`VerifyOutput::verified`] is `false` and the report says
/// why.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance, the profile cannot be chosen, the catalogue cannot
/// be read or the Doctor refuses the base copy.
pub fn verify(
    request: &VerifyRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<VerifyOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("open-instance", "verify.verify.opening"));
    let instance = crate::open(&request.instance, false, VERIFY)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step(
        "read-catalog",
        "verify.verify.reading-catalog",
    ));
    let catalog = load_catalog(request.catalog.as_deref())?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("check-doctor", "verify.verify.checking"));
    let doctor = nvmod_doctor::check_instance(
        &instance,
        request.profile.as_deref(),
        request.masterlist.as_deref(),
        &catalog,
    )
    .map_err(|why| JobError::new(ErrorKind::Refused, VERIFY, why))?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("check-nv-rs", "verify.verify.checking-nv-rs"));
    let nv_rs = check_nv_rs(&instance.root);
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("plan-launch", "verify.verify.planning"));
    let plan = launch_plan(&instance, request.proton.as_deref());
    let doctor_ok = doctor.worst().is_none_or(|w| w < request.fail_on);
    let verified = doctor_ok && nv_rs.verified;
    Ok(Outcome::Done(VerifyOutput {
        verified,
        doctor,
        nv_rs,
        launch_plan: plan,
    }))
}

/// The catalogue to check against: the one at `file`, or the one shipped with nvmod.
fn load_catalog(file: Option<&Path>) -> Result<Catalog, JobError> {
    let Some(file) = file else {
        return Ok(Catalog::builtin());
    };
    let text = std::fs::read_to_string(file).map_err(|e| unreadable(file, &e))?;
    Catalog::parse(&text).map_err(|e| JobError::new(ErrorKind::Refused, VERIFY, e.to_string()))
}

/// The refusal of a catalogue file that could not be read.
fn unreadable(file: &Path, e: &std::io::Error) -> JobError {
    let why = format!("{}: {e}", file.display());
    JobError::new(ErrorKind::Refused, VERIFY, why)
}

/// The installed nv-rs against the file its install record names.
fn check_nv_rs(root: &Path) -> NvRs {
    let record = root.join("nv-rs/install.json");
    let Ok(text) = std::fs::read_to_string(&record) else {
        return NvRs {
            installed: false,
            verified: true,
            message: "nv-rs is not installed".to_owned(),
        };
    };
    let recorded = serde_json::from_str::<serde_json::Value>(&text)
        .ok()
        .and_then(|v| v["sha256"].as_str().map(str::to_owned));
    let bin = root
        .join("nv-rs/bin")
        .join(format!("nv-viewer{}", std::env::consts::EXE_SUFFIX));
    let actual = std::fs::read(&bin).ok().map(|b| sha256_hex(&b));
    let (verified, message) = match (recorded, actual) {
        (Some(r), Some(a)) if r == a => (true, "nv-rs matches its install record".to_owned()),
        (Some(_), Some(_)) => (
            false,
            "nv-rs differs from the checksum recorded when it was installed".to_owned(),
        ),
        (None, _) => (false, "the nv-rs install record has no checksum".to_owned()),
        (_, None) => (false, "the nv-rs program is missing".to_owned()),
    };
    NvRs {
        installed: true,
        verified,
        message,
    }
}
