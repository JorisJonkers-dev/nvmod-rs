// SPDX-License-Identifier: GPL-3.0-only
//! Where the game looks for its INIs, saves and plugin lists: found by reading the prefix layout, never hard
//! coded. On Linux that is a Proton or Wine prefix; the Windows backend builds a [`Target`] from the real folders
//! with [`Target::new`] and the rest of the crate does not change.

use std::path::{Path, PathBuf};

/// The two folders profile files are placed in, and the root nothing may be written outside of.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    /// The prefix as named (the root under which `nvmod profile apply` may write).
    pub root: PathBuf,
    /// `Documents/My Games/FalloutNV`: the game INIs and the save folder.
    pub my_games: PathBuf,
    /// `AppData/Local/FalloutNV`: `plugins.txt` and `loadorder.txt`.
    pub app_data: PathBuf,
}

impl Target {
    /// A target from folders the caller has already found (the Windows backend).
    #[must_use]
    pub fn new(root: PathBuf, my_games: PathBuf, app_data: PathBuf) -> Target {
        Target {
            root,
            my_games,
            app_data,
        }
    }

    /// Find the folders in the prefix `dir`: Proton keeps the Wine prefix in `pfx` (`dir` may name either), the
    /// user is `steamuser` (else the one user folder that is not `Public`). The game folders themselves are made
    /// by the first apply when they are missing.
    ///
    /// # Errors
    /// A reason, when `dir` holds no `drive_c/users` or no user folder.
    pub fn from_prefix(dir: &Path) -> Result<Target, String> {
        let wine = [dir.join("pfx"), dir.to_path_buf()]
            .into_iter()
            .find(|c| c.join("drive_c").join("users").is_dir())
            .ok_or_else(|| {
                format!(
                    "{} is not a Wine or Proton prefix (no drive_c/users, also not under pfx)",
                    dir.display()
                )
            })?;
        let users = wine.join("drive_c").join("users");
        let user = if users.join("steamuser").is_dir() {
            users.join("steamuser")
        } else {
            let mut names: Vec<PathBuf> = std::fs::read_dir(&users)
                .into_iter()
                .flatten()
                .flatten()
                .filter(|e| e.path().is_dir())
                .filter(|e| {
                    let n = e.file_name().to_string_lossy().to_ascii_lowercase();
                    !["public", "default", "default user", "all users"].contains(&n.as_str())
                })
                .map(|e| e.path())
                .collect();
            names.sort();
            names
                .into_iter()
                .next()
                .ok_or_else(|| format!("{} has no user folder", users.display()))?
        };
        Ok(Target {
            root: dir.to_path_buf(),
            my_games: user.join("Documents").join("My Games").join("FalloutNV"),
            app_data: user.join("AppData").join("Local").join("FalloutNV"),
        })
    }
}
