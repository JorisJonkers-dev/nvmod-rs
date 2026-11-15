// SPDX-License-Identifier: GPL-3.0-only
//! The header of a Fallout: New Vegas save (`.fos`): enough to list a save and read its plugin list. Read only and
//! bounded: nothing here allocates more than the limits below, and a short or damaged file is an error, never a
//! panic.
//!
//! **Unverified assumption.** The layout below is reconstructed from the community description of the Fallout 3
//! and New Vegas save format and from memory of MO2's save game reader. It has not been compared with a real save
//! and must be before anyone relies on it (the owner's machine, ADR-0002). The reader fails closed: a file that
//! does not match is reported as unreadable, and nothing is guessed. The layout, little endian:
//!
//! ```text
//! "FO3SAVEGAME"                                  11 bytes
//! u32 header size                                 (not used beyond the bound check)
//! u32 version  '|'
//! u32 width '|'  u32 height '|'  u32 save number '|'
//! string player name '|'   string title '|'   u32 level '|'   string location '|'   string play time '|'
//! width * height * 3 bytes of screenshot
//! u8 form version
//! u32 size of the plugin block
//! u8 plugin count
//! plugin count * ( string name '|' )
//! ```
//!
//! A `string` is a u16 length and that many bytes; `'|'` is the byte 0x7C.

use crate::text::decode;
use std::io::Read;
use std::path::Path;

/// The first bytes of a save.
pub const MAGIC: &[u8; 11] = b"FO3SAVEGAME";
/// The most of a save that is read: the header, the screenshot and the plugin block come first.
pub const MAX_READ: u64 = 16 * 1024 * 1024;
const MAX_DIMENSION: u32 = 4096;
const MAX_STRING: usize = 4096;

/// What a save header says.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SaveHeader {
    /// The format version field.
    pub version: u32,
    /// The save's own number.
    pub save_number: u32,
    /// The player's name.
    pub player: String,
    /// The player's level.
    pub level: u32,
    /// The plugins the save was made with, in order (masters first).
    pub plugins: Vec<String>,
}

/// Why a save header could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveError {
    /// The file ends inside the named part.
    Truncated(&'static str),
    /// It does not start with `FO3SAVEGAME`.
    BadMagic,
    /// A field is outside what the reader accepts (a delimiter missing, a size too large).
    Malformed(&'static str),
    /// The file could not be opened or read.
    Io(String),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Truncated(part) => write!(f, "the file ends inside the {part}"),
            SaveError::BadMagic => f.write_str("not a Fallout save (no FO3SAVEGAME header)"),
            SaveError::Malformed(why) => write!(f, "damaged header: {why}"),
            SaveError::Io(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for SaveError {}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, n: usize, part: &'static str) -> Result<&'a [u8], SaveError> {
        let end = self.at.checked_add(n).ok_or(SaveError::Truncated(part))?;
        let slice = self
            .bytes
            .get(self.at..end)
            .ok_or(SaveError::Truncated(part))?;
        self.at = end;
        Ok(slice)
    }
    fn u8(&mut self, part: &'static str) -> Result<u8, SaveError> {
        Ok(self.take(1, part)?[0])
    }
    fn u16(&mut self, part: &'static str) -> Result<u16, SaveError> {
        let b = self.take(2, part)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }
    fn u32(&mut self, part: &'static str) -> Result<u32, SaveError> {
        let b = self.take(4, part)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }
    fn delimiter(&mut self, part: &'static str) -> Result<(), SaveError> {
        if self.u8(part)? == b'|' {
            Ok(())
        } else {
            Err(SaveError::Malformed("a field delimiter is missing"))
        }
    }
    fn string(&mut self, part: &'static str) -> Result<String, SaveError> {
        let len = usize::from(self.u16(part)?);
        if len > MAX_STRING {
            return Err(SaveError::Malformed("a string is too long"));
        }
        Ok(decode(self.take(len, part)?))
    }
}

/// Read the header of the save in `bytes` (the start of the file is enough).
///
/// # Errors
/// [`SaveError`] for a short, foreign or damaged file.
pub fn parse(bytes: &[u8]) -> Result<SaveHeader, SaveError> {
    let mut c = Cursor { bytes, at: 0 };
    if c.take(MAGIC.len(), "magic")
        .map_err(|_| SaveError::BadMagic)?
        != MAGIC
    {
        return Err(SaveError::BadMagic);
    }
    let _header_size = c.u32("header size")?;
    let version = c.u32("version")?;
    c.delimiter("version")?;
    let width = c.u32("width")?;
    c.delimiter("width")?;
    let height = c.u32("height")?;
    c.delimiter("height")?;
    let save_number = c.u32("save number")?;
    c.delimiter("save number")?;
    let player = c.string("player name")?;
    c.delimiter("player name")?;
    let _title = c.string("title")?;
    c.delimiter("title")?;
    let level = c.u32("level")?;
    c.delimiter("level")?;
    let _location = c.string("location")?;
    c.delimiter("location")?;
    let _play_time = c.string("play time")?;
    c.delimiter("play time")?;
    if width > MAX_DIMENSION || height > MAX_DIMENSION {
        return Err(SaveError::Malformed("the screenshot is implausibly large"));
    }
    let shot = usize::try_from(u64::from(width) * u64::from(height) * 3)
        .map_err(|_| SaveError::Malformed("the screenshot is implausibly large"))?;
    c.take(shot, "screenshot")?;
    let _form_version = c.u8("form version")?;
    let _block = c.u32("plugin block size")?;
    let count = c.u8("plugin count")?;
    let mut plugins = Vec::with_capacity(usize::from(count));
    for _ in 0..count {
        plugins.push(c.string("plugin list")?);
        c.delimiter("plugin list")?;
    }
    Ok(SaveHeader {
        version,
        save_number,
        player,
        level,
        plugins,
    })
}

/// Read the header of the save file at `path`, reading at most [`MAX_READ`] bytes of it.
///
/// # Errors
/// [`SaveError`] when the file cannot be read or its header cannot be understood.
pub fn read_header(path: &Path) -> Result<SaveHeader, SaveError> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .and_then(|f| f.take(MAX_READ).read_to_end(&mut bytes))
        .map_err(|e| SaveError::Io(format!("{}: {e}", path.display())))?;
    parse(&bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    /// The bytes of a synthetic save in the assumed layout.
    fn synthetic(plugins: &[&str]) -> Vec<u8> {
        let s = |out: &mut Vec<u8>, text: &str| {
            out.extend_from_slice(&u16::try_from(text.len()).unwrap().to_le_bytes());
            out.extend_from_slice(text.as_bytes());
            out.push(b'|');
        };
        let mut b = MAGIC.to_vec();
        b.extend_from_slice(&0_u32.to_le_bytes());
        b.extend_from_slice(&48_u32.to_le_bytes());
        b.push(b'|');
        for n in [2_u32, 2, 7] {
            b.extend_from_slice(&n.to_le_bytes());
            b.push(b'|');
        }
        s(&mut b, "Courier");
        s(&mut b, "Title");
        b.extend_from_slice(&12_u32.to_le_bytes());
        b.push(b'|');
        s(&mut b, "Goodsprings");
        s(&mut b, "01.02.03");
        b.extend_from_slice(&[0; 12]);
        b.push(0x30);
        b.extend_from_slice(&0_u32.to_le_bytes());
        b.push(u8::try_from(plugins.len()).unwrap());
        for p in plugins {
            s(&mut b, p);
        }
        b
    }

    #[test]
    fn a_synthetic_save_is_read() {
        let h = parse(&synthetic(&["FalloutNV.esm", "Mod.esp"])).unwrap();
        assert_eq!(h.player, "Courier");
        assert_eq!((h.save_number, h.level, h.version), (7, 12, 48));
        assert_eq!(h.plugins, ["FalloutNV.esm", "Mod.esp"]);
    }

    #[test]
    fn every_truncation_is_an_error_not_a_panic() {
        let full = synthetic(&["FalloutNV.esm", "Mod.esp"]);
        for cut in 0..full.len() {
            assert!(parse(&full[..cut]).is_err(), "cut at {cut}");
        }
        assert!(parse(&full).is_ok());
    }

    #[test]
    fn foreign_and_damaged_files_are_refused() {
        assert_eq!(parse(b"hello"), Err(SaveError::BadMagic));
        let mut bad = synthetic(&["A.esm"]);
        bad[19] = b'#';
        assert!(matches!(parse(&bad), Err(SaveError::Malformed(_))));
    }

    proptest! {
        #[test]
        fn random_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..400)) {
            let _ = parse(&bytes);
            let mut with_magic = MAGIC.to_vec();
            with_magic.extend(bytes);
            let _ = parse(&with_magic);
        }
    }
}
