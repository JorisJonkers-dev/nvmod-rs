// SPDX-License-Identifier: GPL-3.0-only
//! The Instance and Profile data model: the one place these types are defined. Both are persisted as JSON with an
//! integer `schema_version`; a JSON Schema is generated from the types and checked in under `schemas/`. The
//! loaders read the current and the previous version, migrate forward in memory, and refuse a newer version with
//! a reason. Reading and writing files, and the backup before a migration, belong to `nvmod-instance`.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;
use std::path::PathBuf;

/// The schema version of `nvmod-instance.json` this build writes. Version 1 (the skeleton) is still read.
pub const INSTANCE_SCHEMA_VERSION: u32 = 2;
/// The schema version of `profile.json` this build writes.
pub const PROFILE_SCHEMA_VERSION: u32 = 1;

/// Where a base copy came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Storefront {
    /// Steam: `steam_api.dll` beside the executable.
    Steam,
    /// GOG: `goggame-1454587428.info` or `goglog.ini`.
    Gog,
    /// Epic: a `.egstore` folder. Not supported.
    Epic,
    /// No marker of a known store. Not supported.
    Unknown,
}

impl Storefront {
    /// The name for people.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Storefront::Steam => "Steam",
            Storefront::Gog => "GOG",
            Storefront::Epic => "Epic",
            Storefront::Unknown => "unknown storefront",
        }
    }

    /// Why this base copy is refused, or `None` when it is supported.
    #[must_use]
    pub fn refusal(self) -> Option<&'static str> {
        match self {
            Storefront::Steam | Storefront::Gog => None,
            Storefront::Epic => Some(
                "this is an Epic copy of the game; nvmod installs from a Steam or GOG base copy only",
            ),
            Storefront::Unknown => Some(
                "the storefront of this copy could not be recognised; nvmod installs from a Steam or GOG base copy only",
            ),
        }
    }
}

impl fmt::Display for Storefront {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// What the Instance runs on: the vanilla engine with the script extender DLLs, or nv-rs with them built in.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum Target {
    /// The original engine (Proton on Linux, native on Windows); xNVSE and JIP LN are external DLLs.
    #[default]
    Vanilla,
    /// The nv-rs engine port, which has xNVSE and JIP LN built in.
    NvRs,
}

/// The contents of `nvmod-instance.json`: an Instance and the base copy it points at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Instance {
    /// Version of this file's layout; see [`INSTANCE_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// The absolute path of the base copy. Never written to.
    pub base: PathBuf,
    /// The storefront found when the Instance was made.
    pub storefront: Storefront,
    /// What the Instance runs on. Absent in a file of schema 1, which migrates to `vanilla`.
    #[serde(default)]
    pub target: Target,
    /// The programs `nvmod run` starts through the staged view (MO2's `customExecutables`). Absent when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub executables: Vec<Executable>,
    /// The Profile that is active (`nvmod profile select`). Absent until one is chosen: the only Profile there is
    /// is then the active one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_profile: Option<String>,
}

/// One entry of an Instance's executables registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Executable {
    /// The name `nvmod run` takes; unique in the Instance, compared without regard to case.
    pub title: String,
    /// The program, as a path on this machine (a `.exe` runs through Proton on Linux).
    pub binary: String,
    /// The arguments as one string, split on spaces with double quotes grouping words. `%DATA%` is the staged
    /// `Data` folder, `%INSTANCE%` the Instance folder and `%PROFILE%` the profile name.
    #[serde(default)]
    pub arguments: String,
    /// The folder to run in; the program's own folder when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<String>,
    /// The Steam app id the program runs under; the game's own id when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steam_app_id: Option<u32>,
    /// Whether front ends leave the entry out of their lists.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
}

impl Instance {
    /// A new Instance of the current schema.
    #[must_use]
    pub fn new(base: PathBuf, storefront: Storefront) -> Instance {
        Instance {
            schema_version: INSTANCE_SCHEMA_VERSION,
            base,
            storefront,
            target: Target::Vanilla,
            executables: Vec::new(),
            selected_profile: None,
        }
    }
}

/// One Mod in a Profile's list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileMod {
    /// The Mod's folder name in the Instance.
    pub name: String,
    /// Whether the Mod is enabled in this Profile.
    pub enabled: bool,
}

/// The contents of a Profile's `profile.json`: a named Load order together with the list of enabled Mods.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Profile {
    /// Version of this file's layout; see [`PROFILE_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// The Profile's name, which is also its folder name.
    pub name: String,
    /// The Mods in priority order, highest first, as in MO2's `modlist.txt` (the first Mod wins a file conflict).
    pub mods: Vec<ProfileMod>,
    /// All Plugin file names in Load order, later wins.
    pub load_order: Vec<String>,
    /// The Plugins that are active, in Load order.
    pub active_plugins: Vec<String>,
}

impl Profile {
    /// A new, empty Profile of the current schema.
    #[must_use]
    pub fn new(name: &str) -> Profile {
        Profile {
            schema_version: PROFILE_SCHEMA_VERSION,
            name: name.to_owned(),
            mods: Vec::new(),
            load_order: Vec::new(),
            active_plugins: Vec::new(),
        }
    }
}

/// Why a persisted document was not accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// The document is of a schema version newer than this build reads (exit code 2).
    Newer {
        /// The document kind, `Instance` or `Profile`.
        kind: &'static str,
        /// The version found.
        found: u32,
        /// The newest version this build reads.
        supported: u32,
    },
    /// The document is not valid JSON of the expected shape, or carries no usable schema version.
    Malformed(String),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Newer {
                kind,
                found,
                supported,
            } => write!(
                f,
                "this {kind} is of schema {found}, which is newer than this nvmod reads (schema {supported}); \
                 update nvmod, nothing was changed"
            ),
            LoadError::Malformed(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for LoadError {}

/// An Instance read from a document, and the version it was migrated from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedInstance {
    /// The Instance at the current schema.
    pub instance: Instance,
    /// The schema version of the document when it was older, else `None`.
    pub migrated_from: Option<u32>,
}

fn version_of(value: &Value, keys: &[&str]) -> Result<u32, LoadError> {
    let found = keys.iter().find_map(|key| value.get(*key));
    found
        .and_then(Value::as_u64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or_else(|| LoadError::Malformed(format!("no integer `{}` in the file", keys[0])))
}

fn malformed(e: &serde_json::Error) -> LoadError {
    LoadError::Malformed(e.to_string())
}

/// Read an Instance document. Schema 2 is read as it is; schema 1 (which names its version `schema` and has no
/// `target`) is migrated forward in memory; a newer version is refused.
///
/// # Errors
/// [`LoadError::Newer`] for a schema newer than [`INSTANCE_SCHEMA_VERSION`]; [`LoadError::Malformed`] for text that
/// is not an Instance of schema 1 or 2.
pub fn load_instance(text: &str) -> Result<LoadedInstance, LoadError> {
    let value: Value = serde_json::from_str(text).map_err(|e| malformed(&e))?;
    let version = version_of(&value, &["schema_version", "schema"])?;
    match version {
        INSTANCE_SCHEMA_VERSION => Ok(LoadedInstance {
            instance: serde_json::from_value(value).map_err(|e| malformed(&e))?,
            migrated_from: None,
        }),
        1 => migrate_instance_v1(value).map(|instance| LoadedInstance {
            instance,
            migrated_from: Some(1),
        }),
        found if found > INSTANCE_SCHEMA_VERSION => Err(LoadError::Newer {
            kind: "Instance",
            found,
            supported: INSTANCE_SCHEMA_VERSION,
        }),
        found => Err(LoadError::Malformed(format!(
            "an Instance of schema {found} is not known to this nvmod"
        ))),
    }
}

/// Schema 1: `{"schema":1,"base":...,"storefront":...}`. Schema 2 renames `schema` to `schema_version` and adds
/// `target`, which is `vanilla` for every Instance made before it existed.
fn migrate_instance_v1(value: Value) -> Result<Instance, LoadError> {
    #[derive(Deserialize)]
    struct V1 {
        base: PathBuf,
        storefront: Storefront,
    }
    let old: V1 = serde_json::from_value(value).map_err(|e| malformed(&e))?;
    Ok(Instance::new(old.base, old.storefront))
}

/// Read a Profile document (schema 1 is the only one so far).
///
/// # Errors
/// [`LoadError::Newer`] for a schema newer than [`PROFILE_SCHEMA_VERSION`]; [`LoadError::Malformed`] otherwise.
pub fn load_profile(text: &str) -> Result<Profile, LoadError> {
    let value: Value = serde_json::from_str(text).map_err(|e| malformed(&e))?;
    let version = version_of(&value, &["schema_version"])?;
    if version > PROFILE_SCHEMA_VERSION {
        return Err(LoadError::Newer {
            kind: "Profile",
            found: version,
            supported: PROFILE_SCHEMA_VERSION,
        });
    }
    if version != PROFILE_SCHEMA_VERSION {
        return Err(LoadError::Malformed(format!(
            "a Profile of schema {version} is not known to this nvmod"
        )));
    }
    serde_json::from_value(value).map_err(|e| malformed(&e))
}

/// The JSON Schema of an Instance, as pretty JSON with a trailing newline (the text of `schemas/instance.v2.json`).
///
/// # Panics
/// Never in practice: a generated schema always serialises.
#[must_use]
pub fn instance_schema() -> String {
    schema_text(&schemars::schema_for!(Instance))
}

/// The JSON Schema of a Profile (the text of `schemas/profile.v1.json`).
///
/// # Panics
/// Never in practice: a generated schema always serialises.
#[must_use]
pub fn profile_schema() -> String {
    schema_text(&schemars::schema_for!(Profile))
}

fn schema_text(schema: &schemars::Schema) -> String {
    let mut text = serde_json::to_string_pretty(schema).expect("a schema serialises");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    /// Compare the checked-in schema with the generated one; `NVMOD_BLESS=1` rewrites the file.
    fn check_schema(file: &str, generated: &str) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas")
            .join(file);
        if std::env::var_os("NVMOD_BLESS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, generated).unwrap();
        }
        let checked_in = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            checked_in == generated,
            "schemas/{file} differs from the schema generated from the Rust types; if the change is intended, \
             bump the schema version and run the tests with NVMOD_BLESS=1 to rewrite it"
        );
    }

    #[test]
    fn the_checked_in_instance_schema_is_the_generated_one() {
        check_schema("instance.v2.json", &instance_schema());
    }

    #[test]
    fn the_checked_in_profile_schema_is_the_generated_one() {
        check_schema("profile.v1.json", &profile_schema());
    }

    #[test]
    fn the_schema_files_are_named_after_their_version() {
        assert_eq!(INSTANCE_SCHEMA_VERSION, 2);
        assert_eq!(PROFILE_SCHEMA_VERSION, 1);
    }

    #[test]
    fn a_current_instance_round_trips() {
        let i = Instance::new(PathBuf::from("/games/fnv"), Storefront::Gog);
        let text = serde_json::to_string(&i).unwrap();
        let loaded = load_instance(&text).unwrap();
        assert_eq!(loaded.instance, i);
        assert_eq!(loaded.migrated_from, None);
    }

    #[test]
    fn schema_1_migrates_to_the_current_schema() {
        let old = r#"{"schema":1,"base":"/games/fnv","storefront":"steam"}"#;
        let loaded = load_instance(old).unwrap();
        assert_eq!(loaded.migrated_from, Some(1));
        assert_eq!(
            loaded.instance,
            Instance::new(PathBuf::from("/games/fnv"), Storefront::Steam)
        );
    }

    #[test]
    fn a_newer_instance_is_refused_with_a_reason() {
        let why =
            load_instance(r#"{"schema_version":3,"base":"/x","storefront":"steam"}"#).unwrap_err();
        assert!(
            matches!(
                why,
                LoadError::Newer {
                    found: 3,
                    supported: 2,
                    ..
                }
            ),
            "{why}"
        );
        assert!(why.to_string().contains("schema 3"));
        assert!(why.to_string().contains("newer"));
    }

    #[test]
    fn garbage_zero_and_unknown_versions_are_malformed() {
        for text in [
            "{",
            "{}",
            r#"{"schema_version":"2"}"#,
            r#"{"schema_version":0,"base":"/x","storefront":"gog"}"#,
            r#"{"schema_version":2,"base":"/x"}"#,
            r#"{"schema":1,"base":"/x"}"#,
            r#"{"schema_version":-1}"#,
        ] {
            assert!(
                matches!(load_instance(text), Err(LoadError::Malformed(_))),
                "{text}"
            );
        }
    }

    #[test]
    fn a_profile_round_trips_and_a_newer_one_is_refused() {
        let mut p = Profile::new("Default");
        p.mods.push(ProfileMod {
            name: "A".into(),
            enabled: true,
        });
        p.load_order = vec!["FalloutNV.esm".into()];
        p.active_plugins = vec!["FalloutNV.esm".into()];
        let text = serde_json::to_string(&p).unwrap();
        assert_eq!(load_profile(&text).unwrap(), p);
        let why = load_profile(r#"{"schema_version":2}"#).unwrap_err();
        assert!(matches!(why, LoadError::Newer { found: 2, .. }), "{why}");
        assert!(matches!(
            load_profile(r#"{"schema_version":0}"#),
            Err(LoadError::Malformed(_))
        ));
        assert!(load_profile("[]").is_err());
    }

    #[test]
    fn the_storefront_has_labels_and_refusals() {
        assert_eq!(Storefront::Gog.to_string(), "GOG");
        assert!(Storefront::Steam.refusal().is_none());
        assert!(Storefront::Epic.refusal().unwrap().contains("Epic"));
        assert!(Storefront::Unknown.refusal().is_some());
        assert_eq!(Storefront::Steam.label(), "Steam");
        assert_eq!(Storefront::Epic.label(), "Epic");
        assert_eq!(Storefront::Unknown.label(), "unknown storefront");
    }
}
