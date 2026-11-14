// SPDX-License-Identifier: GPL-3.0-only
//! `tools.trust.toml` in an Instance: the checksums an owner confirmed with `nvmod tools trust` for third-party tools
//! that the lock file pins no checksum for. A confirmation names one tool and one version; another version needs
//! its own.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The file name inside the Instance folder.
pub const TRUST_FILE: &str = "tools.trust.toml";

/// One confirmed checksum.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Confirmed {
    /// The tool's name.
    pub tool: String,
    /// The version the checksum belongs to.
    pub version: String,
    /// The SHA-256 the owner saw and confirmed.
    pub sha256: String,
}

/// The confirmations of one Instance.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trust {
    /// Layout version of this file.
    #[serde(default = "one")]
    pub schema_version: u32,
    /// The confirmations.
    #[serde(default, rename = "confirmed")]
    pub confirmed: Vec<Confirmed>,
}

fn one() -> u32 {
    1
}

impl Trust {
    /// The file in the Instance folder `root`.
    #[must_use]
    pub fn path(root: &Path) -> PathBuf {
        root.join(TRUST_FILE)
    }

    /// Read the confirmations of the Instance at `root`; none when the file does not exist.
    ///
    /// # Errors
    /// A reason, when the file exists but cannot be read or is not valid.
    pub fn load(root: &Path) -> Result<Trust, String> {
        let path = Trust::path(root);
        match std::fs::read_to_string(&path) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Trust {
                schema_version: 1,
                confirmed: Vec::new(),
            }),
            Err(e) => Err(format!("{}: {e}", path.display())),
            Ok(text) => {
                let trust: Trust =
                    toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
                if trust.schema_version == 1 {
                    Ok(trust)
                } else {
                    Err(format!(
                        "{}: schema {} is not known to this nvmod",
                        path.display(),
                        trust.schema_version
                    ))
                }
            }
        }
    }

    /// The checksum confirmed for `tool` at `version`.
    #[must_use]
    pub fn find(&self, tool: &str, version: &str) -> Option<&str> {
        self.confirmed
            .iter()
            .find(|c| c.tool == tool && c.version == version)
            .map(|c| c.sha256.as_str())
    }

    /// Record a confirmation (replacing an earlier one for the same tool and version) and write the file.
    ///
    /// # Errors
    /// A reason, when the file cannot be written.
    pub fn record(
        &mut self,
        root: &Path,
        tool: &str,
        version: &str,
        sha256: &str,
    ) -> Result<(), String> {
        self.confirmed
            .retain(|c| !(c.tool == tool && c.version == version));
        self.confirmed.push(Confirmed {
            tool: tool.to_owned(),
            version: version.to_owned(),
            sha256: sha256.to_ascii_lowercase(),
        });
        let path = Trust::path(root);
        let text = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("nvmod-tmp");
        std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &path).map_err(|e| format!("{}: {e}", path.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::TempDir;

    #[test]
    fn a_confirmation_is_per_tool_and_version_and_survives_a_reload() {
        let dir = TempDir::new("trust");
        let mut trust = Trust::load(dir.path()).unwrap();
        assert_eq!(trust.confirmed.len(), 0);
        trust
            .record(dir.path(), "ttw", "1.0", &"AB".repeat(32))
            .unwrap();
        let again = Trust::load(dir.path()).unwrap();
        assert_eq!(again.find("ttw", "1.0"), Some("ab".repeat(32).as_str()));
        assert_eq!(again.find("ttw", "1.1"), None);
        assert_eq!(again.find("other", "1.0"), None);
        trust
            .record(dir.path(), "ttw", "1.0", &"cd".repeat(32))
            .unwrap();
        assert_eq!(Trust::load(dir.path()).unwrap().confirmed.len(), 1);
    }

    #[test]
    fn a_broken_or_future_file_is_reported() {
        let dir = TempDir::new("trust-bad");
        std::fs::write(Trust::path(dir.path()), "confirmed = 5").unwrap();
        assert!(Trust::load(dir.path()).is_err());
        std::fs::write(Trust::path(dir.path()), "schema_version = 7\n").unwrap();
        assert!(Trust::load(dir.path()).unwrap_err().contains("schema 7"));
    }
}
