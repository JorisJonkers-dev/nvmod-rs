// SPDX-License-Identifier: GPL-3.0-only
#![allow(clippy::missing_panics_doc, clippy::must_use_candidate)]
//! Builds small archives in code: a stored (uncompressed) zip and a ustar tar, including the hostile entries the
//! installer has to refuse (a path that escapes, a symbolic link). No archive file is ever committed.

use std::path::Path;

fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for &byte in data {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[derive(Debug, Clone)]
struct Entry {
    name: String,
    data: Vec<u8>,
    /// Unix mode (with the file type bits), as stored by the archive.
    mode: u32,
}

/// A zip archive made of stored entries.
#[derive(Debug, Clone, Default)]
pub struct ZipBuilder {
    entries: Vec<Entry>,
}

impl ZipBuilder {
    /// An empty archive.
    pub fn new() -> ZipBuilder {
        ZipBuilder::default()
    }

    /// Add a file; `name` is stored exactly as given (so `../x` and `a\\b` can be made).
    #[must_use]
    pub fn file(mut self, name: &str, data: &[u8]) -> ZipBuilder {
        self.entries.push(Entry {
            name: name.to_owned(),
            data: data.to_vec(),
            mode: 0o100_644,
        });
        self
    }

    /// Add a symbolic link entry pointing at `target`.
    #[must_use]
    pub fn symlink(mut self, name: &str, target: &str) -> ZipBuilder {
        self.entries.push(Entry {
            name: name.to_owned(),
            data: target.as_bytes().to_vec(),
            mode: 0o120_777,
        });
        self
    }

    /// The bytes of the archive.
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut central = Vec::new();
        for e in &self.entries {
            let offset = u32::try_from(out.len()).unwrap();
            let crc = crc32(&e.data);
            let len = u32::try_from(e.data.len()).unwrap();
            let name_len = u16::try_from(e.name.len()).unwrap();
            out.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
            out.extend_from_slice(&[20, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // version, flags, method, time, date
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&name_len.to_le_bytes());
            out.extend_from_slice(&0_u16.to_le_bytes());
            out.extend_from_slice(e.name.as_bytes());
            out.extend_from_slice(&e.data);

            central.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
            central.extend_from_slice(&[0x1E, 3, 20, 0, 0, 0, 0, 0, 0, 0, 0, 0]); // made by unix, needed, flags, method, time, date
            central.extend_from_slice(&crc.to_le_bytes());
            central.extend_from_slice(&len.to_le_bytes());
            central.extend_from_slice(&len.to_le_bytes());
            central.extend_from_slice(&name_len.to_le_bytes());
            central.extend_from_slice(&[0; 8]); // extra, comment, disk, internal attributes
            central.extend_from_slice(&(e.mode << 16).to_le_bytes());
            central.extend_from_slice(&offset.to_le_bytes());
            central.extend_from_slice(e.name.as_bytes());
        }
        let central_offset = u32::try_from(out.len()).unwrap();
        let count = u16::try_from(self.entries.len()).unwrap();
        out.extend_from_slice(&central);
        out.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&u32::try_from(central.len()).unwrap().to_le_bytes());
        out.extend_from_slice(&central_offset.to_le_bytes());
        out.extend_from_slice(&0_u16.to_le_bytes());
        out
    }

    /// Write the archive to `path`.
    pub fn write(&self, path: &Path) {
        std::fs::write(path, self.bytes()).expect("write zip");
    }
}

/// A ustar tar archive.
#[derive(Debug, Clone, Default)]
pub struct TarBuilder {
    blocks: Vec<u8>,
}

impl TarBuilder {
    /// An empty archive.
    pub fn new() -> TarBuilder {
        TarBuilder::default()
    }

    fn entry(mut self, name: &str, kind: u8, link: &str, data: &[u8]) -> TarBuilder {
        assert!(name.len() <= 100 && link.len() <= 100, "short names only");
        let mut h = [0_u8; 512];
        h[..name.len()].copy_from_slice(name.as_bytes());
        h[100..107].copy_from_slice(b"0000644");
        h[108..115].copy_from_slice(b"0000000");
        h[116..123].copy_from_slice(b"0000000");
        h[124..135].copy_from_slice(format!("{:011o}", data.len()).as_bytes());
        h[136..147].copy_from_slice(b"00000000000");
        h[148..156].fill(b' ');
        h[156] = kind;
        h[157..157 + link.len()].copy_from_slice(link.as_bytes());
        h[257..263].copy_from_slice(b"ustar\0");
        h[263..265].copy_from_slice(b"00");
        let sum: u32 = h.iter().map(|&b| u32::from(b)).sum();
        h[148..155].copy_from_slice(format!("{sum:06o}\0").as_bytes());
        self.blocks.extend_from_slice(&h);
        self.blocks.extend_from_slice(data);
        self.blocks.resize(self.blocks.len().div_ceil(512) * 512, 0);
        self
    }

    /// Add a file; `name` is stored as given.
    #[must_use]
    pub fn file(self, name: &str, data: &[u8]) -> TarBuilder {
        self.entry(name, b'0', "", data)
    }

    /// Add a directory entry.
    #[must_use]
    pub fn dir(self, name: &str) -> TarBuilder {
        self.entry(name, b'5', "", b"")
    }

    /// Add a symbolic link entry.
    #[must_use]
    pub fn symlink(self, name: &str, target: &str) -> TarBuilder {
        self.entry(name, b'2', target, b"")
    }

    /// The bytes of the archive.
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = self.blocks.clone();
        out.extend_from_slice(&[0; 1024]);
        out
    }

    /// Write the archive to `path`.
    pub fn write(&self, path: &Path) {
        std::fs::write(path, self.bytes()).expect("write tar");
    }
}
