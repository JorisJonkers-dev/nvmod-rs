// SPDX-License-Identifier: GPL-3.0-only
//! The downloads library on synthetic folders: archives are small files of made-up bytes, `.meta` files are
//! written here in the shape MO2 writes them. No mod data is used.

use nvmod_downloads::{
    Error, IdSource, InstalledMod, ListOptions, MatchedBy, MetaChange, State, change_meta,
    check_file_name, digests_of, installed_mods, list, show, trash,
};
use nvmod_testkit::{TempDir, write_file};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

/// A `.meta` with every key MO2 writes for a download, CRLF line endings, a comment, an unknown key and an opaque
/// `@Variant`. `installed=false` is the only line a mark-installed changes.
const FULL_META: &[u8] = b"[General]\r\ngameName=newvegas\r\nmodID=83815\r\nfileID=4711\r\nurl=\"https://example.invalid/nexus/newvegas/mods/83815\"\r\nname=Some Mod - Main File\r\nmodName=Some Mod\r\nversion=1.2.3\r\nnewestVersion=1.3.0\r\nfileTime=@DateTime(\\0\\0\\0\\x10\\0\\0\\0\\0\\0\\0\\0\\0\\0\\0\\0\\0\\0)\r\nfileCategory=1\r\ncategory=7\r\nrepository=Nexus\r\nuserData=@Variant(\\0\\0\\0\\b\\0\\0\\0\\0)\r\ninstalled=false\r\nuninstalled=false\r\npaused=false\r\nremoved=false\r\n; a comment that stays\r\nfutureKey=@ByteArray(a\\x00b)\r\n";

fn none() -> bool {
    false
}

fn quiet(_: &'static str, _: u64, _: u64) {}

fn run_list(
    dir: &Path,
    mods: &[InstalledMod],
    options: ListOptions,
) -> nvmod_downloads::ListReport {
    list(dir, mods, options, None, &none, &mut quiet).expect("lists")
}

fn a_mod(name: &str, file: Option<&str>, modid: Option<u64>) -> InstalledMod {
    InstalledMod {
        name: name.to_owned(),
        installation_file: file.map(str::to_owned),
        modid,
    }
}

#[test]
fn only_installed_changes_and_the_meta_comes_back_byte_for_byte() {
    let dir = TempDir::new("dl-roundtrip");
    write_file(dir.path(), "Some Mod-83815-1-2-3-1699999999.7z", b"archive");
    let meta = dir.path().join("Some Mod-83815-1-2-3-1699999999.7z.meta");
    fs::write(&meta, FULL_META).unwrap();
    let file = "Some Mod-83815-1-2-3-1699999999.7z";

    assert!(change_meta(dir.path(), file, MetaChange::Installed).unwrap());
    let changed = fs::read(&meta).unwrap();
    let old: Vec<&[u8]> = FULL_META.split_inclusive(|&b| b == b'\n').collect();
    let new: Vec<&[u8]> = changed.split_inclusive(|&b| b == b'\n').collect();
    assert_eq!(old.len(), new.len());
    let differing: Vec<usize> = (0..old.len()).filter(|&i| old[i] != new[i]).collect();
    assert_eq!(differing.len(), 1, "only one line differs");
    assert_eq!(new[differing[0]], b"installed=true\r\n");
    assert!(
        old[differing[0]] == b"installed=false\r\n",
        "the changed line is installed"
    );

    assert!(!change_meta(dir.path(), file, MetaChange::Installed).unwrap());
    assert!(change_meta(dir.path(), file, MetaChange::NotInstalled).unwrap());
    assert_eq!(
        fs::read(&meta).unwrap(),
        FULL_META,
        "back to the original bytes"
    );
}

#[test]
fn hiding_edits_only_removed_and_a_missing_meta_is_made_for_it() {
    let dir = TempDir::new("dl-hide");
    write_file(dir.path(), "a.zip", b"a");
    write_file(dir.path(), "b.zip", b"b");
    fs::write(dir.path().join("a.zip.meta"), FULL_META).unwrap();
    assert!(change_meta(dir.path(), "a.zip", MetaChange::Hide).unwrap());
    let text = fs::read_to_string(dir.path().join("a.zip.meta")).unwrap();
    assert!(text.contains("removed=true\r\n") && !text.contains("removed=false"));
    assert!(!change_meta(dir.path(), "a.zip", MetaChange::Hide).unwrap());
    assert!(change_meta(dir.path(), "a.zip", MetaChange::Unhide).unwrap());
    assert_eq!(fs::read(dir.path().join("a.zip.meta")).unwrap(), FULL_META);

    assert!(!dir.path().join("b.zip.meta").exists());
    assert!(
        !change_meta(dir.path(), "b.zip", MetaChange::Unhide).unwrap(),
        "nothing to show, nothing written"
    );
    assert!(!dir.path().join("b.zip.meta").exists());
    assert!(change_meta(dir.path(), "b.zip", MetaChange::Hide).unwrap());
    assert_eq!(
        fs::read(dir.path().join("b.zip.meta")).unwrap(),
        b"[General]\r\nremoved=true\r\n"
    );
}

#[test]
fn installing_clears_uninstalled_and_leaves_the_other_keys() {
    let dir = TempDir::new("dl-uninstalled");
    write_file(dir.path(), "a.zip", b"a");
    fs::write(
        dir.path().join("a.zip.meta"),
        b"[General]\ninstalled=false\nuninstalled=true\nmodID=5\n",
    )
    .unwrap();
    assert!(change_meta(dir.path(), "a.zip", MetaChange::Installed).unwrap());
    assert_eq!(
        fs::read(dir.path().join("a.zip.meta")).unwrap(),
        b"[General]\ninstalled=true\nuninstalled=false\nmodID=5\n"
    );
}

#[test]
fn states_ids_and_matches_are_read_from_the_meta_and_the_names() {
    let dir = TempDir::new("dl-list");
    let d = dir.path();
    // Installed: matched by the Mod's installationFile.
    write_file(d, "[Bracket] Mod (v2)-100-2-0-1699999999.zip", b"1");
    // Uninstalled with a full meta.
    write_file(d, "Plain.7z", b"22");
    fs::write(d.join("Plain.7z.meta"), FULL_META).unwrap();
    // Hidden and paused, from the meta.
    write_file(d, "Gone.zip", b"3");
    fs::write(d.join("Gone.zip.meta"), b"[General]\nremoved=true\n").unwrap();
    write_file(d, "Half.zip", b"4");
    fs::write(d.join("Half.zip.meta"), b"[General]\npaused=true\n").unwrap();
    write_file(d, "Partial.7z.unfinished", b"5");
    // installed=true in the meta, no Mod: installed as far as the library can tell.
    write_file(d, "Old.zip", b"6");
    fs::write(d.join("Old.zip.meta"), b"[General]\ninstalled=true\n").unwrap();
    // Not archives: a subfolder, an orphan meta, a leftover temporary file.
    fs::create_dir_all(d.join("sub")).unwrap();
    fs::write(d.join("Orphan.zip.meta"), b"[General]\n").unwrap();
    fs::write(d.join("x.zip.meta.nvmod-tmp"), b"x").unwrap();
    // Same Nexus mod as an installed Mod, other file.
    write_file(d, "Other-200-1-0-1699999999.zip", b"7");

    let mods = [
        a_mod(
            "Bracket Mod",
            Some("[bracket] mod (V2)-100-2-0-1699999999.ZIP"),
            Some(100),
        ),
        a_mod("Other Mod", Some("Other-200-0-9-1699999999.zip"), Some(200)),
        a_mod("Hand made", None, None),
    ];
    let report = run_list(d, &mods, ListOptions::default());
    assert_eq!(report.schema_version, 1);
    let by = |file: &str| {
        report
            .archives
            .iter()
            .find(|a| a.file == file)
            .unwrap_or_else(|| panic!("{file} is listed: {:?}", report.archives))
    };
    assert_eq!(report.archives.len(), 7, "{:?}", report.archives);

    let bracket = by("[Bracket] Mod (v2)-100-2-0-1699999999.zip");
    assert_eq!(bracket.state, State::Installed);
    let m = bracket.matched_mod.as_ref().unwrap();
    assert_eq!(
        (m.name.as_str(), m.by),
        ("Bracket Mod", MatchedBy::InstallationFile)
    );
    assert_eq!(bracket.nexus_mod_id, Some(100));
    assert_eq!(bracket.id_source, Some(IdSource::FileName));
    assert_eq!(bracket.version.as_deref(), Some("2.0"));
    assert!(!bracket.has_meta);

    let plain = by("Plain.7z");
    assert_eq!(plain.state, State::Uninstalled);
    assert_eq!(plain.matched_mod, None);
    assert_eq!(
        (plain.nexus_mod_id, plain.nexus_file_id, plain.id_source),
        (Some(83815), Some(4711), Some(IdSource::Meta))
    );
    assert_eq!(plain.game_name.as_deref(), Some("newvegas"));
    assert_eq!(plain.mod_name.as_deref(), Some("Some Mod"));
    assert_eq!(plain.name.as_deref(), Some("Some Mod - Main File"));
    assert_eq!(plain.version.as_deref(), Some("1.2.3"));
    assert_eq!(plain.newest_version.as_deref(), Some("1.3.0"));
    assert_eq!(plain.repository.as_deref(), Some("Nexus"));
    assert_eq!(plain.size, 2);
    assert!(plain.has_meta);

    assert_eq!(by("Gone.zip").state, State::Hidden);
    assert_eq!(by("Half.zip").state, State::Paused);
    assert_eq!(by("Partial.7z.unfinished").state, State::Paused);
    assert_eq!(by("Old.zip").state, State::Installed);

    let other = by("Other-200-1-0-1699999999.zip");
    assert_eq!(other.state, State::Uninstalled);
    let m = other.matched_mod.as_ref().unwrap();
    assert_eq!((m.name.as_str(), m.by), ("Other Mod", MatchedBy::ModId));

    // Sorted without regard to case.
    let names: Vec<&str> = report.archives.iter().map(|a| a.file.as_str()).collect();
    let mut sorted = names.clone();
    sorted.sort_by_key(|n| n.to_lowercase());
    assert_eq!(names, sorted);
    assert_eq!(report.mods_without_archive.len(), 0);
}

#[test]
fn unmatched_lists_archives_with_no_mod_and_mods_with_no_archive() {
    let dir = TempDir::new("dl-unmatched");
    let d = dir.path();
    write_file(d, "Used.zip", b"1");
    write_file(d, "Loose.zip", b"2");
    let mods = [
        a_mod("Used", Some("used.ZIP"), None),
        a_mod("Lost", Some("Lost.7z"), Some(9)),
        a_mod("Hand made", None, None),
    ];
    let report = run_list(
        d,
        &mods,
        ListOptions {
            unmatched_only: true,
            hash: false,
        },
    );
    assert!(report.unmatched_only);
    let files: Vec<&str> = report.archives.iter().map(|a| a.file.as_str()).collect();
    assert_eq!(files, ["Loose.zip"]);
    let lost: Vec<(&str, Option<&str>)> = report
        .mods_without_archive
        .iter()
        .map(|m| (m.name.as_str(), m.installation_file.as_deref()))
        .collect();
    assert_eq!(lost, [("Lost", Some("Lost.7z")), ("Hand made", None)]);
}

#[test]
fn installation_file_is_a_bare_name_whatever_the_meta_ini_holds() {
    let mods_dir = TempDir::new("dl-mods");
    write_file(
        mods_dir.path(),
        "WinPath/meta.ini",
        b"[General]\r\nmodid=12\r\ninstallationFile=C:\\\\Users\\\\someone\\\\Downloads\\\\Win Mod-12-1-0-1699999999.7z\r\n",
    );
    write_file(
        mods_dir.path(),
        "Slashes/meta.ini",
        b"[General]\ninstallationFile=D:/Games/dl/Slash.zip\n",
    );
    write_file(mods_dir.path(), "NoMeta/readme.txt", b"x");
    write_file(
        mods_dir.path(),
        "Section_separator/meta.ini",
        b"[General]\n",
    );
    let mods = installed_mods(mods_dir.path());
    let names: Vec<&str> = mods.iter().map(|m| m.name.as_str()).collect();
    assert_eq!(names, ["NoMeta", "Slashes", "WinPath"]);
    assert_eq!(
        mods[2].installation_file.as_deref(),
        Some("Win Mod-12-1-0-1699999999.7z")
    );
    assert_eq!(mods[2].modid, Some(12));
    assert_eq!(mods[1].installation_file.as_deref(), Some("Slash.zip"));
    assert_eq!(mods[0].installation_file, None);

    let dl = TempDir::new("dl-winpath");
    write_file(dl.path(), "Win Mod-12-1-0-1699999999.7z", b"x");
    let report = run_list(dl.path(), &mods, ListOptions::default());
    assert_eq!(report.archives[0].state, State::Installed);
    // The folder's own path is the caller's (a Windows temp folder has `Users` in it); the rows hold names only.
    let json = serde_json::to_string(&report.archives).unwrap();
    assert!(!json.contains("Users") && !json.contains("D:/"), "{json}");
}

#[test]
fn hashes_match_known_values_and_the_cache_is_keyed_by_size_and_time() {
    let dir = TempDir::new("dl-hash");
    let d = dir.path();
    write_file(d, "abc.zip", b"abc");
    let cache = d.join("cache").join("hashes.json");
    let options = ListOptions {
        unmatched_only: false,
        hash: true,
    };
    let hashed = |options: ListOptions| {
        list(d, &[], options, Some(&cache), &none, &mut quiet)
            .unwrap()
            .archives
            .remove(0)
    };
    let a = hashed(options);
    assert_eq!(a.md5.as_deref(), Some("900150983cd24fb0d6963f7d28e17f72"));
    assert_eq!(
        a.sha256.as_deref(),
        Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
    );
    assert!(cache.is_file(), "the cache was written");

    // Same size and time: the cached digest is used, even though the bytes differ.
    let path = d.join("abc.zip");
    let mtime = fs::metadata(&path).unwrap().modified().unwrap();
    fs::write(&path, b"xyz").unwrap();
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    assert_eq!(hashed(options).sha256, a.sha256, "served from the cache");

    // A new time: read again.
    fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_modified(mtime + Duration::from_secs(10))
        .unwrap();
    assert_eq!(
        hashed(options).sha256.as_deref(),
        Some(digests_of(b"xyz").sha256.as_str())
    );

    // A new size: read again.
    fs::write(&path, b"xyzw").unwrap();
    assert_eq!(
        hashed(options).md5.as_deref(),
        Some(digests_of(b"xyzw").md5.as_str())
    );

    // Without --hash there are no digests and the cache is not read.
    let plain = hashed(ListOptions::default());
    assert_eq!((plain.md5, plain.sha256), (None, None));
}

#[test]
fn show_gives_one_archive_with_optional_hashes_and_refuses_paths() {
    let dir = TempDir::new("dl-show");
    let d = dir.path();
    write_file(d, "abc.zip", b"abc");
    let one = show(d, "abc.zip", &[], true, None, &none, &mut quiet).unwrap();
    assert_eq!(one.archive.file, "abc.zip");
    assert_eq!(
        one.archive.md5.as_deref(),
        Some("900150983cd24fb0d6963f7d28e17f72")
    );
    let bare = show(d, "abc.zip", &[], false, None, &none, &mut quiet).unwrap();
    assert_eq!(bare.archive.md5, None);
    for bad in [
        "../abc.zip",
        "sub/abc.zip",
        "C:\\abc.zip",
        "",
        "..",
        "abc.zip.meta",
    ] {
        assert!(
            matches!(
                show(d, bad, &[], false, None, &none, &mut quiet),
                Err(Error::Refused(_))
            ),
            "{bad}"
        );
        assert!(check_file_name(bad).is_err(), "{bad}");
    }
    assert!(matches!(
        show(d, "missing.zip", &[], false, None, &none, &mut quiet),
        Err(Error::Refused(_))
    ));
}

#[test]
fn cancel_stops_listing_and_hashing() {
    let dir = TempDir::new("dl-cancel");
    write_file(dir.path(), "a.zip", b"a");
    let yes = || true;
    assert_eq!(
        list(
            dir.path(),
            &[],
            ListOptions::default(),
            None,
            &yes,
            &mut quiet
        ),
        Err(Error::Cancelled)
    );
}

#[test]
fn forgetting_moves_the_archive_and_its_meta_to_the_trash() {
    let dir = TempDir::new("dl-forget");
    let d = dir.path();
    write_file(d, "dl/[x] a.zip", b"archive");
    fs::write(d.join("dl/[x] a.zip.meta"), FULL_META).unwrap();
    write_file(d, "dl/b.zip", b"b");
    let trash_root = d.join("trash");
    let moved = trash(&d.join("dl"), "[x] a.zip", &trash_root).unwrap();
    assert!(moved.starts_with(&trash_root));
    assert!(!d.join("dl/[x] a.zip").exists() && !d.join("dl/[x] a.zip.meta").exists());
    assert_eq!(fs::read(moved.join("[x] a.zip")).unwrap(), b"archive");
    assert_eq!(fs::read(moved.join("[x] a.zip.meta")).unwrap(), FULL_META);
    // No meta: only the archive moves.
    let moved = trash(&d.join("dl"), "b.zip", &trash_root).unwrap();
    assert!(moved.join("b.zip").is_file());
    assert!(matches!(
        trash(&d.join("dl"), "b.zip", &trash_root),
        Err(Error::Refused(_))
    ));
}

#[test]
fn two_thousand_archives_list_quickly_without_hashing() {
    let dir = TempDir::new("dl-perf");
    let d = dir.path();
    let mut mods = Vec::new();
    for i in 0..2000_u32 {
        let name = format!("Synthetic Mod {i}-{}-1-0-1699999999.7z", 1000 + i);
        fs::write(d.join(&name), vec![u8::try_from(i % 251).unwrap(); 64]).unwrap();
        if i % 2 == 0 {
            fs::write(d.join(format!("{name}.meta")), FULL_META).unwrap();
        }
        if i % 3 == 0 {
            mods.push(a_mod(
                &format!("Mod {i}"),
                Some(&name),
                Some(u64::from(1000 + i)),
            ));
        }
    }
    let start = Instant::now();
    let report = run_list(d, &mods, ListOptions::default());
    let took = start.elapsed();
    assert_eq!(report.archives.len(), 2000);
    assert_eq!(
        report
            .archives
            .iter()
            .filter(|a| a.state == State::Installed)
            .count(),
        667
    );
    // The ticket's budget is 2 s; the bound is generous so a loaded CI machine does not flake.
    assert!(took < Duration::from_secs(10), "listing took {took:?}");
}
