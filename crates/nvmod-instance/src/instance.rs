// SPDX-License-Identifier: GPL-3.0-only
//! The Instance skeleton: a folder holding `nvmod-instance.json`, `mods/` and `profiles/`, separate from the base
//! copy it points at.

use crate::detect_storefront;
use nvmod_core::{Instance, LoadError, LoadedInstance, Target, load_instance};
use std::fmt;
use std::path::{Path, PathBuf};

/// The file that marks a folder as an nvmod Instance.
pub const INSTANCE_FILE: &str = "nvmod-instance.json";
/// The file in a profile folder that holds the Profile record (see `nvmod_core::Profile`).
pub const PROFILE_FILE: &str = "profile.json";
/// The kind of journal that a schema migration writes: `nvmod rollback` restores the old Instance file from it.
pub const KIND_MIGRATE: &str = "migrate";

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
    /// Its configuration: the Instance record of `nvmod-instance.json`, at the current schema.
    pub config: Instance,
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
        let storefront = detect_storefront(base);
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
        let config = Instance::new(base, storefront);
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

    /// The folder `nvmod install-nv-rs` installs nv-rs into.
    #[must_use]
    pub fn nv_rs_dir(&self) -> PathBuf {
        self.root.join("nv-rs")
    }

    /// Where the installed nv-rs's `capabilities.json` is read from.
    #[must_use]
    pub fn nv_rs_capabilities(&self) -> PathBuf {
        self.nv_rs_dir().join("capabilities.json")
    }

    /// Change the target the Instance runs on and write `nvmod-instance.json` (to a temporary file, then a rename).
    ///
    /// # Errors
    /// `Io` when writing fails; the old file is then still in place.
    pub fn set_target(&mut self, target: Target) -> Result<(), InstanceError> {
        let mut config = self.config.clone();
        config.target = target;
        let io = |e: std::io::Error| InstanceError::Io(format!("{}: {e}", self.root.display()));
        let text =
            serde_json::to_string_pretty(&config).map_err(|e| InstanceError::Io(e.to_string()))?;
        let temp = self.root.join(format!("{INSTANCE_FILE}.tmp"));
        std::fs::write(&temp, text + "\n").map_err(io)?;
        std::fs::rename(&temp, self.root.join(INSTANCE_FILE)).map_err(|e| {
            let _ = std::fs::remove_file(&temp);
            io(e)
        })?;
        self.config = config;
        Ok(())
    }

    /// Make an empty Profile `name`: the three list files of MO2's format (a comment header, CRLF), with the base
    /// game's master as the only plugin, so that `install`, `sort` and the Doctor have a profile to work on.
    ///
    /// # Errors
    /// `Refused` for a name that is empty, `.`/`..` or holds a path separator, or a Profile that exists; `Io`
    /// when writing fails (the folder is removed again).
    pub fn create_profile(&self, name: &str) -> Result<(), InstanceError> {
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
            return Err(InstanceError::Refused(format!(
                "`{name}` is not a usable profile name"
            )));
        }
        let dir = self.profile_dir(name);
        if dir.exists() {
            return Err(InstanceError::Refused(format!(
                "the Instance already has a profile `{name}`"
            )));
        }
        let header = "# This file was automatically generated by Mod Organizer.\r\n";
        let write = || -> std::io::Result<()> {
            std::fs::create_dir_all(&dir)?;
            std::fs::write(dir.join("modlist.txt"), header)?;
            for file in ["plugins.txt", "loadorder.txt"] {
                std::fs::write(dir.join(file), format!("{header}FalloutNV.esm\r\n"))?;
            }
            Ok(())
        };
        write().map_err(|e| {
            let _ = std::fs::remove_dir_all(&dir);
            InstanceError::Io(format!("{}: {e}", dir.display()))
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

    /// Open the Instance at `root`, migrating an older file forward on disk: the old file is backed up under
    /// `.nvmod/journal/` first, so `nvmod rollback` restores it byte for byte.
    ///
    /// # Errors
    /// `Invalid` when the Instance file is missing, unreadable or malformed; `Refused` when its schema is newer
    /// than this nvmod reads; `Io` when the migration could not be written (the old file is left as it was).
    pub fn open(root: &Path) -> Result<NvmodInstance, InstanceError> {
        let (instance, loaded) = Self::read(root)?;
        if let Some(from) = loaded.migrated_from {
            instance.write_migration(from)?;
        }
        Ok(instance)
    }

    /// Open the Instance at `root` without writing anything: an older file is migrated in memory only.
    ///
    /// # Errors
    /// `Invalid` when the Instance file is missing, unreadable or malformed; `Refused` when its schema is newer
    /// than this nvmod reads.
    pub fn open_read_only(root: &Path) -> Result<NvmodInstance, InstanceError> {
        Self::read(root).map(|(instance, _)| instance)
    }

    fn read(root: &Path) -> Result<(NvmodInstance, LoadedInstance), InstanceError> {
        let file = root.join(INSTANCE_FILE);
        let text = std::fs::read_to_string(&file)
            .map_err(|e| InstanceError::Invalid(format!("{}: {e}", file.display())))?;
        let loaded = load_instance(&text).map_err(|e| match e {
            LoadError::Newer { .. } => InstanceError::Refused(format!("{}: {e}", file.display())),
            LoadError::Malformed(_) => InstanceError::Invalid(format!("{}: {e}", file.display())),
        })?;
        let instance = NvmodInstance {
            root: root.to_path_buf(),
            config: loaded.instance.clone(),
        };
        Ok((instance, loaded))
    }

    /// Back up the Instance file as it is, journal the migration, then replace the file with the current schema.
    fn write_migration(&self, from: u32) -> Result<(), InstanceError> {
        let file = self.root.join(INSTANCE_FILE);
        let io = |e: std::io::Error| InstanceError::Io(format!("{}: {e}", file.display()));
        let millis = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let id = format!("{millis:016}-{}", std::process::id());
        let journal_dir = self.root.join(".nvmod").join("journal").join(&id);
        let write = || -> Result<(), InstanceError> {
            std::fs::create_dir_all(journal_dir.join("backup")).map_err(io)?;
            std::fs::copy(&file, journal_dir.join("backup").join(INSTANCE_FILE)).map_err(io)?;
            let journal = serde_json::json!({
                "schema": 1,
                "id": id,
                "mod": "",
                "profile": "",
                "status": "complete",
                "kind": KIND_MIGRATE,
                "existed": [],
                "staging_existed": false,
                "from_schema": from,
            });
            let text = serde_json::to_string_pretty(&journal)
                .map_err(|e| InstanceError::Io(e.to_string()))?;
            std::fs::write(journal_dir.join("journal.json"), text + "\n").map_err(io)?;
            let text = serde_json::to_string_pretty(&self.config)
                .map_err(|e| InstanceError::Io(e.to_string()))?;
            let tmp = file.with_extension("nvmod-tmp");
            std::fs::write(&tmp, text + "\n").map_err(io)?;
            std::fs::rename(&tmp, &file).map_err(io)
        };
        write().inspect_err(|_| {
            let _ = std::fs::remove_dir_all(&journal_dir);
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
        assert_eq!(opened.config.storefront, nvmod_core::Storefront::Gog);
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
            r#"{"schema_version":9,"base":"/x","storefront":"steam"}"#,
        )
        .unwrap();
        let why = NvmodInstance::open(dir.path()).unwrap_err();
        assert!(matches!(why, InstanceError::Refused(_)), "{why}");
        assert!(why.to_string().contains("schema 9"), "{why}");
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
    fn a_profile_is_made_with_the_three_lists_and_not_overwritten() {
        let base = TempDir::new("inst-newprof-base");
        BaseBuilder::steam().write(base.path());
        let scratch = TempDir::new("inst-newprof");
        let i = NvmodInstance::create(&scratch.path().join("i"), base.path()).unwrap();
        i.create_profile("Default").unwrap();
        assert_eq!(i.resolve_profile(None).unwrap(), "Default");
        let plugins =
            std::fs::read_to_string(i.profile_dir("Default").join("plugins.txt")).unwrap();
        assert!(plugins.ends_with("FalloutNV.esm\r\n"), "{plugins:?}");
        assert!(i.profile_dir("Default").join("modlist.txt").is_file());
        assert!(i.create_profile("Default").is_err());
        for bad in ["", "..", "a/b", "a\\b"] {
            assert!(
                matches!(i.create_profile(bad), Err(InstanceError::Refused(_))),
                "{bad}"
            );
        }
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

    #[test]
    fn a_schema_1_file_opens_read_only_and_migrates_on_open_with_a_backup() {
        let dir = TempDir::new("inst-v1");
        let old = r#"{"schema":1,"base":"/x","storefront":"gog"}"#;
        std::fs::write(dir.path().join(INSTANCE_FILE), old).unwrap();
        let ro = NvmodInstance::open_read_only(dir.path()).unwrap();
        assert_eq!(ro.config.schema_version, 2);
        assert_eq!(
            std::fs::read_to_string(dir.path().join(INSTANCE_FILE)).unwrap(),
            old
        );
        assert!(!dir.path().join(".nvmod").exists());
        let rw = NvmodInstance::open(dir.path()).unwrap();
        assert_eq!(rw.config, ro.config);
        let text = std::fs::read_to_string(dir.path().join(INSTANCE_FILE)).unwrap();
        assert!(text.contains("\"schema_version\": 2"), "{text}");
        let journal = std::fs::read_dir(dir.path().join(".nvmod/journal"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert_eq!(
            std::fs::read_to_string(journal.join("backup").join(INSTANCE_FILE)).unwrap(),
            old
        );
        // Opened again at the current schema: no second migration.
        NvmodInstance::open(dir.path()).unwrap();
        assert_eq!(
            std::fs::read_dir(dir.path().join(".nvmod/journal"))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn a_failed_migration_leaves_the_old_file_and_no_journal() {
        let dir = TempDir::new("inst-v1-fail");
        let old = r#"{"schema":1,"base":"/x","storefront":"gog"}"#;
        std::fs::write(dir.path().join(INSTANCE_FILE), old).unwrap();
        // A file where the journal folder must go makes the backup fail.
        std::fs::write(dir.path().join(".nvmod"), b"x").unwrap();
        let why = NvmodInstance::open(dir.path()).unwrap_err();
        assert!(matches!(why, InstanceError::Io(_)), "{why}");
        assert_eq!(
            std::fs::read_to_string(dir.path().join(INSTANCE_FILE)).unwrap(),
            old
        );
    }
}
