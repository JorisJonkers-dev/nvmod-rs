// SPDX-License-Identifier: GPL-3.0-only
//! The Instance skeleton: a folder holding `nvmod-instance.json`, `mods/` and `profiles/`, separate from the base
//! copy it points at.

use crate::Storefront;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::{Path, PathBuf};

/// The file that marks a folder as an nvmod Instance.
pub const INSTANCE_FILE: &str = "nvmod-instance.json";
const SCHEMA: u32 = 1;

/// The contents of `nvmod-instance.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Config {
    /// Version of this file's layout.
    pub schema: u32,
    /// The absolute path of the base copy. Never written to.
    pub base: PathBuf,
    /// The storefront found when the Instance was made.
    pub storefront: Storefront,
}

/// Why an Instance could not be made or opened.
#[derive(Debug, PartialEq, Eq)]
pub enum InstanceError {
    /// The command is refused for a stated reason (exit code 2): nothing was written.
    Refused(String),
    /// The folder is not an Instance, or its file is unreadable.
    Invalid(String),
    /// A write failed; whatever was started has been removed.
    Io(String),
}

impl fmt::Display for InstanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstanceError::Refused(s) | InstanceError::Invalid(s) | InstanceError::Io(s) => {
                f.write_str(s)
            }
        }
    }
}

impl std::error::Error for InstanceError {}

/// An nvmod Instance on disk.
#[derive(Debug, Clone)]
pub struct NvmodInstance {
    /// The Instance folder.
    pub root: PathBuf,
    /// Its configuration.
    pub config: Config,
}

/// `path` made absolute, with the part that exists resolved through symbolic links.
fn resolve(path: &Path) -> PathBuf {
    let abs = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut rest = Vec::new();
    let mut head = abs.as_path();
    loop {
        if let Ok(real) = head.canonicalize() {
            return rest.iter().rev().fold(real, |p, name| p.join(name));
        }
        match (head.parent(), head.file_name()) {
            (Some(parent), Some(name)) => {
                rest.push(name.to_owned());
                head = parent;
            }
            _ => return abs,
        }
    }
}

impl NvmodInstance {
    /// Make an Instance skeleton at `root` for the base copy `base`.
    ///
    /// # Errors
    /// `Refused` when the base is not a folder, is not a Steam or GOG copy, when `root` exists or lies inside the
    /// base copy; `Io` when writing fails (the folder is removed again).
    pub fn create(root: &Path, base: &Path) -> Result<NvmodInstance, InstanceError> {
        if !base.is_dir() {
            return Err(InstanceError::Refused(format!(
                "{} is not a folder",
                base.display()
            )));
        }
        let storefront = Storefront::detect(base);
        if let Some(reason) = storefront.refusal() {
            return Err(InstanceError::Refused(reason.to_owned()));
        }
        let base = resolve(base);
        let root_abs = resolve(root);
        if root_abs.starts_with(&base) {
            return Err(InstanceError::Refused(
                "the Instance would lie inside the base copy; choose another folder".to_owned(),
            ));
        }
        if root.exists() {
            return Err(InstanceError::Refused(format!(
                "{} already exists; nvmod does not overwrite it",
                root.display()
            )));
        }
        let config = Config {
            schema: SCHEMA,
            base,
            storefront,
        };
        let io = |e: std::io::Error| InstanceError::Io(format!("{}: {e}", root.display()));
        let build = || -> Result<(), InstanceError> {
            std::fs::create_dir_all(root_abs.join("mods")).map_err(io)?;
            std::fs::create_dir_all(root_abs.join("profiles")).map_err(io)?;
            let text = serde_json::to_string_pretty(&config)
                .map_err(|e| InstanceError::Io(e.to_string()))?;
            std::fs::write(root_abs.join(INSTANCE_FILE), text + "\n").map_err(io)
        };
        if let Err(why) = build() {
            let _ = std::fs::remove_dir_all(&root_abs);
            return Err(why);
        }
        Ok(NvmodInstance {
            root: root_abs,
            config,
        })
    }

    /// The folder of the mod `name`.
    #[must_use]
    pub fn mod_dir(&self, name: &str) -> PathBuf {
        self.root.join("mods").join(name)
    }

    /// The folder of the profile `name`.
    #[must_use]
    pub fn profile_dir(&self, name: &str) -> PathBuf {
        self.root.join("profiles").join(name)
    }

    /// The folder that holds the Staging tree of the profile `name` (its `data` folder is the tree).
    #[must_use]
    pub fn staging_dir(&self, name: &str) -> PathBuf {
        self.root.join("staging").join(name)
    }

    /// The names of the profile folders, sorted.
    #[must_use]
    pub fn profiles(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(self.root.join("profiles"))
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.path().is_dir())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// The profile a command works on: `wanted` when given and present, else the only profile there is.
    ///
    /// # Errors
    /// `Refused` when `wanted` names no profile, when there is none, or when there are several and none is named.
    pub fn resolve_profile(&self, wanted: Option<&str>) -> Result<String, InstanceError> {
        let names = self.profiles();
        match (wanted, names.as_slice()) {
            (Some(name), _) if names.iter().any(|n| n == name) => Ok(name.to_owned()),
            (Some(name), _) => Err(InstanceError::Refused(format!(
                "the Instance has no profile `{name}`"
            ))),
            (None, [only]) => Ok(only.clone()),
            (None, []) => Err(InstanceError::Refused(
                "the Instance has no profile yet".to_owned(),
            )),
            (None, _) => Err(InstanceError::Refused(format!(
                "the Instance has several profiles ({}); name one with --profile",
                names.join(", ")
            ))),
        }
    }

    /// Whether `root` holds an Instance file.
    #[must_use]
    pub fn is_instance(root: &Path) -> bool {
        root.join(INSTANCE_FILE).is_file()
    }

    /// Open the Instance at `root`.
    ///
    /// # Errors
    /// `Invalid` when the Instance file is missing, unreadable or of another schema.
    pub fn open(root: &Path) -> Result<NvmodInstance, InstanceError> {
        let file = root.join(INSTANCE_FILE);
        let text = std::fs::read_to_string(&file)
            .map_err(|e| InstanceError::Invalid(format!("{}: {e}", file.display())))?;
        let config: Config = serde_json::from_str(&text)
            .map_err(|e| InstanceError::Invalid(format!("{}: {e}", file.display())))?;
        if config.schema != SCHEMA {
            return Err(InstanceError::Invalid(format!(
                "{}: schema {} is not supported (this nvmod reads schema {SCHEMA})",
                file.display(),
                config.schema
            )));
        }
        Ok(NvmodInstance {
            root: root.to_path_buf(),
            config,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{BaseBuilder, TempDir};

    #[test]
    fn create_then_open_round_trips() {
        let base = TempDir::new("inst-base");
        BaseBuilder::gog().write(base.path());
        let scratch = TempDir::new("inst");
        let made = NvmodInstance::create(&scratch.path().join("i"), base.path()).unwrap();
        assert!(NvmodInstance::is_instance(&made.root));
        let opened = NvmodInstance::open(&made.root).unwrap();
        assert_eq!(opened.config, made.config);
        assert_eq!(opened.config.storefront, Storefront::Gog);
    }

    #[test]
    fn open_rejects_a_missing_garbled_or_future_file() {
        let dir = TempDir::new("inst-bad");
        assert!(matches!(
            NvmodInstance::open(dir.path()),
            Err(InstanceError::Invalid(_))
        ));
        std::fs::write(dir.path().join(INSTANCE_FILE), "{").unwrap();
        assert!(NvmodInstance::open(dir.path()).is_err());
        std::fs::write(
            dir.path().join(INSTANCE_FILE),
            r#"{"schema":9,"base":"/x","storefront":"steam"}"#,
        )
        .unwrap();
        let why = NvmodInstance::open(dir.path()).unwrap_err().to_string();
        assert!(why.contains("schema 9"), "{why}");
    }

    #[test]
    fn the_profile_is_the_named_one_or_the_only_one() {
        let base = TempDir::new("inst-prof-base");
        BaseBuilder::steam().write(base.path());
        let scratch = TempDir::new("inst-prof");
        let i = NvmodInstance::create(&scratch.path().join("i"), base.path()).unwrap();
        assert!(i.resolve_profile(None).is_err());
        std::fs::create_dir(i.profile_dir("A")).unwrap();
        assert_eq!(i.resolve_profile(None).unwrap(), "A");
        assert_eq!(i.resolve_profile(Some("A")).unwrap(), "A");
        assert!(i.resolve_profile(Some("B")).is_err());
        std::fs::create_dir(i.profile_dir("B")).unwrap();
        let why = i.resolve_profile(None).unwrap_err().to_string();
        assert!(why.contains("A, B"), "{why}");
        assert_eq!(i.profiles(), ["A", "B"]);
        assert!(i.mod_dir("M").ends_with("mods/M") && i.staging_dir("A").ends_with("staging/A"));
    }

    #[test]
    fn create_refuses_a_base_that_is_not_a_folder() {
        let dir = TempDir::new("inst-nofolder");
        let why =
            NvmodInstance::create(&dir.path().join("i"), &dir.path().join("none")).unwrap_err();
        assert!(matches!(why, InstanceError::Refused(_)));
    }

    #[test]
    fn a_failed_write_removes_what_was_started() {
        let base = TempDir::new("inst-base2");
        BaseBuilder::steam().write(base.path());
        let scratch = TempDir::new("inst2");
        // A file where the Instance folder's parent should be: creating folders fails.
        std::fs::write(scratch.path().join("blocker"), b"x").unwrap();
        let root = scratch.path().join("blocker").join("i");
        let why = NvmodInstance::create(&root, base.path()).unwrap_err();
        assert!(matches!(why, InstanceError::Io(_)), "{why}");
    }
}
