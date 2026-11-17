// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod verify`, run as a program against a scratch Instance made by `nvmod init` and `nvmod profile create`
//! from a synthetic base copy. nv-rs is a fake file and its record written here. No game data.

use nvmod_testkit::{BaseBuilder, PluginBuilder, TempDir, nv_viewer_name, snapshot, write_file};
use nvmod_tools::sha256_hex;
use serde_json::Value;
use std::path::PathBuf;
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

struct Rig {
    base: TempDir,
    out: TempDir,
}

impl Rig {
    fn new() -> Rig {
        let rig = Rig {
            base: TempDir::new("verify-base"),
            out: TempDir::new("verify-out"),
        };
        BaseBuilder::steam().write(rig.base.path());
        write_file(
            rig.base.path(),
            "nvse_1_4.dll",
            &nvmod_testkit::PeBuilder::dll().build(),
        );
        write_file(
            rig.base.path(),
            "Data/FalloutNV.esm",
            &PluginBuilder::master().build(),
        );
        let root = rig.root();
        let r = root.to_str().unwrap();
        let (code, _, err) = nvmod(&[
            "init",
            "--base",
            rig.base.path().to_str().unwrap(),
            "--instance",
            r,
        ]);
        assert_eq!(code, 0, "{err}");
        let (code, _, err) = nvmod(&["profile", "create", "--instance", r]);
        assert_eq!(code, 0, "{err}");
        let (code, _, err) = nvmod(&["stage", "--instance", r]);
        assert_eq!(code, 0, "{err}");
        rig
    }

    fn root(&self) -> PathBuf {
        self.out.path().join("fnv")
    }

    fn verify(&self, extra: &[&str]) -> (i32, String, String) {
        let root = self.root();
        let mut args = vec!["verify", "--instance", root.to_str().unwrap()];
        args.extend_from_slice(extra);
        nvmod(&args)
    }

    fn json(&self, extra: &[&str]) -> (i32, Value) {
        let mut args = vec!["--json"];
        args.extend_from_slice(extra);
        let (code, out, err) = self.verify(&args);
        (
            code,
            serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out} / {err}")),
        )
    }

    /// A fake nv-rs install with the record `nvmod install-nv-rs` writes.
    fn fake_nv_rs(&self, recorded: &[u8], present: &[u8]) {
        write_file(
            &self.root(),
            &format!("nv-rs/bin/{}", nv_viewer_name()),
            present,
        );
        let record = serde_json::json!({ "method": "release", "sha256": sha256_hex(recorded) });
        write_file(
            &self.root(),
            "nv-rs/install.json",
            record.to_string().as_bytes(),
        );
    }
}

#[test]
fn a_clean_instance_exits_0_and_prints_the_doctor_report_and_the_launch_plan() {
    let rig = Rig::new();
    let (code, out, err) = rig.verify(&[]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(out.contains("nvmod doctor"), "{out}");
    assert!(out.contains("launch plan"), "{out}");
    assert!(out.contains("linux:") && out.contains("windows:"), "{out}");
}

#[test]
fn the_json_report_carries_the_doctor_report_and_both_launch_entries() {
    let rig = Rig::new();
    let (code, doc) = rig.json(&[]);
    assert_eq!(code, 0, "{doc}");
    assert_eq!(doc["verified"], true);
    assert_eq!(doc["schema_version"], 1);
    assert!(doc["doctor"]["findings"].is_array());
    let entries = doc["launch_plan"]["entries"].as_array().unwrap();
    let linux = entries.iter().find(|e| e["platform"] == "linux").unwrap();
    let windows = entries.iter().find(|e| e["platform"] == "windows").unwrap();
    assert_eq!(linux["program"], "proton");
    assert_eq!(linux["args"][0], "run");
    assert!(
        linux["args"][1]
            .as_str()
            .unwrap()
            .ends_with("FalloutNV.exe")
    );
    assert!(linux["env"]["STEAM_COMPAT_DATA_PATH"].is_string());
    assert!(
        windows["program"]
            .as_str()
            .unwrap()
            .ends_with("FalloutNV.exe")
    );
    assert_eq!(windows["args"], serde_json::json!([]));
}

#[test]
fn without_xnvse_the_plan_is_vanilla_and_says_so_with_xnvse_it_uses_the_loader() {
    let rig = Rig::new();
    let (_, doc) = rig.json(&[]);
    assert_eq!(doc["launch_plan"]["extender"], false);
    assert!(
        doc["launch_plan"]["note"]
            .as_str()
            .unwrap()
            .contains("vanilla")
    );
    write_file(rig.base.path(), "nvse_loader.exe", b"loader");
    let (code, doc) = rig.json(&[]);
    assert_eq!(code, 0, "{doc}");
    assert_eq!(doc["launch_plan"]["extender"], true);
    for entry in doc["launch_plan"]["entries"].as_array().unwrap() {
        let last = entry["args"]
            .as_array()
            .unwrap()
            .last()
            .map_or_default(|v| v.as_str().unwrap().to_owned());
        let program = entry["program"].as_str().unwrap();
        assert!(
            last.ends_with("nvse_loader.exe") || program.ends_with("nvse_loader.exe"),
            "{entry}"
        );
    }
}

#[test]
fn the_proton_program_can_be_named() {
    let rig = Rig::new();
    let (_, doc) = rig.json(&["--proton", "/opt/proton/proton"]);
    let linux = doc["launch_plan"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["platform"] == "linux")
        .unwrap()
        .clone();
    assert_eq!(linux["program"], "/opt/proton/proton");
}

#[test]
fn a_doctor_error_is_exit_3_and_the_report_still_prints() {
    let rig = Rig::new();
    write_file(
        &rig.root(),
        "mods/Patch/Patch.esp",
        &PluginBuilder::new().requires("Missing.esm").build(),
    );
    write_file(
        &rig.root(),
        "profiles/Default/modlist.txt",
        b"# This file was automatically generated by Mod Organizer.\r\n+Patch\r\n",
    );
    write_file(
        &rig.root(),
        "profiles/Default/loadorder.txt",
        b"# x\r\nFalloutNV.esm\r\nPatch.esp\r\n",
    );
    write_file(
        &rig.root(),
        "profiles/Default/plugins.txt",
        b"# x\r\nFalloutNV.esm\r\nPatch.esp\r\n",
    );
    let (c, _, err) = nvmod(&["stage", "--instance", rig.root().to_str().unwrap()]);
    assert_eq!(c, 0, "{err}");
    let (code, doc) = rig.json(&[]);
    assert_eq!(code, 3, "{doc}");
    assert_eq!(doc["verified"], false);
    assert!(doc["doctor"]["summary"]["errors"].as_u64().unwrap() >= 1);
    assert!(doc["launch_plan"]["entries"].is_array());
}

#[test]
fn an_nv_rs_that_matches_its_record_is_verified() {
    let rig = Rig::new();
    rig.fake_nv_rs(b"viewer", b"viewer");
    let (code, doc) = rig.json(&[]);
    assert_eq!(code, 0, "{doc}");
    assert_eq!(doc["nv_rs"]["installed"], true);
    assert_eq!(doc["nv_rs"]["verified"], true);
}

#[test]
fn an_nv_rs_changed_after_its_install_is_a_verification_failure() {
    let rig = Rig::new();
    rig.fake_nv_rs(b"viewer", b"tampered");
    let (code, doc) = rig.json(&[]);
    assert_eq!(code, 3, "{doc}");
    assert_eq!(doc["nv_rs"]["verified"], false);
    let (code, out, _) = rig.verify(&[]);
    assert_eq!(code, 3);
    assert!(out.contains("nv-rs"), "{out}");
}

#[test]
fn no_nv_rs_is_reported_not_failed_and_the_plan_names_the_native_command() {
    let rig = Rig::new();
    let (code, doc) = rig.json(&[]);
    assert_eq!(code, 0);
    assert_eq!(doc["nv_rs"]["installed"], false);
    assert!(
        doc["launch_plan"]["nv_rs"]
            .as_str()
            .unwrap()
            .contains("nvmod run nv-rs")
    );
}

#[test]
fn verify_writes_nothing() {
    let rig = Rig::new();
    rig.fake_nv_rs(b"viewer", b"viewer");
    let before = snapshot(rig.out.path());
    rig.verify(&[]);
    rig.verify(&["--json"]);
    assert_eq!(snapshot(rig.out.path()), before);
}

#[test]
fn a_folder_that_is_not_an_instance_is_refused_and_usage_errors_exit_5() {
    let dir = TempDir::new("verify-none");
    let (code, _, err) = nvmod(&["verify", "--instance", dir.path().to_str().unwrap()]);
    assert_eq!(code, 2, "{err}");
    assert_eq!(nvmod(&["verify"]).0, 5);
    assert_eq!(nvmod(&["verify", "--nope"]).0, 5);
}
