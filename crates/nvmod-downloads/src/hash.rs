// SPDX-License-Identifier: GPL-3.0-only
//! SHA-256 and MD5 of an archive, with a cache keyed by file name, size and modification time so a second listing
//! does not read the files again. The cache is a convenience: it can be deleted, and a file that cannot be written
//! is not an error.

use crate::Error;
use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// Both digests of one file, as lower-case hex.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Digests {
    /// 32 hex digits.
    pub md5: String,
    /// 64 hex digits.
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    size: u64,
    mtime_secs: u64,
    mtime_nanos: u32,
    #[serde(flatten)]
    digests: Digests,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    entries: BTreeMap<String, Entry>,
}

/// The hash cache of one Instance (`.nvmod/downloads-hashes.json`).
#[derive(Debug, Default)]
pub struct HashCache {
    path: Option<PathBuf>,
    entries: BTreeMap<String, Entry>,
    dirty: bool,
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

fn parts(modified: Option<SystemTime>) -> (u64, u32) {
    modified
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or((0, 0), |d| (d.as_secs(), d.subsec_nanos()))
}

impl HashCache {
    /// A cache kept in `path`; an unreadable or malformed file is an empty cache. `None` keeps nothing.
    #[must_use]
    pub fn open(path: Option<&Path>) -> HashCache {
        let entries = path
            .and_then(|p| std::fs::read(p).ok())
            .and_then(|b| serde_json::from_slice::<File>(&b).ok())
            .map_or_default(|f| f.entries);
        HashCache {
            path: path.map(Path::to_path_buf),
            entries,
            dirty: false,
        }
    }

    /// Forget the entries of files that are not in `names`.
    pub fn retain_only(&mut self, names: &std::collections::HashSet<&str>) {
        let before = self.entries.len();
        self.entries.retain(|k, _| names.contains(k.as_str()));
        self.dirty |= self.entries.len() != before;
    }

    /// The digests of `file` (named `name`, `size` bytes, last changed `modified`): from the cache when the size
    /// and time still match, else by reading the file, checking `cancelled` between blocks.
    ///
    /// # Errors
    /// [`Error::Cancelled`], or [`Error::Failed`] when the file cannot be read.
    pub fn digests(
        &mut self,
        name: &str,
        file: &Path,
        size: u64,
        modified: Option<SystemTime>,
        cancelled: &dyn Fn() -> bool,
    ) -> Result<Digests, Error> {
        let (secs, nanos) = parts(modified);
        if let Some(e) = self.entries.get(name)
            && e.size == size
            && e.mtime_secs == secs
            && e.mtime_nanos == nanos
        {
            return Ok(e.digests.clone());
        }
        let digests = compute(file, cancelled)?;
        self.entries.insert(
            name.to_owned(),
            Entry {
                size,
                mtime_secs: secs,
                mtime_nanos: nanos,
                digests: digests.clone(),
            },
        );
        self.dirty = true;
        Ok(digests)
    }

    /// Write the cache when it changed. A failure is ignored: the next call hashes again.
    pub fn save(&mut self) {
        let Some(path) = self.path.as_deref().filter(|_| self.dirty) else {
            return;
        };
        let file = File {
            entries: std::mem::take(&mut self.entries),
        };
        if let Ok(mut text) = serde_json::to_string(&file) {
            text.push('\n');
            let tmp = path.with_extension("nvmod-tmp");
            let wrote = path
                .parent()
                .is_none_or(|p| std::fs::create_dir_all(p).is_ok())
                && std::fs::write(&tmp, text).is_ok()
                && std::fs::rename(&tmp, path).is_ok();
            if !wrote {
                let _ = std::fs::remove_file(&tmp);
            }
        }
        self.entries = file.entries;
        self.dirty = false;
    }
}

fn compute(path: &Path, cancelled: &dyn Fn() -> bool) -> Result<Digests, Error> {
    let io = |e: std::io::Error| Error::Failed(format!("{}: {e}", path.display()));
    let mut file = std::fs::File::open(path).map_err(io)?;
    let mut sha = Sha256::new();
    let mut md = Md5::new();
    let mut block = vec![0_u8; 1 << 20];
    loop {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let n = file.read(&mut block).map_err(io)?;
        if n == 0 {
            break;
        }
        sha.update(&block[..n]);
        md.update(&block[..n]);
    }
    Ok(Digests {
        md5: hex(&md.finalize()),
        sha256: hex(&sha.finalize()),
    })
}

/// Both digests of `data`, for tests and callers that hold the bytes.
#[must_use]
pub fn digests_of(data: &[u8]) -> Digests {
    Digests {
        md5: hex(&Md5::digest(data)),
        sha256: hex(&Sha256::digest(data)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_digests_of_abc() {
        let d = digests_of(b"abc");
        assert_eq!(d.md5, "900150983cd24fb0d6963f7d28e17f72");
        assert_eq!(
            d.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
