// SPDX-License-Identifier: GPL-3.0-only
//! Paths as people write them. On Windows `std::fs::canonicalize` gives a verbatim path (`\\?\C:\Games`), which
//! reports, records and other programs do not expect; [`plain_path`] gives `C:\Games` back where that names the same
//! file. Only the text of the path is looked at.

use std::path::{Path, PathBuf};

/// Windows reads a plain path this long or longer differently from its verbatim form (`MAX_PATH`).
const MAX_PATH: usize = 260;

/// `path` without the verbatim prefix of Windows, `\\?\` or `\\?\UNC\`, when the plain form names the same file: a
/// drive path or a UNC path shorter than `MAX_PATH` in which no name means something else without the prefix (`.`,
/// `..`, a name ending in a dot or a space, a reserved device name such as `CON` or `nul.txt`). Any other path, and
/// every path on other platforms, is returned as it is.
#[must_use]
pub fn plain_path(path: &Path) -> PathBuf {
    if cfg!(windows)
        && let Some(plain) = path.to_str().and_then(plain_text)
    {
        return PathBuf::from(plain);
    }
    path.to_path_buf()
}

/// The plain form of the verbatim path `text`, when there is a safe one.
fn plain_text(text: &str) -> Option<String> {
    let (plain, names) = if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        (format!(r"\\{rest}"), rest)
    } else {
        let rest = text.strip_prefix(r"\\?\")?;
        let mut chars = rest.chars();
        let drive = matches!(
            (chars.next(), chars.next(), chars.next()),
            (Some(letter), Some(':'), Some('\\')) if letter.is_ascii_alphabetic()
        );
        if !drive {
            return None;
        }
        (rest.to_owned(), &rest[3..])
    };
    if plain.len() >= MAX_PATH {
        return None;
    }
    let parts: Vec<&str> = names.split('\\').collect();
    for (n, name) in parts.iter().enumerate() {
        let last = n + 1 == parts.len();
        if (name.is_empty() && !last) || !plain_name(name) {
            return None;
        }
    }
    Some(plain)
}

/// Whether Windows reads `name` the same with and without the verbatim prefix.
fn plain_name(name: &str) -> bool {
    if name.is_empty() {
        return true;
    }
    if name == "." || name == ".." || name.ends_with(['.', ' ']) || name.contains('/') {
        return false;
    }
    let stem = name.split('.').next().unwrap_or(name).trim_end();
    let upper = stem.to_ascii_uppercase();
    let reserved = matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ((upper.starts_with("COM") || upper.starts_with("LPT"))
        && upper.len() == 4
        && upper.as_bytes()[3].is_ascii_digit());
    !reserved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_verbatim_drive_or_unc_path_loses_its_prefix() {
        assert_eq!(
            plain_text(r"\\?\C:\Games\FNV").as_deref(),
            Some(r"C:\Games\FNV")
        );
        assert_eq!(plain_text(r"\\?\d:\").as_deref(), Some(r"d:\"));
        assert_eq!(
            plain_text(r"\\?\UNC\server\share\x").as_deref(),
            Some(r"\\server\share\x")
        );
        assert_eq!(
            plain_text(r"\\?\C:\a b\c.d\e").as_deref(),
            Some(r"C:\a b\c.d\e")
        );
    }

    #[test]
    fn a_path_that_would_change_meaning_keeps_its_prefix() {
        for text in [
            r"\\?\C:\Games\CON",
            r"\\?\C:\nul.txt\x",
            r"\\?\C:\com1",
            r"\\?\C:\LPT9.log",
            r"\\?\C:\dot.",
            r"\\?\C:\space \x",
            r"\\?\C:\a\..\b",
            r"\\?\C:\a\.\b",
            r"\\?\C:\a\\b",
            r"\\?\C:\a/b",
            r"\\?\Volume{0123}\x",
            r"\\?\GLOBALROOT\Device\x",
        ] {
            assert_eq!(plain_text(text), None, "{text}");
        }
        let long = format!(r"\\?\C:\{}", "a".repeat(300));
        assert_eq!(plain_text(&long), None);
        // Names that only look like a device are fine.
        assert_eq!(
            plain_text(r"\\?\C:\console\com10\lpt").as_deref(),
            Some(r"C:\console\com10\lpt")
        );
    }

    #[test]
    fn plain_paths_and_other_platforms_are_left_alone() {
        assert_eq!(plain_text(r"C:\Games"), None);
        assert_eq!(plain_text("/home/u"), None);
        assert_eq!(plain_path(Path::new("/home/u")), PathBuf::from("/home/u"));
        let verbatim = Path::new(r"\\?\C:\Games");
        let expected = if cfg!(windows) {
            PathBuf::from(r"C:\Games")
        } else {
            verbatim.to_path_buf()
        };
        assert_eq!(plain_path(verbatim), expected);
    }
}
