// SPDX-License-Identifier: GPL-3.0-only
//! A read-only INI reader for `ModOrganizer.ini`, `meta.ini` and `settings.ini` (Qt `QSettings` files). It never
//! writes: Qt-written files are edited line by line by the installer, not re-serialised (research, 2.12 item 5).

/// Parsed sections and keys, in file order. Section and key lookups ignore case.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Ini {
    sections: Vec<(String, Vec<(String, String)>)>,
}

impl Ini {
    /// Parse `text`. Lines starting with `;` or `#` are comments; a `key=value` before any section belongs to
    /// `General` (as `QSettings` has it); a value is everything after the first `=`, trimmed.
    #[must_use]
    pub fn parse(text: &str) -> Ini {
        let mut ini = Ini::default();
        let mut current = "General".to_owned();
        for line in crate::text::lines(text) {
            let line = line.trim();
            if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
                continue;
            }
            if let Some(name) = line
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            {
                name.trim().clone_into(&mut current);
                if ini.section_index(&current).is_none() {
                    ini.sections.push((current.clone(), Vec::new()));
                }
            } else if let Some((key, value)) = line.split_once('=') {
                let at = if let Some(at) = ini.section_index(&current) {
                    at
                } else {
                    ini.sections.push((current.clone(), Vec::new()));
                    ini.sections.len() - 1
                };
                ini.sections[at]
                    .1
                    .push((key.trim().to_owned(), value.trim().to_owned()));
            }
        }
        ini
    }

    fn section_index(&self, name: &str) -> Option<usize> {
        self.sections
            .iter()
            .position(|(n, _)| n.eq_ignore_ascii_case(name))
    }

    /// The raw value of `key` in `section` (the last one when repeated).
    #[must_use]
    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        let (_, keys) = &self.sections[self.section_index(section)?];
        keys.iter()
            .rev()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    /// The raw value of the first `key` in `section`: the line a game reading its own INI uses when the key
    /// is repeated.
    #[must_use]
    pub fn get_first(&self, section: &str, key: &str) -> Option<&str> {
        let (_, keys) = &self.sections[self.section_index(section)?];
        keys.iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(key))
            .map(|(_, v)| v.as_str())
    }

    /// The value of `key` in `section` as a string with Qt's wrappers removed (see [`qt_string`]).
    #[must_use]
    pub fn get_string(&self, section: &str, key: &str) -> Option<String> {
        self.get(section, key).map(qt_string)
    }

    /// True when the file has the section (even if it is empty).
    #[must_use]
    pub fn has_section(&self, section: &str) -> bool {
        self.section_index(section).is_some()
    }
}

/// A Qt string value as text: `@ByteArray(x)` becomes `x`, surrounding double quotes are removed, and the doubled
/// backslashes Qt writes for a single one are folded (`Z:\\home` is `Z:\home`). Other `@Variant(...)` values are
/// returned unchanged: they are opaque.
#[must_use]
pub fn qt_string(raw: &str) -> String {
    let raw = raw.trim();
    if raw.starts_with("@Variant(") {
        return raw.to_owned();
    }
    let inner = raw
        .strip_prefix("@ByteArray(")
        .and_then(|r| r.strip_suffix(')'))
        .unwrap_or(raw);
    let inner = inner
        .strip_prefix('"')
        .and_then(|r| r.strip_suffix('"'))
        .unwrap_or(inner);
    inner.replace("\\\\", "\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    const SAMPLE: &str = "[General]\r\ngameName=TTW\r\ngamePath=@ByteArray(Z:\\\\home\\\\u\\\\Game)\r\n; comment\r\n\r\n[Settings]\r\nbase_directory=\r\n[General]\r\nextra=1\r\n";

    #[test]
    fn sections_keys_and_wrappers() {
        let ini = Ini::parse(SAMPLE);
        assert_eq!(ini.get("general", "GAMENAME"), Some("TTW"));
        assert_eq!(
            ini.get_string("General", "gamePath").unwrap(),
            "Z:\\home\\u\\Game"
        );
        assert_eq!(ini.get("Settings", "base_directory"), Some(""));
        assert_eq!(
            ini.get("General", "extra"),
            Some("1"),
            "a repeated section is merged"
        );
        assert!(ini.has_section("settings") && !ini.has_section("Nope"));
        assert_eq!(ini.get("Nope", "x"), None);
        assert_eq!(ini.get("General", "missing"), None);
    }

    #[test]
    fn keys_before_a_section_belong_to_general_and_the_last_value_wins() {
        let ini = Ini::parse("a=1\nb = two = three\na=2\n");
        assert_eq!(ini.get("General", "a"), Some("2"));
        assert_eq!(ini.get("General", "b"), Some("two = three"));
    }

    #[test]
    fn qt_strings() {
        assert_eq!(
            qt_string("@ByteArray(TTW Wasteland Survival Guide)"),
            "TTW Wasteland Survival Guide"
        );
        assert_eq!(qt_string("\"quoted \\\\ value\""), "quoted \\ value");
        assert_eq!(qt_string("@Variant(\\0\\0\\0)"), "@Variant(\\0\\0\\0)");
        assert_eq!(qt_string("plain"), "plain");
    }

    proptest! {
        #[test]
        fn parsing_never_panics(text in ".{0,300}") {
            let ini = Ini::parse(&text);
            let _ = ini.get("General", "a");
            let _ = qt_string(&text);
        }
    }
}
