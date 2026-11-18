// SPDX-License-Identifier: GPL-3.0-only
//! Installing nv-rs into an Instance: build it from a source workspace, or fetch and verify the fork's release,
//! then run the installation check. Every step is staged and rolled back on failure, and the game executable is
//! never copied.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
pub use nvmod_install::{DEFAULT_ASSET, DEFAULT_RELEASE_URL, DEFAULT_REPO};
use nvmod_install::{InstallError, Method as NvRsMethod, Options};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

const INSTALL: &str = "install_nvrs.install";

/// The release URL of a request that names none.
fn default_release_url() -> String {
    DEFAULT_RELEASE_URL.to_owned()
}

/// The release asset of a request that names none.
fn default_asset() -> String {
    DEFAULT_ASSET.to_owned()
}

/// The repository of a request that names none.
fn default_repo() -> String {
    DEFAULT_REPO.to_owned()
}

/// The `gh` program of a request that names none.
fn default_gh() -> PathBuf {
    PathBuf::from("gh")
}

/// The `curl` program of a request that names none.
fn default_curl() -> PathBuf {
    PathBuf::from("curl")
}

/// The wait between fetch retries of a request that names none.
fn default_retry_delay_ms() -> u64 {
    1000
}

/// What [`install`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The nv-rs workspace to build; `None` when the release is fetched.
    #[serde(default)]
    pub source: Option<PathBuf>,
    /// Fetch the release even when a Rust toolchain exists.
    #[serde(default)]
    pub release: bool,
    /// The `cargo` program; `None` looks for one on `PATH`.
    #[serde(default)]
    pub cargo: Option<PathBuf>,
    /// Where the release is fetched from.
    #[serde(default = "default_release_url")]
    pub release_url: String,
    /// The release asset.
    #[serde(default = "default_asset")]
    pub asset: String,
    /// The repository the attestation must name.
    #[serde(default = "default_repo")]
    pub repo: String,
    /// The `gh` program.
    #[serde(default = "default_gh")]
    pub gh: PathBuf,
    /// The `curl` program.
    #[serde(default = "default_curl")]
    pub curl: PathBuf,
    /// How long to wait between fetch retries, in milliseconds.
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
}

/// How nv-rs was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Method {
    /// Built from a source workspace.
    Source,
    /// The fork's release, fetched and verified.
    Release,
}

/// The installation check that passed: `nv-viewer <data folder> --check`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Check {
    /// The program and its arguments, as run.
    pub command: Vec<String>,
    /// What the check printed (standard output, then standard error).
    pub output: String,
}

/// The result of [`install`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct InstallOutput {
    /// How nv-rs was obtained.
    pub method: Method,
    /// The installed program.
    pub binary: PathBuf,
    /// The installation check that passed.
    pub check: Check,
}

/// Install nv-rs into `request.instance`: build it from `request.source` when a Rust toolchain is found, or fetch
/// and verify the fork's release, then run the installation check.
///
/// # Errors
/// [`ErrorKind::Refused`] before anything was written (nv-rs is already installed, or there is no source to build);
/// [`ErrorKind::Corrupt`] when a checksum, an attestation or a response did not check out, in which case nothing was
/// installed; [`ErrorKind::Failed`] when a step failed and what it had written was removed.
pub fn install(
    request: &InstallRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<InstallOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step(
        "install-nv-rs",
        "install_nvrs.install.installing",
    ));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let options = Options {
        instance: request.instance.clone(),
        source: request.source.clone(),
        release: request.release,
        cargo: request.cargo.clone(),
        release_url: request.release_url.clone(),
        asset: request.asset.clone(),
        repo: request.repo.clone(),
        gh: request.gh.clone(),
        curl: request.curl.clone(),
        retry_delay: Duration::from_millis(request.retry_delay_ms),
    };
    match nvmod_install::install(&options) {
        Ok(done) => Ok(Outcome::Done(InstallOutput {
            method: match done.method {
                NvRsMethod::Source => Method::Source,
                NvRsMethod::Release => Method::Release,
            },
            binary: done.binary,
            check: Check {
                command: done.check.command,
                output: done.check.output,
            },
        })),
        Err(InstallError::Refused(why)) => Err(JobError::new(ErrorKind::Refused, INSTALL, why)),
        Err(InstallError::Verification(why)) => {
            Err(JobError::new(ErrorKind::Corrupt, INSTALL, why))
        }
        Err(InstallError::Failed(why)) => Err(JobError::new(ErrorKind::Failed, INSTALL, why)),
    }
}
