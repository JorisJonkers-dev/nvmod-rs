// SPDX-License-Identifier: GPL-3.0-only
//! Which names a Profile folder may have: Mod Organizer 2's rule for directory names, plus the names no
//! Windows folder can have, so a Profile made on one system opens on every system.

/// Names Windows reserves for devices, with or without an extension.
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Check `name` as the name of a Profile folder.
///
/// MO2 refuses an empty name, `.` and `..`, and a name holding one of `\ / : * ? " < > |`, because the name is a
/// directory name. This also refuses control characters, a leading or trailing blank, a trailing dot and the
/// Windows device names, which Windows cannot create either.
///
/// # Errors
/// The reason, in words for the person who typed the name.
pub fn check_profile_name(name: &str) -> Result<(), String> {
    let bad = |why: &str| Err(format!("`{name}` is not a usable profile name: {why}"));
    if name.is_empty() {
        return bad("it is empty");
    }
    if name == "." || name == ".." {
        return bad("it is a folder reference");
    }
    if let Some(c) = name
        .chars()
        .find(|c| "\\/:*?\"<>|".contains(*c) || c.is_control())
    {
        return bad(&format!("it holds `{}`", c.escape_default()));
    }
    if name.starts_with(char::is_whitespace) || name.ends_with(char::is_whitespace) {
        return bad("it starts or ends with a blank");
    }
    if name.ends_with('.') {
        return bad("it ends with a dot");
    }
    let stem = name.split('.').next().unwrap_or(name);
    if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(stem)) {
        return bad("Windows reserves it for a device");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::check_profile_name;
    use nvmod_testkit::{covers, ids};

    #[test]
    fn usable_names_pass() {
        for name in [
            "Default",
            "TTW Wasteland Survival Guide",
            "a.b",
            "[x] 2",
            "é",
        ] {
            assert!(check_profile_name(name).is_ok(), "{name}");
        }
    }

    #[test]
    fn unusable_names_are_refused_with_a_reason() {
        covers(&[ids::instance::profiles::NAME_RULES]);
        for name in [
            "", ".", "..", "a/b", "a\\b", "a:b", "a*", "a?", "a\"b", "a<b", "a>b", "a|b", "a\nb",
            " a", "a ", "a.", "con", "NUL.txt", "Com3",
        ] {
            let why = check_profile_name(name).unwrap_err();
            assert!(why.contains("not a usable profile name"), "{name}: {why}");
        }
    }
}
