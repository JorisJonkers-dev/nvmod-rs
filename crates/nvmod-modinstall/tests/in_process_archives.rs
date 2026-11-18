// SPDX-License-Identifier: GPL-3.0-only
//! The in-process readers (7z, tar.gz, tar.xz, tar.bz2, tar.zst) against small synthetic archives built here.
//! Each format must extract clean trees and refuse traversal, absolute and drive paths, links, corrupt input
//! and anything over the entry or byte limits. Nothing is written outside a per-test temp directory.

use nvmod_modinstall::{Limits, extract::extract};
use nvmod_testkit::TempDir;
use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// One archive member: a file holding `data`, or a symbolic link whose target is `data`.
struct Item<'a> {
    name: &'a str,
    data: &'a [u8],
    symlink: bool,
}

fn file<'a>(name: &'a str, data: &'a [u8]) -> Item<'a> {
    Item {
        name,
        data,
        symlink: false,
    }
}

fn link<'a>(name: &'a str, target: &'a str) -> Item<'a> {
    Item {
        name,
        data: target.as_bytes(),
        symlink: true,
    }
}

/// A ustar stream. Names are written into the header bytes as given, so `../x` survives (`Builder::append_data`
/// would refuse it).
fn tar_bytes(items: &[Item]) -> Vec<u8> {
    let mut builder = tar::Builder::new(Vec::new());
    for item in items {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.as_old_mut().name[..item.name.len()].copy_from_slice(item.name.as_bytes());
        if item.symlink {
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_size(0);
            header.as_old_mut().linkname[..item.data.len()].copy_from_slice(item.data);
            header.set_cksum();
            builder.append(&header, io::empty()).unwrap();
        } else {
            header.set_entry_type(tar::EntryType::Regular);
            header.set_size(u64::try_from(item.data.len()).unwrap());
            header.set_cksum();
            builder.append(&header, item.data).unwrap();
        }
    }
    builder.into_inner().unwrap()
}

fn gzip(tar: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    encoder.write_all(tar).unwrap();
    encoder.finish().unwrap()
}

fn xz(tar: &[u8]) -> Vec<u8> {
    let mut writer =
        lzma_rust2::XzWriter::new(Vec::new(), lzma_rust2::XzOptions::with_preset(1)).unwrap();
    writer.write_all(tar).unwrap();
    writer.finish().unwrap()
}

fn bzip2(tar: &[u8]) -> Vec<u8> {
    let mut encoder = bzip2::write::BzEncoder::new(Vec::new(), bzip2::Compression::fast());
    encoder.write_all(tar).unwrap();
    encoder.finish().unwrap()
}

fn zstd(tar: &[u8]) -> Vec<u8> {
    ruzstd::encoding::compress_to_vec(tar, ruzstd::encoding::CompressionLevel::Fastest)
}

fn make_7z(path: &Path, items: &[Item]) {
    let mut writer = sevenz_rust2::ArchiveWriter::create(path).unwrap();
    for item in items {
        let mut entry = sevenz_rust2::ArchiveEntry::new_file(item.name);
        if item.symlink {
            // Unix mode in the high 16 bits, flagged by 0x8000: a symbolic link.
            entry.has_windows_attributes = true;
            entry.windows_attributes = 0x8000 | (0o120_777 << 16);
        }
        writer.push_archive_entry(entry, Some(item.data)).unwrap();
    }
    writer.finish().unwrap();
}

/// Write the archive made from `items` by `write`, extract it into a fresh `out` folder and return the outcome.
/// The temp directory is returned so the caller can check what was written beside `out`.
fn extract_made(label: &str, write: impl FnOnce(&Path)) -> (TempDir, Result<(), String>) {
    let dir = TempDir::new(label);
    let archive = dir.path().join("archive");
    write(&archive);
    fs::create_dir(dir.path().join("out")).unwrap();
    let result = extract(&archive, &dir.path().join("out"), None, Limits::default());
    (dir, result)
}

fn assert_refused(result: &Result<(), String>, needle: &str) {
    match result {
        Err(why) => assert!(why.contains(needle), "expected `{needle}` in: {why}"),
        Ok(()) => panic!("archive was accepted; expected a refusal containing `{needle}`"),
    }
}

fn is_empty_dir(path: &Path) -> bool {
    fs::read_dir(path).unwrap().next().is_none()
}

/// One test per behaviour, for one format. `$make` writes an archive from items; `$magic` starts a corrupt one.
macro_rules! format_tests {
    ($module:ident, $make:expr, $magic:expr) => {
        mod $module {
            use super::*;

            fn make(path: &Path, items: &[Item]) {
                ($make)(path, items);
            }

            #[test]
            fn nested_folders_extract() {
                let items = [file("a/b/c.txt", b"deep"), file("top.txt", b"top")];
                let (dir, result) = extract_made(stringify!($module), |p| make(p, &items));
                result.unwrap();
                let out = dir.path().join("out");
                assert_eq!(fs::read(out.join("a/b/c.txt")).unwrap(), b"deep");
                assert_eq!(fs::read(out.join("top.txt")).unwrap(), b"top");
            }

            #[test]
            fn backslash_names_become_folders() {
                let items = [file("a\\b\\c.txt", b"win")];
                let (dir, result) = extract_made(stringify!($module), |p| make(p, &items));
                result.unwrap();
                assert_eq!(fs::read(dir.path().join("out/a/b/c.txt")).unwrap(), b"win");
            }

            #[test]
            fn traversal_is_refused_and_nothing_leaves_dest() {
                let items = [file("../evil.txt", b"x"), file("ok.txt", b"ok")];
                let (dir, result) = extract_made(stringify!($module), |p| make(p, &items));
                assert_refused(&result, "escapes");
                assert!(!dir.path().join("evil.txt").exists());
                assert!(is_empty_dir(&dir.path().join("out")));
            }

            #[test]
            fn absolute_path_is_refused() {
                let items = [file("/abs.txt", b"x")];
                let (dir, result) = extract_made(stringify!($module), |p| make(p, &items));
                assert_refused(&result, "absolute");
                assert!(is_empty_dir(&dir.path().join("out")));
            }

            #[test]
            fn drive_path_is_refused() {
                let items = [file("C:\\x.txt", b"x")];
                let (dir, result) = extract_made(stringify!($module), |p| make(p, &items));
                assert_refused(&result, "absolute");
                assert!(is_empty_dir(&dir.path().join("out")));
            }

            #[test]
            fn symbolic_link_is_refused() {
                let items = [link("evil", "../../outside.txt")];
                let (dir, result) = extract_made(stringify!($module), |p| make(p, &items));
                assert_refused(&result, "link or special file");
                assert!(!dir.path().join("out/evil").exists());
            }

            #[test]
            fn corrupt_archive_is_refused_without_panic() {
                let mut bytes = $magic.to_vec();
                bytes.extend_from_slice(&[0xA5; 300]);
                let (_dir, result) = extract_made(stringify!($module), |p| {
                    fs::write(p, &bytes).unwrap();
                });
                assert_refused(&result, "not a readable");
            }

            #[test]
            fn too_many_entries_is_refused() {
                let items = [
                    file("1.txt", b"1"),
                    file("2.txt", b"2"),
                    file("3.txt", b"3"),
                ];
                let dir = TempDir::new(stringify!($module));
                let archive = dir.path().join("archive");
                make(&archive, &items);
                fs::create_dir(dir.path().join("out")).unwrap();
                let limits = Limits {
                    max_entries: 2,
                    ..Limits::default()
                };
                let result = extract(&archive, &dir.path().join("out"), None, limits);
                assert_refused(&result, "entries");
            }

            #[test]
            fn too_many_bytes_is_refused() {
                let data = [7_u8; 100];
                let items = [file("big.txt", &data)];
                let dir = TempDir::new(stringify!($module));
                let archive = dir.path().join("archive");
                make(&archive, &items);
                fs::create_dir(dir.path().join("out")).unwrap();
                let limits = Limits {
                    max_bytes: 10,
                    ..Limits::default()
                };
                let result = extract(&archive, &dir.path().join("out"), None, limits);
                assert_refused(&result, "bytes once extracted");
            }

            #[test]
            fn decompression_bomb_is_refused() {
                let zeros = vec![0_u8; 1 << 20];
                let items = [file("zeros.bin", &zeros)];
                let dir = TempDir::new(stringify!($module));
                let archive = dir.path().join("archive");
                make(&archive, &items);
                fs::create_dir(dir.path().join("out")).unwrap();
                let limits = Limits {
                    max_bytes: 1024,
                    ..Limits::default()
                };
                let result = extract(&archive, &dir.path().join("out"), None, limits);
                assert_refused(&result, "bytes once extracted");
                assert!(!dir.path().join("out/zeros.bin").exists());
            }
        }
    };
}

fn make_tar_gz(path: &Path, items: &[Item]) {
    fs::write(path, gzip(&tar_bytes(items))).unwrap();
}

fn make_tar_xz(path: &Path, items: &[Item]) {
    fs::write(path, xz(&tar_bytes(items))).unwrap();
}

fn make_tar_bz2(path: &Path, items: &[Item]) {
    fs::write(path, bzip2(&tar_bytes(items))).unwrap();
}

fn make_tar_zst(path: &Path, items: &[Item]) {
    fs::write(path, zstd(&tar_bytes(items))).unwrap();
}

format_tests!(sevenz, make_7z, b"7z\xbc\xaf\x27\x1c");
format_tests!(tar_gz, make_tar_gz, b"\x1f\x8b");
format_tests!(tar_xz, make_tar_xz, b"\xfd7zXZ\x00");
format_tests!(tar_bz2, make_tar_bz2, b"BZh9");
format_tests!(tar_zst, make_tar_zst, b"\x28\xb5\x2f\xfd");
