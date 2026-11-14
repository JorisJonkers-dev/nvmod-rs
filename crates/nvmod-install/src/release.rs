// SPDX-License-Identifier: GPL-3.0-only
//! Fetching the fork's release, checking its checksum and attestation, and unpacking it.

use crate::install::InstallError;
use nvmod_tools::{
    Attestation, Fetcher, VerifyError, check_sha256, fetch_bytes, verify_attestation,
};
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// Where a release is fetched from and how it is checked.
pub(crate) struct Source<'a> {
    pub fetcher: &'a dyn Fetcher,
    pub base_url: &'a str,
    pub asset: &'a str,
    pub repo: &'a str,
    pub gh: &'a Path,
    pub delay: Duration,
}

fn to_install(e: VerifyError) -> InstallError {
    match e {
        VerifyError::Verification(why) => InstallError::Verification(why),
        VerifyError::Failed(why) => InstallError::Failed(why),
    }
}

fn fetch(src: &Source, name: &str) -> Result<Vec<u8>, InstallError> {
    let url = format!("{}/{name}", src.base_url.trim_end_matches('/'));
    fetch_bytes(src.fetcher, &url, src.delay).map_err(to_install)
}

/// The hash `SHA256SUMS` lists for `asset`.
fn listed_hash(sums: &[u8], asset: &str) -> Result<String, InstallError> {
    let malformed = |why: &str| InstallError::Verification(format!("malformed SHA256SUMS: {why}"));
    let text = std::str::from_utf8(sums).map_err(|_| malformed("not text"))?;
    let mut found = None;
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let (hash, name) = line.split_once(' ').ok_or_else(|| malformed(line))?;
        let name = name.trim_start_matches([' ', '*']);
        let valid = hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit());
        if !valid || name.is_empty() {
            return Err(malformed(line));
        }
        if name == asset {
            found = Some(hash.to_ascii_lowercase());
        }
    }
    found.ok_or_else(|| {
        InstallError::Verification(format!(
            "SHA256SUMS does not list {asset}; nothing installed"
        ))
    })
}

/// Fetch the asset into `dir`, verify its checksum, then its attestation with `gh`. Returns the asset's path.
pub(crate) fn fetch_verified(src: &Source, dir: &Path) -> Result<PathBuf, InstallError> {
    let sums = fetch(src, "SHA256SUMS")?;
    let listed = listed_hash(&sums, src.asset)?;
    let bytes = fetch(src, src.asset)?;
    check_sha256(src.asset, "SHA256SUMS", &bytes, &listed).map_err(to_install)?;
    let path = dir.join(src.asset);
    std::fs::write(&path, &bytes)
        .map_err(|e| InstallError::Failed(format!("{}: {e}", path.display())))?;
    let identity = Attestation {
        repository: src.repo,
        workflow: None,
    };
    verify_attestation(src.gh, &path, &identity).map_err(to_install)?;
    Ok(path)
}

fn tar(args: &[&str], archive: &Path, into: Option<&Path>) -> Result<String, InstallError> {
    let mut cmd = Command::new("tar");
    cmd.args(args).arg(archive);
    if let Some(dir) = into {
        cmd.arg("-C").arg(dir);
    }
    let out = cmd
        .output()
        .map_err(|e| InstallError::Failed(format!("cannot run tar: {e}")))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(InstallError::Failed(format!(
            "tar failed on {}: {}",
            archive.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )))
    }
}

/// Unpack a verified `.tar.gz` into `into` after checking that no member leaves the folder and none is a link.
pub(crate) fn extract(archive: &Path, into: &Path) -> Result<(), InstallError> {
    let names = tar(&["-tzf"], archive, None)?;
    for name in names.lines() {
        let path = Path::new(name);
        let bad = path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, Component::ParentDir | Component::Prefix(_)));
        if bad {
            return Err(InstallError::Verification(format!(
                "archive member {name} lies outside the extraction folder"
            )));
        }
    }
    let kinds = tar(&["-tvzf"], archive, None)?;
    if let Some(line) = kinds
        .lines()
        .find(|l| !matches!(l.chars().next(), Some('-' | 'd')))
    {
        return Err(InstallError::Verification(format!(
            "archive member is not a plain file or folder: {line}"
        )));
    }
    std::fs::create_dir_all(into).map_err(|e| InstallError::Failed(e.to_string()))?;
    tar(&["-xzf"], archive, Some(into)).map(|_| ())
}

/// The first file called `name` under `dir`.
pub(crate) fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const HASH: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn the_listed_hash_is_found_in_either_checksum_style() {
        let text = format!("{HASH}  a.tgz\n{HASH} *b.tgz\n\n");
        assert_eq!(listed_hash(text.as_bytes(), "a.tgz").unwrap(), HASH);
        assert_eq!(listed_hash(text.as_bytes(), "b.tgz").unwrap(), HASH);
    }

    #[test]
    fn malformed_lists_and_unlisted_assets_fail_verification() {
        for bad in ["nonsense", "zz  a.tgz", &format!("{HASH}  ")] {
            let why = listed_hash(bad.as_bytes(), "a.tgz")
                .unwrap_err()
                .to_string();
            assert!(why.contains("malformed"), "{why}");
        }
        assert!(listed_hash(&[0xff, 0xfe], "a").is_err());
        assert!(listed_hash(format!("{HASH}  b.tgz").as_bytes(), "a.tgz").is_err());
    }
}
