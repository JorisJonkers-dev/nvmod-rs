// SPDX-License-Identifier: GPL-3.0-only
//! `plugins.txt` and `loadorder.txt`. For Fallout: New Vegas and TTW, `plugins.txt` lists only the active plugins,
//! with no `*` prefix, in the system code page; `loadorder.txt` lists all plugins in UTF-8. Both are read the same
//! way here: the `*` prefix is tolerated, comments and blanks skipped, the bytes decoded as UTF-8 or Windows-1252.

/// The plugin names in file order.
#[must_use]
pub fn parse_plugin_list(bytes: &[u8]) -> Vec<String> {
    crate::text::decode(bytes)
        .split('\n')
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.strip_prefix('*').unwrap_or(line).trim().to_owned())
        .filter(|name| !name.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_comments_and_stars() {
        let list = parse_plugin_list(
            b"# header\r\nFalloutNV.esm\r\n*Starred.esp\r\n\r\n  Padded.esp  \r\n",
        );
        assert_eq!(list, ["FalloutNV.esm", "Starred.esp", "Padded.esp"]);
    }

    #[test]
    fn a_code_page_name_is_decoded() {
        assert_eq!(parse_plugin_list(b"Caf\xE9.esp\r\n"), ["Caf\u{e9}.esp"]);
    }

    #[test]
    fn an_empty_file_has_no_plugins() {
        assert_eq!(parse_plugin_list(b""), Vec::<String>::new());
        assert_eq!(parse_plugin_list(b"*\r\n"), Vec::<String>::new());
    }
}
