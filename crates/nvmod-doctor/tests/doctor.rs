// SPDX-License-Identifier: GPL-3.0-only
//! The doctor on instances and game folders built in code.

use nvmod_catalog::{Catalog, Kind};
use nvmod_core::Severity;
use nvmod_doctor::{Report, run};
use nvmod_formats::Mo2Instance;
use nvmod_formats::winpath::PathStyle;
use nvmod_testkit::{Mo2Builder, ModSpec, PeBuilder, TempDir, write_file};
use std::path::Path;

fn plugin() -> Vec<u8> {
    PeBuilder::nvse_plugin().nvse_version(1, 7).build()
}

#[allow(clippy::fn_params_excessive_bools)]
fn game_dir(
    patched: bool,
    backup: bool,
    steam_stub: bool,
    loader: bool,
    xnvse: bool,
    version: [u16; 4],
) -> TempDir {
    let game = TempDir::new("doctor-game");
    let mut exe = PeBuilder::exe()
        .versions(version, version)
        .large_address_aware(patched);
    if steam_stub {
        exe = exe.import("nvse_steam_loader.dll");
    }
    write_file(game.path(), "FalloutNV.exe", &exe.build());
    if backup {
        write_file(game.path(), "FalloutNV_backup.exe", b"backup");
    }
    if loader {
        write_file(game.path(), "nvse_loader.exe", b"loader");
    }
    if xnvse {
        write_file(
            game.path(),
            "nvse_1_4.dll",
            &PeBuilder::dll()
                .versions([6, 4, 0, 0], [6, 4, 0, 0])
                .build(),
        );
    }
    game
}

fn healthy_game() -> TempDir {
    game_dir(true, true, true, false, true, [1, 4, 0, 525])
}

fn codes(report: &Report) -> Vec<&'static str> {
    report.findings.iter().map(|f| f.code).collect()
}

fn find<'a>(report: &'a Report, code: &str) -> &'a nvmod_core::Finding {
    report
        .findings
        .iter()
        .find(|f| f.code == code)
        .unwrap_or_else(|| panic!("no {code} in {:?}", codes(report)))
}

fn diagnose(builder: &Mo2Builder, game: Option<&Path>, catalog: &Catalog) -> Report {
    let root = TempDir::new("doctor-mo2");
    builder.write(root.path());
    let instance = Mo2Instance::open(root.path()).unwrap();
    let profile = instance.load_profile("Default").unwrap();
    run(&instance, &profile, game, catalog)
}

fn builder(game: &TempDir) -> Mo2Builder {
    Mo2Builder::new(game.path())
}

#[test]
fn a_healthy_setup_has_no_warnings_or_errors() {
    let game = healthy_game();
    let b = builder(&game)
        .mod_dir(
            ModSpec::new("JIP LN NVSE Plugin")
                .file("NVSE/Plugins/jip_nvse.dll", &plugin())
                .file("NVSE/Plugins/jip_nvse.ini", b"[x]"),
        )
        .profile("Default", &["+JIP LN NVSE Plugin"], &["FalloutNV.esm"]);
    let report = diagnose(&b, None, &Catalog::builtin());
    assert!(
        report.worst().is_none_or(|s| s == Severity::Info),
        "{:?}",
        report.findings
    );
    assert_eq!(report.plugins.len(), 1);
    let p = &report.plugins[0];
    assert_eq!(
        (p.dll.as_str(), p.winner.as_str(), p.catalog_id.as_deref()),
        ("jip_nvse.dll", "JIP LN NVSE Plugin", Some("jip-ln"))
    );
    assert_eq!(p.nv_rs, Kind::BuiltinPartial);
    assert_eq!(
        p.pe.as_ref().unwrap().nvse.version_struct.unwrap().version,
        7
    );
    assert_eq!(
        (
            report.summary.plugins,
            report.summary.catalogued,
            report.summary.errors,
            report.summary.warnings
        ),
        (1, 1, 0, 0)
    );
    let exe = report.game.exe.as_ref().unwrap();
    assert!(exe.large_address_aware && exe.backup_present && exe.imports_nvse_steam_loader);
    assert_eq!(report.game.xnvse_version.unwrap().to_string(), "6.4.0.0");
    let text = report.to_text();
    assert!(
        text.contains("jip_nvse.dll <- JIP LN NVSE Plugin") && text.contains("xNVSE 6.4.0.0"),
        "{text}"
    );
}

#[test]
fn bad_plugin_files_are_each_named() {
    let game = healthy_game();
    let b = builder(&game)
        .mod_dir(ModSpec::new("X64").file(
            "NVSE/Plugins/x64.dll",
            &PeBuilder::nvse_plugin().machine(0x8664).pe32_plus().build(),
        ))
        .mod_dir(
            ModSpec::new("Helper").file(
                "nvse/plugins/helper.dll",
                &PeBuilder::dll()
                    .import("MSVCP140.dll")
                    .import("KERNEL32.dll")
                    .build(),
            ),
        )
        .mod_dir(ModSpec::new("Junk").file("NVSE/Plugins/junk.dll", b"this is not a PE file"))
        .mod_dir(ModSpec::new("Exe").file(
            "NVSE/Plugins/exe.dll",
            &PeBuilder::exe().export_code("NVSEPlugin_Load").build(),
        ))
        .mod_dir(
            ModSpec::new("Old").file("NVSE/Plugins/old.dll", &PeBuilder::nvse_plugin().build()),
        )
        .mod_dir(ModSpec::new("Damaged").file("NVSE/Plugins/damaged.dll", &{
            let mut file = PeBuilder::nvse_plugin().build();
            let rva = u32::from_le_bytes(file[0xF8..0xFC].try_into().unwrap()) as usize;
            let export = 0x400 + rva - 0x1000;
            file[export + 24..export + 28].copy_from_slice(&u32::MAX.to_le_bytes());
            file
        }))
        .profile(
            "Default",
            &["+X64", "+Helper", "+Junk", "+Exe", "+Old", "+Damaged"],
            &[],
        );
    let report = diagnose(&b, None, &Catalog::builtin());
    let on = |code: &str, subject: &str| {
        report
            .findings
            .iter()
            .any(|f| f.code == code && f.subject == subject)
    };
    assert!(on("pe.not-32-bit", "x64.dll"));
    assert!(on("pe.not-nvse-plugin", "helper.dll") && on("pe.needs-vc-runtime", "helper.dll"));
    assert!(on("pe.unreadable", "junk.dll"));
    assert!(on("pe.not-a-dll", "exe.dll"));
    assert!(on("pe.damaged", "damaged.dll"), "{:?}", codes(&report));
    assert_eq!(find(&report, "pe.unreadable").severity, Severity::Error);
    assert_eq!(report.worst(), Some(Severity::Error));
    assert_eq!(report.summary.catalogued, 0);
    assert!(
        report
            .findings
            .iter()
            .filter(|f| f.code == "catalog.unknown-plugin")
            .count()
            >= 6
    );
    assert!(
        report
            .plugins
            .iter()
            .find(|p| p.dll == "junk.dll")
            .unwrap()
            .pe
            .is_none()
    );
    assert!(report.to_text().contains("unreadable"));
}

#[test]
fn a_dll_in_two_mods_is_reported_as_shadowed_and_the_first_mod_wins() {
    let game = healthy_game();
    let b = builder(&game)
        .mod_dir(ModSpec::new("New").file("NVSE/Plugins/johnnyguitar.dll", &plugin()))
        .mod_dir(
            ModSpec::new("Old")
                .file("nvse/plugins/JohnnyGuitar.dll", &plugin())
                .file("NVSE/Plugins/johnnyguitar.dll", &plugin()),
        )
        .mod_dir(ModSpec::new("Off").file("NVSE/Plugins/johnnyguitar.dll", &plugin()))
        .profile("Default", &["+New", "+Old", "-Off"], &[]);
    let report = diagnose(&b, None, &Catalog::builtin());
    assert_eq!(report.plugins.len(), 1);
    assert_eq!(
        (
            report.plugins[0].winner.as_str(),
            report.plugins[0].shadowed.clone()
        ),
        ("New", vec!["Old".to_owned()])
    );
    assert!(find(&report, "plugin.shadowed").message.contains("Old"));
}

#[test]
fn overwrite_beats_every_mod() {
    let game = healthy_game();
    let root = TempDir::new("doctor-overwrite");
    let b = builder(&game)
        .mod_dir(ModSpec::new("Mod").file("NVSE/Plugins/a.dll", &plugin()))
        .profile("Default", &["+Mod"], &[]);
    b.write(root.path());
    write_file(
        &root.path().join("overwrite"),
        "NVSE/Plugins/a.dll",
        &plugin(),
    );
    let instance = Mo2Instance::open(root.path()).unwrap();
    let profile = instance.load_profile("Default").unwrap();
    let report = run(&instance, &profile, None, &Catalog::builtin());
    assert_eq!(report.plugins[0].winner, nvmod_doctor::OVERWRITE);
    assert_eq!(report.plugins[0].shadowed, ["Mod"]);
}

#[test]
fn catalogue_rules_requirements_conflicts_replacements_and_inis() {
    let text = |extra: &str| {
        let prov =
            "[plugin.provenance]\nsource = \"t\"\nverified_on = \"2026-11-14\"\nverified = false\n";
        format!(
            "schema = 1\n[[plugin]]\nid = \"a\"\nname = \"A\"\ndll = \"a.dll\"\nini = [\"a.ini\"]\nrequires = [\"fw\"]\nconflicts = [\"b\"]\nreplaced_by = \"c\"\nprovided_by = {{ nv-rs = {{ kind = \"absent\" }} }}\n{prov}\n[[plugin]]\nid = \"b\"\nname = \"B\"\ndll = \"b.dll\"\nconflicts = [\"a\"]\n{prov}\n[[plugin]]\nid = \"fw\"\nname = \"FW\"\ndll = \"fw.dll\"\n{prov}\n[[plugin]]\nid = \"c\"\nname = \"C\"\ndll = \"c.dll\"\n{prov}{extra}"
        )
    };
    let catalog = Catalog::parse(&text("")).unwrap();
    let game = healthy_game();
    let b = builder(&game)
        .mod_dir(ModSpec::new("A").file("NVSE/Plugins/a.dll", &plugin()))
        .mod_dir(ModSpec::new("B").file("NVSE/Plugins/b.dll", &plugin()))
        .mod_dir(ModSpec::new("C").file("NVSE/Plugins/c.dll", &plugin()))
        .profile("Default", &["+A", "+B", "+C"], &[]);
    let report = diagnose(&b, None, &catalog);
    assert_eq!(
        find(&report, "plugin.missing-requirement").severity,
        Severity::Error
    );
    assert!(
        find(&report, "plugin.missing-requirement")
            .message
            .contains("fw.dll")
    );
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|f| f.code == "plugin.conflict")
            .count(),
        1,
        "reported once, not per side"
    );
    assert_eq!(find(&report, "plugin.ini-missing").subject, "a.dll");
    assert!(find(&report, "plugin.replaced").message.contains("c.dll"));
    assert_eq!(
        report
            .plugins
            .iter()
            .find(|p| p.dll == "a.dll")
            .unwrap()
            .nv_rs,
        Kind::Absent
    );
}

#[test]
fn the_known_mac_ten_id_mismatch_is_found() {
    let game = healthy_game();
    let b = builder(&game)
        .mod_dir(ModSpec::new("MAC-TEN").nexus(10, "14.5"))
        .mod_dir(ModSpec::new("Goodies").nexus(91028, "2.4"))
        .mod_dir(ModSpec::new("LODIFY"))
        .mod_dir(ModSpec::new("MCM Extender").nexus(93642, "1.6"))
        .profile(
            "Default",
            &["+MAC-TEN", "+Goodies", "+LODIFY", "+MCM Extender"],
            &[],
        );
    let root_dir = TempDir::new("doctor-mac");
    b.write(root_dir.path());
    write_file(
        &root_dir.path().join("mods/LODIFY"),
        "meta.ini",
        b"[General]\r\nmodid=0\r\n",
    );
    let instance = Mo2Instance::open(root_dir.path()).unwrap();
    let profile = instance.load_profile("Default").unwrap();
    let report = run(&instance, &profile, None, &Catalog::builtin());
    let mismatch = find(&report, "meta.modid-mismatch");
    assert_eq!(mismatch.subject, "MAC-TEN");
    assert!(mismatch.message.contains("10") && mismatch.message.contains("83815"));
    assert_eq!(find(&report, "meta.modid-missing").subject, "LODIFY");
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|f| f.code.starts_with("meta."))
            .count(),
        2
    );
}

#[test]
fn profile_problems_are_reported() {
    let game = healthy_game();
    let root = TempDir::new("doctor-profile");
    builder(&game)
        .mod_dir(ModSpec::new("Present"))
        .mod_dir(ModSpec::new("Loose"))
        .profile(
            "Default",
            &["+Present", "+Ghost", "-Group_separator", "*DLC: Pack"],
            &[],
        )
        .write(root.path());
    std::fs::remove_file(root.path().join("profiles/Default/plugins.txt")).unwrap();
    std::fs::remove_file(root.path().join("profiles/Default/loadorder.txt")).unwrap();
    let ini = root.path().join("profiles/Default/fallout.ini");
    let mut permissions = std::fs::metadata(&ini).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&ini, permissions).unwrap();
    let instance = Mo2Instance::open(root.path()).unwrap();
    let profile = instance.load_profile("Default").unwrap();
    let report = run(&instance, &profile, None, &Catalog::builtin());
    assert_eq!(find(&report, "profile.mod-folder-missing").subject, "Ghost");
    assert_eq!(find(&report, "profile.mod-unlisted").subject, "Loose");
    assert!(
        find(&report, "profile.fallout-ini-read-only")
            .message
            .contains("hidden dialog")
    );
    assert_eq!(
        report
            .findings
            .iter()
            .filter(|f| f.code == "profile.list-missing")
            .count(),
        2
    );
}

#[test]
fn game_folder_checks() {
    let b = |g: &TempDir| builder(g).profile("Default", &[], &[]);
    // Unpatched, no backup concern, wrong version, no xNVSE, no loader, no stub.
    let game = game_dir(false, false, false, false, false, [1, 3, 0, 452]);
    let report = diagnose(&b(&game), None, &Catalog::builtin());
    for code in [
        "game.not-4gb",
        "game.exe-version",
        "nvse.dll-missing",
        "launch.no-loader",
    ] {
        assert_eq!(
            find(&report, code).severity,
            if code == "nvse.dll-missing" {
                Severity::Error
            } else {
                Severity::Warning
            },
            "{code}"
        );
    }
    assert!(
        find(&report, "game.exe-version")
            .message
            .contains("1.3.0.452")
    );
    // Patched without a backup.
    let game = game_dir(true, false, true, false, true, [1, 4, 0, 525]);
    let report = diagnose(&b(&game), None, &Catalog::builtin());
    assert_eq!(
        codes(&report)
            .iter()
            .filter(|c| c.starts_with("game.")
                || c.starts_with("launch.")
                || c.starts_with("nvse."))
            .collect::<Vec<_>>(),
        [&"game.no-backup"]
    );
    // The loader is launched by an MO2 executable but missing.
    let game = game_dir(true, true, false, false, true, [1, 4, 0, 525]);
    let launcher = game.path().join("nvse_loader.exe");
    let report = diagnose(
        &builder(&game)
            .executable("NVSE", &launcher)
            .profile("Default", &[], &[]),
        None,
        &Catalog::builtin(),
    );
    assert_eq!(
        find(&report, "nvse.loader-missing").severity,
        Severity::Error
    );
    assert!(!codes(&report).contains(&"launch.no-loader"));
    // The loader exists and is launched: clean.
    write_file(game.path(), "nvse_loader.exe", b"x");
    let report = diagnose(
        &builder(&game)
            .executable("NVSE", &launcher)
            .profile("Default", &[], &[]),
        None,
        &Catalog::builtin(),
    );
    assert!(
        !codes(&report)
            .iter()
            .any(|c| c.starts_with("launch.") || *c == "nvse.loader-missing")
    );
    assert!(report.game.nvse_loader_present);
}

#[test]
fn missing_or_broken_game_files_are_reported() {
    let b = |g: &TempDir| builder(g).profile("Default", &[], &[]);
    let empty = TempDir::new("doctor-empty-game");
    let report = diagnose(&b(&empty), None, &Catalog::builtin());
    assert_eq!(find(&report, "game.exe-missing").severity, Severity::Error);
    assert!(codes(&report).contains(&"launch.unchecked"));
    // An executable that is not a PE file, a 64-bit one, one without a version, and an unreadable xNVSE DLL.
    let game = TempDir::new("doctor-broken-game");
    write_file(game.path(), "FALLOUTNV.EXE", b"junk");
    write_file(game.path(), "nvse_1_4.dll", b"junk");
    let report = diagnose(&b(&game), None, &Catalog::builtin());
    assert!(
        codes(&report).contains(&"game.exe-unreadable")
            && codes(&report).contains(&"nvse.dll-unreadable")
    );
    write_file(
        game.path(),
        "FALLOUTNV.EXE",
        &PeBuilder::exe().machine(0x8664).pe32_plus().build(),
    );
    let report = diagnose(&b(&game), None, &Catalog::builtin());
    assert!(
        codes(&report).contains(&"game.exe-not-32-bit")
            && codes(&report).contains(&"game.exe-version-unknown")
    );
    // The explicit game directory overrides the instance's.
    let healthy = healthy_game();
    let report = diagnose(&b(&game), Some(healthy.path()), &Catalog::builtin());
    assert_eq!(report.game.path.as_deref(), healthy.path().to_str());
}

#[test]
fn an_unusable_game_path_is_a_warning_not_a_failure() {
    let root = TempDir::new("doctor-nogame");
    write_file(
        root.path(),
        "ModOrganizer.ini",
        b"[General]\r\ngamePath=@ByteArray(C:\\\\nvmod-no-such-game\\\\FNV)\r\n",
    );
    write_file(root.path(), "profiles/Default/modlist.txt", b"");
    let instance = Mo2Instance::open(root.path()).unwrap();
    let profile = instance.load_profile("Default").unwrap();
    let report = run(&instance, &profile, None, &Catalog::builtin());
    assert!(
        codes(&report).contains(&"game.path-unusable"),
        "{:?}",
        codes(&report)
    );
    // Under Wine `C:` has no mapping, which is reported too; on Windows it is a drive and the folder is only
    // missing.
    assert_eq!(
        codes(&report).contains(&"instance.path-unresolved"),
        PathStyle::host() == PathStyle::Wine,
        "{:?}",
        codes(&report)
    );
    let report = run(
        &instance,
        &profile,
        Some(&root.path().join("nope")),
        &Catalog::builtin(),
    );
    assert!(
        find(&report, "game.path-unusable")
            .message
            .contains("does not exist")
    );
    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["schema_version"], 1);
    assert!(json["findings"].is_array() && json["summary"]["errors"].is_number());
}

#[test]
fn a_dll_from_the_players_overlay_is_recognised_and_its_source_shown() {
    let game = healthy_game();
    let b = builder(&game)
        .mod_dir(ModSpec::new("Mine").file("NVSE/Plugins/my_own.dll", &plugin()))
        .profile("Default", &["+Mine"], &["FalloutNV.esm"]);
    let overlay = "schema = 1\n[[plugin]]\nid = \"my-own\"\nname = \"Mine\"\ndll = \"my_own.dll\"\n[plugin.provenance]\nsource = \"my notes\"\nverified_on = \"2026-11-21\"\nverified = true\n".to_owned();
    let catalog = Catalog::builtin().with_overlay(&[("mine.toml".to_owned(), overlay)]);
    let report = diagnose(&b, None, &catalog);
    assert_eq!(report.plugins[0].catalog_id.as_deref(), Some("my-own"));
    assert_eq!(report.plugins[0].catalog_source, Some("user"));
    assert!(!codes(&report).contains(&"catalog.unknown-plugin"));
    let f = find(&report, "catalog.user-entry");
    assert_eq!(
        (f.subject.as_str(), f.severity),
        ("my_own.dll", Severity::Info)
    );
    let json = serde_json::to_string(&report).unwrap();
    assert!(json.contains("\"catalog_source\":\"user\""));
}

#[test]
fn an_ignored_overlay_file_is_a_warning_in_the_report() {
    let game = healthy_game();
    let b = builder(&game).profile("Default", &[], &["FalloutNV.esm"]);
    let catalog =
        Catalog::builtin().with_overlay(&[("broken.toml".to_owned(), "schema = 9".to_owned())]);
    let report = diagnose(&b, None, &catalog);
    let f = find(&report, "catalog.overlay-invalid");
    assert_eq!(
        (f.subject.as_str(), f.severity),
        ("broken.toml", Severity::Warning)
    );
}
