// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod init` and `nvmod doctor` on a base copy, run as a program against scratch folders built from synthetic
//! fixtures (a fake Steam, GOG or Epic copy). Nothing here depends on the machine or on any game file.

use nvmod_testkit::{BaseBuilder, TempDir, snapshot};
use serde_json::Value;
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

fn base(builder: &BaseBuilder) -> TempDir {
    let dir = TempDir::new("base");
    builder.write(dir.path());
    dir
}

fn doctor_json(dir: &TempDir) -> (i32, Value) {
    let (code, out, err) = nvmod(&["doctor", "--base", dir.path().to_str().unwrap(), "--json"]);
    let json = serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out} / {err}"));
    (code, json)
}

fn codes(report: &Value) -> Vec<String> {
    report["findings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["code"].as_str().unwrap().to_owned())
        .collect()
}

#[test]
fn a_steam_base_copy_gets_a_clean_report() {
    let dir = base(&BaseBuilder::steam());
    let (code, report) = doctor_json(&dir);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["storefront"], "steam");
    assert_eq!(report["summary"]["errors"], 0);
    assert_eq!(report["summary"]["warnings"], 0);
}

#[test]
fn a_gog_base_copy_gets_a_clean_report() {
    let dir = base(&BaseBuilder::gog());
    let (code, report) = doctor_json(&dir);
    assert_eq!(code, 0, "{report}");
    assert_eq!(report["storefront"], "gog");
    assert_eq!(report["summary"]["errors"], 0);
    assert_eq!(report["summary"]["warnings"], 0);
}

#[test]
fn the_text_report_names_the_storefront_and_the_executable() {
    let dir = base(&BaseBuilder::gog());
    let (code, out, _) = nvmod(&["doctor", "--base", dir.path().to_str().unwrap()]);
    assert_eq!(code, 0);
    assert!(out.contains("GOG"), "{out}");
    assert!(out.contains("1.4.0.525"), "{out}");
}

#[test]
fn a_4gb_patched_executable_is_accepted() {
    for builder in [BaseBuilder::steam(), BaseBuilder::gog()] {
        let dir = base(&builder.patched_4gb());
        let (code, report) = doctor_json(&dir);
        assert_eq!(code, 0, "{report}");
        assert_eq!(report["exe"]["large_address_aware"], true);
        assert_eq!(report["summary"]["errors"], 0);
        assert_eq!(report["summary"]["warnings"], 0);
    }
}

#[test]
fn an_unknown_executable_is_reported() {
    let dir = base(&BaseBuilder::steam().exe_version([1, 0, 0, 1]));
    let (code, report) = doctor_json(&dir);
    assert_eq!(code, 1);
    assert!(codes(&report).contains(&"base.exe-unknown".to_owned()));
}

#[test]
fn a_file_that_is_not_an_executable_is_reported() {
    let dir = base(&BaseBuilder::steam());
    std::fs::write(dir.path().join("FalloutNV.exe"), b"not a pe file").unwrap();
    let (code, report) = doctor_json(&dir);
    assert_eq!(code, 1);
    assert!(codes(&report).contains(&"base.exe-unreadable".to_owned()));
}

#[test]
fn a_missing_executable_or_data_folder_is_reported() {
    let dir = base(&BaseBuilder::steam().without_executable());
    let (code, report) = doctor_json(&dir);
    assert_eq!(code, 1);
    assert!(codes(&report).contains(&"base.exe-missing".to_owned()));

    let dir = base(&BaseBuilder::gog().without_data());
    let (code, report) = doctor_json(&dir);
    assert_eq!(code, 1);
    assert!(codes(&report).contains(&"base.data-missing".to_owned()));
}

#[test]
fn an_epic_base_copy_is_refused_with_a_reason() {
    let dir = base(&BaseBuilder::epic());
    let (code, _, err) = nvmod(&["doctor", "--base", dir.path().to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(
        err.contains("Epic") && err.contains("Steam or GOG"),
        "{err}"
    );
    let (code, out, _) = nvmod(&["doctor", "--base", dir.path().to_str().unwrap(), "--json"]);
    assert_eq!(code, 2);
    let json: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(json["refused"], true);
    assert!(json["reason"].as_str().unwrap().contains("Epic"));
}

#[test]
fn a_base_copy_of_an_unknown_storefront_is_refused_with_a_reason() {
    let dir = base(&BaseBuilder::unknown());
    let (code, _, err) = nvmod(&["doctor", "--base", dir.path().to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(
        err.contains("storefront") && err.contains("Steam or GOG"),
        "{err}"
    );
}

#[test]
fn a_base_folder_that_does_not_exist_is_refused() {
    let dir = TempDir::new("missing");
    let gone = dir.path().join("gone");
    let (code, _, err) = nvmod(&["doctor", "--base", gone.to_str().unwrap()]);
    assert_eq!(code, 2);
    assert!(err.contains("not a folder"), "{err}");
}

#[test]
fn doctor_never_writes_to_the_base_copy() {
    for builder in [
        BaseBuilder::steam(),
        BaseBuilder::gog().patched_4gb(),
        BaseBuilder::epic(),
        BaseBuilder::unknown(),
        BaseBuilder::steam().exe_version([2, 0, 0, 0]),
    ] {
        let dir = base(&builder);
        let before = snapshot(dir.path());
        nvmod(&["doctor", "--base", dir.path().to_str().unwrap()]);
        nvmod(&["doctor", "--base", dir.path().to_str().unwrap(), "--json"]);
        assert_eq!(snapshot(dir.path()), before);
    }
}

#[test]
fn doctor_needs_exactly_one_of_base_and_instance() {
    let (code, _, err) = nvmod(&["doctor"]);
    assert_eq!(code, 5);
    assert!(err.contains("--base"), "{err}");
    let (code, _, _) = nvmod(&["doctor", "--base", "a", "--instance", "b"]);
    assert_eq!(code, 5);
}

#[test]
fn the_help_lists_the_exit_codes() {
    let (code, out, _) = nvmod(&["--help"]);
    assert_eq!(code, 0);
    for line in [
        "0  success",
        "1  problems found",
        "2  refused",
        "3  verification failure",
        "4  step failed and rolled back",
    ] {
        assert!(out.contains(line), "{line} missing from {out}");
    }
}

mod init {
    use super::*;

    fn init(base: &TempDir, instance: &std::path::Path) -> (i32, String, String) {
        nvmod(&[
            "init",
            "--base",
            base.path().to_str().unwrap(),
            "--instance",
            instance.to_str().unwrap(),
        ])
    }

    #[test]
    fn init_builds_an_instance_skeleton_the_doctor_accepts() {
        let base = base(&BaseBuilder::steam());
        // The vanilla target needs xNVSE's DLL in the base copy (ticket 29).
        nvmod_testkit::write_file(
            base.path(),
            "nvse_1_4.dll",
            &nvmod_testkit::PeBuilder::dll().build(),
        );
        let scratch = TempDir::new("instance");
        let instance = scratch.path().join("fnv");
        let before = snapshot(base.path());
        let (code, _, err) = init(&base, &instance);
        assert_eq!(code, 0, "{err}");
        assert!(instance.join("nvmod-instance.json").is_file());
        assert!(instance.join("mods").is_dir());
        assert!(instance.join("profiles").is_dir());
        assert_eq!(
            snapshot(base.path()),
            before,
            "the base copy is not written"
        );

        let (code, out, _) = nvmod(&["doctor", "--instance", instance.to_str().unwrap(), "--json"]);
        assert_eq!(code, 0, "{out}");
        let report: Value = serde_json::from_str(&out).unwrap();
        assert_eq!(report["storefront"], "steam");
        assert_eq!(report["summary"]["errors"], 0);
    }

    #[test]
    fn the_doctor_on_an_instance_reports_a_base_copy_that_went_bad_without_writing() {
        let base = base(&BaseBuilder::gog());
        let scratch = TempDir::new("instance");
        let instance = scratch.path().join("fnv");
        assert_eq!(init(&base, &instance).0, 0);
        std::fs::remove_file(base.path().join("FalloutNV.exe")).unwrap();
        let before = snapshot(scratch.path());
        let (code, out, _) = nvmod(&["doctor", "--instance", instance.to_str().unwrap(), "--json"]);
        assert_eq!(code, 1);
        let report: Value = serde_json::from_str(&out).unwrap();
        assert!(codes(&report).contains(&"base.exe-missing".to_owned()));
        assert_eq!(snapshot(scratch.path()), before);
    }

    #[test]
    fn init_refuses_an_unsupported_base_and_writes_nothing() {
        let base = base(&BaseBuilder::epic());
        let scratch = TempDir::new("instance");
        let instance = scratch.path().join("fnv");
        let (code, _, err) = init(&base, &instance);
        assert_eq!(code, 2);
        assert!(err.contains("Epic"), "{err}");
        assert!(!instance.exists());
    }

    #[test]
    fn init_refuses_to_overwrite_an_instance_and_one_inside_the_base() {
        let base = base(&BaseBuilder::steam());
        let scratch = TempDir::new("instance");
        let instance = scratch.path().join("fnv");
        assert_eq!(init(&base, &instance).0, 0);
        let before = snapshot(scratch.path());
        let (code, _, err) = init(&base, &instance);
        assert_eq!(code, 2);
        assert!(err.contains("already"), "{err}");
        assert_eq!(snapshot(scratch.path()), before);

        let inside = base.path().join("instance");
        let (code, _, err) = init(&base, &inside);
        assert_eq!(code, 2);
        assert!(err.contains("inside"), "{err}");
        assert!(!inside.exists());
    }

    #[test]
    fn init_without_its_arguments_is_a_usage_error() {
        let (code, _, err) = nvmod(&["init"]);
        assert_eq!(code, 5);
        assert!(err.contains("usage: nvmod init"), "{err}");
    }

    #[test]
    fn the_doctor_refuses_a_folder_that_is_not_an_instance() {
        let scratch = TempDir::new("notinstance");
        let (code, _, err) = nvmod(&["doctor", "--instance", scratch.path().to_str().unwrap()]);
        assert_ne!(code, 0);
        assert_ne!(err, "");
    }
}
