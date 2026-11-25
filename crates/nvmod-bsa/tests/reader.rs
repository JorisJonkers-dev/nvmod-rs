// SPDX-License-Identifier: GPL-3.0-only
#![allow(clippy::assert_is_empty)]
//! The reader against archives generated in code.

use nvmod_bsa::{BsaError, read_bytes, read_directory, read_file};
use nvmod_testkit::{BsaBuilder, TempDir};
use nvmod_testkit::{covers, ids};
use proptest::prelude::*;
use std::io::{Read, Seek, SeekFrom};

#[test]
fn an_empty_archive_has_no_files() {
    let dir = read_bytes(&BsaBuilder::new().bytes()).unwrap();
    assert!(dir.files.is_empty());
    assert_eq!(dir.folders, 0);
}

#[test]
fn names_are_lowercased_and_use_slashes() {
    covers(&[ids::formats::archives::READ_NAMES]);
    let bytes = BsaBuilder::new()
        .file("Meshes\\Fake\\Rock.NIF", 10)
        .file("textures/fake/Rock_d.dds", 20)
        .file("top.txt", 3)
        .bytes();
    let dir = read_bytes(&bytes).unwrap();
    let mut got: Vec<_> = dir
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.size))
        .collect();
    got.sort_unstable();
    assert_eq!(
        got,
        [
            ("meshes/fake/rock.nif", 10),
            ("textures/fake/rock_d.dds", 20),
            ("top.txt", 3)
        ]
    );
    assert_eq!(dir.folders, 3);
}

#[test]
fn many_files_and_long_names_are_read() {
    let mut builder = BsaBuilder::new();
    for i in 0..5000 {
        builder = builder.file(&format!("meshes/fake/dir{}/file{i}.nif", i % 50), 1);
    }
    let long = format!("textures/{}/{}.dds", "d".repeat(100), "n".repeat(120));
    builder = builder.file(&long, 2);
    let dir = read_bytes(&builder.bytes()).unwrap();
    assert_eq!(dir.files.len(), 5001);
    assert!(dir.files.iter().any(|f| f.path == long));
}

#[test]
fn the_compression_flag_and_the_toggle_bit_decide_compressed() {
    covers(&[ids::formats::archives::COMPRESSION]);
    let stored = read_bytes(&BsaBuilder::new().file("a/b.txt", 4).bytes()).unwrap();
    assert!(!stored.files[0].compressed);
    let all = read_bytes(&BsaBuilder::new().flags(0x7).file("a/b.txt", 4).bytes()).unwrap();
    assert!(all.files[0].compressed);
    let toggled = read_bytes(
        &BsaBuilder::new()
            .flags(0x7)
            .file("a/b.txt", 4 | 0x4000_0000)
            .directory_bytes(),
    );
    // The data region is not there (the toggle bit is not part of the size), so the offset check refuses it.
    assert!(matches!(toggled, Err(BsaError::Corrupt(_))));
}

#[test]
fn a_wrong_signature_or_version_is_named() {
    covers(&[ids::formats::archives::BAD_HEADER_REFUSED]);
    let mut bytes = BsaBuilder::new().file("a/b", 1).bytes();
    assert_eq!(
        read_bytes(&BsaBuilder::new().version(105).bytes()),
        Err(BsaError::UnsupportedVersion(105))
    );
    bytes[0] = b'X';
    assert_eq!(read_bytes(&bytes), Err(BsaError::NotBsa));
}

#[test]
fn truncated_input_is_refused_at_every_length() {
    covers(&[ids::formats::archives::BAD_HEADER_REFUSED]);
    let bytes = BsaBuilder::new()
        .file("meshes/fake/a.nif", 5)
        .file("textures/b.dds", 7)
        .bytes();
    for cut in 0..bytes.len() {
        assert!(read_bytes(&bytes[..cut]).is_err(), "cut at {cut}");
    }
    assert!(read_bytes(&bytes).is_ok());
}

fn patch(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

#[test]
fn oversized_counts_are_refused() {
    covers(&[ids::formats::archives::BAD_HEADER_REFUSED]);
    let base = BsaBuilder::new().file("a/b.txt", 2).bytes();
    for (at, value) in [
        (16, u32::MAX),
        (20, u32::MAX),
        (16, 100_000),
        (20, 100_000),
        (28, u32::MAX),
        (20, 2),
    ] {
        let mut bytes = base.clone();
        patch(&mut bytes, at, value);
        assert!(
            matches!(read_bytes(&bytes), Err(BsaError::Corrupt(_))),
            "{at} {value}"
        );
    }
}

#[test]
fn an_out_of_range_offset_is_refused() {
    let mut bytes = BsaBuilder::new().file("a/b.txt", 2).bytes();
    // folder record: hash 8, count 4, offset 4 at 36 + 12; file record offset follows the folder name block.
    let folder_offset = 36 + 12;
    patch(&mut bytes, folder_offset, u32::MAX);
    assert!(matches!(read_bytes(&bytes), Err(BsaError::Corrupt(_))));
    let mut bytes = BsaBuilder::new().file("a/b.txt", 2).bytes();
    let record = 36 + 16 + 1 + 2 + 1; // header, folder record, folder name "a\0" with its length byte -> hash starts here
    patch(&mut bytes, record + 12, u32::MAX - 1);
    assert!(matches!(read_bytes(&bytes), Err(BsaError::Corrupt(_))));
    let mut bytes = BsaBuilder::new().file("a/b.txt", 2).bytes();
    patch(&mut bytes, record + 8, 0x3FFF_FFFF);
    assert!(matches!(read_bytes(&bytes), Err(BsaError::Corrupt(_))));
}

/// A reader that counts the bytes it hands out.
struct Counting<R> {
    inner: R,
    read: u64,
}

impl<R: Read> Read for Counting<R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read += n as u64;
        Ok(n)
    }
}

impl<R: Seek> Seek for Counting<R> {
    fn seek(&mut self, to: SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(to)
    }
}

#[test]
fn a_two_gigabyte_class_archive_costs_only_its_directory() {
    covers(&[ids::formats::archives::LARGE_ARCHIVE_CHEAP]);
    let builder = BsaBuilder::new()
        .file("meshes/fake/huge.nif", 1_000_000_000)
        .file("textures/fake/huge.dds", 1_000_000_000)
        .file("small.txt", 1);
    let directory = builder.directory_bytes();
    let dir = TempDir::new("bsa-sparse");
    let path = dir.path().join("Fake - Big.bsa");
    std::fs::write(&path, &directory).unwrap();
    let file = std::fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_len(directory.len() as u64 + builder.data_len())
        .unwrap();
    let length = file.metadata().unwrap().len();
    assert!(length > 2_000_000_000);
    let mut counting = Counting {
        inner: std::fs::File::open(&path).unwrap(),
        read: 0,
    };
    let listed = read_directory(&mut counting, length).unwrap();
    assert_eq!(listed.files.len(), 3);
    assert_eq!(read_file(&path).unwrap(), listed);
    assert!(
        counting.read <= directory.len() as u64,
        "read {} bytes for a {}-byte directory",
        counting.read,
        directory.len()
    );
}

proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = read_bytes(&bytes);
    }

    #[test]
    fn mutated_archives_never_panic(
        edits in proptest::collection::vec((0_usize..400, any::<u8>()), 1..8),
    ) {
        covers(&[ids::formats::archives::NEVER_PANICS]);
        let mut bytes = BsaBuilder::new()
            .file("meshes/fake/a.nif", 5)
            .file("textures/fake/b.dds", 7)
            .file("c.txt", 1)
            .bytes();
        for (at, value) in edits {
            let at = at % bytes.len();
            bytes[at] = value;
        }
        let _ = read_bytes(&bytes);
    }

    #[test]
    fn generated_archives_round_trip(
        names in proptest::collection::btree_set("[a-z]{1,8}(/[a-z]{1,8}){0,3}\\.[a-z]{3}", 0..30),
        size in 0_u32..64,
    ) {
        let mut builder = BsaBuilder::new();
        for n in &names {
            builder = builder.file(n, size);
        }
        let dir = read_bytes(&builder.bytes()).unwrap();
        let mut got: Vec<_> = dir.files.iter().map(|f| f.path.clone()).collect();
        got.sort();
        let want: Vec<_> = names.into_iter().collect();
        prop_assert_eq!(got, want);
    }
}

#[test]
fn a_bare_header_with_flags_one_is_an_empty_archive() {
    // The shape of the dummy archive MO2's invalidation writes: a header and nothing else.
    let mut header = [0_u8; 36];
    header[..4].copy_from_slice(b"BSA\0");
    header[4] = 104;
    header[8] = 36;
    header[12] = 1;
    assert!(read_bytes(&header).unwrap().files.is_empty());
}
