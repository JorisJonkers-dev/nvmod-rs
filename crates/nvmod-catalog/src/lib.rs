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

/// What provides a catalogue entry on one target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A DLL the user installs. On the vanilla target it is checked at the catalogue's versions; on nv-rs nothing
    /// records that nv-rs loads it, so whether it works there is unknown.
    #[default]
    External,
    /// Built into nv-rs with every command. The coverage is still read from `capabilities.json`.
    BuiltinFull,
    /// Built into nv-rs with some of its commands. The numbers are read from `capabilities.json`.
    BuiltinPartial,
    /// Not available on this target: a DLL of it cannot load there.
    Absent,
}

impl Kind {
    /// The name used in the catalogue file and in reports.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Kind::External => "external",
            Kind::BuiltinFull => "builtin-full",
            Kind::BuiltinPartial => "builtin-partial",
            Kind::Absent => "absent",
        }
    }

    /// Whether nv-rs has it built in (fully or partly).
    #[must_use]
    pub fn is_builtin(self) -> bool {
        matches!(self, Kind::BuiltinFull | Kind::BuiltinPartial)
    }
}

/// How one target provides an entry. No number lives here: coverage is read from the installed nv-rs's
/// `capabilities.json`, through the `extender` id.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provided {
    /// The kind.
    #[serde(default)]
    pub kind: Kind,
    /// The id of the extender in `capabilities.json`; required for the built-in kinds and refused for the others.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extender: Option<String>,
}

/// `provided_by.<target>`: what provides an entry on each target. An entry that says nothing is `external` on
/// both.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvidedBy {
    /// Vanilla executable with xNVSE (Proton on Linux, native on Windows).
    #[serde(default)]
    pub vanilla: Provided,
    /// nv-rs.
    #[serde(default, rename = "nv-rs")]
    pub nv_rs: Provided,
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
    /// What provides it on each target.
    #[serde(default)]
    pub provided_by: ProvidedBy,
    /// The game executable version (`a.b.c.d`) the plugin requires. Absent means nobody recorded it: the Doctor
    /// says "unknown" and never guesses.
    #[serde(default)]
    pub game_version: Option<String>,
    /// The lowest xNVSE version (`a.b.c.d`) the plugin requires. Absent means nobody recorded it.
    #[serde(default)]
    pub min_xnvse: Option<String>,
    /// For a script extender (`role = "extender"`): the DLL version the catalogue expects on the vanilla target
    /// (`a.b.c.d`). Absent means nobody recorded it.
    #[serde(default)]
    pub version: Option<String>,
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

fn check_provided(id: &str, by: &ProvidedBy) -> Result<(), CatalogError> {
    for (target, provided) in [("vanilla", &by.vanilla), ("nv-rs", &by.nv_rs)] {
        if target == "vanilla" && provided.kind.is_builtin() {
            return fail(format!(
                "plugin {id}: provided_by.vanilla cannot be {}; the vanilla target has no built-in extender",
                provided.kind.label()
            ));
        }
        match (provided.kind.is_builtin(), provided.extender.as_deref()) {
            (true, None | Some("")) => {
                return fail(format!(
                    "plugin {id}: provided_by.{target} is {} and needs `extender`, the id in capabilities.json",
                    provided.kind.label()
                ));
            }
            (false, Some(_)) => {
                return fail(format!(
                    "plugin {id}: provided_by.{target} is {}, which takes no `extender`",
                    provided.kind.label()
                ));
            }
            _ => {}
        }
    }
    Ok(())
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
            check_provided(&p.id, &p.provided_by)?;
            for (field, value) in [
                ("game_version", &p.game_version),
                ("min_xnvse", &p.min_xnvse),
                ("version", &p.version),
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
    fn the_mods_the_doctor_found_missing_have_entries() {
        let catalog = Catalog::builtin();
        for (id, dll, nexus) in [
            (
                "vanilla-plus-terrain",
                "VanillaPlusTerrain.dll",
                Some(92519),
            ),
            ("vats-lag-fix", "VATSLagFix.dll", None),
            (
                "viewmodel-shading-fix",
                "Viewmodel Shading Fix.dll",
                Some(84781),
            ),
            ("yui", "yUI.dll", Some(74357)),
            ("zlib-updated", "zlibUpdate.dll", Some(85267)),
        ] {
            let p = catalog.plugin(id).unwrap();
            assert_eq!(p.dll, dll, "{id}");
            assert_eq!(p.nexus_mod_id, nexus, "{id}");
            assert_eq!(catalog.by_dll(&dll.to_uppercase()).unwrap().id, id);
            assert_eq!(p.provided_by.vanilla.kind, Kind::External, "{id}");
            assert_eq!(p.provided_by.nv_rs.kind, Kind::External, "{id}");
            assert!(p.provenance.verified && p.provenance.verified_on == "2026-11-14");
        }
        assert_eq!(catalog.plugin("yui").unwrap().ini, ["yUI.ini"]);
        // The owner has not decided these two; neither is marked absent on any target.
        for id in ["johnnyguitar", "showoff"] {
            if let Some(p) = catalog.plugin(id) {
                assert_ne!(p.provided_by.nv_rs.kind, Kind::Absent, "{id}");
                assert_ne!(p.provided_by.vanilla.kind, Kind::Absent, "{id}");
            }
        }
    }

    #[test]
    fn defaults_are_empty_and_every_target_is_external() {
        let catalog = Catalog::parse(&one("")).unwrap();
        let p = &catalog.plugins[0];
        assert!(p.ini.is_empty() && p.requires.is_empty() && p.nexus_mod_id.is_none());
        assert_eq!(p.provided_by, ProvidedBy::default());
        assert_eq!(p.provided_by.nv_rs.kind, Kind::External);
    }

    #[test]
    fn provided_by_has_the_four_kinds_per_target() {
        let text =
            one("[plugin.provided_by.nv-rs]\nkind = \"builtin-partial\"\nextender = \"jip-ln\"\n");
        let text = text.replace("[plugin.provenance]", "\n[plugin.provenance]");
        let catalog = Catalog::parse(&text).unwrap();
        let by = &catalog.plugins[0].provided_by;
        assert_eq!(by.vanilla.kind, Kind::External);
        assert_eq!(by.nv_rs.kind, Kind::BuiltinPartial);
        assert_eq!(by.nv_rs.extender.as_deref(), Some("jip-ln"));
        for (kind, parsed) in [
            ("external", Kind::External),
            ("builtin-full", Kind::BuiltinFull),
            ("absent", Kind::Absent),
        ] {
            let extender = if kind == "builtin-full" {
                "extender = \"x\"\n"
            } else {
                ""
            };
            let text = format!(
                "schema = 1\n[[plugin]]\nid = \"a\"\nname = \"A\"\ndll = \"a.dll\"\n[plugin.provided_by.nv-rs]\nkind = \"{kind}\"\n{extender}\n{PROV}"
            );
            assert_eq!(
                Catalog::parse(&text).unwrap().plugins[0]
                    .provided_by
                    .nv_rs
                    .kind,
                parsed
            );
        }
    }

    #[test]
    fn a_provided_by_that_makes_no_sense_is_refused() {
        let with = |table: &str| {
            format!(
                "schema = 1\n[[plugin]]\nid = \"a\"\nname = \"A\"\ndll = \"a.dll\"\n{table}\n{PROV}"
            )
        };
        let bad = |table: &str, want: &str| {
            let error = Catalog::parse(&with(table)).unwrap_err().to_string();
            assert!(error.contains(want), "{error} should contain {want}");
        };
        bad(
            "[plugin.provided_by.nv-rs]\nkind = \"builtin-partial\"",
            "needs `extender`",
        );
        bad(
            "[plugin.provided_by.nv-rs]\nkind = \"absent\"\nextender = \"x\"",
            "takes no `extender`",
        );
        bad(
            "[plugin.provided_by.vanilla]\nkind = \"builtin-full\"\nextender = \"x\"",
            "vanilla target has no built-in",
        );
        bad(
            "[plugin.provided_by.nv-rs]\nkind = \"sometimes\"",
            "sometimes",
        );
        // Coverage is never typed into the catalogue: there is no field for a number.
        bad(
            "[plugin.provided_by.nv-rs]\nkind = \"builtin-partial\"\nextender = \"x\"\nimplemented = 3",
            "implemented",
        );
        bad(
            "[plugin.provided_by.nv-rs]\nkind = \"builtin-partial\"\nextender = \"x\"\ntotal = 3",
            "total",
        );
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
