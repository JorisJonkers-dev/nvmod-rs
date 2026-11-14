// SPDX-License-Identifier: GPL-3.0-only
//! Where the bytes come from. Every read is checked against the length of the file and against a fixed ceiling,
//! so no field of an untrusted header can make the parser allocate more than the file holds.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// The largest single read, in bytes. The biggest thing the parser ever reads is a resource version block
/// (65 535 bytes) or a table of at most 16 384 four-byte entries (64 KiB); a megabyte is far above both.
pub const MAX_READ: usize = 1 << 20;

/// Random access to a file's bytes. Reads outside the file, or larger than [`MAX_READ`], return `None`.
pub trait ReadAt {
    /// Length of the file in bytes.
    fn len(&self) -> u64;

    /// True for an empty file.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Exactly `len` bytes starting at `offset`, or `None` when they are not all inside the file.
    fn read_at(&mut self, offset: u64, len: usize) -> Option<Vec<u8>>;
}

/// An in-memory file.
pub struct SliceSource<'a>(pub &'a [u8]);

impl ReadAt for SliceSource<'_> {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&mut self, offset: u64, len: usize) -> Option<Vec<u8>> {
        if len > MAX_READ {
            return None;
        }
        let start = usize::try_from(offset).ok()?;
        let end = start.checked_add(len)?;
        Some(self.0.get(start..end)?.to_vec())
    }
}

/// A file on disk, read with seeks. Nothing is ever read beyond the length the file had when it was opened.
pub struct FileSource {
    file: File,
    length: u64,
}

impl FileSource {
    /// Open `path` for reading.
    ///
    /// # Errors
    /// The file cannot be opened or its metadata cannot be read.
    pub fn open(path: &Path) -> std::io::Result<Self> {
        let file = File::open(path)?;
        let length = file.metadata()?.len();
        Ok(FileSource { file, length })
    }
}

impl ReadAt for FileSource {
    fn len(&self) -> u64 {
        self.length
    }

    fn read_at(&mut self, offset: u64, len: usize) -> Option<Vec<u8>> {
        if len > MAX_READ || offset.checked_add(len as u64)? > self.length {
            return None;
        }
        self.file.seek(SeekFrom::Start(offset)).ok()?;
        let mut buffer = vec![0; len];
        self.file.read_exact(&mut buffer).ok()?;
        Some(buffer)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slice_reads_inside_and_refuses_outside() {
        let mut s = SliceSource(&[1, 2, 3, 4]);
        assert_eq!(s.len(), 4);
        assert!(!s.is_empty());
        assert_eq!(s.read_at(1, 2), Some(vec![2, 3]));
        assert_eq!(s.read_at(4, 0), Some(vec![]));
        assert_eq!(s.read_at(3, 2), None);
        assert_eq!(s.read_at(u64::MAX, 1), None);
        assert_eq!(s.read_at(0, MAX_READ + 1), None);
        assert!(SliceSource(&[]).is_empty());
    }

    #[test]
    fn file_reads_inside_and_refuses_outside() {
        let dir = std::env::temp_dir().join(format!("nvmod-pe-source-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("f.bin");
        std::fs::write(&path, [9u8, 8, 7, 6, 5]).unwrap();
        let mut f = FileSource::open(&path).unwrap();
        assert_eq!(f.len(), 5);
        assert_eq!(f.read_at(2, 3), Some(vec![7, 6, 5]));
        assert_eq!(f.read_at(2, 4), None);
        assert_eq!(f.read_at(u64::MAX, 2), None);
        assert_eq!(f.read_at(0, MAX_READ + 1), None);
        assert!(FileSource::open(&dir.join("missing")).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
