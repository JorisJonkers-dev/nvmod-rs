// SPDX-License-Identifier: GPL-3.0-only
//! `initweaks.ini`: each enabled Mod's `INI Tweaks` folder merged in priority order, then `profile_tweaks.ini`,
//! later wins per key, with `[Archive] bInvalidateOlderFiles=1` forced (research, 2.7). The output follows what
//! `WritePrivateProfileString` writes: `[Section]` and `key=value` lines, CRLF, no blank lines (the live file of
//! the owner's instance is exactly the 36 bytes of the forced key alone).

use nvmod_formats::text::{decode, encode_windows_1252, lines, read_limited};
use nvmod_formats::{EntryKind, ModState, Modlist};
use nvmod_instance::{NvmodInstance, find_ci};
use std::fmt::Write;
use std::path::{Path, PathBuf};

type Merged = Vec<(String, Vec<(String, String)>)>;

fn put(merged: &mut Merged, section: &str, key: &str, value: &str) {
    let at = if let Some(i) = merged
        .iter()
        .position(|(s, _)| s.eq_ignore_ascii_case(section))
    {
        i
    } else {
        merged.push((section.to_owned(), Vec::new()));
        merged.len() - 1
    };
    let keys = &mut merged[at].1;
    if let Some(k) = keys.iter_mut().find(|(k, _)| k.eq_ignore_ascii_case(key)) {
        value.clone_into(&mut k.1);
    } else {
        keys.push((key.to_owned(), value.to_owned()));
    }
}

fn merge_text(merged: &mut Merged, text: &str) {
    let mut section = String::new();
    for line in lines(text) {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            name.trim().clone_into(&mut section);
        } else if let Some((key, value)) = line.split_once('=')
            && !section.is_empty()
        {
            put(merged, &section, key.trim(), value.trim());
        }
    }
}

/// The `.ini` files of a Mod's `INI Tweaks` folder (any case of the name), sorted by name.
fn tweak_files(mod_dir: &Path) -> Vec<PathBuf> {
    let Some(dir) = find_ci(mod_dir, "INI Tweaks").filter(|p| p.is_dir()) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.extension().is_some_and(|e| e.eq_ignore_ascii_case("ini")))
        .collect();
    files.sort_by_key(|p| {
        p.file_name()
            .map(|n| n.to_string_lossy().to_ascii_lowercase())
    });
    files
}

/// The bytes of `initweaks.ini` for `profile`.
#[must_use]
pub fn build(instance: &NvmodInstance, profile: &str) -> Vec<u8> {
    let dir = instance.profile_dir(profile);
    let list = nvmod_formats::text::read_limited(&dir.join("modlist.txt"))
        .map_or_default(|b| Modlist::parse(&decode(&b)));
    let mut merged = Merged::new();
    // modlist.txt puts the highest priority first, and the later write wins: walk it backwards.
    for entry in list.entries.iter().rev() {
        if entry.kind != EntryKind::Mod || entry.state != ModState::Enabled {
            continue;
        }
        for file in tweak_files(&instance.mod_dir(&entry.name)) {
            if let Ok(bytes) = read_limited(&file) {
                merge_text(&mut merged, &decode(&bytes));
            }
        }
    }
    if let Some(file) = find_ci(&dir, "profile_tweaks.ini")
        && let Ok(bytes) = read_limited(&file)
    {
        merge_text(&mut merged, &decode(&bytes));
    }
    put(&mut merged, "Archive", "bInvalidateOlderFiles", "1");
    let mut text = String::new();
    for (section, keys) in &merged {
        let _ = write!(text, "[{section}]\r\n");
        for (key, value) in keys {
            let _ = write!(text, "{key}={value}\r\n");
        }
    }
    encode_windows_1252(&text).unwrap_or_else(|| text.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn later_wins_per_key_and_sections_keep_first_seen_order() {
        let mut m = Merged::new();
        merge_text(
            &mut m,
            "[Display]\r\niSize=1\r\nfoo=a\r\n; c\r\n[Other]\r\nx=1\r\n",
        );
        merge_text(&mut m, "[display]\nISIZE=2\nbar=b\n");
        assert_eq!(
            m,
            vec![
                (
                    "Display".to_owned(),
                    vec![
                        ("iSize".to_owned(), "2".to_owned()),
                        ("foo".to_owned(), "a".to_owned()),
                        ("bar".to_owned(), "b".to_owned())
                    ]
                ),
                ("Other".to_owned(), vec![("x".to_owned(), "1".to_owned())])
            ]
        );
    }
}
