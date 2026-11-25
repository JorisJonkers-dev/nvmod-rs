// SPDX-License-Identifier: GPL-3.0-only
//! The binary, run as a program.

use nvmod_testkit::{covers, ids};
use std::process::Command;

fn nvmod(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_nvmod"))
        .args(args)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

#[test]
fn version_prints_the_package_version() {
    covers(&[ids::api::cli::VERSION]);
    let (code, out, _) = nvmod(&["--version"]);
    assert_eq!(code, 0);
    assert_eq!(out.trim(), concat!("nvmod ", env!("CARGO_PKG_VERSION")));
}

#[test]
fn no_arguments_print_the_help() {
    covers(&[ids::api::cli::HELP]);
    let (code, out, _) = nvmod(&[]);
    assert_eq!(code, 0);
    assert!(out.contains("nvmod pe <file>"));
}

#[test]
fn an_unknown_command_is_a_usage_error() {
    covers(&[ids::api::cli::UNKNOWN_COMMAND]);
    let (code, _, err) = nvmod(&["frobnicate"]);
    assert_eq!(code, 5);
    assert!(err.contains("unknown command `frobnicate`"));
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("nvmod-cli-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn pe_describes_a_plugin_as_text_and_as_json() {
    covers(&[ids::doctor::nvse::PE_INSPECTION]);
    let dir = temp_dir("pe");
    let path = dir.join("plugin.dll");
    std::fs::write(
        &path,
        nvmod_testkit::PeBuilder::nvse_plugin()
            .nvse_version(1, 3)
            .versions([1, 2, 3, 4], [1, 2, 3, 4])
            .build(),
    )
    .unwrap();
    let file = path.to_str().unwrap();
    let (code, out, _) = nvmod(&["pe", file]);
    assert_eq!(code, 0);
    assert!(
        out.contains("NVSE plugin: query yes, load yes, version struct info 1 version 3"),
        "{out}"
    );
    assert!(out.contains("file version: 1.2.3.4") && out.contains("imports: KERNEL32.dll"));
    let (code, out, _) = nvmod(&["pe", file, "--json"]);
    assert_eq!(code, 0);
    let json: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(json["report"]["nvse"]["version_struct"]["version"], 3);
    assert_eq!(json["report"]["file_version"], "1.2.3.4");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn pe_says_when_a_file_is_not_an_nvse_plugin() {
    covers(&[ids::doctor::nvse::PE_NOT_A_PLUGIN]);
    let dir = temp_dir("pe-text");
    let plain = dir.join("plain.dll");
    std::fs::write(&plain, nvmod_testkit::PeBuilder::dll().build()).unwrap();
    let (_, out, _) = nvmod(&["pe", plain.to_str().unwrap()]);
    assert!(out.contains("NVSE plugin: no"), "{out}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn pe_on_a_file_that_is_not_a_pe_file_exits_with_one() {
    let dir = temp_dir("pe-bad");
    let path = dir.join("bad.dll");
    std::fs::write(
        &path,
        b"not a pe file at all, just text that is long enough to have a DOS header..........",
    )
    .unwrap();
    let file = path.to_str().unwrap();
    let (code, _, err) = nvmod(&["pe", file]);
    assert_eq!(code, 1);
    assert!(err.contains("not a PE file"), "{err}");
    let (code, out, _) = nvmod(&["pe", file, "--json"]);
    assert_eq!(code, 1);
    assert!(
        serde_json::from_str::<serde_json::Value>(&out).unwrap()["error"]
            .as_str()
            .unwrap()
            .contains("not a PE file")
    );
    let (code, _, err) = nvmod(&["pe", dir.join("missing.dll").to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(err.contains("cannot read the file"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn pe_usage_errors_exit_with_five() {
    assert_eq!(nvmod(&["pe"]).0, 5);
    assert_eq!(nvmod(&["pe", "a", "b"]).0, 5);
    let (code, _, err) = nvmod(&["pe", "--bogus"]);
    assert_eq!(code, 5);
    assert!(err.contains("unknown option"));
}

mod doctor {
    use super::nvmod;
    use nvmod_testkit::{Mo2Builder, ModSpec, PeBuilder, TempDir, write_file};
    use nvmod_testkit::{covers, ids};

    fn setup() -> (TempDir, TempDir) {
        let game = TempDir::new("cli-game");
        write_file(
            game.path(),
            "FalloutNV.exe",
            &PeBuilder::exe()
                .versions([1, 4, 0, 525], [1, 4, 0, 525])
                .large_address_aware(true)
                .import("nvse_steam_loader.dll")
                .build(),
        );
        write_file(game.path(), "FalloutNV_backup.exe", b"b");
        write_file(
            game.path(),
            "nvse_1_4.dll",
            &PeBuilder::dll()
                .versions([6, 4, 0, 0], [6, 4, 0, 0])
                .build(),
        );
        let root = TempDir::new("cli-mo2");
        Mo2Builder::new(game.path())
            .mod_dir(ModSpec::new("JIP LN NVSE Plugin").file(
                "NVSE/Plugins/jip_nvse.dll",
                &PeBuilder::nvse_plugin().nvse_version(1, 3).build(),
            ))
            .mod_dir(ModSpec::new("Helper").file(
                "NVSE/Plugins/helper.dll",
                &PeBuilder::dll().import("MSVCP140.dll").build(),
            ))
            .mod_dir(ModSpec::new("MAC-TEN").nexus(10, "14.5"))
            .profile(
                "Default",
                &["+JIP LN NVSE Plugin", "+Helper", "+MAC-TEN"],
                &["FalloutNV.esm"],
            )
            .profile("Other", &[], &[])
            .selected("Default")
            .write(root.path());
        (game, root)
    }

    #[test]
    fn the_json_report_matches_the_golden_file() {
        covers(&[ids::doctor::checks::JSON_REPORT]);
        let (game, root) = setup();
        let (code, out, _) = nvmod(&[
            "doctor",
            "--instance",
            root.path().to_str().unwrap(),
            "--json",
        ]);
        assert_eq!(code, 0, "only warnings: below the default --fail-on error");
        // A path as it stands in JSON: on Windows its backslashes are escaped.
        let json = |p: &std::path::Path| {
            serde_json::to_string(p.to_str().unwrap())
                .unwrap()
                .trim_matches('"')
                .to_owned()
        };
        let normalised = out
            .replace(&json(root.path()), "<instance>")
            .replace(&json(game.path()), "<game>");
        let golden =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden/doctor.json");
        if std::env::var_os("UPDATE_GOLDEN").is_some() {
            std::fs::write(&golden, &normalised).unwrap();
        }
        let expected =
            std::fs::read_to_string(&golden).expect("golden file; run with UPDATE_GOLDEN=1");
        assert_eq!(
            normalised, expected,
            "run with UPDATE_GOLDEN=1 and review the diff"
        );
    }

    #[test]
    fn text_output_and_the_fail_on_threshold() {
        covers(&[ids::doctor::checks::FAIL_ON_THRESHOLD]);
        let (_game, root) = setup();
        let dir = root.path().to_str().unwrap();
        let (code, out, _) = nvmod(&["doctor", "--instance", dir]);
        assert_eq!(code, 0);
        assert!(
            out.contains("meta.modid-mismatch")
                && out.contains("jip_nvse.dll <- JIP LN NVSE Plugin"),
            "{out}"
        );
        assert_eq!(
            nvmod(&["doctor", "--instance", dir, "--fail-on", "warning"]).0,
            1
        );
        assert_eq!(
            nvmod(&["doctor", "--instance", dir, "--fail-on", "info"]).0,
            1
        );
        assert_eq!(
            nvmod(&[
                "doctor",
                "--instance",
                dir,
                "--fail-on",
                "error",
                "--profile",
                "Other"
            ])
            .0,
            0
        );
    }

    #[test]
    fn an_explicit_game_folder_and_profile_are_used() {
        let (_game, root) = setup();
        let empty = TempDir::new("cli-empty-game");
        let (code, out, _) = nvmod(&[
            "doctor",
            "--instance",
            root.path().to_str().unwrap(),
            "--profile",
            "Other",
            "--game",
            empty.path().to_str().unwrap(),
        ]);
        assert_eq!(code, 1, "no game executable is an error");
        assert!(out.contains("game.exe-missing"), "{out}");
    }

    #[test]
    fn problems_with_the_arguments_or_the_instance_are_reported() {
        let (_game, root) = setup();
        let dir = root.path().to_str().unwrap();
        assert_eq!(nvmod(&["doctor"]).0, 5);
        assert_eq!(nvmod(&["doctor", "--instance"]).0, 5);
        assert_eq!(nvmod(&["doctor", "--instance", dir, "extra"]).0, 5);
        let (code, _, err) = nvmod(&["doctor", "--instance", dir, "--fail-on", "fatal"]);
        assert_eq!((code, err.contains("--fail-on")), (5, true));
        let (code, _, err) = nvmod(&["doctor", "--instance", dir, "--profile", "Nope"]);
        assert_eq!(code, 1);
        assert!(err.contains("no profile `Nope`"), "{err}");
        let (code, out, _) = nvmod(&["doctor", "--instance", dir, "--profile", "../x", "--json"]);
        assert_eq!(code, 1);
        assert!(
            serde_json::from_str::<serde_json::Value>(&out).unwrap()["error"]
                .as_str()
                .unwrap()
                .contains("not a profile folder name")
        );
        let empty = TempDir::new("cli-not-instance");
        let (code, _, err) = nvmod(&["doctor", "--instance", empty.path().to_str().unwrap()]);
        assert_eq!(code, 1);
        assert!(err.contains("not a Mod Organizer 2 instance"));
        write_file(empty.path(), "ModOrganizer.ini", b"[General]\r\n");
        let (code, _, err) = nvmod(&["doctor", "--instance", empty.path().to_str().unwrap()]);
        assert_eq!(code, 1);
        assert!(err.contains("no selected profile"), "{err}");
    }
}

#[test]
fn catalog_lists_plugins_as_text_and_json() {
    covers(&[ids::doctor::nvse::CATALOGUE_LISTING]);
    let (code, out, _) = nvmod(&["catalog"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("jip-ln")
            && out.contains("jip_nvse.dll")
            && out.contains("showoff")
            && out.contains("nexus 72541"),
        "{out}"
    );
    let (code, out, _) = nvmod(&["catalog", "--json"]);
    assert_eq!(code, 0);
    let json: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(json["schema"], 1);
    assert!(json["plugins"].as_array().unwrap().len() > 30 && json["mods"][0]["name"] == "MAC-TEN");
    assert_eq!(nvmod(&["catalog", "x"]).0, 5);
    assert_eq!(nvmod(&["catalog", "--nope"]).0, 5);
}
