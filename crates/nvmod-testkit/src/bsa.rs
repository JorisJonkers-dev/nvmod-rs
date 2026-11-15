// SPDX-License-Identifier: GPL-3.0-only
#![allow(clippy::missing_panics_doc, clippy::must_use_candidate)]
//! Builds BSA archives of version 104 in code, with fake names and zero-filled contents. Hashes are zero: the
//! reader never uses them. No archive file is ever committed.

use std::collections::BTreeMap;

/// A version 104 archive: files by path (`/` or `\` separated) with a stored size each.
#[derive(Debug, Clone, Default)]
pub struct BsaBuilder {
    files: Vec<(String, u32)>,
    flags: u32,
    version: u32,
}

impl BsaBuilder {
    /// An empty archive that stores folder and file names.
    pub fn new() -> BsaBuilder {
        BsaBuilder {
            files: Vec::new(),
            flags: 0x3,
            version: 104,
        }
    }

    /// Add a file of `size` stored bytes.
    #[must_use]
    pub fn file(mut self, path: &str, size: u32) -> BsaBuilder {
        self.files.push((path.to_owned(), size));
        self
    }

    /// Set the archive flags (default 3: folder and file names).
    #[must_use]
    pub fn flags(mut self, flags: u32) -> BsaBuilder {
        self.flags = flags;
        self
    }

    /// Set the version field (default 104).
    #[must_use]
    pub fn version(mut self, version: u32) -> BsaBuilder {
        self.version = version;
        self
    }

    /// Header, directory and name block, without file data; offsets point just past the end of this directory
    /// and are laid out as if the data followed, so appending zeros of [`BsaBuilder::data_len`] makes a
    /// complete archive.
    pub fn directory_bytes(&self) -> Vec<u8> {
        let mut folders: BTreeMap<String, Vec<(String, u32)>> = BTreeMap::new();
        for (path, size) in &self.files {
            let path = path.replace('/', "\\");
            let (folder, name) = path.rsplit_once('\\').unwrap_or(("", path.as_str()));
            folders
                .entry(folder.to_owned())
                .or_default()
                .push((name.to_owned(), *size));
        }
        let folder_names: usize = folders.keys().map(|f| f.len() + 1).sum();
        let file_names: usize = self
            .files
            .iter()
            .map(|(p, _)| p.rsplit(['/', '\\']).next().unwrap().len() + 1)
            .sum();
        let blocks_len: usize = folders
            .iter()
            .map(|(f, files)| 1 + f.len() + 1 + 16 * files.len())
            .sum();
        let directory_len = 36 + 16 * folders.len() + blocks_len + file_names;
        let mut out = Vec::new();
        out.extend_from_slice(b"BSA\0");
        for value in [
            self.version,
            36,
            self.flags,
            u32::try_from(folders.len()).unwrap(),
            u32::try_from(self.files.len()).unwrap(),
            u32::try_from(folder_names).unwrap(),
            u32::try_from(file_names).unwrap(),
            0,
        ] {
            out.extend_from_slice(&value.to_le_bytes());
        }
        let mut block_at = 36 + 16 * folders.len() + file_names;
        for (folder, files) in &folders {
            out.extend_from_slice(&0_u64.to_le_bytes());
            out.extend_from_slice(&u32::try_from(files.len()).unwrap().to_le_bytes());
            out.extend_from_slice(&u32::try_from(block_at).unwrap().to_le_bytes());
            block_at += 1 + folder.len() + 1 + 16 * files.len();
        }
        let mut data_at = directory_len;
        for (folder, files) in &folders {
            out.push(u8::try_from(folder.len() + 1).unwrap());
            out.extend_from_slice(folder.as_bytes());
            out.push(0);
            for (_, size) in files {
                out.extend_from_slice(&0_u64.to_le_bytes());
                out.extend_from_slice(&size.to_le_bytes());
                out.extend_from_slice(&u32::try_from(data_at).unwrap().to_le_bytes());
                data_at += *size as usize;
            }
        }
        for files in folders.values() {
            for (name, _) in files {
                out.extend_from_slice(name.as_bytes());
                out.push(0);
            }
        }
        out
    }

    /// Total bytes of file data the directory points at.
    pub fn data_len(&self) -> u64 {
        self.files
            .iter()
            .map(|(_, s)| u64::from(*s & 0x3FFF_FFFF))
            .sum()
    }

    /// The whole archive, file data zero-filled.
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = self.directory_bytes();
        out.resize(out.len() + usize::try_from(self.data_len()).unwrap(), 0);
        out
    }
}
