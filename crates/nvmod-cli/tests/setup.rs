// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod setup`, run as a program: a synthetic base copy, a fake nv-rs release served from a folder, a fake TTW
//! installer script, fixture archives and a masterlist, all written in code. No network, no game data.
#![cfg(unix)]

use nvmod_testkit::{
    BaseBuilder, FakeRelease, Mo2Builder, ModSpec, PluginBuilder, TempDir, ZipBuilder, snapshot,
    write_file, write_script,
};
use nvmod_tools::{platform, sha256_hex};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn nvmod(args: &[String]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_nvmod"))
        .args(args)
        .env_remove("NVMOD_CARGO")
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn strings(words: &[&str]) -> Vec<String> {
    words.iter().map(|w| (*w).to_owned()).collect()
}

const MASTERLIST: &str =
    "globals:\n  - {type: say, content: 'Masterlist for tests'}\nplugins: []\n";

struct Rig {
    base: TempDir,
    fo3: TempDir,
    work: TempDir,
    out: TempDir,
}

impl Rig {
    fn new() -> Rig {
        let rig = Rig {
            base: TempDir::new("setup-base"),
            fo3: TempDir::new("setup-fo3"),
            work: TempDir::new("setup-work"),
            out: TempDir::new("setup-out"),
        };
        BaseBuilder::steam().write(rig.base.path());
        // The vanilla target needs xNVSE's DLL in the base copy.
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
        write_file(rig.fo3.path(), "Fallout3.exe", b"synthetic fallout 3");
        std::fs::write(rig.work.path().join("masterlist.yaml"), MASTERLIST).unwrap();
        std::fs::create_dir_all(rig.work.path().join("release")).unwrap();
        FakeRelease::new().write(&rig.work.path().join("release"), sha256_hex);
        rig
    }

    fn root(&self) -> PathBuf {
        self.out.path().join("fnv")
    }

    fn work(&self, name: &str) -> String {
        self.work.path().join(name).to_str().unwrap().to_owned()
    }

    fn zip(&self, name: &str, files: &[(&str, Vec<u8>)]) -> String {
        let mut zip = ZipBuilder::new();
        for (path, bytes) in files {
            zip = zip.file(path, bytes);
        }
        zip.write(&self.work.path().join(name));
        self.work(name)
    }

    fn core_zip(&self) -> String {
        self.zip(
            "Core.zip",
            &[
                ("Core.esp", PluginBuilder::new().build()),
                ("Textures/dirt.dds", b"dirt".to_vec()),
            ],
        )
    }

    fn gh(&self) -> String {
        write_script(self.work.path(), "gh", "exit 0")
            .to_str()
            .unwrap()
            .to_owned()
    }

    /// The fake TTW installer and a lock that pins it. Returns (installer, lock).
    fn ttw_tools(&self) -> (String, String) {
        let esm = self.work.path().join("ttw.esm");
        std::fs::write(&esm, PluginBuilder::master().build()).unwrap();
        let body = format!(
            "dest=\"\"\nprev=\"\"\nfor a in \"$@\"; do [ \"$prev\" = --dest ] && dest=\"$a\"; prev=\"$a\"; done\n\
             mkdir -p \"$dest/Meshes\"\necho generated > \"$dest/Meshes/ttw.nif\"\n\
             cp {} \"$dest/TaleOfTwoWastelands.esm\"\n",
            esm.display()
        );
        let installer = write_script(self.work.path(), "ttw-installer", &body)
            .to_str()
            .unwrap()
            .to_owned();
        let hash = sha256_hex(&std::fs::read(&installer).unwrap());
        let text = format!(
            "schema_version = 1\n\n[[tool]]\nname = \"ttw-installer\"\nversion = \"0.2.0\"\nkind = \"third-party\"\n\
             contracts = {{ ttw-installer-result = 1 }}\n\n[tool.assets.{}]\nurl = \"http://127.0.0.1:9/ttw\"\nsha256 = \"{hash}\"\n",
            platform()
        );
        std::fs::write(self.work.path().join("tools.lock.toml"), text).unwrap();
        (installer, self.work("tools.lock.toml"))
    }

    /// The arguments of a full run: nv-rs from the fake release, TTW and one archive.
    fn full_args(&self) -> Vec<String> {
        let (installer, lock) = self.ttw_tools();
        let mut args = strings(&[
            "setup",
            "--base",
            self.base.path().to_str().unwrap(),
            "--instance",
            self.root().to_str().unwrap(),
            "--masterlist",
            &self.work("masterlist.yaml"),
            "--release",
            "--release-url",
            &format!("file://{}/release", self.work.path().display()),
            "--gh",
            &self.gh(),
            "--retry-delay-ms",
            "0",
            "--fallout3",
            self.fo3.path().to_str().unwrap(),
            "--installer",
            &installer,
            "--lock",
            &lock,
            "--mod",
            &self.core_zip(),
        ]);
        args.push("--json".to_owned());
        args
    }

    fn doctor(&self) -> (i32, String, String) {
        nvmod(&strings(&[
            "doctor",
            "--instance",
            self.root().to_str().unwrap(),
            "--masterlist",
            &self.work("masterlist.yaml"),
        ]))
    }
}

fn events(out: &str) -> Vec<Value> {
    out.lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{e}: {l}")))
        .collect()
}

fn without_json(mut args: Vec<String>) -> Vec<String> {
    args.retain(|a| a != "--json");
    args
}

#[test]
fn one_run_takes_a_base_copy_to_a_doctor_clean_instance() {
    let rig = Rig::new();
    let (code, out, err) = nvmod(&without_json(rig.full_args()));
    assert_eq!(code, 0, "{out}\n{err}");
    let root = rig.root();
    assert!(root.join("nv-rs/bin/nv-viewer").is_file());
    assert!(root.join("mods/Core/Core.esp").is_file());
    assert!(
        root.join("mods/Tale of Two Wastelands/TaleOfTwoWastelands.esm")
            .is_file()
    );
    let (code, out, err) = rig.doctor();
    assert_eq!(code, 0, "{out}\n{err}");
}

#[test]
fn the_run_ends_with_sort_apply_so_the_plugin_times_encode_the_load_order() {
    let rig = Rig::new();
    let (code, out, err) = nvmod(&without_json(rig.full_args()));
    assert_eq!(code, 0, "{out}\n{err}");
    let order = std::fs::read_to_string(rig.root().join("profiles/Default/loadorder.txt")).unwrap();
    let names: Vec<&str> = order
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .collect();
    assert!(names.len() >= 3, "{names:?}");
    let times: Vec<_> = names
        .iter()
        .map(|n| {
            let path = rig
                .root()
                .join("staging/Default/data")
                .join(n.trim().to_lowercase());
            std::fs::metadata(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()))
                .modified()
                .unwrap()
        })
        .collect();
    assert!(
        times.windows(2).all(|w| w[0] < w[1]),
        "strictly increasing: {names:?} {times:?}"
    );
}

#[test]
fn json_is_a_stream_of_step_events_and_a_final_result() {
    let rig = Rig::new();
    let (code, out, err) = nvmod(&rig.full_args());
    assert_eq!(code, 0, "{out}\n{err}");
    let events = events(&out);
    let seqs: Vec<u64> = events.iter().map(|e| e["seq"].as_u64().unwrap()).collect();
    assert!(seqs.windows(2).all(|w| w[0] < w[1]), "{seqs:?}");
    let done: Vec<&str> = events
        .iter()
        .filter(|e| e["event"] == "step" && e["status"] == "done")
        .map(|e| e["step"].as_str().unwrap())
        .collect();
    assert_eq!(
        done,
        [
            "init",
            "profile",
            "nv-rs",
            "target",
            "ttw",
            "mod:Core.zip",
            "stage",
            "sort",
            "verify"
        ]
    );
    for step in &done {
        assert!(
            events
                .iter()
                .any(|e| e["step"] == *step && e["status"] == "started"),
            "{step} has a started event"
        );
    }
    let last = events.last().unwrap();
    assert_eq!(last["event"], "finished");
    assert_eq!(last["status"], "ok");
    assert_eq!(last["exit_code"], 0);
    assert_eq!(last["rolled_back"], false);
}

#[test]
fn dry_run_lists_the_planned_changes_and_writes_nothing() {
    let rig = Rig::new();
    let mut args = rig.full_args();
    let before = (
        snapshot(rig.base.path()),
        snapshot(rig.work.path()),
        snapshot(rig.out.path()),
    );
    args.retain(|a| a != "--json");
    args.push("--dry-run".to_owned());
    let (code, out, err) = nvmod(&args);
    assert_eq!(code, 0, "{out}\n{err}");
    for word in ["init", "nv-rs", "ttw", "Core.zip", "sort"] {
        assert!(out.contains(word), "{word} in {out}");
    }
    assert!(!rig.root().exists());
    assert_eq!(
        before,
        (
            snapshot(rig.base.path()),
            snapshot(rig.work.path()),
            snapshot(rig.out.path())
        )
    );
}

#[test]
fn dry_run_json_marks_every_step_as_planned() {
    let rig = Rig::new();
    let mut args = rig.full_args();
    args.push("--dry-run".to_owned());
    let (code, out, err) = nvmod(&args);
    assert_eq!(code, 0, "{out}\n{err}");
    let events = events(&out);
    let planned = events.iter().filter(|e| e["status"] == "planned").count();
    assert_eq!(planned, 9, "{out}");
    assert_eq!(events.last().unwrap()["status"], "dry-run");
    assert!(!rig.root().exists());
}

#[test]
fn dry_run_still_refuses_what_a_real_run_would_refuse() {
    let rig = Rig::new();
    let mut args = rig.full_args();
    args.push("--dry-run".to_owned());
    args.extend(strings(&["--mod", &rig.work("missing.zip")]));
    let (code, _, err) = nvmod(&args);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("missing.zip"), "{err}");
}

#[test]
fn a_failing_nv_rs_check_exits_4_and_leaves_no_instance() {
    let rig = Rig::new();
    let dir = rig.work.path().join("release");
    FakeRelease::new().failing_check().write(&dir, sha256_hex);
    let before = snapshot(rig.base.path());
    let (code, out, err) = nvmod(&rig.full_args());
    assert_eq!(code, 4, "{out}\n{err}");
    assert!(!rig.root().exists(), "rolled back to the state before");
    assert_eq!(snapshot(rig.base.path()), before);
    let events = events(&out);
    let failed = events.iter().find(|e| e["status"] == "failed").unwrap();
    assert_eq!(failed["step"], "nv-rs");
    assert_eq!(failed["code"], 4);
    let last = events.last().unwrap();
    assert_eq!(last["status"], "failed");
    assert_eq!(last["exit_code"], 4);
    assert_eq!(last["rolled_back"], true);
    assert_eq!(last["failed_step"], "nv-rs");
}

#[test]
fn a_release_with_a_wrong_checksum_exits_3_and_leaves_no_instance() {
    let rig = Rig::new();
    let dir = rig.work.path().join("release");
    FakeRelease::new().wrong_checksum().write(&dir, sha256_hex);
    let (code, out, err) = nvmod(&without_json(rig.full_args()));
    assert_eq!(code, 3, "{out}\n{err}");
    assert!(!rig.root().exists());
}

#[test]
fn a_failing_mod_install_rolls_back_the_earlier_steps_too() {
    let rig = Rig::new();
    let mut args = rig.full_args();
    let bad = rig.work("bad.zip");
    std::fs::write(&bad, b"this is not an archive").unwrap();
    args.extend(strings(&["--mod", &bad]));
    let (code, out, err) = nvmod(&args);
    assert!(code == 2 || code == 4, "{code}: {out}\n{err}");
    assert!(!rig.root().exists(), "nv-rs, TTW and Core are undone");
    let events = events(&out);
    assert_eq!(events.last().unwrap()["rolled_back"], true);
}

#[test]
fn a_doctor_finding_after_the_steps_is_a_verification_failure_and_rolls_back() {
    let rig = Rig::new();
    let mut args = rig.full_args();
    let patch = rig.zip(
        "Patch.zip",
        &[(
            "Patch.esp",
            PluginBuilder::new().requires("Missing.esm").build(),
        )],
    );
    args.extend(strings(&["--mod", &patch]));
    let (code, out, err) = nvmod(&args);
    assert_eq!(code, 3, "{out}\n{err}");
    assert!(!rig.root().exists());
    let events = events(&out);
    let failed = events.iter().find(|e| e["status"] == "failed").unwrap();
    assert_eq!(failed["step"], "verify");
}

#[test]
fn an_existing_instance_is_refused_with_exit_2_and_left_alone() {
    let rig = Rig::new();
    write_file(&rig.root(), "keep.txt", b"mine");
    let before = snapshot(rig.out.path());
    let (code, out, err) = nvmod(&without_json(rig.full_args()));
    assert_eq!(code, 2, "{out}\n{err}");
    assert_eq!(snapshot(rig.out.path()), before);
}

#[test]
fn an_unsupported_base_copy_is_refused_before_anything_is_written() {
    let rig = Rig::new();
    write_file(rig.base.path(), ".egstore/x", b"epic");
    let (code, out, err) = nvmod(&without_json(rig.full_args()));
    assert_eq!(code, 2, "{out}\n{err}");
    assert!(!rig.root().exists());
}

#[test]
fn a_missing_required_option_is_a_usage_error() {
    let (code, _, err) = nvmod(&strings(&["setup", "--base", "/nowhere"]));
    assert_eq!(code, 5, "{err}");
    let (code, _, _) = nvmod(&strings(&["setup", "--nope"]));
    assert_eq!(code, 5);
}

#[test]
fn without_ttw_options_the_ttw_step_is_not_run() {
    let rig = Rig::new();
    let mut args = rig.full_args();
    let at = args.iter().position(|a| a == "--fallout3").unwrap();
    args.drain(at..at + 6);
    let (code, out, err) = nvmod(&args);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(!out.contains("\"ttw\""), "{out}");
    assert!(!rig.root().join("mods/Tale of Two Wastelands").exists());
}

#[test]
fn import_mo2_starts_from_a_profile_instead_of_init() {
    let rig = Rig::new();
    let mo2 = TempDir::new("setup-mo2");
    Mo2Builder::new(rig.base.path())
        .mod_dir(ModSpec::new("Existing").file("Existing.esp", &PluginBuilder::new().build()))
        .profile(
            "Default",
            &["+Existing"],
            &["FalloutNV.esm", "Existing.esp"],
        )
        .write(mo2.path());
    let mut args = rig.full_args();
    args.extend(strings(&["--import-mo2", mo2.path().to_str().unwrap()]));
    let (code, out, err) = nvmod(&args);
    assert_eq!(code, 0, "{out}\n{err}");
    let events = events(&out);
    assert!(
        events
            .iter()
            .any(|e| e["step"] == "import-mo2" && e["status"] == "done"),
        "{out}"
    );
    assert!(!events.iter().any(|e| e["step"] == "init"));
    assert!(rig.root().join("mods/Existing/Existing.esp").is_file());
    assert!(rig.root().join("mods/Core/Core.esp").is_file());
    let (code, out, err) = rig.doctor();
    assert_eq!(code, 0, "{out}\n{err}");
    let _: &Path = mo2.path();
}

fn target_of(rig: &Rig) -> String {
    let text = std::fs::read_to_string(rig.root().join("nvmod-instance.json")).unwrap();
    let doc: Value = serde_json::from_str(&text).unwrap();
    doc["target"].as_str().unwrap_or("absent").to_owned()
}

#[test]
fn installing_nv_rs_makes_nv_rs_the_target_of_the_instance() {
    let rig = Rig::new();
    let (code, out, err) = nvmod(&without_json(rig.full_args()));
    assert_eq!(code, 0, "{out}\n{err}");
    assert_eq!(target_of(&rig), "nv-rs");
}

#[test]
fn without_nv_rs_the_target_stays_vanilla_and_xnvse_must_be_in_the_base_copy() {
    let rig = Rig::new();
    let mut args = without_json(rig.full_args());
    args.push("--skip-nv-rs".to_owned());
    let (code, out, err) = nvmod(&args);
    assert_eq!(code, 0, "{out}\n{err}");
    assert_eq!(target_of(&rig), "vanilla");
    assert!(!rig.root().join("nv-rs").exists());
    let again = Rig::new();
    std::fs::remove_file(again.base.path().join("nvse_1_4.dll")).unwrap();
    let mut args = without_json(again.full_args());
    args.push("--skip-nv-rs".to_owned());
    let (code, out, err) = nvmod(&args);
    assert_eq!(code, 3, "{out}\n{err}");
    assert!(out.contains("extender.missing"), "{out}");
    assert!(!again.root().exists());
}

#[test]
fn the_target_command_sets_the_target_and_refuses_what_it_cannot() {
    let rig = Rig::new();
    let (code, _, err) = nvmod(&without_json(rig.full_args()));
    assert_eq!(code, 0, "{err}");
    let root = rig.root();
    let r = root.to_str().unwrap();
    let (code, out, err) = nvmod(&strings(&["target", "vanilla", "--instance", r]));
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("vanilla"), "{out}");
    assert_eq!(target_of(&rig), "vanilla");
    assert_eq!(nvmod(&strings(&["target", "dos", "--instance", r])).0, 5);
    assert_eq!(nvmod(&strings(&["target", "nv-rs"])).0, 5);
    let none = TempDir::new("setup-notinst");
    let n = none.path().to_str().unwrap();
    assert_eq!(nvmod(&strings(&["target", "nv-rs", "--instance", n])).0, 2);
}
