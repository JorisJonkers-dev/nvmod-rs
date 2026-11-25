// SPDX-License-Identifier: GPL-3.0-only
//! Golden tests of `nvmod_profile::apply` and `finish` on synthetic profiles and a synthetic prefix. Every file is
//! made here; nothing is game data.

use nvmod_instance::NvmodInstance;
use nvmod_profile::{Error, Target, apply, finish};
use nvmod_testkit::{BaseBuilder, TempDir, snapshot, write_file};
use nvmod_testkit::{covers, ids};
use std::fs;
use std::path::{Path, PathBuf};

const FALLOUT_INI: &str = "[General]\r\nSLocalSavePath=Saves\\\r\n\r\n[Archive]\r\nSArchiveList=Fallout - Meshes.bsa, Fallout - Textures.bsa\r\nSInvalidationFile=ArchiveInvalidation.txt\r\n";

struct Fixture {
    _base: TempDir,
    scratch: TempDir,
}

impl Fixture {
    fn new() -> Fixture {
        let base = TempDir::new("pf-base");
        BaseBuilder::steam().write(base.path());
        let scratch = TempDir::new("pf");
        NvmodInstance::create(&scratch.path().join("inst"), base.path()).unwrap();
        let f = Fixture {
            _base: base,
            scratch,
        };
        let docs = f
            .prefix()
            .join("pfx/drive_c/users/steamuser/Documents/My Games");
        fs::create_dir_all(&docs).unwrap();
        fs::create_dir_all(f.prefix().join("pfx/drive_c/users/steamuser/AppData/Local")).unwrap();
        f
    }
    fn root(&self) -> PathBuf {
        self.scratch.path().join("inst")
    }
    fn prefix(&self) -> PathBuf {
        self.scratch.path().join("prefix")
    }
    fn instance(&self) -> NvmodInstance {
        NvmodInstance::open(&self.root()).unwrap()
    }
    fn target(&self) -> Target {
        Target::from_prefix(&self.prefix()).unwrap()
    }
    fn profile(&self, name: &str, settings: &str) {
        self.instance().create_profile(name).unwrap();
        write_file(
            &self.root(),
            &format!("profiles/{name}/settings.ini"),
            format!("[General]\r\n{settings}").as_bytes(),
        );
    }
    fn put(&self, relative: &str, bytes: &[u8]) {
        write_file(&self.root(), relative, bytes);
    }
    fn read(&self, relative: &str) -> Vec<u8> {
        fs::read(self.root().join(relative)).unwrap()
    }
    fn games(&self) -> PathBuf {
        self.target().my_games
    }
    fn data(&self) -> PathBuf {
        self.target().app_data
    }
}

fn text(path: &Path) -> String {
    String::from_utf8(fs::read(path).unwrap()).unwrap()
}

#[test]
fn the_prefix_layout_is_found_under_pfx_or_directly() {
    let f = Fixture::new();
    let t = f.target();
    assert!(t.my_games.ends_with("Documents/My Games/FalloutNV"));
    assert!(t.app_data.ends_with("AppData/Local/FalloutNV"));
    assert!(t.my_games.starts_with(f.prefix().join("pfx")));
    let direct = f.prefix().join("pfx");
    assert!(Target::from_prefix(&direct).is_ok());
    assert!(Target::from_prefix(&f.scratch.path().join("nope")).is_err());
}

#[test]
fn local_settings_place_the_five_inis_and_off_puts_the_users_own_back() {
    covers(&[ids::run::prefix::LOCAL_SETTINGS]);
    let f = Fixture::new();
    f.profile("A", "LocalSettings=true\r\n");
    for name in nvmod_profile::INIS {
        f.put(
            &format!("profiles/A/{name}"),
            format!("[{name}]\r\nfrom=profile\r\n").as_bytes(),
        );
    }
    // The user's own fallout.ini is already in the prefix; falloutprefs.ini is not.
    fs::create_dir_all(f.games()).unwrap();
    fs::write(f.games().join("fallout.ini"), b"[General]\r\nmine=1\r\n").unwrap();
    let report = apply(&f.instance(), "A", &f.target(), false).unwrap();
    assert_eq!(report.status, "applied");
    for name in nvmod_profile::INIS {
        assert_eq!(
            text(&f.games().join(name)),
            format!("[{name}]\r\nfrom=profile\r\n")
        );
    }
    // Switch LocalSettings off: the user's own file returns, the placed one without an original goes.
    f.put(
        "profiles/A/settings.ini",
        b"[General]\r\nLocalSettings=false\r\n",
    );
    apply(&f.instance(), "A", &f.target(), false).unwrap();
    assert_eq!(
        text(&f.games().join("fallout.ini")),
        "[General]\r\nmine=1\r\n"
    );
    assert!(!f.games().join("falloutprefs.ini").exists());
    assert!(!f.games().join("GECKPrefs.ini").exists());
}

#[test]
fn a_profile_without_an_ini_is_seeded_from_the_users_own_file() {
    let f = Fixture::new();
    f.profile("A", "LocalSettings=true\r\n");
    fs::create_dir_all(f.games()).unwrap();
    fs::write(f.games().join("fallout.ini"), b"[General]\r\nmine=1\r\n").unwrap();
    apply(&f.instance(), "A", &f.target(), false).unwrap();
    assert_eq!(f.read("profiles/A/fallout.ini"), b"[General]\r\nmine=1\r\n");
    assert_eq!(
        text(&f.games().join("fallout.ini")),
        "[General]\r\nmine=1\r\n"
    );
}

#[test]
fn initweaks_merge_in_priority_order_with_the_profile_tweaks_last() {
    covers(&[ids::run::prefix::INITWEAKS_MERGE]);
    let f = Fixture::new();
    f.profile("A", "");
    // Highest priority first, as modlist.txt has it; Off is disabled and Foreign has no folder.
    f.put(
        "profiles/A/modlist.txt",
        b"# This file was automatically generated by Mod Organizer.\r\n+High\r\n-Off\r\n+Low\r\n",
    );
    f.put(
        "mods/Low/INI Tweaks/a.ini",
        b"[Display]\r\niSize=1\r\nfoo=low\r\n[Only]\r\nlow=1\r\n",
    );
    f.put("mods/Off/INI Tweaks/a.ini", b"[Display]\r\nfoo=off\r\n");
    f.put("mods/High/ini tweaks/b.INI", b"[display]\r\nFOO=high\r\n");
    f.put("profiles/A/profile_tweaks.ini", b"[Display]\r\niSize=9\r\n");
    apply(&f.instance(), "A", &f.target(), false).unwrap();
    assert_eq!(
        text(&f.root().join("profiles/A/initweaks.ini")),
        "[Display]\r\niSize=9\r\nfoo=high\r\n[Only]\r\nlow=1\r\n[Archive]\r\nbInvalidateOlderFiles=1\r\n"
    );
}

#[test]
fn initweaks_with_nothing_to_merge_is_the_forced_key_alone() {
    let f = Fixture::new();
    f.profile("A", "");
    apply(&f.instance(), "A", &f.target(), false).unwrap();
    assert_eq!(
        f.read("profiles/A/initweaks.ini"),
        b"[Archive]\r\nbInvalidateOlderFiles=1\r\n"
    );
    assert_eq!(f.read("profiles/A/initweaks.ini").len(), 36);
}

#[test]
fn local_saves_redirect_and_restore_byte_for_byte() {
    covers(&[ids::run::prefix::LOCAL_SAVES_REDIRECT]);
    let f = Fixture::new();
    f.profile(
        "A",
        "LocalSettings=true\r\nLocalSaves=true\r\nAutomaticArchiveInvalidation=true\r\n",
    );
    f.put("profiles/A/fallout.ini", FALLOUT_INI.as_bytes());
    apply(&f.instance(), "A", &f.target(), false).unwrap();
    let on = "[General]\r\nSLocalSavePath=__MO_Saves\\\r\nbUseMyGamesDirectory=1\r\n\r\n[Archive]\r\nSArchiveList=Fallout - Invalidation.bsa, Fallout - Meshes.bsa, Fallout - Textures.bsa\r\nSInvalidationFile=\r\nbInvalidateOlderFiles=1\r\n";
    assert_eq!(text(&f.root().join("profiles/A/fallout.ini")), on);
    assert_eq!(text(&f.games().join("fallout.ini")), on);
    assert_eq!(
        f.read("profiles/A/savepath.ini"),
        b"[General]\r\nsLocalSavePath=Saves\\\r\n"
    );
    let link = f.games().join("__MO_Saves");
    assert_eq!(
        fs::read_link(&link).unwrap(),
        f.root().join("profiles/A/saves")
    );
    assert!(f.root().join("profiles/A/saves").is_dir());
    assert_eq!(
        f.read("overwrite/Fallout - Invalidation.bsa").len(),
        36,
        "the dummy archive is generated"
    );
    // Both off: the INI is the original again, savepath.ini and the link and the archive are gone.
    f.put(
        "profiles/A/settings.ini",
        b"[General]\r\nLocalSettings=true\r\n",
    );
    apply(&f.instance(), "A", &f.target(), false).unwrap();
    // MO2 leaves bInvalidateOlderFiles=1 behind, and so does nvmod.
    let off = format!("{FALLOUT_INI}bInvalidateOlderFiles=1\r\n");
    assert_eq!(text(&f.root().join("profiles/A/fallout.ini")), off);
    assert_eq!(text(&f.games().join("fallout.ini")), off);
    assert!(!f.root().join("profiles/A/savepath.ini").exists());
    assert!(fs::symlink_metadata(&link).is_err());
    assert!(
        !f.root()
            .join("overwrite/Fallout - Invalidation.bsa")
            .exists()
    );
}

#[test]
fn local_saves_with_the_users_ini_in_the_prefix_edit_it_in_place_and_keep_the_first_original() {
    let f = Fixture::new();
    f.profile("A", "LocalSaves=true\r\n");
    fs::create_dir_all(f.games()).unwrap();
    fs::write(f.games().join("fallout.ini"), FALLOUT_INI).unwrap();
    apply(&f.instance(), "A", &f.target(), false).unwrap();
    assert!(text(&f.games().join("fallout.ini")).contains("SLocalSavePath=__MO_Saves\\\r\n"));
    f.put(
        "profiles/A/settings.ini",
        b"[General]\r\nLocalSaves=false\r\n",
    );
    apply(&f.instance(), "A", &f.target(), false).unwrap();
    assert_eq!(text(&f.games().join("fallout.ini")), FALLOUT_INI);
}

#[test]
fn local_saves_without_any_ini_to_patch_are_refused_before_a_write() {
    let f = Fixture::new();
    f.profile("A", "LocalSaves=true\r\n");
    let before = snapshot(f.scratch.path());
    let err = apply(&f.instance(), "A", &f.target(), false).unwrap_err();
    assert!(matches!(err, Error::Refused(_)), "{err:?}");
    assert_eq!(snapshot(f.scratch.path()), before);
}

#[test]
fn a_save_folder_with_saves_in_it_is_never_touched() {
    let f = Fixture::new();
    f.profile("A", "LocalSettings=true\r\nLocalSaves=true\r\n");
    f.put("profiles/A/fallout.ini", FALLOUT_INI.as_bytes());
    fs::create_dir_all(f.games().join("__MO_Saves")).unwrap();
    fs::write(f.games().join("__MO_Saves/Save1.fos"), b"keep").unwrap();
    let before = snapshot(f.scratch.path());
    let err = apply(&f.instance(), "A", &f.target(), false).unwrap_err();
    assert!(matches!(err, Error::Refused(_)), "{err:?}");
    assert_eq!(
        snapshot(f.scratch.path()),
        before,
        "nothing changed, nothing left behind"
    );
}

#[test]
fn plugin_lists_are_placed_in_the_prefix_appdata() {
    let f = Fixture::new();
    f.profile("A", "");
    f.put("profiles/A/plugins.txt", b"FalloutNV.esm\r\nA.esp\r\n");
    f.put(
        "profiles/A/loadorder.txt",
        b"FalloutNV.esm\r\nA.esp\r\nB.esp\r\n",
    );
    apply(&f.instance(), "A", &f.target(), false).unwrap();
    assert_eq!(
        fs::read(f.data().join("plugins.txt")).unwrap(),
        b"FalloutNV.esm\r\nA.esp\r\n"
    );
    assert_eq!(
        fs::read(f.data().join("loadorder.txt")).unwrap(),
        b"FalloutNV.esm\r\nA.esp\r\nB.esp\r\n"
    );
}

#[test]
fn applying_twice_changes_nothing_and_a_b_a_leaves_the_prefix_as_the_first_a() {
    covers(&[ids::run::prefix::APPLY_IDEMPOTENT]);
    let f = Fixture::new();
    f.profile(
        "A",
        "LocalSettings=true\r\nLocalSaves=true\r\nAutomaticArchiveInvalidation=true\r\n",
    );
    f.put("profiles/A/fallout.ini", FALLOUT_INI.as_bytes());
    f.put("profiles/A/plugins.txt", b"FalloutNV.esm\r\nA.esp\r\n");
    f.profile("B", "LocalSettings=false\r\n");
    f.put("profiles/B/plugins.txt", b"FalloutNV.esm\r\nB.esp\r\n");
    fs::create_dir_all(f.games()).unwrap();
    fs::write(f.games().join("fallout.ini"), b"[General]\r\nmine=1\r\n").unwrap();
    let a1 = apply(&f.instance(), "A", &f.target(), false).unwrap();
    assert_eq!(a1.status, "applied");
    let after_a = snapshot(&f.prefix());
    let a2 = apply(&f.instance(), "A", &f.target(), false).unwrap();
    assert_eq!(a2.status, "unchanged", "{:?}", a2.actions);
    assert_eq!(snapshot(&f.prefix()), after_a);
    apply(&f.instance(), "B", &f.target(), false).unwrap();
    assert_eq!(
        text(&f.games().join("fallout.ini")),
        "[General]\r\nmine=1\r\n"
    );
    assert_eq!(
        text(&f.data().join("plugins.txt")),
        "FalloutNV.esm\r\nB.esp\r\n"
    );
    assert!(fs::symlink_metadata(f.games().join("__MO_Saves")).is_err());
    let a3 = apply(&f.instance(), "A", &f.target(), false).unwrap();
    assert_eq!(a3.previous.as_deref(), Some("B"));
    assert_eq!(snapshot(&f.prefix()), after_a);
}

#[test]
fn what_the_game_writes_comes_back_with_finish_and_with_the_next_apply() {
    let f = Fixture::new();
    f.profile("A", "LocalSettings=true\r\n");
    f.put("profiles/A/fallout.ini", b"[General]\r\nx=1\r\n");
    f.put("profiles/A/plugins.txt", b"FalloutNV.esm\r\n");
    apply(&f.instance(), "A", &f.target(), false).unwrap();
    // A fake run: the game changes fallout.ini and a tool rewrites plugins.txt.
    fs::write(f.games().join("fallout.ini"), b"[General]\r\nx=2\r\n").unwrap();
    let r = finish(&f.instance(), &f.target(), false).unwrap();
    assert_eq!(r.status, "collected");
    assert_eq!(f.read("profiles/A/fallout.ini"), b"[General]\r\nx=2\r\n");
    assert_eq!(f.read("profiles/A/plugins.txt"), b"FalloutNV.esm\r\n");
    assert_eq!(
        finish(&f.instance(), &f.target(), false).unwrap().status,
        "unchanged"
    );
    // A change nobody collected is collected by the next apply, also of another profile.
    fs::write(
        f.data().join("plugins.txt"),
        b"FalloutNV.esm\r\nTool.esp\r\n",
    )
    .unwrap();
    f.profile("B", "");
    apply(&f.instance(), "B", &f.target(), false).unwrap();
    assert_eq!(
        f.read("profiles/A/plugins.txt"),
        b"FalloutNV.esm\r\nTool.esp\r\n"
    );
}

#[test]
fn a_read_only_ini_is_reported_before_any_write() {
    let f = Fixture::new();
    f.profile(
        "A",
        "LocalSettings=true\r\nAutomaticArchiveInvalidation=true\r\n",
    );
    let ini = f.root().join("profiles/A/fallout.ini");
    fs::write(&ini, FALLOUT_INI).unwrap();
    let mut perms = fs::metadata(&ini).unwrap().permissions();
    perms.set_readonly(true);
    fs::set_permissions(&ini, perms).unwrap();
    let before = snapshot(f.scratch.path());
    let err = apply(&f.instance(), "A", &f.target(), false).unwrap_err();
    match err {
        Error::Refused(why) => assert!(why.contains("read-only"), "{why}"),
        other @ Error::Failed(_) => panic!("{other:?}"),
    }
    assert_eq!(snapshot(f.scratch.path()), before);
}

#[test]
fn a_dry_run_reports_the_changes_and_writes_nothing() {
    let f = Fixture::new();
    f.profile("A", "LocalSettings=true\r\nLocalSaves=true\r\n");
    f.put("profiles/A/fallout.ini", FALLOUT_INI.as_bytes());
    let before = snapshot(f.scratch.path());
    let r = apply(&f.instance(), "A", &f.target(), true).unwrap();
    assert_eq!(r.status, "dry-run");
    assert!(r.actions.iter().any(|a| a.action == "link"));
    assert!(r.actions.iter().any(|a| a.path.ends_with("savepath.ini")));
    assert_eq!(snapshot(f.scratch.path()), before);
}

#[test]
fn nothing_is_written_outside_the_instance_and_the_named_prefix() {
    covers(&[ids::run::prefix::NOTHING_OUTSIDE]);
    let f = Fixture::new();
    let sentinel = f.scratch.path().join("sentinel");
    write_file(&sentinel, "keep.txt", b"keep");
    write_file(
        &sentinel,
        "Documents/My Games/FalloutNV/fallout.ini",
        b"theirs",
    );
    let guarded = snapshot(&sentinel);
    f.profile(
        "A",
        "LocalSettings=true\r\nLocalSaves=true\r\nAutomaticArchiveInvalidation=true\r\n",
    );
    f.put("profiles/A/fallout.ini", FALLOUT_INI.as_bytes());
    f.put("profiles/A/plugins.txt", b"FalloutNV.esm\r\n");
    apply(&f.instance(), "A", &f.target(), false).unwrap();
    finish(&f.instance(), &f.target(), false).unwrap();
    assert_eq!(snapshot(&sentinel), guarded);
}

#[test]
fn a_run_that_was_cut_off_is_put_back_by_the_next_one() {
    covers(&[ids::run::prefix::CUT_OFF_RUN_RECOVERED]);
    let f = Fixture::new();
    f.profile("A", "LocalSettings=true\r\n");
    f.put("profiles/A/fallout.ini", FALLOUT_INI.as_bytes());
    let before = snapshot(f.scratch.path());
    // Fake the journal of a crashed run: one changed file and one made file, as the transaction writes them.
    let jdir = f.root().join(".nvmod/apply-journal/0001");
    fs::create_dir_all(&jdir).unwrap();
    fs::write(jdir.join("0000.bak"), FALLOUT_INI).unwrap();
    let ini = f.root().join("profiles/A/fallout.ini");
    let made = f.root().join("profiles/A/stray.ini");
    fs::write(&ini, b"half written").unwrap();
    fs::write(&made, b"x").unwrap();
    let journal = serde_json::json!({"schema": 1, "entries": [
        {"path": ini.display().to_string(), "before": {"kind": "file", "value": "0000.bak"}},
        {"path": made.display().to_string(), "before": {"kind": "absent"}},
    ]});
    fs::write(jdir.join("journal.json"), journal.to_string()).unwrap();
    assert_eq!(nvmod_profile::txn::recover(&f.root()).unwrap(), 1);
    let after = snapshot(f.scratch.path());
    assert_eq!(
        after.get("inst/profiles/A/fallout.ini"),
        before.get("inst/profiles/A/fallout.ini")
    );
    assert!(!after.contains_key("inst/profiles/A/stray.ini"));
    assert!(!after.keys().any(|k| k.contains("apply-journal")));
}
