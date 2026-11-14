// SPDX-License-Identifier: GPL-3.0-only
//! Property tests of the `meta.ini` editor: an untouched file is byte identical, and setting one key changes only
//! that key's line.

use nvmod_formats::metaini::MetaIni;
use proptest::prelude::*;

/// A generated file: sections, keys, comments, blank lines, odd values, either line ending, maybe no final newline.
fn file() -> impl Strategy<Value = Vec<u8>> {
    let line = prop_oneof![
        "[a-zA-Z]{1,8}".prop_map(|s| format!("[{s}]")),
        ("[a-zA-Z0-9\\\\]{1,8}", "[ -~]{0,20}").prop_map(|(k, v)| format!("{k}={v}")),
        "[;#][ -~]{0,12}",
        Just(String::new()),
        "[ -~]{0,10}",
        Just("v=@Variant(\\0\\0\\0\\x43\\x1)".to_owned()),
        Just("é=ü".to_owned()),
    ];
    (
        proptest::collection::vec(line, 0..20),
        prop_oneof![Just("\r\n"), Just("\n")],
        any::<bool>(),
    )
        .prop_map(|(lines, eol, last)| {
            let mut text = lines.join(eol);
            if last && !text.is_empty() {
                text.push_str(eol);
            }
            text.into_bytes()
        })
}

proptest! {
    #[test]
    fn parse_then_write_is_byte_identical(bytes in file()) {
        prop_assert_eq!(MetaIni::parse(&bytes).to_bytes(), bytes);
    }

    #[test]
    fn setting_an_existing_key_changes_only_that_line(
        bytes in file(),
        pick in any::<prop::sample::Index>(),
        value in "[a-z0-9.]{0,12}",
    ) {
        let meta = MetaIni::parse(&bytes);
        // Choose a key that the file sets, with the section the editor sees it in.
        let text = String::from_utf8_lossy(&bytes).into_owned();
        let mut section = "General".to_owned();
        let mut keys = Vec::new();
        for (i, line) in text.split_inclusive('\n').enumerate() {
            let t = line.trim();
            if t.starts_with(';') || t.starts_with('#') { continue; }
            if let Some(name) = t.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
                section = name.trim().to_owned();
            } else if let Some((k, _)) = line.split_once('=') {
                keys.push((i, section.clone(), k.trim().to_owned()));
            }
        }
        prop_assume!(!keys.is_empty());
        let (index, section, key) = &keys[pick.index(keys.len())];
        // The editor edits the last line that sets the key; skip a key that is set twice.
        prop_assume!(keys.iter().filter(|(_, s, k)| s.eq_ignore_ascii_case(section) && k.eq_ignore_ascii_case(key)).count() == 1);
        let mut edited = meta.clone();
        edited.set(section, key, &value);
        let new = edited.to_bytes();
        let old_lines: Vec<&[u8]> = bytes.split_inclusive(|&b| b == b'\n').collect();
        let new_lines: Vec<&[u8]> = new.split_inclusive(|&b| b == b'\n').collect();
        prop_assert_eq!(old_lines.len(), new_lines.len());
        for i in 0..old_lines.len() {
            if i != *index {
                prop_assert_eq!(old_lines[i], new_lines[i]);
            }
        }
    }
}
