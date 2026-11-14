// SPDX-License-Identifier: GPL-3.0-only
//! The checks that decide whether a downloaded tool is trusted, in one place: the response must be well formed,
//! its SHA-256 must be the pinned one, and its build attestation must name the pinned repository and workflow. The
//! attestation is checked through the `gh attestation verify` command: no maintained in-process verifier was
//! adopted (docs/DECISIONS.md), and a missing `gh` fails closed.

use crate::fetch::{FetchError, Fetcher, get_with_retries};
use crate::sha256_hex;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

/// Why a download was not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    /// A checksum, an attestation or a response did not check out (exit code 3); nothing was installed.
    Verification(String),
    /// The fetch itself failed, after the retries (exit code 4).
    Failed(String),
}

impl std::fmt::Display for VerifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (VerifyError::Verification(why) | VerifyError::Failed(why)) = self;
        f.write_str(why)
    }
}

impl std::error::Error for VerifyError {}

/// Fetch `url` with the timeout and retries of [`get_with_retries`]. A malformed response is a verification
/// failure and is not retried; a network failure is a failed step.
///
/// # Errors
/// See [`VerifyError`].
pub fn fetch_bytes(
    fetcher: &dyn Fetcher,
    url: &str,
    delay: Duration,
) -> Result<Vec<u8>, VerifyError> {
    get_with_retries(fetcher, url, delay).map_err(|e| match e {
        FetchError::Malformed(why) => {
            VerifyError::Verification(format!("malformed response: {why}"))
        }
        FetchError::Transient(why) | FetchError::Fatal(why) => {
            VerifyError::Failed(format!("fetch failed: {why}"))
        }
    })
}

/// Check that `bytes` has the SHA-256 `expected` (hex, either case). `name` and `source` say what was checked and
/// where the expected value came from, for the message.
///
/// # Errors
/// `Verification` with both hashes when they differ.
pub fn check_sha256(
    name: &str,
    source: &str,
    bytes: &[u8],
    expected: &str,
) -> Result<(), VerifyError> {
    let actual = sha256_hex(bytes);
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(VerifyError::Verification(format!(
            "checksum mismatch for {name}: {source} says {expected}, the download is {actual}"
        )))
    }
}

/// The identity a build attestation must name.
#[derive(Debug, Clone, Copy)]
pub struct Attestation<'a> {
    /// The repository, `owner/name`.
    pub repository: &'a str,
    /// The workflow file in that repository that built it, such as `.github/workflows/release.yml`; `None`
    /// checks the repository only.
    pub workflow: Option<&'a str>,
}

/// Verify the attestation of `file` with `gh attestation verify`, against the repository and workflow.
///
/// # Errors
/// `Verification` when `gh` cannot be run (the check fails closed, with a message that says so) or when it rejects
/// the file, which includes a file with no attestation.
pub fn verify_attestation(
    gh: &Path,
    file: &Path,
    identity: &Attestation,
) -> Result<(), VerifyError> {
    let mut command = Command::new(gh);
    command
        .args(["attestation", "verify"])
        .arg(file)
        .args(["--repo", identity.repository]);
    if let Some(workflow) = identity.workflow {
        command
            .arg("--signer-workflow")
            .arg(format!("{}/{workflow}", identity.repository));
    }
    let name = file
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    let out = command.output().map_err(|e| {
        VerifyError::Verification(format!(
            "attestation could not be checked, cannot run {} (install the GitHub CLI, `gh`, or pass its path): {e}",
            gh.display()
        ))
    })?;
    if out.status.success() {
        Ok(())
    } else {
        Err(VerifyError::Verification(format!(
            "attestation verification failed for {name} (a missing attestation counts as failed): {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_checksum_is_compared_ignoring_case_and_a_mismatch_shows_both() {
        let abc = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
        assert!(check_sha256("a", "the lock", b"abc", abc).is_ok());
        assert!(check_sha256("a", "the lock", b"abc", &abc.to_ascii_uppercase()).is_ok());
        let why = check_sha256("a.bin", "the lock", b"abd", abc)
            .unwrap_err()
            .to_string();
        assert!(
            why.contains("a.bin") && why.contains("the lock says") && why.contains(abc),
            "{why}"
        );
    }

    #[test]
    fn a_missing_gh_fails_closed_and_says_how_to_fix_it() {
        let why = verify_attestation(
            Path::new("/nonexistent/gh"),
            Path::new("tool"),
            &Attestation {
                repository: "o/r",
                workflow: None,
            },
        )
        .unwrap_err();
        assert!(
            matches!(&why, VerifyError::Verification(m) if m.contains("GitHub CLI")),
            "{why}"
        );
    }
}
