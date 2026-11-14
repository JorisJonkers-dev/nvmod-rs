// SPDX-License-Identifier: GPL-3.0-only
//! Archive invalidation as MO2 does it (research, 2.6): with the profile's `AutomaticArchiveInvalidation` on,
//! `[Archive] bInvalidateOlderFiles=1`, `SInvalidationFile=` empty and `Fallout - Invalidation.bsa` at the front
//! of `SArchiveList`; off removes the entry and sets `SInvalidationFile=ArchiveInvalidation.txt`. The dummy
//! archive is a generated empty BSA of 36 bytes (a header and nothing else), never game data.

use nvmod_formats::metaini::MetaIni;

/// The file name of the dummy archive.
pub const BSA_NAME: &str = "Fallout - Invalidation.bsa";

/// An empty Fallout: New Vegas archive: `BSA\0`, version 104, offset 36, archive flags 1, and zero folders,
/// files and names. Generated here, not copied from the game.
pub const EMPTY_BSA: [u8; 36] = {
    let mut b = [0_u8; 36];
    b[0] = b'B';
    b[1] = b'S';
    b[2] = b'A';
    b[4] = 104;
    b[8] = 36;
    b[12] = 1;
    b
};

fn split(list: &str) -> Vec<String> {
    list.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Set a key only when its value differs, so a file that is right already stays byte for byte.
pub fn set(ini: &mut MetaIni, section: &str, key: &str, value: &str) {
    if ini.get(section, key).as_deref() != Some(value) {
        ini.set(section, key, value);
    }
}

/// Apply the switch to the INI. Returns a note when something was left alone on purpose.
pub fn edit(ini: &mut MetaIni, on: bool) -> Option<String> {
    let list = ini.get("Archive", "SArchiveList");
    let is_bsa = |a: &String| a.eq_ignore_ascii_case(BSA_NAME);
    if on {
        set(ini, "Archive", "bInvalidateOlderFiles", "1");
        set(ini, "Archive", "SInvalidationFile", "");
        let mut note = None;
        if let Some(list) = list {
            let mut archives = split(&list);
            if !archives.iter().any(is_bsa) {
                archives.insert(0, BSA_NAME.to_owned());
                ini.set("Archive", "SArchiveList", &archives.join(", "));
            }
        } else {
            note = Some(
                "SArchiveList is not in the INI, so the invalidation archive was not added (run the game once)"
                    .to_owned(),
            );
        }
        note
    } else {
        // Without an archive list there is nothing to undo, and no section is made for it.
        let list = list?;
        let archives = split(&list);
        if archives.iter().any(is_bsa) {
            let kept: Vec<String> = archives.into_iter().filter(|a| !is_bsa(a)).collect();
            ini.set("Archive", "SArchiveList", &kept.join(", "));
        }
        set(
            ini,
            "Archive",
            "SInvalidationFile",
            "ArchiveInvalidation.txt",
        );
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(text: &str, on: bool) -> String {
        let mut ini = MetaIni::parse(text.as_bytes());
        edit(&mut ini, on);
        String::from_utf8(ini.to_bytes()).unwrap()
    }

    #[test]
    fn on_puts_the_archive_first_and_is_idempotent() {
        let src = "[Archive]\r\nSArchiveList=A.bsa, B.bsa\r\nSInvalidationFile=ArchiveInvalidation.txt\r\n";
        let once = run(src, true);
        assert_eq!(
            once,
            "[Archive]\r\nSArchiveList=Fallout - Invalidation.bsa, A.bsa, B.bsa\r\nSInvalidationFile=\r\nbInvalidateOlderFiles=1\r\n"
        );
        assert_eq!(run(&once, true), once);
    }

    #[test]
    fn off_removes_the_archive_and_is_idempotent() {
        let src =
            "[Archive]\r\nSArchiveList=fallout - invalidation.bsa, A.bsa\r\nSInvalidationFile=\r\n";
        let off = run(src, false);
        assert_eq!(
            off,
            "[Archive]\r\nSArchiveList=A.bsa\r\nSInvalidationFile=ArchiveInvalidation.txt\r\n"
        );
        assert_eq!(run(&off, false), off);
    }

    #[test]
    fn the_empty_archive_is_a_36_byte_header() {
        assert_eq!(&EMPTY_BSA[..4], b"BSA\0");
        assert_eq!(EMPTY_BSA.len(), 36);
    }
}
