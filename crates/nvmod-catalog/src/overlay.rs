// SPDX-License-Identifier: GPL-3.0-only
//! The player's catalogue overlay: `catalogue.d/*.toml` in the config directory. Data only: the same entry schema
//! as the shipped file, a provenance on every entry, no claim beyond `external`, and no shadowing of a shipped
//! entry. A bad file or entry is reported as a [`Note`] and ignored; the rest of the catalogue still loads.

use super::{Catalog, File, Plugin, Source, check_plugin};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// The most overlay files read.
pub const MAX_FILES: usize = 32;
/// The most bytes of one overlay file.
pub const MAX_FILE_BYTES: u64 = 64 * 1024;
/// The most entries taken from all overlay files together.
pub const MAX_ENTRIES: usize = 256;
/// The folder of overlay files inside the config directory.
pub const OVERLAY_DIR: &str = "catalogue.d";

/// One thing wrong with the overlay. `code` is stable (`catalog.overlay-clash`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// Stable identifier.
    pub code: &'static str,
    /// The file, or the entry in a file (`file.toml: id`).
    pub subject: String,
    /// What was found and what happened to it.
    pub message: String,
}

fn note(code: &'static str, subject: impl Into<String>, message: impl Into<String>) -> Note {
    Note {
        code,
        subject: subject.into(),
        message: message.into(),
    }
}

/// The config directory of nvmod: `NVMOD_CONFIG_DIR`; else `$XDG_CONFIG_HOME/nvmod` or `~/.config/nvmod`; on
/// Windows `%APPDATA%\nvmod`.
#[must_use]
pub fn config_dir() -> PathBuf {
    let var = |name: &str| {
        std::env::var_os(name)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if let Some(dir) = var("NVMOD_CONFIG_DIR") {
        return dir;
    }
    if cfg!(windows) {
        return var("APPDATA")
            .unwrap_or_else(|| PathBuf::from("."))
            .join("nvmod");
    }
    var("XDG_CONFIG_HOME")
        .or_else(|| var("HOME").map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("nvmod")
}

impl Catalog {
    /// The shipped catalogue plus the overlay in the config directory.
    #[must_use]
    pub fn user() -> Catalog {
        Catalog::builtin().with_overlay_dir(&config_dir())
    }

    /// This catalogue plus the files of `<config>/catalogue.d`. A missing folder adds nothing.
    #[must_use]
    pub fn with_overlay_dir(self, config: &Path) -> Catalog {
        let dir = config.join(OVERLAY_DIR);
        let Ok(read) = fs::read_dir(&dir) else {
            return self;
        };
        let mut names: Vec<PathBuf> = read
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "toml") && p.is_file())
            .collect();
        names.sort();
        let mut notes = Vec::new();
        if names.len() > MAX_FILES {
            notes.push(note(
                "catalog.overlay-too-many",
                OVERLAY_DIR,
                format!(
                    "{} files; only the first {MAX_FILES} (by name) are read",
                    names.len()
                ),
            ));
            names.truncate(MAX_FILES);
        }
        let mut files = Vec::new();
        for path in names {
            let name = path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            match fs::metadata(&path) {
                Ok(m) if m.len() > MAX_FILE_BYTES => notes.push(note(
                    "catalog.overlay-too-large",
                    &name,
                    format!(
                        "{} bytes; the limit is {MAX_FILE_BYTES}; file ignored",
                        m.len()
                    ),
                )),
                Ok(_) => match fs::read_to_string(&path) {
                    Ok(text) => files.push((name, text)),
                    Err(e) => notes.push(note(
                        "catalog.overlay-invalid",
                        &name,
                        format!("cannot be read: {e}; file ignored"),
                    )),
                },
                Err(e) => notes.push(note(
                    "catalog.overlay-invalid",
                    &name,
                    format!("cannot be read: {e}; file ignored"),
                )),
            }
        }
        let mut catalog = self.with_overlay(&files);
        notes.append(&mut catalog.notes);
        catalog.notes = notes;
        catalog
    }

    /// This catalogue plus overlay files given as `(file name, text)`, in order.
    #[must_use]
    pub fn with_overlay(mut self, files: &[(String, String)]) -> Catalog {
        let mut taken = 0usize;
        let mut ids: HashSet<String> = self.plugins.iter().map(|p| p.id.clone()).collect();
        let mut dlls: HashSet<String> = self.plugins.iter().map(|p| p.dll.to_lowercase()).collect();
        for (name, text) in files {
            let entries = match parse_file(text) {
                Ok(entries) => entries,
                Err(why) => {
                    self.notes.push(note(
                        "catalog.overlay-invalid",
                        name,
                        format!("{why}; file ignored"),
                    ));
                    continue;
                }
            };
            // References resolve against the shipped catalogue, earlier files and this file.
            let known: HashSet<&str> = ids
                .iter()
                .map(String::as_str)
                .chain(entries.iter().map(|p| p.id.as_str()))
                .collect();
            if let Some(why) = entries.iter().find_map(|p| unknown_reference(p, &known)) {
                self.notes.push(note(
                    "catalog.overlay-invalid",
                    name,
                    format!("{why}; file ignored"),
                ));
                continue;
            }
            for mut p in entries {
                let subject = format!("{name}: {}", p.id);
                if taken >= MAX_ENTRIES {
                    self.notes.push(note(
                        "catalog.overlay-too-many",
                        subject,
                        format!("more than {MAX_ENTRIES} overlay entries; entry ignored"),
                    ));
                    continue;
                }
                if ids.contains(&p.id) || dlls.contains(&p.dll.to_lowercase()) {
                    self.notes.push(note(
                        "catalog.overlay-clash",
                        subject,
                        "the id or the dll is already in the catalogue; an overlay entry never shadows another; entry ignored",
                    ));
                    continue;
                }
                if p.provided_by.vanilla.kind != crate::Kind::External
                    || p.provided_by.nv_rs.kind != crate::Kind::External
                {
                    self.notes.push(note(
                        "catalog.overlay-claim",
                        subject,
                        "an overlay entry can only be `external`; it cannot say what nv-rs provides; entry ignored",
                    ));
                    continue;
                }
                taken += 1;
                ids.insert(p.id.clone());
                dlls.insert(p.dll.to_lowercase());
                p.source = Source::User;
                self.plugins.push(p);
            }
        }
        self
    }
}

/// The entries of one overlay file, each checked on its own.
fn parse_file(text: &str) -> Result<Vec<Plugin>, String> {
    let file: File = toml::from_str(text).map_err(|e| e.message().to_owned())?;
    if file.schema != crate::SCHEMA {
        return Err(format!("schema {} is not understood", file.schema));
    }
    if !file.mods.is_empty() {
        return Err("`[[mod]]` entries are not allowed in an overlay".to_owned());
    }
    let mut seen = HashSet::new();
    for p in &file.plugin {
        if p.id.is_empty() || p.dll.is_empty() || !seen.insert(p.id.as_str()) {
            return Err(format!(
                "plugin id `{}` is empty or repeated, or has no dll",
                p.id
            ));
        }
        check_plugin(p).map_err(|e| e.0)?;
    }
    Ok(file.plugin)
}

fn unknown_reference(p: &Plugin, known: &HashSet<&str>) -> Option<String> {
    p.requires
        .iter()
        .chain(&p.conflicts)
        .chain(p.replaced_by.iter())
        .find(|t| !known.contains(t.as_str()))
        .map(|t| format!("plugin {}: refers to unknown plugin `{t}`", p.id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Write as _;

    const PROV: &str =
        "[plugin.provenance]\nsource = \"s\"\nverified_on = \"2026-11-14\"\nverified = false\n";

    fn entry(id: &str, dll: &str, extra: &str) -> String {
        format!(
            "schema = 1\n[[plugin]]\nid = \"{id}\"\nname = \"N\"\ndll = \"{dll}\"\n{extra}\n{PROV}"
        )
    }

    fn with(files: &[(&str, String)]) -> Catalog {
        let files: Vec<(String, String)> = files
            .iter()
            .map(|(n, t)| ((*n).to_owned(), t.clone()))
            .collect();
        Catalog::builtin().with_overlay(&files)
    }

    #[test]
    fn an_overlay_entry_is_recognised_and_marked_as_user() {
        let c = with(&[("a.toml", entry("mine", "Mine.dll", ""))]);
        assert!(c.notes.is_empty(), "{:?}", c.notes);
        let p = c.by_dll("mine.DLL").unwrap();
        assert_eq!(p.source, Source::User);
        assert_eq!(c.plugin("xnvse").unwrap().source, Source::Shipped);
        assert_eq!(c.plugins.len(), Catalog::builtin().plugins.len() + 1);
    }

    #[test]
    fn an_invalid_file_is_a_note_and_the_rest_still_loads() {
        let c = with(&[
            (
                "bad.toml",
                "schema = 1\n[[plugin]]\nid = \"x\"\n".to_owned(),
            ),
            ("good.toml", entry("mine", "Mine.dll", "")),
        ]);
        assert_eq!(c.notes.len(), 1);
        assert_eq!(c.notes[0].code, "catalog.overlay-invalid");
        assert!(c.plugin("mine").is_some());
    }

    #[test]
    fn an_entry_without_provenance_a_user_source_field_or_a_mod_table_is_refused() {
        let no_prov = "schema = 1\n[[plugin]]\nid = \"x\"\nname = \"N\"\ndll = \"x.dll\"\n";
        let sets_source = entry("x", "x.dll", "source = \"shipped\"");
        let mods = format!(
            "{}\n[[mod]]\nname = \"M\"\nnexus_mod_id = 1\n[mod.provenance]\nsource = \"s\"\nverified_on = \"2026-11-14\"\nverified = false\n",
            entry("x", "x.dll", "")
        );
        for text in [no_prov.to_owned(), sets_source, mods] {
            let c = with(&[("f.toml", text)]);
            assert_eq!(c.notes.len(), 1, "{:?}", c.notes);
            assert!(c.plugin("x").is_none());
        }
    }

    #[test]
    fn an_overlay_never_shadows_a_shipped_entry() {
        let shipped = Catalog::builtin().plugins[0].clone();
        let by_id = entry(&shipped.id, "other.dll", "");
        let by_dll = entry("fresh", &shipped.dll.to_uppercase(), "");
        let c = with(&[("a.toml", by_id), ("b.toml", by_dll)]);
        assert_eq!(
            c.notes.iter().map(|n| n.code).collect::<Vec<_>>(),
            ["catalog.overlay-clash", "catalog.overlay-clash"]
        );
        assert_eq!(c.plugins.len(), Catalog::builtin().plugins.len());
        assert_eq!(c.plugin(&shipped.id), Some(&shipped));
    }

    #[test]
    fn an_overlay_cannot_claim_more_than_external() {
        let text = format!(
            "schema = 1\n[[plugin]]\nid = \"x\"\nname = \"N\"\ndll = \"x.dll\"\n[plugin.provided_by.nv-rs]\nkind = \"builtin-full\"\nextender = \"xnvse\"\n{PROV}"
        );
        let c = with(&[("f.toml", text)]);
        assert_eq!(c.notes[0].code, "catalog.overlay-claim");
        assert!(c.plugin("x").is_none());
    }

    #[test]
    fn references_resolve_against_the_shipped_entries_and_unknown_ones_refuse_the_file() {
        let ok = entry("x", "x.dll", "requires = [\"xnvse\"]");
        let bad = entry("y", "y.dll", "requires = [\"nobody\"]");
        let c = with(&[("ok.toml", ok), ("bad.toml", bad)]);
        assert!(c.plugin("x").is_some() && c.plugin("y").is_none());
        assert_eq!(c.notes.len(), 1);
    }

    #[test]
    fn the_entry_cap_is_enforced() {
        let mut many = String::new();
        for i in 0..MAX_ENTRIES + 3 {
            writeln!(
                many,
                "[[plugin]]\nid = \"p{i}\"\nname = \"N\"\ndll = \"p{i}.dll\"\n{PROV}"
            )
            .unwrap();
        }
        let c = with(&[("big.toml", format!("schema = 1\n{many}"))]);
        let added = c.plugins.len() - Catalog::builtin().plugins.len();
        assert_eq!(added, MAX_ENTRIES);
        assert_eq!(c.notes.len(), 3);
        assert!(c.notes.iter().all(|n| n.code == "catalog.overlay-too-many"));
    }

    #[test]
    fn the_folder_is_read_in_name_order_with_size_and_count_caps() {
        let tmp = std::env::temp_dir().join(format!("nvmod-overlay-{}", std::process::id()));
        let dir = tmp.join(OVERLAY_DIR);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("b.toml"), entry("b", "b.dll", "")).unwrap();
        fs::write(dir.join("a.toml"), entry("a", "a.dll", "")).unwrap();
        fs::write(dir.join("ignored.txt"), "x").unwrap();
        fs::write(
            dir.join("huge.toml"),
            vec![b'#'; usize::try_from(MAX_FILE_BYTES).unwrap() + 1],
        )
        .unwrap();
        let c = Catalog::builtin().with_overlay_dir(&tmp);
        let n = Catalog::builtin().plugins.len();
        assert_eq!(c.plugins[n].id, "a");
        assert_eq!(c.plugins[n + 1].id, "b");
        assert_eq!(c.notes.len(), 1);
        assert_eq!(c.notes[0].code, "catalog.overlay-too-large");
        for i in 0..MAX_FILES {
            fs::write(dir.join(format!("z{i:02}.toml")), "schema = 1\n").unwrap();
        }
        let c = Catalog::builtin().with_overlay_dir(&tmp);
        assert!(c.notes.iter().any(|n| n.code == "catalog.overlay-too-many"));
        let _ = fs::remove_dir_all(&tmp);
        let none = Catalog::builtin().with_overlay_dir(&tmp);
        assert_eq!(none.notes, []);
    }
}
