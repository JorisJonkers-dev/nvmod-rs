// SPDX-License-Identifier: GPL-3.0-only
//! A Mod Organizer 2 instance on disk, read only: where its folders are, its profiles, its mods.

use crate::ini::{Ini, qt_string};
use crate::modlist::{EntryKind, ModState, Modlist};
use crate::plugins::parse_plugin_list;
use crate::text::{decode, read_limited};
use crate::winpath::to_host_path;
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

/// Most `customExecutables` entries read.
pub const MAX_EXECUTABLES: usize = 1000;

/// Why an instance or a profile could not be opened.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstanceError {
    /// The directory has no `ModOrganizer.ini`.
    NotAnInstance(PathBuf),
    /// A file could not be read.
    Io(String),
    /// A profile name that is not a plain folder name (it could point outside `profiles/`).
    BadProfileName(String),
    /// The profile folder does not exist.
    NoSuchProfile(String),
}

impl fmt::Display for InstanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstanceError::NotAnInstance(dir) => {
                write!(
                    f,
                    "{} has no ModOrganizer.ini: not a Mod Organizer 2 instance",
                    dir.display()
                )
            }
            InstanceError::Io(why) => f.write_str(why),
            InstanceError::BadProfileName(name) => {
                write!(f, "`{name}` is not a profile folder name")
            }
            InstanceError::NoSuchProfile(name) => write!(f, "the instance has no profile `{name}`"),
        }
    }
}

impl std::error::Error for InstanceError {}

/// One entry of `[customExecutables]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Executable {
    /// The name shown in MO2.
    pub title: String,
    /// The program, as written (a Wine path such as `Z:/home/...`).
    pub binary: String,
    /// The program on this machine, when the path could be mapped.
    pub binary_host: Option<PathBuf>,
    /// Its arguments, as written.
    pub arguments: String,
}

/// What `ModOrganizer.ini` says, with the folders resolved.
#[derive(Debug, Clone)]
pub struct Instance {
    /// The instance folder (where `ModOrganizer.ini` is).
    pub root: PathBuf,
    /// The MO2 game plugin name (`TTW`, `FalloutNV`).
    pub game_name: String,
    /// `gamePath` as written (a Wine path).
    pub game_path_raw: Option<String>,
    /// The game folder on this machine, when the path could be mapped.
    pub game_path: Option<PathBuf>,
    /// The profile MO2 last used.
    pub selected_profile: Option<String>,
    /// The `mods` folder.
    pub mods_dir: PathBuf,
    /// The `profiles` folder.
    pub profiles_dir: PathBuf,
    /// The `downloads` folder.
    pub downloads_dir: PathBuf,
    /// The `overwrite` folder.
    pub overwrite_dir: PathBuf,
    /// The configured executables.
    pub executables: Vec<Executable>,
    /// Things that could not be resolved (for example an unmapped drive), one line each.
    pub issues: Vec<String>,
}

/// A profile folder, read.
#[derive(Debug, Clone)]
pub struct Profile {
    /// The profile's name (its folder name).
    pub name: String,
    /// The profile folder.
    pub dir: PathBuf,
    /// `modlist.txt`.
    pub modlist: Modlist,
    /// `plugins.txt` (active plugins in order), `None` when the file is missing.
    pub plugins: Option<Vec<String>>,
    /// `loadorder.txt` (all plugins in order), `None` when the file is missing.
    pub load_order: Option<Vec<String>>,
    /// True when `fallout.ini` exists and is read-only (a recorded trap: MO2 stalls on a hidden dialog).
    pub fallout_ini_read_only: bool,
}

/// The parts of a mod's `meta.ini` the doctor uses.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModMeta {
    /// Nexus mod id; `None` for 0 or absent (unknown).
    pub modid: Option<u64>,
    /// `version`, as written (MO2 date versions start with `d`).
    pub version: Option<String>,
    /// `installationFile`: the archive's file name only. The directory part is personal and not portable.
    pub installation_file: Option<String>,
    /// `repository` (`Nexus` by default).
    pub repository: Option<String>,
}

impl ModMeta {
    /// Read the text of a `meta.ini`.
    #[must_use]
    pub fn parse(text: &str) -> ModMeta {
        let ini = Ini::parse(text);
        let general = |key| ini.get_string("General", key).filter(|v| !v.is_empty());
        ModMeta {
            modid: general("modid")
                .and_then(|v| v.parse().ok())
                .filter(|&id| id != 0),
            version: general("version"),
            installation_file: general("installationFile")
                .map(|p| {
                    p.replace('\\', "/")
                        .rsplit('/')
                        .next()
                        .unwrap_or_default()
                        .to_owned()
                })
                .filter(|n| !n.is_empty()),
            repository: general("repository"),
        }
    }
}

fn read_text(path: &Path) -> Result<String, InstanceError> {
    read_limited(path)
        .map(|b| decode(&b))
        .map_err(|e| InstanceError::Io(format!("{}: {e}", path.display())))
}

impl Instance {
    /// Open the instance in `root`.
    ///
    /// # Errors
    /// `root` has no `ModOrganizer.ini`, or it cannot be read.
    pub fn open(root: &Path) -> Result<Instance, InstanceError> {
        let ini_path = root.join("ModOrganizer.ini");
        if !ini_path.is_file() {
            return Err(InstanceError::NotAnInstance(root.to_path_buf()));
        }
        let ini = Ini::parse(&read_text(&ini_path)?);
        let mut issues = Vec::new();
        let drives = BTreeMap::new();
        let mut host = |what: &str, raw: &str| match to_host_path(raw, &drives) {
            Ok(path) => Some(path),
            Err(e) => {
                issues.push(format!("{what}: {e}"));
                None
            }
        };
        let game_path_raw = ini
            .get_string("General", "gamePath")
            .filter(|v| !v.is_empty());
        let game_path = game_path_raw
            .as_deref()
            .and_then(|raw| host("gamePath", raw));
        let base = match ini
            .get_string("Settings", "base_directory")
            .filter(|v| !v.is_empty())
        {
            Some(raw) => host("base_directory", &raw).unwrap_or_else(|| root.to_path_buf()),
            None => root.to_path_buf(),
        };
        let mut resolve = |key: &str, default: &str| -> PathBuf {
            match ini.get_string("Settings", key).filter(|v| !v.is_empty()) {
                Some(raw) => {
                    let replaced = raw.replace("%BASE_DIR%", &base.to_string_lossy());
                    let path = host(key, &replaced).unwrap_or_else(|| base.join(default));
                    if path.is_absolute() {
                        path
                    } else {
                        base.join(path)
                    }
                }
                None => base.join(default),
            }
        };
        let mods_dir = resolve("mod_directory", "mods");
        let profiles_dir = resolve("profiles_directory", "profiles");
        let downloads_dir = resolve("download_directory", "downloads");
        let overwrite_dir = resolve("overwrite_directory", "overwrite");
        let executables = read_executables(&ini, &mut host);
        Ok(Instance {
            root: root.to_path_buf(),
            game_name: ini.get_string("General", "gameName").unwrap_or_default(),
            game_path_raw,
            game_path,
            selected_profile: ini
                .get_string("General", "selected_profile")
                .filter(|v| !v.is_empty()),
            mods_dir,
            profiles_dir,
            downloads_dir,
            overwrite_dir,
            executables,
            issues,
        })
    }

    /// The names of the profile folders, sorted.
    ///
    /// # Errors
    /// The `profiles` folder cannot be read.
    pub fn profile_names(&self) -> Result<Vec<String>, InstanceError> {
        let entries = std::fs::read_dir(&self.profiles_dir)
            .map_err(|e| InstanceError::Io(format!("{}: {e}", self.profiles_dir.display())))?;
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        Ok(names)
    }

    /// Read the profile `name`.
    ///
    /// # Errors
    /// The name is not a plain folder name, the folder does not exist, or `modlist.txt` cannot be read.
    pub fn load_profile(&self, name: &str) -> Result<Profile, InstanceError> {
        if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
            return Err(InstanceError::BadProfileName(name.to_owned()));
        }
        let dir = self.profiles_dir.join(name);
        if !dir.is_dir() {
            return Err(InstanceError::NoSuchProfile(name.to_owned()));
        }
        let list = |file: &str| {
            read_limited(&dir.join(file))
                .ok()
                .map(|b| parse_plugin_list(&b))
        };
        Ok(Profile {
            name: name.to_owned(),
            modlist: Modlist::parse(&read_text(&dir.join("modlist.txt"))?),
            plugins: list("plugins.txt"),
            load_order: list("loadorder.txt"),
            fallout_ini_read_only: std::fs::metadata(dir.join("fallout.ini"))
                .is_ok_and(|m| m.permissions().readonly()),
            dir,
        })
    }

    /// The folder of the mod `name` (it may not exist).
    #[must_use]
    pub fn mod_dir(&self, name: &str) -> PathBuf {
        self.mods_dir.join(name)
    }

    /// The `meta.ini` of the mod `name`, when it has one.
    #[must_use]
    pub fn mod_meta(&self, name: &str) -> Option<ModMeta> {
        read_limited(&self.mod_dir(name).join("meta.ini"))
            .ok()
            .map(|b| ModMeta::parse(&decode(&b)))
    }

    /// Mod names of `profile` (separators and foreign entries left out) whose folder is missing, and folders
    /// under `mods/` that the profile does not list. MO2 repairs both on its next start; nvmod only reports.
    #[must_use]
    pub fn reconcile(&self, profile: &Profile) -> (Vec<String>, Vec<String>) {
        let listed: std::collections::HashSet<&str> = profile
            .modlist
            .entries
            .iter()
            .map(|e| e.name.as_str())
            .collect();
        let missing = profile
            .modlist
            .entries
            .iter()
            .filter(|e| {
                e.kind == EntryKind::Mod
                    && e.state != ModState::Foreign
                    && !self.mod_dir(&e.name).is_dir()
            })
            .map(|e| e.name.clone())
            .collect();
        let mut unlisted: Vec<String> = std::fs::read_dir(&self.mods_dir)
            .into_iter()
            .flatten()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| !listed.contains(n.as_str()))
            .collect();
        unlisted.sort();
        (missing, unlisted)
    }
}

fn read_executables(
    ini: &Ini,
    host: &mut dyn FnMut(&str, &str) -> Option<PathBuf>,
) -> Vec<Executable> {
    let size: usize = ini
        .get("customExecutables", "size")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0)
        .min(MAX_EXECUTABLES);
    (1..=size)
        .filter_map(|n| {
            let field = |name: &str| {
                ini.get("customExecutables", &format!("{n}\\{name}"))
                    .map(qt_string)
            };
            let binary = field("binary")?;
            Some(Executable {
                title: field("title").unwrap_or_default(),
                binary_host: host(&format!("executable {n}"), &binary),
                binary,
                arguments: field("arguments").unwrap_or_default(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn meta_keeps_the_archive_file_name_only_and_treats_zero_as_unknown() {
        let meta = ModMeta::parse(
            "[General]\nmodid=83815\nversion=d2026.3.1.0\ninstallationFile=C:/Users/me/Downloads/MAC-TEN-83815-15.7z\nrepository=Nexus\n",
        );
        assert_eq!(meta.modid, Some(83815));
        assert_eq!(meta.version.as_deref(), Some("d2026.3.1.0"));
        assert_eq!(
            meta.installation_file.as_deref(),
            Some("MAC-TEN-83815-15.7z")
        );
        assert_eq!(meta.repository.as_deref(), Some("Nexus"));
        assert_eq!(
            ModMeta::parse("[General]\nmodid=0\ninstallationFile=\n"),
            ModMeta::default()
        );
        assert_eq!(ModMeta::parse("modid=x\n").modid, None);
    }

    #[test]
    fn errors_say_what_is_wrong() {
        assert!(
            InstanceError::NotAnInstance(PathBuf::from("/x"))
                .to_string()
                .contains("ModOrganizer.ini")
        );
        assert!(
            InstanceError::BadProfileName("a/b".into())
                .to_string()
                .contains("a/b")
        );
        assert!(
            InstanceError::NoSuchProfile("p".into())
                .to_string()
                .contains('p')
        );
        assert_eq!(InstanceError::Io("boom".into()).to_string(), "boom");
    }
}
