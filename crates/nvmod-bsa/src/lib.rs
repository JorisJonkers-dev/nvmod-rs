// SPDX-License-Identifier: GPL-3.0-only
//! A read-only reader of the directory of a BSA archive, version 104 (Fallout 3 and Fallout: New Vegas).
//!
//! It lists names and sizes. It never extracts, decompresses or hashes a file, and it never reads file data.
//! The layout is the one described in the public format notes (UESP, "Tes4Mod:BSA File Format"):
//!
//! ```text
//! header (36 bytes)  "BSA\0", version u32, folder records offset u32 (36), archive flags u32,
//!                    folder count u32, file count u32, total folder name length u32,
//!                    total file name length u32, file flags u16, padding u16
//! folder records     per folder: name hash u64, file count u32, offset u32      (16 bytes each)
//! file record blocks per folder: name (length byte, text, NUL), then per file: hash u64, size u32, offset u32
//! file names         every file name once, NUL terminated, in file record order
//! ```
//!
//! Every count and offset of the header is checked against the length of the file before anything is allocated
//! or read, so a hostile header cannot make the reader allocate more than the directory it describes, and the
//! directory cannot be larger than the file. Memory use is bounded by the directory size, not the file size: a
//! 2 GB archive costs what its directory costs.

use serde::Serialize;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The header length of a version 104 archive.
pub const HEADER_LEN: u64 = 36;
/// The one version this reader understands.
pub const VERSION_104: u32 = 104;
/// The archive flag that says folder names are present.
pub const FLAG_FOLDER_NAMES: u32 = 0x1;
/// The archive flag that says file names are present.
pub const FLAG_FILE_NAMES: u32 = 0x2;
/// The archive flag that says files are compressed by default.
pub const FLAG_COMPRESSED: u32 = 0x4;
/// The file size bit that inverts the archive's compression default for one file.
const SIZE_COMPRESSION_TOGGLE: u32 = 0x4000_0000;
/// The bits of a file size field that hold the size.
const SIZE_MASK: u32 = 0x3FFF_FFFF;
/// The most a directory may hold. A real archive has well under 200 000 files; this is a ceiling for hostile
/// input, applied on top of the check against the file length.
pub const MAX_FILES: u32 = 4_000_000;
/// The most folders a directory may hold.
pub const MAX_FOLDERS: u32 = 1_000_000;

/// Why a directory could not be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BsaError {
    /// The bytes cannot be read.
    Io(String),
    /// The file does not start with `BSA\0`.
    NotBsa,
    /// A BSA of a version other than 104.
    UnsupportedVersion(u32),
    /// The header or directory is damaged or lies about the file (what is wrong).
    Corrupt(String),
}

impl std::fmt::Display for BsaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BsaError::Io(why) => write!(f, "cannot be read: {why}"),
            BsaError::NotBsa => write!(f, "does not start with the BSA signature"),
            BsaError::UnsupportedVersion(v) => {
                write!(f, "archive version {v} is not supported (104 is)")
            }
            BsaError::Corrupt(why) => write!(f, "damaged directory: {why}"),
        }
    }
}

impl std::error::Error for BsaError {}

fn corrupt<T>(why: impl Into<String>) -> Result<T, BsaError> {
    Err(BsaError::Corrupt(why.into()))
}

/// One file of an archive's directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Entry {
    /// The path inside the archive: lowercased, `/` as the separator, as the Staging tree spells paths.
    pub path: String,
    /// Stored size in bytes (the compressed size for a compressed file).
    pub size: u32,
    /// Whether the file is stored compressed.
    pub compressed: bool,
}

/// The directory of an archive.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Directory {
    /// The archive flags of the header.
    pub flags: u32,
    /// Number of folders.
    pub folders: u32,
    /// The files, in archive order.
    pub files: Vec<Entry>,
}

fn le32(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

/// Sequential, length-checked reading: a request larger than what is left of the file is refused before anything
/// is allocated.
struct Cursor<'a, R> {
    reader: &'a mut R,
    position: u64,
    length: u64,
}

impl<R: Read + Seek> Cursor<'_, R> {
    fn take(&mut self, len: u64, what: &str) -> Result<Vec<u8>, BsaError> {
        if len > self.length - self.position {
            return corrupt(format!("{what} runs past the end of the file"));
        }
        let mut buffer =
            vec![0; usize::try_from(len).map_err(|_| BsaError::Corrupt(what.to_owned()))?];
        self.reader
            .read_exact(&mut buffer)
            .map_err(|e| BsaError::Io(e.to_string()))?;
        self.position += len;
        Ok(buffer)
    }

    fn seek(&mut self, to: u64) -> Result<(), BsaError> {
        self.reader
            .seek(SeekFrom::Start(to))
            .map_err(|e| BsaError::Io(e.to_string()))?;
        self.position = to;
        Ok(())
    }
}

/// Normalise an archive path: lowercase, `\` to `/`.
fn normal(text: &[u8]) -> String {
    let text = String::from_utf8_lossy(text);
    text.to_lowercase().replace('\\', "/")
}

/// Read the directory of the archive behind `reader`, whose total length is `length`.
///
/// # Errors
/// [`BsaError::NotBsa`], [`BsaError::UnsupportedVersion`], or [`BsaError::Corrupt`] when a count or an offset
/// does not fit the file, a name is missing, or the directory is cut short.
pub fn read_directory<R: Read + Seek>(reader: &mut R, length: u64) -> Result<Directory, BsaError> {
    if length < HEADER_LEN {
        return corrupt("the file is shorter than the 36-byte header");
    }
    let mut at = Cursor {
        reader,
        position: 0,
        length,
    };
    at.seek(0)?;
    let header = at.take(HEADER_LEN, "the header")?;
    if &header[..4] != b"BSA\0" {
        return Err(BsaError::NotBsa);
    }
    let version = le32(&header, 4);
    if version != VERSION_104 {
        return Err(BsaError::UnsupportedVersion(version));
    }
    let folder_offset = u64::from(le32(&header, 8));
    let flags = le32(&header, 12);
    let folders = le32(&header, 16);
    let files = le32(&header, 20);
    let file_names = u64::from(le32(&header, 28));
    if folder_offset != HEADER_LEN {
        return corrupt(format!(
            "the folder records start at {folder_offset}, not 36"
        ));
    }
    if folders > MAX_FOLDERS || files > MAX_FILES {
        return corrupt(format!(
            "{folders} folders and {files} files is not believable"
        ));
    }
    // The lower bound of the directory's size, from the counts alone: nothing is allocated before this fits.
    let minimum = HEADER_LEN + u64::from(folders) * 18 + u64::from(files) * 16 + file_names;
    if minimum > length {
        return corrupt(format!(
            "the counts need at least {minimum} bytes, the file has {length}"
        ));
    }
    // An empty archive (the invalidation archive of MO2 is one) has no names to store.
    if files > 0 && (flags & FLAG_FILE_NAMES == 0 || flags & FLAG_FOLDER_NAMES == 0) {
        return corrupt("the archive does not store names");
    }
    let records = at.take(u64::from(folders) * 16, "the folder records")?;
    let mut counts = Vec::with_capacity(folders as usize);
    let mut total = 0_u64;
    for folder in 0..folders as usize {
        let count = le32(&records, folder * 16 + 8);
        let offset = u64::from(le32(&records, folder * 16 + 12));
        if offset < file_names || offset - file_names >= length {
            return corrupt(format!("folder {folder} points outside the file"));
        }
        total += u64::from(count);
        counts.push(count);
    }
    if total != u64::from(files) {
        return corrupt(format!(
            "the folders hold {total} files, the header says {files}"
        ));
    }
    // The file record blocks follow the folder records; the name block follows them.
    let mut blocks: Vec<(String, Vec<(u32, u32)>)> = Vec::with_capacity(folders as usize);
    for count in counts {
        let name_len = at.take(1, "a folder name")?[0];
        let name = at.take(u64::from(name_len), "a folder name")?;
        let name = normal(name.strip_suffix(&[0]).unwrap_or(&name));
        let raw = at.take(u64::from(count) * 16, "a folder's file records")?;
        let records = (0..count as usize)
            .map(|i| (le32(&raw, i * 16 + 8), le32(&raw, i * 16 + 12)))
            .collect();
        blocks.push((name, records));
    }
    let names = at.take(file_names, "the file name block")?;
    let mut name_list = names.split(|b| *b == 0);
    let mut out = Vec::with_capacity(files as usize);
    for (folder, records) in blocks {
        for (raw, offset) in records {
            let size = raw & SIZE_MASK;
            if u64::from(offset).saturating_add(u64::from(size)) > length {
                return corrupt("a file's data lies outside the archive");
            }
            let Some(file) = name_list.next().filter(|n| !n.is_empty()) else {
                return corrupt("a file has no name");
            };
            let file = normal(file);
            let compressed = (flags & FLAG_COMPRESSED != 0) != (raw & SIZE_COMPRESSION_TOGGLE != 0);
            out.push(Entry {
                path: if folder.is_empty() {
                    file
                } else {
                    format!("{folder}/{file}")
                },
                size,
                compressed,
            });
        }
    }
    Ok(Directory {
        flags,
        folders,
        files: out,
    })
}

/// Read the directory of the archive at `path`.
///
/// # Errors
/// See [`read_directory`]; [`BsaError::Io`] when the file cannot be opened.
pub fn read_file(path: &Path) -> Result<Directory, BsaError> {
    let file = std::fs::File::open(path).map_err(|e| BsaError::Io(e.to_string()))?;
    let length = file
        .metadata()
        .map_err(|e| BsaError::Io(e.to_string()))?
        .len();
    read_directory(&mut std::io::BufReader::new(file), length)
}

/// Read the directory of an archive held in memory.
///
/// # Errors
/// See [`read_directory`].
pub fn read_bytes(bytes: &[u8]) -> Result<Directory, BsaError> {
    read_directory(&mut std::io::Cursor::new(bytes), bytes.len() as u64)
}
