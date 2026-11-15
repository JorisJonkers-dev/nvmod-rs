// SPDX-License-Identifier: GPL-3.0-only
//! Tests of `nvmod_saves` on synthetic saves: every file is built here from the assumed header layout.

use nvmod_instance::NvmodInstance;
use nvmod_profile::Target;
use nvmod_saves::{Error, Guard, Location, backup, check, list, locate, restore, transfer};
use nvmod_testkit::{BaseBuilder, SaveBuilder, TempDir, snapshot, write_file};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const NOW: i64 = 1_794_657_600;

struct Fixture {
    _base: TempDir,
    scratch: TempDir,
}

impl Fixture {
    fn new() -> Fixture {
        let base = TempDir::new("sv-base");
        BaseBuilder::steam().write(base.path());
        let scratch = TempDir::new("sv");
        let instance = NvmodInstance::create(&scratch.path().join("inst"), base.path()).unwrap();
        for (name, local) in [("A", true), ("B", true), ("Shared", false)] {
            instance.create_profile(name).unwrap();
            write_file(
                &instance.root,
                &format!("profiles/{name}/settings.ini"),
                format!("[General]\r\nLocalSaves={local}\r\n").as_bytes(),
            );
            write_file(
                &instance.root,
                &format!("profiles/{name}/plugins.txt"),
                b"FalloutNV.esm\r\nActive.esp\r\n",
            );
        }
        fs::create_dir_all(
            scratch
                .path()
                .join("prefix/pfx/drive_c/users/steamuser/Documents"),
        )
        .unwrap();
        Fixture {
            _base: base,
            scratch,
        }
    }
    fn instance(&self) -> NvmodInstance {
        NvmodInstance::open(&self.scratch.path().join("inst")).unwrap()
    }
    fn target(&self) -> Target {
        Target::from_prefix(&self.scratch.path().join("prefix")).unwrap()
    }
    fn loc(&self, profile: &str) -> Location {
        locate(&self.instance(), profile, Some(&self.target())).unwrap()
    }
    fn save(&self, profile: &str, name: &str, bytes: &[u8]) {
        let file = self.loc(profile).dir.join(name);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, bytes).unwrap();
    }
}

fn b_folder(f: &Fixture) -> PathBuf {
    f.loc("B").dir.parent().unwrap().to_path_buf()
}

fn names(dir: &Path) -> BTreeMap<String, Vec<u8>> {
    snapshot(dir)
}

#[test]
fn the_save_folder_follows_local_saves() {
    let f = Fixture::new();
    assert!(f.loc("A").local);
    assert!(f.loc("A").dir.ends_with("profiles/A/saves"));
    let shared = f.loc("Shared");
    assert!(!shared.local);
    assert!(shared.dir.ends_with("My Games/FalloutNV/Saves"));
    assert!(matches!(
        locate(&f.instance(), "Shared", None),
        Err(Error::Refused(_))
    ));
    assert!(matches!(
        locate(&f.instance(), "Nope", None),
        Err(Error::Refused(_))
    ));
}

#[test]
fn saves_list_with_their_plugin_count_and_a_truncated_header_is_unreadable() {
    let f = Fixture::new();
    let good = SaveBuilder::new(1)
        .plugins(&["FalloutNV.esm", "Active.esp"])
        .build();
    f.save("A", "Save 1.fos", &good);
    f.save("A", "Save 1.nvse", b"co");
    f.save("A", "Save 2.fos", &good[..good.len() / 2]);
    f.save("A", "notes.txt", b"not a save");
    let saves = list(&f.loc("A").dir);
    assert_eq!(saves.len(), 2);
    assert_eq!(
        (
            saves[0].name.as_str(),
            saves[0].status,
            saves[0].plugins,
            saves[0].cosave
        ),
        ("Save 1.fos", "ok", Some(2), true)
    );
    assert_eq!(saves[0].size, good.len() as u64);
    assert_eq!(saves[1].status, "unreadable");
    assert!(saves[1].problem.as_deref().unwrap().contains("ends inside"));
    assert!(!saves[1].cosave);
    assert_eq!(list(&f.scratch.path().join("nowhere")), vec![]);
}

#[test]
fn backup_then_delete_then_restore_is_byte_identical() {
    let f = Fixture::new();
    f.save("A", "Save 1.fos", &SaveBuilder::new(1).body(b"one").build());
    f.save("A", "Save 1.nvse", b"cosave one");
    f.save("A", "sub/Deep.fos", b"deep");
    let dir = f.loc("A").dir;
    let before = names(&dir);
    let report = backup(&f.instance(), "A", &f.loc("A"), NOW).unwrap();
    assert_eq!(report.files, 3);
    assert_eq!(report.id, "20261114T120000Z");
    fs::remove_dir_all(&dir).unwrap();
    let restored = restore(&f.instance(), "A", &f.loc("A"), "latest", NOW + 5).unwrap();
    assert_eq!(restored.files, 3);
    assert_eq!(
        restored.pre_restore, None,
        "an empty folder needs no backup"
    );
    assert_eq!(names(&dir), before);
}

#[test]
fn restore_backs_up_what_is_there_and_never_deletes() {
    let f = Fixture::new();
    f.save("A", "Save 1.fos", b"original");
    let id = backup(&f.instance(), "A", &f.loc("A"), NOW).unwrap().id;
    f.save("A", "Save 1.fos", b"changed since");
    f.save("A", "Save 2.fos", b"new since");
    let r = restore(&f.instance(), "A", &f.loc("A"), &id, NOW).unwrap();
    let dir = f.loc("A").dir;
    assert_eq!(fs::read(dir.join("Save 1.fos")).unwrap(), b"original");
    assert_eq!(fs::read(dir.join("Save 2.fos")).unwrap(), b"new since");
    assert_eq!(r.kept_extra, ["Save 2.fos"]);
    let pre = r.pre_restore.unwrap();
    assert_ne!(pre, id);
    // The way back: restoring the pre-restore backup brings the changed save back.
    restore(&f.instance(), "A", &f.loc("A"), &pre, NOW + 9).unwrap();
    assert_eq!(fs::read(dir.join("Save 1.fos")).unwrap(), b"changed since");
}

#[test]
fn a_damaged_backup_is_refused_and_nothing_changes() {
    let f = Fixture::new();
    f.save("A", "Save 1.fos", b"original");
    let id = backup(&f.instance(), "A", &f.loc("A"), NOW).unwrap().id;
    let file = f
        .scratch
        .path()
        .join(format!("inst/.nvmod/saves-backup/{id}/files/Save 1.fos"));
    fs::write(&file, b"tampered").unwrap();
    f.save("A", "Save 1.fos", b"current");
    let before = snapshot(f.scratch.path());
    assert!(matches!(
        restore(&f.instance(), "A", &f.loc("A"), &id, NOW),
        Err(Error::Corrupt(_))
    ));
    assert_eq!(snapshot(f.scratch.path()), before);
    assert!(matches!(
        restore(&f.instance(), "A", &f.loc("A"), "../x", NOW),
        Err(Error::Refused(_))
    ));
    assert!(matches!(
        restore(&f.instance(), "B", &f.loc("B"), &id, NOW),
        Err(Error::Refused(_)),
    ));
}

#[test]
fn transfer_copies_saves_with_their_cosaves_and_refuses_a_missing_partner() {
    let f = Fixture::new();
    f.save("A", "Save 1.fos", b"fos one");
    f.save("A", "Save 1.nvse", b"nvse one");
    f.save("A", "Save 2.fos", b"fos two");
    let before = snapshot(&b_folder(&f));
    let err = transfer(("A", &f.loc("A")), ("B", &f.loc("B")), false).unwrap_err();
    match err {
        Error::Refused(why) => assert!(
            why.contains("Save 2.fos") && why.contains("--force"),
            "{why}"
        ),
        other => panic!("{other:?}"),
    }
    assert_eq!(snapshot(&b_folder(&f)), before);
    let r = transfer(("A", &f.loc("A")), ("B", &f.loc("B")), true).unwrap();
    assert_eq!(r.copied, ["Save 1.fos", "Save 1.nvse", "Save 2.fos"]);
    assert_eq!(r.without_cosave, ["Save 2.fos"]);
    assert_eq!(
        fs::read(f.loc("B").dir.join("Save 1.nvse")).unwrap(),
        b"nvse one"
    );
    // Again: identical files are already there, a different one is never overwritten.
    let again = transfer(("A", &f.loc("A")), ("B", &f.loc("B")), true).unwrap();
    assert!(again.copied.is_empty() && again.already_there.len() == 3);
    f.save("B", "Save 1.fos", b"different");
    assert!(matches!(
        transfer(("A", &f.loc("A")), ("B", &f.loc("B")), true),
        Err(Error::Refused(_))
    ));
    assert_eq!(
        fs::read(f.loc("B").dir.join("Save 1.fos")).unwrap(),
        b"different"
    );
    assert!(matches!(
        transfer(("A", &f.loc("A")), ("A", &f.loc("A")), true),
        Err(Error::Refused(_))
    ));
}

#[test]
fn check_reports_a_save_with_a_plugin_the_profile_has_not_active() {
    let f = Fixture::new();
    f.save(
        "A",
        "Fine.fos",
        &SaveBuilder::new(1)
            .plugins(&["FalloutNV.esm", "ACTIVE.esp"])
            .build(),
    );
    f.save(
        "A",
        "Broken.fos",
        &SaveBuilder::new(2)
            .plugins(&["FalloutNV.esm", "Gone.esp", "Active.esp", "Also.esm"])
            .build(),
    );
    f.save("A", "Cut.fos", b"FO3SAVEGAME");
    let r = check(&f.instance(), "A", &f.loc("A")).unwrap();
    assert!(!r.clean);
    let by: BTreeMap<&str, &nvmod_saves::SaveCheck> =
        r.saves.iter().map(|s| (s.name.as_str(), s)).collect();
    assert_eq!(by["Fine.fos"].status, "ok");
    assert_eq!(by["Broken.fos"].missing, ["Gone.esp", "Also.esm"]);
    assert_eq!(by["Cut.fos"].status, "unreadable");
    assert_eq!(r.active, 2);
}

#[test]
fn the_guard_warns_about_a_save_the_profile_did_not_expect_to_change() {
    let f = Fixture::new();
    f.save("A", "Save 1.fos", b"one");
    f.save("A", "Quick.fos", b"quick");
    f.save("B", "Other.fos", b"other profile");
    f.save("Shared", "Shared.fos", b"shared");
    let target = f.target();
    // The fake launcher: the game saves a new file and overwrites a quicksave in the active folder.
    let guard = Guard::before(&f.instance(), "A", Some(&target));
    f.save("A", "Save 2.fos", b"two");
    f.save("A", "Quick.fos", b"quick again");
    let clean = guard.after();
    assert!(clean.warnings.is_empty(), "{:?}", clean.warnings);
    assert_eq!(clean.changes.len(), 2);
    assert!(
        clean
            .changes
            .iter()
            .all(|c| c.expected && c.folder == "active")
    );
    // Now the launcher also damages a save of another profile, the shared folder, and removes an own save.
    let guard = Guard::before(&f.instance(), "A", Some(&target));
    f.save("B", "Other.fos", b"damaged");
    f.save("Shared", "Shared.fos", b"damaged");
    fs::remove_file(f.loc("A").dir.join("Save 1.fos")).unwrap();
    let report = guard.after();
    assert_eq!(report.warnings.len(), 3, "{:?}", report.warnings);
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("Other.fos") && w.contains("profile B"))
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("Shared.fos") && w.contains("shared"))
    );
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("Save 1.fos") && w.contains("removed"))
    );
}

#[test]
fn nothing_outside_the_save_folder_and_the_backup_folder_is_touched() {
    let f = Fixture::new();
    let sentinel = f.scratch.path().join("sentinel");
    write_file(&sentinel, "Save.fos", b"theirs");
    write_file(&sentinel, "Saves/Other.fos", b"theirs too");
    let guarded = snapshot(&sentinel);
    let profile_files = snapshot(&f.instance().profile_dir("A"));
    f.save("A", "Save 1.fos", &SaveBuilder::new(1).build());
    f.save("B", "Save 9.fos", &SaveBuilder::new(9).build());
    let id = backup(&f.instance(), "A", &f.loc("A"), NOW).unwrap().id;
    restore(&f.instance(), "A", &f.loc("A"), &id, NOW).unwrap();
    transfer(("A", &f.loc("A")), ("B", &f.loc("B")), true).unwrap();
    let _ = list(&f.loc("A").dir);
    let _ = check(&f.instance(), "A", &f.loc("A")).unwrap();
    assert_eq!(snapshot(&sentinel), guarded);
    let after = snapshot(&f.instance().profile_dir("A"));
    for (path, bytes) in &profile_files {
        assert_eq!(after.get(path), Some(bytes), "{path} changed");
    }
    let extra: Vec<&String> = after
        .keys()
        .filter(|k| !profile_files.contains_key(*k))
        .collect();
    assert!(extra.iter().all(|k| k.starts_with("saves/")), "{extra:?}");
}
