// SPDX-License-Identifier: GPL-3.0-only
//! The per-profile switches of `settings.ini` (a Qt file MO2 writes).

use nvmod_formats::ini::Ini;
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

impl Settings {
    /// Read `settings.ini` of the profile folder `dir`. A missing or unreadable file means every switch is off.
    #[must_use]
    pub fn read(dir: &Path) -> Settings {
        let text = nvmod_instance::find_ci(dir, "settings.ini")
            .and_then(|p| read_limited(&p).ok())
            .map(|b| decode(&b))
            .unwrap_or_default();
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
}
