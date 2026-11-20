// SPDX-License-Identifier: GPL-3.0-only
//! A mod's `meta.ini` (a Qt `QSettings` file) read and edited line by line, so a file Mod Organizer 2 wrote comes
//! back byte for byte except for the lines that were set. Comments, blank lines, key order, line endings, unknown
//! keys and `@Variant` / `@ByteArray` values are never re-serialised (research, 2.8 and 2.12 item 5).

use crate::ini::qt_string;
use crate::text::decode;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Section(String),
    Entry { key: String, value_at: usize },
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Line {
    raw: Vec<u8>,
    eol: Vec<u8>,
    kind: Kind,
    /// The section the line belongs to (`General` before the first header).
    section: String,
}

/// The `meta.ini` section of the marker the Toolbox writes into a generated LOD Mod.
pub const GENERATED_SECTION: &str = "raulGeneratedLod";

/// Whether the Mod folder `mod_dir` is marked as generated: its `meta.ini` (found without case) has
/// `generated=true` and a `profile` in the [`GENERATED_SECTION`] section. A missing or unreadable file is not marked.
#[must_use]
pub fn is_generated_mod_dir(mod_dir: &std::path::Path) -> bool {
    let Some(file) = std::fs::read_dir(mod_dir).ok().and_then(|entries| {
        entries
            .flatten()
            .find(|e| {
                e.file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("meta.ini")
            })
            .map(|e| e.path())
    }) else {
        return false;
    };
    crate::text::read_limited(&file).is_ok_and(|bytes| MetaIni::parse(&bytes).is_generated())
}

/// An editable `meta.ini`.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MetaIni {
    lines: Vec<Line>,
}

/// One entry of the `[installedFiles]` array.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledFile {
    /// Nexus mod id (0 when absent).
    pub modid: u64,
    /// Nexus file id (0 when absent).
    pub fileid: u64,
}

/// The keys of `docs/research/mo2-internals-and-import.md` section 2.8 that nvmod keeps, read as values.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct MetaFields {
    /// `gameName`.
    pub game_name: Option<String>,
    /// `modid`; `None` for 0 or absent.
    pub modid: Option<u64>,
    /// `version`, as written.
    pub version: Option<String>,
    /// `newestVersion`.
    pub newest_version: Option<String>,
    /// `ignoredVersion`.
    pub ignored_version: Option<String>,
    /// `installationFile`, the file name only (never a directory, never a Windows path).
    pub installation_file: Option<String>,
    /// `repository`.
    pub repository: Option<String>,
    /// `category`, as written (`"<primary>,<csv of ids>,"`).
    pub category: Option<String>,
    /// `nexusCategory`.
    pub nexus_category: Option<String>,
    /// `comments`.
    pub comments: Option<String>,
    /// `notes`.
    pub notes: Option<String>,
    /// `color`, as written (an opaque `@Variant`).
    pub color: Option<String>,
    /// `endorsed`, as written.
    pub endorsed: Option<String>,
    /// `tracked`, as written.
    pub tracked: Option<String>,
    /// The `[installedFiles]` array.
    pub installed_files: Vec<InstalledFile>,
    /// `[Plugins]`: settings per plugin (`<plugin>/<key>`).
    pub plugins: BTreeMap<String, BTreeMap<String, String>>,
}

/// The file name of an archive path of either kind (`C:\Users\x\Downloads\a.7z` and `/home/x/a.7z` give `a.7z`).
#[must_use]
pub fn file_name_only(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or_default()
}

/// A value as `QSettings` writes a plain string: quoted, with `\` and `"` escaped, when it holds a character that
/// would otherwise be read as syntax.
#[must_use]
pub fn quote_value(value: &str) -> String {
    let plain = value == value.trim()
        && !value.starts_with(['@', '"'])
        && !value.contains([',', ';', '"', '\\', '\n', '\r', '=']);
    if plain {
        value.to_owned()
    } else {
        format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
    }
}

fn trim_ascii(bytes: &[u8]) -> &[u8] {
    bytes.trim_ascii()
}

impl MetaIni {
    /// Read `bytes`. Never fails: a line that is not a header, an entry or a comment is kept as it is.
    #[must_use]
    pub fn parse(bytes: &[u8]) -> MetaIni {
        let mut lines = Vec::new();
        let mut section = "General".to_owned();
        let mut rest = bytes;
        while !rest.is_empty() {
            let (body, tail, newline) = match rest.iter().position(|&b| b == b'\n') {
                Some(i) => (&rest[..i], &rest[i + 1..], true),
                None => (rest, &rest[rest.len()..], false),
            };
            let (raw, eol): (&[u8], &[u8]) = if newline {
                match body.strip_suffix(b"\r") {
                    Some(r) => (r, b"\r\n"),
                    None => (body, b"\n"),
                }
            } else {
                (body, b"")
            };
            let trimmed = trim_ascii(raw);
            let kind = if trimmed.starts_with(b";") || trimmed.starts_with(b"#") {
                Kind::Other
            } else if let Some(name) = trimmed
                .strip_prefix(b"[")
                .and_then(|r| r.strip_suffix(b"]"))
            {
                section = decode(trim_ascii(name));
                Kind::Section(section.clone())
            } else if let Some(eq) = raw.iter().position(|&b| b == b'=') {
                Kind::Entry {
                    key: decode(trim_ascii(&raw[..eq])),
                    value_at: eq + 1,
                }
            } else {
                Kind::Other
            };
            lines.push(Line {
                raw: raw.to_vec(),
                eol: eol.to_vec(),
                kind,
                section: section.clone(),
            });
            rest = tail;
        }
        MetaIni { lines }
    }

    /// The file as bytes: what was parsed, with the edits made.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for line in &self.lines {
            out.extend_from_slice(&line.raw);
            out.extend_from_slice(&line.eol);
        }
        out
    }

    fn find(&self, section: &str, key: &str) -> Option<usize> {
        self.lines.iter().rposition(|l| {
            l.section.eq_ignore_ascii_case(section)
                && matches!(&l.kind, Kind::Entry { key: k, .. } if k.eq_ignore_ascii_case(key))
        })
    }

    /// The raw value of `key` in `section` (the last one when repeated), trimmed.
    #[must_use]
    pub fn get(&self, section: &str, key: &str) -> Option<String> {
        let line = &self.lines[self.find(section, key)?];
        let Kind::Entry { value_at, .. } = line.kind else {
            return None;
        };
        Some(decode(trim_ascii(&line.raw[value_at..])))
    }

    /// Whether the file carries the marker of a generated Mod: `generated=true` and a non-empty `profile` in
    /// [`GENERATED_SECTION`]. Every other key is left alone.
    #[must_use]
    pub fn is_generated(&self) -> bool {
        self.get(GENERATED_SECTION, "generated")
            .is_some_and(|v| v.eq_ignore_ascii_case("true"))
            && self
                .get_string(GENERATED_SECTION, "profile")
                .is_some_and(|p| !p.is_empty())
    }

    /// Like [`MetaIni::get`] with Qt's `@ByteArray`, quotes and doubled backslashes removed.
    #[must_use]
    pub fn get_string(&self, section: &str, key: &str) -> Option<String> {
        self.get(section, key).map(|v| qt_string(&v))
    }

    fn eol(&self) -> Vec<u8> {
        self.lines
            .iter()
            .find(|l| !l.eol.is_empty())
            .map_or_else(|| b"\r\n".to_vec(), |l| l.eol.clone())
    }

    /// Set `key` in `section` to the raw text `value`. An existing line changes only after its `=`; a new key goes
    /// at the end of its section (a new section at the end of the file), in the file's own line ending.
    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        if let Some(at) = self.find(section, key) {
            if let Kind::Entry { value_at, .. } = self.lines[at].kind {
                self.lines[at].raw.truncate(value_at);
                self.lines[at].raw.extend_from_slice(value.as_bytes());
            }
            return;
        }
        let eol = self.eol();
        let last_content = |lines: &[Line], name: &str| {
            lines.iter().rposition(|l| {
                l.section.eq_ignore_ascii_case(name) && !trim_ascii(&l.raw).is_empty()
            })
        };
        let entry = |section: &str| Line {
            raw: format!("{key}={value}").into_bytes(),
            eol: eol.clone(),
            kind: Kind::Entry {
                key: key.to_owned(),
                value_at: key.len() + 1,
            },
            section: section.to_owned(),
        };
        if let Some(i) = last_content(&self.lines, section) {
            if self.lines[i].eol.is_empty() {
                self.lines[i].eol.clone_from(&eol);
            }
            let name = self.lines[i].section.clone();
            self.lines.insert(i + 1, entry(&name));
        } else if section.eq_ignore_ascii_case("General") && self.lines.is_empty() {
            self.lines.push(Line {
                raw: b"[General]".to_vec(),
                eol: eol.clone(),
                kind: Kind::Section("General".to_owned()),
                section: "General".to_owned(),
            });
            self.lines.push(entry("General"));
        } else {
            if let Some(last) = self.lines.last_mut() {
                if last.eol.is_empty() {
                    last.eol.clone_from(&eol);
                }
                let section = last.section.clone();
                self.lines.push(Line {
                    raw: Vec::new(),
                    eol: eol.clone(),
                    kind: Kind::Other,
                    section,
                });
            }
            self.lines.push(Line {
                raw: format!("[{section}]").into_bytes(),
                eol: eol.clone(),
                kind: Kind::Section(section.to_owned()),
                section: section.to_owned(),
            });
            self.lines.push(entry(section));
        }
    }

    /// Remove every line that sets `key` in `section`.
    pub fn remove(&mut self, section: &str, key: &str) {
        self.lines.retain(|l| {
            !(l.section.eq_ignore_ascii_case(section)
                && matches!(&l.kind, Kind::Entry { key: k, .. } if k.eq_ignore_ascii_case(key)))
        });
    }

    /// Set `[General] modid`.
    pub fn set_modid(&mut self, id: u64) {
        self.set("General", "modid", &id.to_string());
    }

    /// Set `[General] version`.
    pub fn set_version(&mut self, version: &str) {
        self.set("General", "version", &quote_value(version));
    }

    /// Set `[General] installationFile` to the file name of `path` (the directory part is personal and not
    /// portable, so it is never written).
    pub fn set_installation_file(&mut self, path: &str) {
        self.set(
            "General",
            "installationFile",
            &quote_value(file_name_only(path)),
        );
    }

    /// The typed view of the keys nvmod keeps.
    #[must_use]
    pub fn fields(&self) -> MetaFields {
        let general = |key: &str| self.get_string("General", key).filter(|v| !v.is_empty());
        let number = |section: &str, key: &str| {
            self.get_string(section, key)
                .and_then(|v| v.trim().parse::<u64>().ok())
                .unwrap_or(0)
        };
        let size = number("installedFiles", "size");
        let installed_files = (1..=size.min(100_000))
            .map(|i| InstalledFile {
                modid: number("installedFiles", &format!("{i}\\modid")),
                fileid: number("installedFiles", &format!("{i}\\fileid")),
            })
            .collect();
        let mut plugins: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for line in &self.lines {
            if let Kind::Entry { key, value_at } = &line.kind
                && line.section.eq_ignore_ascii_case("Plugins")
                && let Some((plugin, setting)) = key.split_once('/')
            {
                plugins.entry(plugin.to_owned()).or_default().insert(
                    setting.to_owned(),
                    decode(trim_ascii(&line.raw[*value_at..])),
                );
            }
        }
        MetaFields {
            game_name: general("gameName"),
            modid: general("modid")
                .and_then(|v| v.parse().ok())
                .filter(|&id| id != 0),
            version: general("version"),
            newest_version: general("newestVersion"),
            ignored_version: general("ignoredVersion"),
            installation_file: general("installationFile")
                .map(|p| file_name_only(&p).to_owned())
                .filter(|n| !n.is_empty()),
            repository: general("repository"),
            category: general("category"),
            nexus_category: general("nexusCategory"),
            comments: general("comments"),
            notes: general("notes"),
            color: self.get("General", "color").filter(|v| !v.is_empty()),
            endorsed: general("endorsed"),
            tracked: general("tracked"),
            installed_files,
            plugins,
        }
    }
}

/// What a Nexus archive name carries: `<name>-<modid>-<version>-<timestamp>.<ext>`, the version written with `-`
/// for `.`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NexusName {
    /// The mod's Nexus id.
    pub modid: u64,
    /// The version, with dots.
    pub version: String,
}

/// Read the Nexus naming from an archive file name. The timestamp must be 9 to 11 digits; the id is the first
/// number after which only version-like parts (short, digits with at most a letter) and the timestamp follow, so
/// a name that itself holds such a run of numbers can be read wrongly (the id is then wrong, never invented).
#[must_use]
pub fn parse_nexus_name(file_name: &str) -> Option<NexusName> {
    let stem = file_name.rsplit_once('.').map_or(file_name, |(s, _)| s);
    let stem = stem.strip_suffix(".tar").unwrap_or(stem);
    let tokens: Vec<&str> = stem.split('-').collect();
    let (timestamp, rest) = tokens.split_last()?;
    if !(9..=11).contains(&timestamp.len()) || !timestamp.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let version_like = |t: &&str| {
        let digits = t.chars().take_while(char::is_ascii_digit).count();
        digits > 0
            && digits <= 6
            && t.len() - digits <= 1
            && t[digits..].chars().all(|c| c.is_ascii_alphabetic())
    };
    let found = (1..rest.len()).find_map(|i| {
        let id = rest[i];
        let version = &rest[i + 1..];
        if id.is_empty()
            || !id.chars().all(|c| c.is_ascii_digit())
            || version.is_empty()
            || !version.iter().all(version_like)
        {
            return None;
        }
        Some(NexusName {
            modid: id.parse().ok()?,
            version: version.join("."),
        })
    });
    found.filter(|n| n.modid != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &[u8] = b"[General]\r\ngameName=newvegas\r\nmodid=83815\r\nversion=d2026.3.1.0\r\ninstallationFile=C:/Users/someone/Downloads/Mod-83815.7z\r\ncolor=@Variant(\\0\\0\\0\\x43\\x1\\xff\\xff\\0\\0\\0\\0\\0\\0)\r\n; kept comment\r\nfuture=@ByteArray(a\\x00b)\r\n\r\n[installedFiles]\r\n1\\modid=83815\r\n1\\fileid=4\r\nsize=1\r\n\r\n[Plugins]\r\nA.esp/lock=true\r\nA.esp/priority=5\r\n";

    #[test]
    fn an_untouched_file_comes_back_byte_for_byte() {
        assert_eq!(MetaIni::parse(SAMPLE).to_bytes(), SAMPLE);
        let odd = b"x=1\n\r\n[a]\nk = v\r\n;c\nno newline at end";
        assert_eq!(MetaIni::parse(odd).to_bytes(), odd);
    }

    #[test]
    fn fields_are_read_with_the_file_name_only() {
        let f = MetaIni::parse(SAMPLE).fields();
        assert_eq!(f.game_name.as_deref(), Some("newvegas"));
        assert_eq!(f.modid, Some(83815));
        assert_eq!(f.version.as_deref(), Some("d2026.3.1.0"));
        assert_eq!(f.installation_file.as_deref(), Some("Mod-83815.7z"));
        assert_eq!(
            f.installed_files,
            [InstalledFile {
                modid: 83815,
                fileid: 4
            }]
        );
        assert_eq!(f.plugins["A.esp"]["priority"], "5");
        assert!(f.color.unwrap().starts_with("@Variant("));
    }

    #[test]
    fn setting_a_key_changes_only_its_line() {
        let mut meta = MetaIni::parse(SAMPLE);
        meta.set("General", "version", "d2026.11.2.0");
        let new = meta.to_bytes();
        let old: Vec<&[u8]> = SAMPLE.split_inclusive(|&b| b == b'\n').collect();
        let new_lines: Vec<&[u8]> = new.split_inclusive(|&b| b == b'\n').collect();
        assert_eq!(old.len(), new_lines.len());
        let changed: Vec<usize> = (0..old.len()).filter(|&i| old[i] != new_lines[i]).collect();
        assert_eq!(changed, [3]);
        assert_eq!(new_lines[3], b"version=d2026.11.2.0\r\n");
    }

    #[test]
    fn a_new_key_goes_to_the_end_of_its_section_and_a_new_section_to_the_end() {
        let mut meta = MetaIni::parse(SAMPLE);
        meta.set("General", "newestVersion", "2.0");
        meta.set("Extra", "k", "v");
        let text = String::from_utf8(meta.to_bytes()).unwrap();
        assert!(
            text.contains(
                "future=@ByteArray(a\\x00b)\r\nnewestVersion=2.0\r\n\r\n[installedFiles]"
            ),
            "{text}"
        );
        assert!(
            text.ends_with("A.esp/priority=5\r\n\r\n[Extra]\r\nk=v\r\n"),
            "{text}"
        );
        let mut empty = MetaIni::default();
        empty.set_modid(7);
        assert_eq!(empty.to_bytes(), b"[General]\r\nmodid=7\r\n");
        let mut lf = MetaIni::parse(b"[General]\nmodid=1");
        lf.set_version("1.0");
        assert_eq!(lf.to_bytes(), b"[General]\nmodid=1\nversion=1.0\n");
    }

    #[test]
    fn the_installation_file_is_a_file_name_and_values_are_quoted_when_needed() {
        let mut meta = MetaIni::default();
        meta.set_installation_file("C:\\Users\\someone\\Downloads\\Mod, v2.7z");
        assert_eq!(
            meta.fields().installation_file.as_deref(),
            Some("Mod, v2.7z")
        );
        assert!(
            !String::from_utf8(meta.to_bytes())
                .unwrap()
                .contains("Users")
        );
        meta.remove("General", "installationFile");
        assert_eq!(meta.fields().installation_file, None);
    }

    #[test]
    fn nexus_names_carry_the_id_and_the_version() {
        assert_eq!(
            parse_nexus_name("Some Mod-83815-1-10-2-1699999999.7z"),
            Some(NexusName {
                modid: 83815,
                version: "1.10.2".into()
            })
        );
        assert_eq!(
            parse_nexus_name("Two-Part-Name-12-5-17a-1699999999.zip").map(|n| (n.modid, n.version)),
            Some((12, "5.17a".to_owned()))
        );
        assert_eq!(parse_nexus_name("plain-archive.zip"), None);
        assert_eq!(parse_nexus_name("Mod-0-1-0-1699999999.zip"), None);
        assert_eq!(parse_nexus_name("Mod-12-1-0.zip"), None);
    }
}
