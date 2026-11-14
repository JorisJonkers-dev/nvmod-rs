// SPDX-License-Identifier: GPL-3.0-only
//! The NVSE plugin catalogue: data this repository owns (`data/nvse-plugins.toml`), loaded and checked here.
//! Every entry carries provenance; the loader refuses a file whose references do not resolve.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

/// The catalogue shipped with the program.
pub const BUILTIN: &str = include_str!("../../../data/nvse-plugins.toml");

/// The only schema version understood.
pub const SCHEMA: u32 = 1;

/// How nv-rs treats a plugin. nv-rs has xNVSE and JIP built in and ignores plugin DLLs (plan, section 10.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NvRs {
    /// The plugin's commands exist in nv-rs.
    BuiltIn,
    /// Some of them do.
    Partial,
    /// The plugin does hook-level engine work nv-rs cannot host.
    Unsupported,
    /// The plugin is ignored and does not matter.
    Inert,
    /// Nobody has checked.
    #[default]
    Unknown,
}

/// Where a catalogue fact came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// What was read or seen.
    pub source: String,
    /// The date it was seen, `YYYY-MM-DD`.
    pub verified_on: String,
    /// True when the facts of the entry were observed first hand, false when copied from a document.
    pub verified: bool,
    /// Anything the reader should know about the limits of the claim.
    #[serde(default)]
    pub note: Option<String>,
}

/// One NVSE plugin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plugin {
    /// Stable identifier.
    pub id: String,
    /// The mod's folder name in the owner's instance.
    pub name: String,
    /// The DLL file name in `NVSE/Plugins` (compared without case).
    pub dll: String,
    /// INI files next to the DLL that the plugin reads.
    #[serde(default)]
    pub ini: Vec<String>,
    /// `framework` when other mods need the plugin; absent otherwise.
    #[serde(default)]
    pub role: Option<String>,
    /// The mod's Nexus id, only where a document states it.
    #[serde(default)]
    pub nexus_mod_id: Option<u64>,
    /// Ids of plugins that must be present.
    #[serde(default)]
    pub requires: Vec<String>,
    /// Ids of plugins that must not be present together with this one.
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// Id of the plugin that replaces this one.
    #[serde(default)]
    pub replaced_by: Option<String>,
    /// How nv-rs treats it.
    #[serde(default)]
    pub nv_rs: NvRs,
    /// The game executable version (`a.b.c.d`) the plugin requires. Absent means nobody recorded it: the Doctor
    /// says "unknown" and never guesses.
    #[serde(default)]
    pub game_version: Option<String>,
    /// The lowest xNVSE version (`a.b.c.d`) the plugin requires. Absent means nobody recorded it.
    #[serde(default)]
    pub min_xnvse: Option<String>,
    /// Where the entry came from.
    pub provenance: Provenance,
}

/// A mod folder name with its Nexus id, for the `meta.ini` mismatch check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownMod {
    /// The folder name under `mods/`.
    pub name: String,
    /// The Nexus id.
    pub nexus_mod_id: u64,
    /// Where it came from.
    pub provenance: Provenance,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    schema: u32,
    #[serde(default)]
    plugin: Vec<Plugin>,
    #[serde(default, rename = "mod")]
    mods: Vec<KnownMod>,
}

/// The catalogue could not be loaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogError(pub String);

impl fmt::Display for CatalogError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "catalogue: {}", self.0)
    }
}

impl std::error::Error for CatalogError {}

fn fail<T>(why: impl Into<String>) -> Result<T, CatalogError> {
    Err(CatalogError(why.into()))
}

fn valid_date(text: &str) -> bool {
    let parts: Vec<&str> = text.split('-').collect();
    matches!(parts.as_slice(), [y, m, d]
        if y.len() == 4 && m.len() == 2 && d.len() == 2
            && y.bytes().all(|b| b.is_ascii_digit()) && m.bytes().all(|b| b.is_ascii_digit()) && d.bytes().all(|b| b.is_ascii_digit())
            && (1..=12).contains(&m.parse::<u8>().unwrap_or(0)) && (1..=31).contains(&d.parse::<u8>().unwrap_or(0)))
}

/// One to four dot-separated numbers that fit in 16 bits.
fn valid_version(text: &str) -> bool {
    let parts: Vec<&str> = text.split('.').collect();
    (1..=4).contains(&parts.len())
        && parts.iter().all(|p| {
            !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()) && p.parse::<u16>().is_ok()
        })
}

fn check_provenance(what: &str, p: &Provenance) -> Result<(), CatalogError> {
    if p.source.trim().is_empty() {
        return fail(format!("{what}: provenance has no source"));
    }
    if !valid_date(&p.verified_on) {
        return fail(format!(
            "{what}: verified_on `{}` is not YYYY-MM-DD",
            p.verified_on
        ));
    }
    Ok(())
}

/// The loaded, validated catalogue.
#[derive(Debug, Clone)]
pub struct Catalog {
    /// Plugins in file order.
    pub plugins: Vec<Plugin>,
    /// Known mod names with Nexus ids.
    pub mods: Vec<KnownMod>,
}

impl Catalog {
    /// The catalogue shipped with the program.
    ///
    /// # Panics
    /// Never in a released build: a test loads the shipped file.
    #[must_use]
    pub fn builtin() -> Catalog {
        Catalog::parse(BUILTIN).expect("the shipped catalogue is valid (tested)")
    }

    /// Parse and validate a catalogue.
    ///
    /// # Errors
    /// Invalid TOML, an unknown field or schema, a duplicate id or DLL, a reference to an unknown plugin, an
    /// entry without provenance, or a Nexus id of zero.
    pub fn parse(text: &str) -> Result<Catalog, CatalogError> {
        let file: File = toml::from_str(text).map_err(|e| CatalogError(e.message().to_owned()))?;
        if file.schema != SCHEMA {
            return fail(format!(
                "schema {} is not understood (expected {SCHEMA})",
                file.schema
            ));
        }
        let mut ids = HashSet::new();
        let mut dlls = HashSet::new();
        for p in &file.plugin {
            if p.id.is_empty() || !ids.insert(p.id.as_str()) {
                return fail(format!("plugin id `{}` is empty or repeated", p.id));
            }
            if p.dll.is_empty() || !dlls.insert(p.dll.to_lowercase()) {
                return fail(format!(
                    "plugin {}: dll `{}` is empty or repeated",
                    p.id, p.dll
                ));
            }
            if p.nexus_mod_id == Some(0) {
                return fail(format!(
                    "plugin {}: nexus_mod_id 0 means unknown; leave it out",
                    p.id
                ));
            }
            check_provenance(&format!("plugin {}", p.id), &p.provenance)?;
            for (field, value) in [
                ("game_version", &p.game_version),
                ("min_xnvse", &p.min_xnvse),
            ] {
                if value.as_deref().is_some_and(|v| !valid_version(v)) {
                    return fail(format!(
                        "plugin {}: {field} `{}` is not a version like 1.4.0.525",
                        p.id,
                        value.as_deref().unwrap_or_default()
                    ));
                }
            }
        }
        for p in &file.plugin {
            let references = p
                .requires
                .iter()
                .chain(&p.conflicts)
                .chain(p.replaced_by.iter());
            for target in references {
                if !ids.contains(target.as_str()) {
                    return fail(format!(
                        "plugin {}: refers to unknown plugin `{target}`",
                        p.id
                    ));
                }
            }
        }
        let mut names = HashSet::new();
        for m in &file.mods {
            if m.nexus_mod_id == 0 || !names.insert(m.name.as_str()) {
                return fail(format!(
                    "mod `{}`: Nexus id is 0 or the name is repeated",
                    m.name
                ));
            }
            check_provenance(&format!("mod {}", m.name), &m.provenance)?;
        }
        Ok(Catalog {
            plugins: file.plugin,
            mods: file.mods,
        })
    }

    /// The plugin whose DLL is `file_name`, ignoring case.
    #[must_use]
    pub fn by_dll(&self, file_name: &str) -> Option<&Plugin> {
        self.plugins
            .iter()
            .find(|p| p.dll.eq_ignore_ascii_case(file_name))
    }

    /// The plugin with this id.
    #[must_use]
    pub fn plugin(&self, id: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.id == id)
    }

    /// The known Nexus id of the mod folder `name`.
    #[must_use]
    pub fn known_mod(&self, name: &str) -> Option<&KnownMod> {
        self.mods.iter().find(|m| m.name == name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const PROV: &str =
        "[plugin.provenance]\nsource = \"s\"\nverified_on = \"2026-11-14\"\nverified = false\n";

    fn one(extra: &str) -> String {
        format!(
            "schema = 1\n[[plugin]]\nid = \"a\"\nname = \"A\"\ndll = \"a.dll\"\n{extra}\n{PROV}"
        )
    }

    #[test]
    fn the_shipped_catalogue_is_valid_and_every_entry_has_provenance() {
        let catalog = Catalog::builtin();
        assert!(catalog.plugins.len() > 30 && !catalog.mods.is_empty());
        for p in &catalog.plugins {
            assert!(
                !p.provenance.source.is_empty() && !p.name.is_empty(),
                "{}",
                p.id
            );
            assert!(p.dll.to_lowercase().ends_with(".dll"), "{}", p.id);
        }
        assert_eq!(catalog.by_dll("JIP_NVSE.DLL").unwrap().id, "jip-ln");
        assert_eq!(
            catalog.by_dll("jip_nvse.dll").unwrap().role.as_deref(),
            Some("framework")
        );
        assert_eq!(catalog.plugin("showoff").unwrap().nexus_mod_id, Some(72541));
        assert_eq!(catalog.known_mod("MAC-TEN").unwrap().nexus_mod_id, 83815);
        assert!(
            catalog.by_dll("nope.dll").is_none()
                && catalog.plugin("nope").is_none()
                && catalog.known_mod("x").is_none()
        );
    }

    #[test]
    fn defaults_are_empty_and_nv_rs_is_unknown() {
        let catalog = Catalog::parse(&one("")).unwrap();
        let p = &catalog.plugins[0];
        assert!(p.ini.is_empty() && p.requires.is_empty() && p.nexus_mod_id.is_none());
        assert_eq!(p.nv_rs, NvRs::Unknown);
        let catalog = Catalog::parse(&one("nv_rs = \"built-in\"")).unwrap();
        assert_eq!(catalog.plugins[0].nv_rs, NvRs::BuiltIn);
    }

    #[test]
    fn each_kind_of_mistake_is_refused_with_its_reason() {
        let bad = |text: &str, want: &str| {
            let error = Catalog::parse(text).unwrap_err().to_string();
            assert!(error.contains(want), "{error} should contain {want}");
        };
        bad("schema = 2\n", "schema 2");
        bad("schema = 1\nbogus = 1\n", "bogus");
        bad("not toml [", "");
        bad(&one("nexus_mod_id = 0"), "nexus_mod_id 0");
        bad(&one("requires = [\"ghost\"]"), "unknown plugin `ghost`");
        bad(&one("replaced_by = \"ghost\""), "unknown plugin `ghost`");
        bad(
            &format!(
                "{}\n[[plugin]]\nid = \"a\"\nname = \"B\"\ndll = \"b.dll\"\n{PROV}",
                one("")
            ),
            "repeated",
        );
        bad(
            &format!(
                "{}\n[[plugin]]\nid = \"b\"\nname = \"B\"\ndll = \"A.DLL\"\n{PROV}",
                one("")
            ),
            "dll",
        );
        bad(
            &one("").replace("source = \"s\"", "source = \" \""),
            "no source",
        );
        bad(&one("").replace("2026-11-14", "14/11/2026"), "YYYY-MM-DD");
        bad(&one("").replace("2026-11-14", "2026-13-01"), "YYYY-MM-DD");
        let mods = "schema = 1\n[[mod]]\nname = \"M\"\nnexus_mod_id = 0\n[mod.provenance]\nsource = \"s\"\nverified_on = \"2026-11-14\"\nverified = false\n";
        bad(mods, "Nexus id is 0");
    }

    #[test]
    fn required_versions_are_optional_and_checked() {
        let catalog = Catalog::parse(&one("")).unwrap();
        assert!(
            catalog.plugins[0].game_version.is_none() && catalog.plugins[0].min_xnvse.is_none()
        );
        let catalog =
            Catalog::parse(&one("game_version = \"1.4.0.525\"\nmin_xnvse = \"6.4\"")).unwrap();
        assert_eq!(
            catalog.plugins[0].game_version.as_deref(),
            Some("1.4.0.525")
        );
        assert_eq!(catalog.plugins[0].min_xnvse.as_deref(), Some("6.4"));
        for bad in ["", "1.x", "1.2.3.4.5", "70000", "1..2"] {
            let text = one(&format!("min_xnvse = \"{bad}\""));
            let error = Catalog::parse(&text).unwrap_err().to_string();
            assert!(error.contains("min_xnvse"), "{bad}: {error}");
        }
        assert!(Catalog::parse(&one("game_version = \"latest\"")).is_err());
    }

    #[test]
    fn references_between_plugins_resolve() {
        let text = format!(
            "{}\n[[plugin]]\nid = \"b\"\nname = \"B\"\ndll = \"b.dll\"\nrequires = [\"a\"]\nconflicts = [\"a\"]\nreplaced_by = \"a\"\n{PROV}",
            one("")
        );
        let catalog = Catalog::parse(&text).unwrap();
        assert_eq!(catalog.plugin("b").unwrap().requires, ["a"]);
    }

    proptest! {
        #[test]
        fn arbitrary_text_never_panics(text in ".{0,300}") {
            let _ = Catalog::parse(&text);
        }
    }
}
