// SPDX-License-Identifier: GPL-3.0-only
//! The verified fetch: the one routine that gets a pinned tool onto the machine. It fetches only from the URL in the
//! lock, checks the SHA-256 before the file is made executable, checks the attestation against the pinned
//! repository and workflow, and stores the file by content hash. A failed check installs nothing.
//!
//! A third-party tool whose lock entry pins no checksum is refused unless the Instance holds a confirmation made
//! with `nvmod tools trust` for exactly that version.

use crate::lock::{Asset, Kind, ToolEntry, platform};
use crate::store::{Store, StoreState};
use crate::trust::Trust;
use crate::verify::{Attestation, VerifyError, check_sha256, fetch_bytes, verify_attestation};
use crate::{EXIT_REFUSED, EXIT_TOOL_FAILED, EXIT_VERIFICATION, Fetcher};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Why a tool was not acquired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcquireError {
    /// The lock or the trust rules forbid it (exit code 2); nothing was changed.
    Refused(String),
    /// A checksum, an attestation or a response did not check out (exit code 3); nothing was installed.
    Verification(String),
    /// The fetch or the write failed (exit code 4); nothing was installed.
    Failed(String),
}

impl AcquireError {
    /// The exit code of the command that hit this error.
    #[must_use]
    pub fn exit_code(&self) -> u8 {
        match self {
            AcquireError::Refused(_) => EXIT_REFUSED,
            AcquireError::Verification(_) => EXIT_VERIFICATION,
            AcquireError::Failed(_) => EXIT_TOOL_FAILED,
        }
    }
}

impl std::fmt::Display for AcquireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (AcquireError::Refused(why)
        | AcquireError::Verification(why)
        | AcquireError::Failed(why)) = self;
        f.write_str(why)
    }
}

impl std::error::Error for AcquireError {}

impl From<VerifyError> for AcquireError {
    fn from(e: VerifyError) -> AcquireError {
        match e {
            VerifyError::Verification(why) => AcquireError::Verification(why),
            VerifyError::Failed(why) => AcquireError::Failed(why),
        }
    }
}

/// Where a checksum came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pin<'a> {
    /// Pinned in `tools.lock.toml`.
    Lock(&'a str),
    /// Confirmed by the owner with `nvmod tools trust`, recorded in the Instance.
    Trusted(&'a str),
    /// Neither: the tool must not be run.
    None,
}

impl<'a> Pin<'a> {
    /// The checksum, if there is one.
    #[must_use]
    pub fn sha256(self) -> Option<&'a str> {
        match self {
            Pin::Lock(h) | Pin::Trusted(h) => Some(h),
            Pin::None => None,
        }
    }

    /// Where it came from, for messages.
    #[must_use]
    pub fn source(self) -> &'static str {
        match self {
            Pin::Lock(_) => "tools.lock.toml",
            Pin::Trusted(_) => "the owner's `nvmod tools trust` confirmation",
            Pin::None => "nothing",
        }
    }
}

/// The checksum a tool's `asset` must have: the lock's, else the owner's confirmation for this version.
#[must_use]
pub fn pin_of<'a>(entry: &'a ToolEntry, asset: &'a Asset, trust: Option<&'a Trust>) -> Pin<'a> {
    if let Some(hash) = &asset.sha256 {
        return Pin::Lock(hash);
    }
    match trust.and_then(|t| t.find(&entry.name, &entry.version)) {
        Some(hash) => Pin::Trusted(hash),
        None => Pin::None,
    }
}

/// What a fetch did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acquired {
    /// The stored program.
    pub path: PathBuf,
    /// Its SHA-256.
    pub sha256: String,
    /// False when the verified file was already in the store and nothing was fetched.
    pub fetched: bool,
    /// The checks that ran, one line each.
    pub checks: Vec<String>,
}

/// What the fetch needs besides the lock entry.
pub struct Acquire<'a> {
    /// Gets URLs.
    pub fetcher: &'a dyn Fetcher,
    /// The GitHub CLI, for the attestation.
    pub gh: &'a Path,
    /// The wait between retries.
    pub retry_delay: Duration,
    /// Where tools are kept.
    pub store: &'a Store,
    /// The Instance's confirmations, if an Instance was named.
    pub trust: Option<&'a Trust>,
}

fn temp_name(store: &Store, sha256: &str) -> PathBuf {
    store.root().join(format!(
        ".fetching-{}-{}",
        std::process::id(),
        &sha256[..16.min(sha256.len())]
    ))
}

impl Acquire<'_> {
    /// Fetch `entry` for this machine's platform, verify it and store it by content hash.
    ///
    /// # Errors
    /// `Refused` for a linked tool, a platform with no asset, or a tool with neither a pinned nor a confirmed
    /// checksum. `Verification` for a malformed response, a checksum mismatch or a failed attestation: nothing
    /// is stored and the file is not made executable. `Failed` when the fetch (after its retries) or the write fails.
    pub fn fetch(&self, entry: &ToolEntry) -> Result<Acquired, AcquireError> {
        if entry.kind == Kind::Linked {
            return Err(AcquireError::Refused(format!(
                "{} is linked into nvmod, not fetched",
                entry.name
            )));
        }
        let platform = platform();
        let asset = entry.asset_here().ok_or_else(|| {
            AcquireError::Refused(format!(
                "{} {} pins no file for {platform}",
                entry.name, entry.version
            ))
        })?;
        let pin = pin_of(entry, asset, self.trust);
        let Some(expected) = pin.sha256() else {
            return Err(AcquireError::Refused(format!(
                "{} {} has no pinned checksum and no confirmation: check the download yourself, then run \
                 `nvmod tools trust {} {} --instance <dir>`",
                entry.name, entry.version, entry.name, entry.version
            )));
        };
        let mut checks = Vec::new();
        if self.store.state(expected, &entry.name) == StoreState::Verified {
            checks.push(format!(
                "already stored, SHA-256 re-checked against {}",
                pin.source()
            ));
            return Ok(Acquired {
                path: self.store.path(expected, &entry.name),
                sha256: expected.to_ascii_lowercase(),
                fetched: false,
                checks,
            });
        }
        let bytes = fetch_bytes(self.fetcher, &asset.url, self.retry_delay)?;
        check_sha256(&entry.name, pin.source(), &bytes, expected)?;
        checks.push(format!("SHA-256 matches {}", pin.source()));
        // The file is written without the executable bit and only made executable once both checks passed.
        std::fs::create_dir_all(self.store.root())
            .map_err(|e| AcquireError::Failed(format!("{}: {e}", self.store.root().display())))?;
        let temp = temp_name(self.store, expected);
        std::fs::write(&temp, &bytes)
            .map_err(|e| AcquireError::Failed(format!("{}: {e}", temp.display())))?;
        let attested = match &entry.attestation {
            Some(identity) => verify_attestation(
                self.gh,
                &temp,
                &Attestation {
                    repository: &identity.repository,
                    workflow: identity.workflow.as_deref(),
                },
            )
            .map(|()| {
                format!(
                    "attestation names {}{}",
                    identity.repository,
                    identity
                        .workflow
                        .as_ref()
                        .map_or_else(String::new, |w| format!(" workflow {w}"))
                )
            }),
            None => Ok(
                "no attestation pinned (third-party tool); the checksum is the only check"
                    .to_owned(),
            ),
        };
        match attested {
            Ok(line) => checks.push(line),
            Err(e) => {
                let _ = std::fs::remove_file(&temp);
                return Err(e.into());
            }
        }
        let path = self
            .store
            .put(expected, &entry.name, &temp)
            .map_err(|why| {
                let _ = std::fs::remove_file(&temp);
                AcquireError::Failed(why)
            })?;
        Ok(Acquired {
            path,
            sha256: expected.to_ascii_lowercase(),
            fetched: true,
            checks,
        })
    }
}
