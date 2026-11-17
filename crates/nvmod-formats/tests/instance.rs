// SPDX-License-Identifier: GPL-3.0-only
//! The instance reader on instances built in code.

use nvmod_formats::winpath::PathStyle;
use nvmod_formats::{EntryKind, Mo2Error, Mo2Instance, ModState};
use nvmod_testkit::{Mo2Builder, ModSpec, TempDir, write_file};

fn sample() -> (TempDir, TempDir) {
    let game = TempDir::new("game");
    let root = TempDir::new("mo2");
    Mo2Builder::new(game.path())
        .mod_dir(
            ModSpec::new("Alpha")
                .file("NVSE/Plugins/a.dll", b"x")
                .nexus(111, "1.0"),
        )
        .mod_dir(ModSpec::new("Beta").file("nvse/plugins/b.dll", b"y"))
        .mod_dir(ModSpec::new("Unlisted"))
        .executable("Game", &game.path().join("FalloutNV.exe"))
        .profile(
            "Default",
            &[
                "+Alpha",
                "-Beta",
                "-Group_separator",
                "+Ghost",
                "*DLC: Pack",
            ],
            &["FalloutNV.esm", "Alpha.esp"],
        )
        .profile("Second", &["+Beta"], &[])
        .selected("Second")
        .write(root.path());
    (game, root)
}

#[test]
fn the_instance_is_read_with_its_paths_mapped_to_this_machine() {
    let (game, root) = sample();
    let instance = Mo2Instance::open(root.path()).unwrap();
    assert_eq!(instance.game_name, "TTW");
    assert_eq!(instance.game_path.as_deref(), Some(game.path()));
    assert_eq!(instance.selected_profile.as_deref(), Some("Second"));
    assert_eq!(instance.mods_dir, root.path().join("mods"));
    assert_eq!(instance.overwrite_dir, root.path().join("overwrite"));
    assert_eq!(instance.executables.len(), 1);
    assert_eq!(instance.executables[0].title, "Game");
    assert_eq!(
        instance.executables[0].binary_host.as_deref(),
        Some(game.path().join("FalloutNV.exe").as_path())
    );
    assert!(instance.issues.is_empty(), "{:?}", instance.issues);
    assert_eq!(instance.profile_names().unwrap(), ["Default", "Second"]);
}

#[test]
fn a_profile_is_read_with_its_lists() {
    let (_game, root) = sample();
    let instance = Mo2Instance::open(root.path()).unwrap();
    let profile = instance.load_profile("Default").unwrap();
    assert_eq!(profile.modlist.entries.len(), 5);
    assert_eq!(profile.modlist.entries[2].kind, EntryKind::Separator);
    assert_eq!(profile.modlist.entries[4].state, ModState::Foreign);
    assert_eq!(
        profile.plugins.as_deref(),
        Some(&["FalloutNV.esm".to_owned(), "Alpha.esp".to_owned()][..])
    );
    assert!(profile.load_order.is_some());
    assert!(!profile.fallout_ini_read_only);
    let meta = instance.mod_meta("Alpha").unwrap();
    assert_eq!(
        (meta.modid, meta.version.as_deref()),
        (Some(111), Some("1.0"))
    );
    assert_eq!(meta.installation_file.as_deref(), Some("Alpha-111.7z"));
    assert!(instance.mod_meta("Beta").is_none());
}

#[test]
fn reconcile_reports_missing_and_unlisted_mods() {
    let (_game, root) = sample();
    let instance = Mo2Instance::open(root.path()).unwrap();
    let profile = instance.load_profile("Default").unwrap();
    let (missing, unlisted) = instance.reconcile(&profile);
    assert_eq!(missing, ["Ghost"]);
    assert_eq!(unlisted, ["Unlisted"]);
}

#[test]
fn a_read_only_fallout_ini_is_noticed() {
    let (_game, root) = sample();
    let path = root.path().join("profiles/Default/fallout.ini");
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&path, permissions).unwrap();
    let instance = Mo2Instance::open(root.path()).unwrap();
    assert!(
        instance
            .load_profile("Default")
            .unwrap()
            .fallout_ini_read_only
    );
}

#[test]
fn bad_input_is_refused_with_a_reason() {
    let (_game, root) = sample();
    let instance = Mo2Instance::open(root.path()).unwrap();
    for name in ["", "..", ".", "a/b", "a\\b"] {
        assert!(
            matches!(
                instance.load_profile(name),
                Err(Mo2Error::BadProfileName(_))
            ),
            "{name:?}"
        );
    }
    assert!(matches!(
        instance.load_profile("Nope"),
        Err(Mo2Error::NoSuchProfile(_))
    ));
    let empty = TempDir::new("empty");
    assert!(matches!(
        Mo2Instance::open(empty.path()),
        Err(Mo2Error::NotAnInstance(_))
    ));
    std::fs::remove_file(root.path().join("profiles/Second/modlist.txt")).unwrap();
    assert!(matches!(
        instance.load_profile("Second"),
        Err(Mo2Error::Io(_))
    ));
    assert!(matches!(
        Mo2Instance::open(&root.path().join("ModOrganizer.ini")),
        Err(Mo2Error::NotAnInstance(_))
    ));
    let missing_profiles = TempDir::new("noprofiles");
    write_file(
        missing_profiles.path(),
        "ModOrganizer.ini",
        b"[General]\r\n",
    );
    let bare = Mo2Instance::open(missing_profiles.path()).unwrap();
    assert!(matches!(bare.profile_names(), Err(Mo2Error::Io(_))));
    assert_eq!(bare.game_path, None);
    assert_eq!(bare.executables.len(), 0);
}

#[test]
fn directory_overrides_and_unmapped_drives_are_honoured_or_reported() {
    let root = TempDir::new("overrides");
    write_file(
        root.path(),
        "ModOrganizer.ini",
        b"[General]\r\ngamePath=@ByteArray(C:\\\\Games\\\\FNV)\r\n[Settings]\r\nbase_directory=@ByteArray(Z:/data/mo2)\r\nmod_directory=%BASE_DIR%/elsewhere/mods\r\ndownload_directory=dl\r\nprofiles_directory=\r\n[customExecutables]\r\nsize=2\r\n1\\binary=C:/x/tool.exe\r\n1\\title=Tool\r\n1\\arguments=-a \"b\"\r\n",
    );
    let instance = Mo2Instance::open(root.path()).unwrap();
    if PathStyle::host() == PathStyle::Windows {
        // MO2 on Windows wrote these: every path is a host path, `Z:` and `C:` are drives like any other, and
        // nothing is unmapped.
        let path = |p: &str| Some(std::path::PathBuf::from(p));
        assert_eq!(Some(instance.mods_dir), path(r"Z:\data\mo2\elsewhere\mods"));
        assert_eq!(Some(instance.downloads_dir), path(r"Z:\data\mo2\dl"));
        assert_eq!(Some(instance.profiles_dir), path(r"Z:\data\mo2\profiles"));
        assert_eq!(instance.game_path, path(r"C:\Games\FNV"));
        assert_eq!(instance.executables.len(), 1);
        assert_eq!(instance.executables[0].binary_host, path(r"C:\x\tool.exe"));
        assert!(instance.issues.is_empty(), "{:?}", instance.issues);
        return;
    }
    // MO2 under Wine wrote these: `Z:` is the root, and `C:` has no mapping here.
    assert_eq!(
        instance.mods_dir,
        std::path::PathBuf::from("/data/mo2/elsewhere/mods")
    );
    assert_eq!(
        instance.downloads_dir,
        std::path::PathBuf::from("/data/mo2/dl")
    );
    assert_eq!(
        instance.profiles_dir,
        std::path::PathBuf::from("/data/mo2/profiles")
    );
    assert_eq!(instance.game_path, None);
    assert_eq!(instance.game_path_raw.as_deref(), Some("C:\\Games\\FNV"));
    assert_eq!(
        instance.executables.len(),
        1,
        "an entry without a binary (index 2) is skipped"
    );
    assert_eq!(instance.executables[0].binary_host, None);
    assert!(
        instance.issues.iter().any(|i| i.starts_with("gamePath:")),
        "{:?}",
        instance.issues
    );
}
