// SPDX-License-Identifier: GPL-3.0-only
//! The tool store: executables kept under their own SHA-256, `<store>/<sha256>/<name>`. A new pin has a new hash and
//! so a new folder; a tool that is in use is never overwritten. Each file is re-hashed whenever it is looked at.

use crate::sha256_hex;
use std::path::{Path, PathBuf};

/// What a stored file looks like when checked.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreState {
    /// Nothing is stored under that hash.
    Missing,
    /// The file is there and has the hash it is stored under.
    Verified,
    /// The file is there but its content hashes to something else (carrying the hash found).
    Corrupt(String),
}

/// A folder of tools stored by content hash.
#[derive(Debug, Clone)]
pub struct Store {
    root: PathBuf,
}

/// The file name of the program of a tool on this platform.
#[must_use]
pub fn exe_file(name: &str) -> String {
    format!("{name}{}", std::env::consts::EXE_SUFFIX)
}

impl Store {
    /// A store in `root` (made when first written to).
    #[must_use]
    pub fn new(root: PathBuf) -> Store {
        Store { root }
    }

    /// The default place: `NVMOD_TOOLS_DIR`, else the user's data folder under `nvmod/tools`.
    #[must_use]
    pub fn default_location() -> PathBuf {
        let var = |name: &str| {
            std::env::var_os(name)
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        if let Some(dir) = var("NVMOD_TOOLS_DIR") {
            return dir;
        }
        let data = var("XDG_DATA_HOME")
            .or_else(|| var("LOCALAPPDATA"))
            .or_else(|| var("HOME").map(|h| h.join(".local").join("share")))
            .unwrap_or_else(|| PathBuf::from("."));
        data.join("nvmod").join("tools")
    }

    /// The store's folder.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the tool `name` with content hash `sha256` is (or would be) stored.
    #[must_use]
    pub fn path(&self, sha256: &str, name: &str) -> PathBuf {
        self.root
            .join(sha256.to_ascii_lowercase())
            .join(exe_file(name))
    }

    /// Re-hash the stored file.
    #[must_use]
    pub fn state(&self, sha256: &str, name: &str) -> StoreState {
        match std::fs::read(self.path(sha256, name)) {
            Err(_) => StoreState::Missing,
            Ok(bytes) => {
                let actual = sha256_hex(&bytes);
                if actual.eq_ignore_ascii_case(sha256) {
                    StoreState::Verified
                } else {
                    StoreState::Corrupt(actual)
                }
            }
        }
    }

    /// Move the already verified file `temp` into the store under `sha256` and make it executable. A tool already
    /// stored under that hash is left alone.
    ///
    /// # Errors
    /// A reason, when the folder, the permission change or the move fails.
    pub fn put(&self, sha256: &str, name: &str, temp: &Path) -> Result<PathBuf, String> {
        let target = self.path(sha256, name);
        let io = |e: std::io::Error| format!("{}: {e}", target.display());
        if self.state(sha256, name) == StoreState::Verified {
            let _ = std::fs::remove_file(temp);
            return Ok(target);
        }
        std::fs::create_dir_all(target.parent().unwrap_or(&self.root)).map_err(io)?;
        make_executable(temp).map_err(io)?;
        std::fs::rename(temp, &target).map_err(io)?;
        Ok(target)
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> std::io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::TempDir;

    #[test]
    fn a_tool_is_stored_under_its_hash_and_checked_each_time() {
        let dir = TempDir::new("store");
        let store = Store::new(dir.path().join("tools"));
        let hash = sha256_hex(b"tool");
        assert_eq!(store.state(&hash, "t"), StoreState::Missing);
        let temp = dir.path().join("t.tmp");
        std::fs::write(&temp, b"tool").unwrap();
        let at = store.put(&hash, "t", &temp).unwrap();
        assert!(at.ends_with(format!("{hash}/{}", exe_file("t"))));
        assert!(!temp.exists());
        assert_eq!(store.state(&hash, "t"), StoreState::Verified);
        std::fs::write(&at, b"changed").unwrap();
        assert_eq!(
            store.state(&hash, "t"),
            StoreState::Corrupt(sha256_hex(b"changed"))
        );
    }

    #[test]
    fn a_second_pin_does_not_touch_the_first_tool() {
        let dir = TempDir::new("store-two");
        let store = Store::new(dir.path().join("tools"));
        let (h1, h2) = (sha256_hex(b"one"), sha256_hex(b"two"));
        for (hash, body) in [(&h1, "one"), (&h2, "two")] {
            let temp = dir.path().join("tmp");
            std::fs::write(&temp, body).unwrap();
            store.put(hash, "t", &temp).unwrap();
        }
        assert_eq!(std::fs::read(store.path(&h1, "t")).unwrap(), b"one");
        assert_eq!(std::fs::read(store.path(&h2, "t")).unwrap(), b"two");
        assert_ne!(store.path(&h1, "t"), store.path(&h2, "t"));
    }

    #[test]
    fn storing_what_is_already_there_leaves_it_alone() {
        let dir = TempDir::new("store-same");
        let store = Store::new(dir.path().join("tools"));
        let hash = sha256_hex(b"x");
        for _ in 0..2 {
            let temp = dir.path().join("tmp");
            std::fs::write(&temp, b"x").unwrap();
            store.put(&hash, "t", &temp).unwrap();
            assert!(!temp.exists());
        }
    }

    #[test]
    fn the_default_location_follows_the_environment() {
        // Only the shape is checked; the variables are the process's own.
        assert!(Store::default_location().ends_with("tools"));
    }

    #[cfg(unix)]
    #[test]
    fn a_stored_tool_is_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("store-exec");
        let store = Store::new(dir.path().join("tools"));
        let hash = sha256_hex(b"x");
        let temp = dir.path().join("tmp");
        std::fs::write(&temp, b"x").unwrap();
        assert_eq!(
            std::fs::metadata(&temp).unwrap().permissions().mode() & 0o111,
            0
        );
        let at = store.put(&hash, "t", &temp).unwrap();
        assert_eq!(
            std::fs::metadata(at).unwrap().permissions().mode() & 0o111,
            0o111
        );
    }
}
