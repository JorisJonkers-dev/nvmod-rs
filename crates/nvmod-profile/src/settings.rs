// SPDX-License-Identifier: GPL-3.0-only
//! The per-profile switches of `settings.ini` (a Qt file MO2 writes).

use nvmod_formats::ini::Ini;
use nvmod_formats::metaini::MetaIni;
use nvmod_formats::text::{decode, read_limited};
use serde::Serialize;
use std::path::Path;

/// What a profile asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct Settings {
    /// `LocalSettings`: the profile holds its own game INIs.
    pub local_settings: bool,
    /// `LocalSaves`: the profile's `saves/` folder is the save target.
    pub local_saves: bool,
    /// `AutomaticArchiveInvalidation`.
    pub archive_invalidation: bool,
}

fn truthy(value: Option<&str>) -> bool {
    value.is_some_and(|v| {
        let v = v.trim();
        v.eq_ignore_ascii_case("true") || v == "1"
    })
}

/// The keys of `[General]` that `nvmod profile set` edits, with the spelling MO2 uses.
pub const KEYS: [&str; 3] = [
    "LocalSaves",
    "LocalSettings",
    "AutomaticArchiveInvalidation",
];

/// The spelling of `key` in [`KEYS`], matched without regard to case.
#[must_use]
pub fn known_key(key: &str) -> Option<&'static str> {
    KEYS.iter().find(|k| k.eq_ignore_ascii_case(key)).copied()
}

impl Settings {
    /// Set `key` (one of [`KEYS`]) to `value` in `[General]` of the `settings.ini` of the profile folder `dir`,
    /// with a line-preserving edit: every other line, comment, line ending and byte stays as it is, an existing
    /// line changes only after its `=`, and a missing key or file is added (the file in MO2's CRLF).
    /// Returns whether the file changed; a value the file already holds writes nothing.
    ///
    /// # Errors
    /// `key` is not one of [`KEYS`], or the file could not be read or written.
    pub fn set(dir: &Path, key: &str, value: bool) -> Result<bool, String> {
        let key = known_key(key).ok_or_else(|| format!("`{key}` is not a profile setting"))?;
        let path = nvmod_instance::find_ci(dir, "settings.ini")
            .unwrap_or_else(|| dir.join("settings.ini"));
        let old = match read_limited(&path) {
            Ok(bytes) => bytes,
            Err(_) if !path.exists() => Vec::new(),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        let mut ini = MetaIni::parse(&old);
        let text = if value { "true" } else { "false" };
        if truthy(ini.get("General", key).as_deref()) == value && ini.get("General", key).is_some()
        {
            return Ok(false);
        }
        ini.set("General", key, text);
        let new = ini.to_bytes();
        if new == old {
            return Ok(false);
        }
        let tmp = path.with_extension("nvmod-tmp");
        let io = |e: std::io::Error| format!("{}: {e}", path.display());
        std::fs::write(&tmp, &new).map_err(io)?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            io(e)
        })?;
        Ok(true)
    }

    /// Read `settings.ini` of the profile folder `dir`. A missing or unreadable file means every switch is off.
    #[must_use]
    pub fn read(dir: &Path) -> Settings {
        let text = nvmod_instance::find_ci(dir, "settings.ini")
            .and_then(|p| read_limited(&p).ok())
            .map_or_default(|b| decode(&b));
        let ini = Ini::parse(&text);
        Settings {
            local_settings: truthy(ini.get("General", "LocalSettings")),
            local_saves: truthy(ini.get("General", "LocalSaves")),
            archive_invalidation: truthy(ini.get("General", "AutomaticArchiveInvalidation")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switches_are_read_and_default_to_off() {
        let dir = std::env::temp_dir().join(format!("nvmod-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert_eq!(Settings::read(&dir), Settings::default());
        std::fs::write(
            dir.join("settings.ini"),
            "[General]\r\nLocalSettings=true\r\nLocalSaves=false\r\nAutomaticArchiveInvalidation=true\r\n",
        )
        .unwrap();
        assert_eq!(
            Settings::read(&dir),
            Settings {
                local_settings: true,
                local_saves: false,
                archive_invalidation: true
            }
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn temp(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nvmod-settings-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn setting_a_key_changes_only_its_line() {
        let dir = temp("line");
        let before = "; kept\r\n[General]\r\nLocalSaves=false\r\nOther=x y\r\n\r\n[Other]\r\nLocalSaves=false\r\n";
        std::fs::write(dir.join("settings.ini"), before).unwrap();
        assert!(Settings::set(&dir, "localsaves", true).unwrap());
        assert_eq!(
            std::fs::read_to_string(dir.join("settings.ini")).unwrap(),
            before.replacen("LocalSaves=false", "LocalSaves=true", 1)
        );
        assert!(Settings::read(&dir).local_saves);
        assert!(
            !Settings::set(&dir, "LocalSaves", true).unwrap(),
            "no change, no write"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_missing_key_or_file_is_added_and_an_unknown_key_refused() {
        let dir = temp("add");
        assert!(Settings::set(&dir, "LocalSettings", true).unwrap());
        assert_eq!(
            std::fs::read(dir.join("settings.ini")).unwrap(),
            b"[General]\r\nLocalSettings=true\r\n"
        );
        std::fs::write(dir.join("settings.ini"), "[General]\nLocalSaves=true").unwrap();
        assert!(Settings::set(&dir, "AutomaticArchiveInvalidation", false).unwrap());
        assert_eq!(
            std::fs::read_to_string(dir.join("settings.ini")).unwrap(),
            "[General]\nLocalSaves=true\nAutomaticArchiveInvalidation=false\n"
        );
        assert!(Settings::set(&dir, "Nope", true).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
