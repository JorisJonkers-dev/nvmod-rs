// SPDX-License-Identifier: GPL-3.0-only
//! The limits on archive extraction (#81): per-entry size, expansion ratio, path depth, entry count, the step and
//! time budgets, case-fold and duplicate collisions, hard links and nested archives. Every archive is built in
//! code; each limit has a refusal and a positive control just under it, for zip, tar.gz and 7z alike. Nothing here
//! relies on a Unix-only feature except the one test marked `cfg(unix)`.

use nvmod_modinstall::{Limits, extract::extract};
use nvmod_testkit::{TarBuilder, TempDir, ZipBuilder};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::time::Duration;

type Items<'a> = &'a [(&'a str, &'a [u8])];

fn make_zip(path: &Path, items: Items) {
    let file = fs::File::create(path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in items {
        writer.start_file(*name, options).unwrap();
        writer.write_all(data).unwrap();
    }
    writer.finish().unwrap();
}

fn make_tar_gz(path: &Path, items: Items) {
    let mut builder = tar::Builder::new(Vec::new());
    for (name, data) in items {
        let mut header = tar::Header::new_gnu();
        header.set_mode(0o644);
        header.set_size(u64::try_from(data.len()).unwrap());
        if name.len() < 100 {
            // Written into the header as given, so an absolute or UNC name survives (`append_data` refuses it).
            header.as_old_mut().name[..name.len()].copy_from_slice(name.as_bytes());
            header.set_cksum();
            builder.append(&header, *data).unwrap();
        } else {
            builder.append_data(&mut header, name, *data).unwrap();
        }
    }
    let tar = builder.into_inner().unwrap();
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(&tar).unwrap();
    fs::write(path, encoder.finish().unwrap()).unwrap();
}

fn make_7z(path: &Path, items: Items) {
    let mut writer = sevenz_rust2::ArchiveWriter::create(path).unwrap();
    for (name, data) in items {
        writer
            .push_archive_entry(sevenz_rust2::ArchiveEntry::new_file(name), Some(*data))
            .unwrap();
    }
    writer.finish().unwrap();
}

/// Extract the archive made by `make` from `items` with `limits`; the outcome and the temp folder come back.
fn run(make: fn(&Path, Items), items: Items, limits: Limits) -> (TempDir, Result<(), String>, u64) {
    let dir = TempDir::new("bounded");
    let archive = dir.path().join("archive");
    make(&archive, items);
    let len = fs::metadata(&archive).unwrap().len();
    fs::create_dir(dir.path().join("out")).unwrap();
    let result = extract(&archive, &dir.path().join("out"), limits);
    (dir, result, len)
}

fn refused(result: &Result<(), String>, needle: &str) {
    match result {
        Err(why) => assert!(why.contains(needle), "expected `{needle}` in: {why}"),
        Ok(()) => panic!("accepted; expected a refusal containing `{needle}`"),
    }
}

/// Bytes that do not compress (an xorshift stream), so a ratio check sees about 1:1.
fn noise(len: usize) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state.to_le_bytes()[0]
        })
        .collect()
}

macro_rules! bounded_tests {
    ($module:ident, $make:path) => {
        mod $module {
            use super::*;

            #[test]
            fn a_file_over_the_entry_limit_is_refused_and_one_at_it_is_not() {
                let data = noise(100);
                let at = Limits {
                    max_entry_bytes: 100,
                    ..Limits::default()
                };
                let (dir, result, _) = run($make, &[("a.bin", &data)], at);
                result.unwrap();
                assert_eq!(fs::read(dir.path().join("out/a.bin")).unwrap(), data);
                let under = Limits {
                    max_entry_bytes: 99,
                    ..Limits::default()
                };
                let (_dir, result, _) = run($make, &[("a.bin", &data)], under);
                refused(&result, "--max-extract-entry-bytes");
            }

            #[test]
            fn the_total_is_bounded_with_a_positive_control() {
                let data = noise(600);
                let items: Items = &[("1", &data), ("2", &data)];
                let ok = Limits {
                    max_bytes: 1200,
                    ..Limits::default()
                };
                run($make, items, ok).1.unwrap();
                let over = Limits {
                    max_bytes: 1199,
                    ..Limits::default()
                };
                refused(&run($make, items, over).1, "--max-extract-bytes");
            }

            #[test]
            fn an_expansion_bomb_is_refused_and_a_ratio_just_over_it_is_not() {
                let zeros = vec![0_u8; 1 << 20];
                let items: Items = &[("zeros.bin", &zeros)];
                let base = Limits {
                    ratio_floor_bytes: 1024,
                    max_ratio: 20,
                    ..Limits::default()
                };
                let (dir, result, len) = run($make, items, base);
                refused(&result, "--max-extract-ratio");
                assert!(!dir.path().join("out/zeros.bin").exists());
                // The archive is the base of the ratio: allow exactly what this archive expands to.
                let actual = (1_u64 << 20).div_ceil(len);
                assert!(actual > 20, "the fixture must be a bomb ({actual}:1)");
                let at = Limits {
                    max_ratio: actual,
                    ..base
                };
                run($make, items, at).1.unwrap();
                let under = Limits {
                    max_ratio: actual / 2,
                    ..base
                };
                refused(&run($make, items, under).1, "--max-extract-ratio");
            }

            #[test]
            fn a_compressible_file_under_the_ratio_floor_is_accepted() {
                let zeros = vec![0_u8; 100_000];
                let limits = Limits {
                    max_ratio: 2,
                    ..Limits::default()
                };
                run($make, &[("zeros.bin", &zeros)], limits).1.unwrap();
            }

            #[test]
            fn incompressible_data_passes_a_strict_ratio() {
                let data = noise(64 * 1024);
                let limits = Limits {
                    ratio_floor_bytes: 1024,
                    max_ratio: 2,
                    ..Limits::default()
                };
                run($make, &[("noise.bin", &data)], limits).1.unwrap();
            }

            #[test]
            fn deep_nesting_is_refused_one_level_past_the_limit() {
                let limits = Limits {
                    max_path_depth: 8,
                    ..Limits::default()
                };
                let ok = format!("{}f.txt", "d/".repeat(7));
                let (dir, result, _) = run($make, &[(&ok, b"x")], limits);
                result.unwrap();
                assert!(dir.path().join("out").join(&ok).is_file());
                let deep = format!("{}f.txt", "d/".repeat(8));
                refused(
                    &run($make, &[(&deep, b"x")], limits).1,
                    "--max-extract-depth",
                );
            }

            #[test]
            fn the_default_depth_bound_refuses_a_path_of_65_folders() {
                let deep = format!("{}f.txt", "d/".repeat(64));
                refused(
                    &run($make, &[(&deep, b"x")], Limits::default()).1,
                    "--max-extract-depth",
                );
                let ok = format!("{}f.txt", "d/".repeat(63));
                run($make, &[(&ok, b"x")], Limits::default()).1.unwrap();
            }

            #[test]
            fn a_huge_entry_count_is_refused_and_the_limit_itself_is_not() {
                let names: Vec<String> = (0..2000).map(|i| format!("f/{i}.txt")).collect();
                let items: Vec<(&str, &[u8])> =
                    names.iter().map(|n| (n.as_str(), &b""[..])).collect();
                let at = Limits {
                    max_entries: 2000,
                    ..Limits::default()
                };
                run($make, &items, at).1.unwrap();
                let under = Limits {
                    max_entries: 1999,
                    ..Limits::default()
                };
                refused(&run($make, &items, under).1, "--max-extract-entries");
            }

            #[test]
            fn the_step_budget_refuses_and_a_larger_one_does_not() {
                let items: Items = &[("1", b"1"), ("2", b"2"), ("3", b"3")];
                let tiny = Limits {
                    max_operations: 1,
                    ..Limits::default()
                };
                refused(&run($make, items, tiny).1, "--max-extract-operations");
                let roomy = Limits {
                    max_operations: 1000,
                    ..Limits::default()
                };
                run($make, items, roomy).1.unwrap();
            }

            #[test]
            fn the_time_budget_refuses_and_a_generous_one_does_not() {
                let items: Items = &[("1", b"1")];
                let none = Limits {
                    max_duration: Duration::ZERO,
                    ..Limits::default()
                };
                refused(&run($make, items, none).1, "--max-extract-seconds");
                let minute = Limits {
                    max_duration: Duration::from_secs(60),
                    ..Limits::default()
                };
                run($make, items, minute).1.unwrap();
            }

            #[test]
            fn files_that_differ_only_in_case_are_refused() {
                let cases: [Items; 3] = [
                    &[("Readme.txt", b"a"), ("readme.txt", b"b")],
                    &[("Meshes/a.nif", b"a"), ("meshes/A.nif", b"b")],
                    &[("Data", b"a"), ("data/x.txt", b"b")],
                ];
                for items in cases {
                    let (_dir, result, _) = run($make, items, Limits::default());
                    match &result {
                        Err(why) => assert!(
                            why.contains("letter case") || why.contains("both a file and a folder"),
                            "{why}"
                        ),
                        Ok(()) => panic!("accepted {items:?}"),
                    }
                }
            }

            #[test]
            fn folders_that_differ_only_in_case_are_accepted() {
                let items: Items = &[
                    ("Textures/a.dds", b"a"),
                    ("textures/b.dds", b"b"),
                    ("Meshes/Armor/x.nif", b"x"),
                    ("meshes/armor/y.nif", b"y"),
                ];
                let (dir, result, _) = run($make, items, Limits::default());
                result.unwrap();
                let out = dir.path().join("out");
                for (name, data) in items {
                    assert_eq!(&fs::read(out.join(name)).unwrap(), data, "{name}");
                }
            }

            #[test]
            fn different_file_names_in_one_folder_are_accepted() {
                run(
                    $make,
                    &[("Meshes/a.nif", b"a"), ("Meshes/b.nif", b"b")],
                    Limits::default(),
                )
                .1
                .unwrap();
            }

            #[test]
            fn unc_and_drive_paths_are_refused_with_a_control_beside_them() {
                for name in [
                    "\\\\server\\share\\x.txt",
                    "//server/share/x.txt",
                    "D:/x.txt",
                ] {
                    let (_dir, result, _) = run($make, &[(name, b"x")], Limits::default());
                    refused(&result, "absolute");
                }
                run($make, &[("server/share/x.txt", b"x")], Limits::default())
                    .1
                    .unwrap();
            }

            #[test]
            fn a_name_that_windows_would_rewrite_is_refused() {
                for name in ["dir/file.", "dir /file.txt", "trailing "] {
                    let (_dir, result, _) = run($make, &[(name, b"x")], Limits::default());
                    refused(&result, "dot or a space");
                }
            }

            #[test]
            fn a_nested_archive_is_written_as_a_file_and_never_opened() {
                let inner_zeros = vec![0_u8; 1 << 20];
                let inner_dir = TempDir::new("inner");
                let inner = inner_dir.path().join("inner.zip");
                make_zip(&inner, &[("zeros.bin", &inner_zeros)]);
                let inner_bytes = fs::read(&inner).unwrap();
                let (dir, result, _) =
                    run($make, &[("inner.zip", &inner_bytes)], Limits::default());
                result.unwrap();
                let out = dir.path().join("out");
                assert_eq!(fs::read(out.join("inner.zip")).unwrap(), inner_bytes);
                assert!(!out.join("zeros.bin").exists());
                assert_eq!(fs::read_dir(&out).unwrap().count(), 1);
            }
        }
    };
}

bounded_tests!(zip_archive, make_zip);
bounded_tests!(tar_gz, make_tar_gz);
bounded_tests!(sevenz, make_7z);

/// A tar may name a file twice (the zip reader keeps one entry per name, so a zip cannot).
#[test]
fn a_tar_entry_that_appears_twice_is_refused() {
    let dir = TempDir::new("duplicate");
    let archive = dir.path().join("dup.tar");
    let tar = TarBuilder::new().file("a.txt", b"1").file("a.txt", b"2");
    fs::write(&archive, tar.bytes()).unwrap();
    let out = dir.path().join("out");
    fs::create_dir(&out).unwrap();
    refused(&extract(&archive, &out, Limits::default()), "appears twice");
}

#[test]
fn a_file_and_a_folder_of_the_same_name_are_refused() {
    let dir = TempDir::new("file-folder");
    let archive = dir.path().join("a.zip");
    ZipBuilder::new()
        .file("a", b"1")
        .file("a/b", b"2")
        .write(&archive);
    let out = dir.path().join("out");
    fs::create_dir(&out).unwrap();
    refused(
        &extract(&archive, &out, Limits::default()),
        "both a file and a folder",
    );
}

#[test]
fn a_hard_link_entry_is_refused_and_nothing_is_written_for_it() {
    // A tar header of type `1` (hard link) pointing at a file of the same archive.
    let mut builder = tar::Builder::new(Vec::new());
    let mut file = tar::Header::new_gnu();
    file.set_size(1);
    file.set_mode(0o644);
    builder
        .append_data(&mut file, "real.txt", &b"x"[..])
        .unwrap();
    let mut link = tar::Header::new_gnu();
    link.set_entry_type(tar::EntryType::Link);
    link.set_size(0);
    link.set_mode(0o644);
    builder
        .append_link(&mut link, "hard.txt", "real.txt")
        .unwrap();
    let dir = TempDir::new("hardlink");
    let archive = dir.path().join("a.tar");
    fs::write(&archive, builder.into_inner().unwrap()).unwrap();
    let out = dir.path().join("out");
    fs::create_dir(&out).unwrap();
    refused(
        &extract(&archive, &out, Limits::default()),
        "link or special file",
    );
    assert!(!out.join("hard.txt").exists());
}

/// A folder of the destination that is a link leading out is not written through (a link cannot come from an
/// archive, so the test puts one there). Creating a link needs a privilege on Windows, so this runs on Unix only.
#[cfg(unix)]
#[test]
fn a_write_through_an_existing_link_out_of_the_destination_is_refused() {
    let dir = TempDir::new("link-escape");
    let outside = dir.path().join("outside");
    fs::create_dir(&outside).unwrap();
    let out = dir.path().join("out");
    fs::create_dir(&out).unwrap();
    std::os::unix::fs::symlink(&outside, out.join("link")).unwrap();
    let archive = dir.path().join("a.zip");
    ZipBuilder::new().file("link/x.txt", b"x").write(&archive);
    refused(
        &extract(&archive, &out, Limits::default()),
        "outside the Mod folder",
    );
    assert!(!outside.join("x.txt").exists());
}
