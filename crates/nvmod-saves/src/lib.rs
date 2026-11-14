// SPDX-License-Identifier: GPL-3.0-only
//! Saves are never damaged (requirement R4). The save folder of a profile is its own `saves/` with
//! `LocalSaves=true`, else the prefix's `My Games/FalloutNV/Saves`. This crate lists the saves (the header read is
//! bounded and read only), backs the folder up with a manifest of hashes, restores a backup byte for byte,
//! transfers saves between profiles with their `.nvse` co-saves, checks the plugin list of each save against the
//! profile, and guards the folders across a run. It reads and writes only the save folders and
//! `.nvmod/saves-backup/` (and the profile's `plugins.txt`, which `check` reads).

mod backup;
mod check;
mod guard;
mod list;
mod transfer;

use nvmod_instance::NvmodInstance;
use nvmod_profile::{Settings, Target};
use std::path::{Path, PathBuf};

pub use backup::{BackupReport, RestoreReport, backup, restore};
pub use check::{CheckReport, SaveCheck, check};
pub use guard::{Change, Guard, GuardReport, checksums};
pub use list::{SaveInfo, list};
pub use transfer::{TransferReport, transfer};

/// Why a command did nothing (or stopped).
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Refused before anything was changed (exit code 2).
    Refused(String),
    /// A backup does not match its manifest (exit code 3); nothing was changed.
    Corrupt(String),
    /// A step failed and what was started has been removed (exit code 4).
    Failed(String),
}

/// Where the saves of a profile are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    /// The save folder (it may not exist yet).
    pub dir: PathBuf,
    /// True when it is the profile's own `saves/` folder (`LocalSaves=true`).
    pub local: bool,
}

/// The save folder of `profile`: its `saves/` with local saves on, else `Saves` under `My Games/FalloutNV` of
/// `target`.
///
/// # Errors
/// `Refused` when the profile does not exist, or it uses the shared folder and no prefix was found.
pub fn locate(
    instance: &NvmodInstance,
    profile: &str,
    target: Option<&Target>,
) -> Result<Location, Error> {
    let dir = instance.profile_dir(profile);
    if !dir.is_dir() {
        return Err(Error::Refused(format!(
            "the Instance has no profile `{profile}`"
        )));
    }
    if Settings::read(&dir).local_saves {
        return Ok(Location {
            dir: dir.join("saves"),
            local: true,
        });
    }
    let target = target.ok_or_else(|| {
        Error::Refused(format!(
            "profile `{profile}` keeps its saves in the prefix, and no prefix was found (give --prefix)"
        ))
    })?;
    Ok(Location {
        dir: target.my_games.join("Saves"),
        local: false,
    })
}

pub(crate) fn io(path: &Path, e: &std::io::Error) -> String {
    format!("{}: {e}", path.display())
}

/// The `.fos` saves of a folder, sorted by name (a folder that is not there has none). Links are skipped.
pub(crate) fn save_files(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out: Vec<(String, PathBuf)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter(|e| {
            Path::new(&e.file_name())
                .extension()
                .is_some_and(|x| x.eq_ignore_ascii_case("fos"))
        })
        .map(|e| (e.file_name().to_string_lossy().into_owned(), e.path()))
        .collect();
    out.sort();
    out
}

/// The name of the co-save of `save` (`X.fos` has `X.nvse`).
pub(crate) fn cosave_name(save: &str) -> String {
    let stem = Path::new(save)
        .file_stem()
        .map_or_else(|| save.to_owned(), |s| s.to_string_lossy().into_owned());
    format!("{stem}.nvse")
}

/// A digest as lowercase hex.
pub(crate) fn to_hex(digest: &[u8]) -> String {
    use std::fmt::Write;
    digest.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// The size and SHA-256 of the file at `path`.
pub(crate) fn hash_file(path: &Path) -> Result<(u64, String), String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = std::fs::File::open(path).map_err(|e| io(path, &e))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 64 * 1024];
    let mut size = 0_u64;
    loop {
        let n = file.read(&mut buffer).map_err(|e| io(path, &e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
        size += n as u64;
    }
    Ok((size, to_hex(&hasher.finalize())))
}

/// Copy `from` to `to` through a temporary file in the same folder and a rename.
pub(crate) fn copy_atomic(from: &Path, to: &Path) -> Result<(u64, String), String> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).map_err(|e| io(parent, &e))?;
    }
    let mut tmp = to.as_os_str().to_owned();
    tmp.push(".nvmod-tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::copy(from, &tmp).map_err(|e| io(from, &e))?;
    let sum = hash_file(&tmp).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    std::fs::rename(&tmp, to).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        io(to, &e)
    })?;
    Ok(sum)
}
