// SPDX-License-Identifier: GPL-3.0-only
//! What a program did to the Staging tree since it was built, told from the per-path records of the manifest.
//!
//! The tree is made of hard links, so a program that writes in place changes the Mod's own file, and one that
//! replaces a file (write a temporary, rename over it) breaks the link and leaves new content in the tree. This
//! module only looks: it classifies every path ([`scan`]); the files that matter are moved to Overwrite by
//! `nvmod sweep` (in `nvmod-modinstall`), and building the tree again puts every provider's file back.

use crate::records::{Record, identity, mtime_ns};
use crate::{NvmodInstance, read_manifest};
use nvmod_formats::text::read_limited;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

/// The file in a profile folder that lists the globs of paths staged as copies, one per line (`#` starts a
/// comment). It is `nvmod`'s own file; Mod Organizer ignores it.
pub const PROTECT_FILE: &str = "nvmod-protect.txt";

/// What happened to one path of the tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// The program created it: no provider has it.
    New,
    /// The program replaced it (the link broke): the content in the tree is new, the provider's file is intact.
    Replaced,
    /// The program wrote through the shared file: the provider's own file changed.
    ModifiedInPlace,
    /// The program removed it from the tree; the provider still has it.
    DeletedByProgram,
}

/// One path that differs from the record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Change {
    /// The path in the tree as it is on disk, `/` separated (for a deleted path, its lowercased key).
    pub path: String,
    /// What happened.
    pub kind: Kind,
    /// The Mod (or Overwrite, or the base copy) that provided the path when the tree was built.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider: Option<String>,
    /// The path as the file should be spelled when it is moved out: the provider's spelling, or for a new file
    /// the spelling of the folders that already exist.
    pub spelled: String,
}

/// The globs of the profile's protected paths (see [`PROTECT_FILE`]); empty when there is no file.
#[must_use]
pub fn protected_globs(instance: &NvmodInstance, profile: &str) -> Vec<String> {
    read_limited(&instance.profile_dir(profile).join(PROTECT_FILE))
        .map_or_default(|bytes| parse_globs(&nvmod_formats::text::decode(&bytes)))
}

/// The globs in the text of a protect file.
#[must_use]
pub fn parse_globs(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

fn walk(dir: &Path, prefix: &str, out: &mut Vec<(String, std::path::PathBuf)>) {
    let mut entries: Vec<_> = fs::read_dir(dir).into_iter().flatten().flatten().collect();
    entries.sort_by_key(fs::DirEntry::file_name);
    for entry in entries {
        let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_dir() {
            walk(&entry.path(), &format!("{name}/"), out);
        } else if kind.is_file() {
            out.push((name, entry.path()));
        }
    }
}

/// The folder spellings the providers use, by lowercased folder path.
fn folder_spellings(records: &BTreeMap<String, Record>) -> BTreeMap<String, String> {
    let mut spelled = BTreeMap::new();
    for record in records.values() {
        let parts: Vec<&str> = record.source.split('/').collect();
        for end in 1..parts.len() {
            let dir = parts[..end].join("/");
            spelled.entry(dir.to_lowercase()).or_insert(dir);
        }
    }
    spelled
}

/// `real` with each folder spelled as a provider spells it, where one does.
fn spell_folders(real: &str, spellings: &BTreeMap<String, String>) -> String {
    let parts: Vec<&str> = real.split('/').collect();
    let mut out: Vec<String> = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        if i + 1 == parts.len() {
            out.push((*part).to_owned());
            break;
        }
        let key = parts[..=i].join("/").to_lowercase();
        let name = spellings.get(&key).map_or_else(
            || (*part).to_owned(),
            |s| s.rsplit('/').next().unwrap_or(part).to_owned(),
        );
        out.push(name);
    }
    out.join("/")
}

/// Compare the tree of `profile` with the records of its manifest.
///
/// # Errors
/// The reason, when the profile has no Staging tree or its manifest has no records (built by an older nvmod:
/// stage it again).
pub fn scan(instance: &NvmodInstance, profile: &str) -> Result<Vec<Change>, String> {
    let manifest = read_manifest(instance, profile)
        .filter(|m| !m.records.is_empty() || m.files == 0)
        .ok_or_else(|| {
            format!("profile `{profile}` has no Staging tree with a record of its files; run `nvmod stage`")
        })?;
    if crate::incremental::journal_path(instance, profile).is_file() {
        return Err(format!(
            "the last staging run of profile `{profile}` did not finish; run `nvmod stage` first"
        ));
    }
    let data = instance.staging_dir(profile).join("data");
    if !data.is_dir() {
        return Err(format!(
            "profile `{profile}` has no Staging tree; run `nvmod stage`"
        ));
    }
    let spellings = folder_spellings(&manifest.records);
    let mut on_disk = Vec::new();
    walk(&data, "", &mut on_disk);
    let mut changes = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (real, path) in on_disk {
        let key = real.to_lowercase();
        seen.insert(key.clone());
        let Some(record) = manifest.records.get(&key) else {
            changes.push(Change {
                spelled: spell_folders(&real, &spellings),
                path: real,
                kind: Kind::New,
                provider: None,
            });
            continue;
        };
        let Ok(meta) = fs::metadata(&path) else {
            continue;
        };
        let (id, _) = identity(&path, &meta);
        let moved = id != record.id;
        let touched = meta.len() != record.size || mtime_ns(&meta) != record.mtime_ns;
        let kind = if moved || (touched && record.links <= 1) {
            Kind::Replaced
        } else if touched {
            Kind::ModifiedInPlace
        } else {
            continue;
        };
        changes.push(Change {
            path: real,
            kind,
            provider: Some(record.provider.clone()),
            spelled: record.source.clone(),
        });
    }
    for (key, record) in &manifest.records {
        if !seen.contains(key) {
            changes.push(Change {
                path: key.clone(),
                kind: Kind::DeletedByProgram,
                provider: Some(record.provider.clone()),
                spelled: record.source.clone(),
            });
        }
    }
    Ok(changes)
}
