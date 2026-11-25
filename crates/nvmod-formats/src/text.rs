// SPDX-License-Identifier: GPL-3.0-only
//! Reading text files that mods and tools wrote in different encodings.

use std::io::Read;
use std::path::Path;

/// Largest text file read, in bytes. A `modlist.txt` of 400 mods is about 15 KiB; a `meta.ini` with a long
/// description is tens of KiB.
pub const MAX_TEXT_FILE: u64 = 16 * 1024 * 1024;

/// Windows-1252 for the bytes 0x80 to 0x9F (the rest of the table is Latin-1). Undefined bytes map to the control
/// character of the same value, as Windows does.
const CP1252_HIGH: [char; 32] = [
    '\u{20AC}', '\u{81}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{8D}', '\u{017D}', '\u{8F}',
    '\u{90}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}', '\u{0153}', '\u{9D}', '\u{017E}', '\u{0178}',
];

/// Read a file whole, refusing one larger than [`MAX_TEXT_FILE`] (so a hostile instance cannot make a reader
/// allocate without bound).
///
/// # Errors
/// The file cannot be read, or is larger than the limit.
pub fn read_limited(path: &Path) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(MAX_TEXT_FILE + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_TEXT_FILE {
        return Err(std::io::Error::other(format!(
            "larger than {MAX_TEXT_FILE} bytes"
        )));
    }
    Ok(bytes)
}

/// Decode as UTF-8 when valid, else as Windows-1252 (the recommendation for `plugins.txt`, which MO2 writes in the
/// system code page). A leading byte order mark is dropped.
#[must_use]
pub fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_owned(),
        Err(_) => bytes
            .iter()
            .map(|&b| match b {
                0x80..=0x9F => CP1252_HIGH[usize::from(b - 0x80)],
                other => char::from(other),
            })
            .collect(),
    }
}

/// Encode as Windows-1252, the code page `plugins.txt` is written in; `None` when a character has no place in it.
#[must_use]
pub fn encode_windows_1252(text: &str) -> Option<Vec<u8>> {
    text.chars()
        .map(|c| {
            let code = u32::from(c);
            if code < 0x80 || (0xA0..=0xFF).contains(&code) {
                u8::try_from(code).ok()
            } else {
                CP1252_HIGH
                    .iter()
                    .position(|&h| h == c)
                    .and_then(|i| u8::try_from(0x80 + i).ok())
            }
        })
        .collect()
}

/// The lines of `text`, split on `\n` with a trailing `\r` removed.
pub fn lines(text: &str) -> impl Iterator<Item = &str> {
    text.split('\n')
        .map(|line| line.strip_suffix('\r').unwrap_or(line))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{covers, ids};

    #[test]
    fn utf8_passes_through_and_the_bom_is_dropped() {
        assert_eq!(decode("Caf\u{e9}.esp".as_bytes()), "Caf\u{e9}.esp");
        assert_eq!(decode(b"\xEF\xBB\xBFa"), "a");
    }

    #[test]
    fn invalid_utf8_is_read_as_windows_1252() {
        covers(&[ids::formats::files::TEXT_CODE_PAGES]);
        assert_eq!(
            decode(b"Caf\xE9 \x80 \x93x\x94"),
            "Caf\u{e9} \u{20ac} \u{201c}x\u{201d}"
        );
        assert_eq!(decode(b"\x81"), "\u{81}");
    }

    #[test]
    fn encoding_is_the_inverse_of_decoding_and_refuses_what_the_code_page_lacks() {
        covers(&[ids::formats::files::TEXT_CODE_PAGES]);
        let all: Vec<u8> = (0..=255).collect();
        assert_eq!(
            encode_windows_1252(&decode(&all)),
            Some(all.clone()),
            "every byte round-trips"
        );
        assert_eq!(
            encode_windows_1252("Caf\u{e9}.esp"),
            Some(b"Caf\xE9.esp".to_vec())
        );
        assert_eq!(encode_windows_1252("\u{6f22}"), None);
    }

    #[test]
    fn lines_split_on_lf_and_drop_cr() {
        assert_eq!(
            lines("a\r\nb\nc\r\n").collect::<Vec<_>>(),
            ["a", "b", "c", ""]
        );
    }

    #[test]
    fn read_limited_reads_small_files_and_refuses_missing_ones() {
        let dir = std::env::temp_dir().join(format!("nvmod-text-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a.txt"), b"hi").unwrap();
        assert_eq!(read_limited(&dir.join("a.txt")).unwrap(), b"hi");
        assert!(read_limited(&dir.join("missing")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
