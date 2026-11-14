// SPDX-License-Identifier: GPL-3.0-only
//! The Instance file's schema version, run as a program: a schema 1 Instance keeps opening and is migrated on
//! the first command that writes, a newer one is refused with exit 2 and a reason, and `nvmod rollback` undoes the
//! migration. Instances are made by `nvmod init` from a synthetic base copy; the old file is written in code.

// Tests start the built `nvmod` binary itself; the runner rule (ADR-0005) is for the product code.
#![allow(clippy::disallowed_methods)]

use nvmod_testkit::{BaseBuilder, TempDir, write_file};
use serde_json::Value;
use std::path::{Path, PathBuf};
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

struct Scratch {
    _base: TempDir,
    dir: TempDir,
}

impl Scratch {
    fn new() -> Scratch {
        let base = TempDir::new("schema-base");
        BaseBuilder::steam().write(base.path());
        write_file(base.path(), "Data/Meshes/Rock.nif", b"base rock");
        let dir = TempDir::new("schema-inst");
        let (code, _, err) = nvmod(&[
            "init",
            "--base",
            base.path().to_str().unwrap(),
            "--instance",
            dir.path().join("fnv").to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "{err}");
        let s = Scratch { _base: base, dir };
        write_file(&s.root(), "profiles/Default/modlist.txt", b"# header\r\n");
        s
    }

    fn root(&self) -> PathBuf {
        self.dir.path().join("fnv")
    }

    fn file(&self) -> PathBuf {
        self.root().join("nvmod-instance.json")
    }

    fn json(&self) -> Value {
        serde_json::from_slice(&std::fs::read(self.file()).unwrap()).unwrap()
    }

    /// Replace the Instance file with a schema 1 file for the same base copy.
    fn downgrade_to_v1(&self) -> Vec<u8> {
        let base = self.json()["base"].as_str().unwrap().to_owned();
        let old = format!(
            "{{\n  \"schema\": 1,\n  \"base\": {},\n  \"storefront\": \"steam\"\n}}\n",
            serde_json::to_string(&base).unwrap()
        );
        std::fs::write(self.file(), old.as_bytes()).unwrap();
        old.into_bytes()
    }

    fn stage(&self) -> (i32, String, String) {
        nvmod(&["stage", "--instance", self.root().to_str().unwrap()])
    }
}

fn path_str(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn init_writes_the_current_schema() {
    let s = Scratch::new();
    let json = s.json();
    assert_eq!(json["schema_version"], 2);
    assert_eq!(json["target"], "vanilla");
    assert!(json.get("schema").is_none());
}

#[test]
fn a_schema_1_instance_is_migrated_by_the_first_write_with_a_backup() {
    let s = Scratch::new();
    let old = s.downgrade_to_v1();
    let (code, _, err) = s.stage();
    assert_eq!(code, 0, "{err}");
    let json = s.json();
    assert_eq!(json["schema_version"], 2);
    assert_eq!(json["storefront"], "steam");
    assert_eq!(json["target"], "vanilla");
    let journals = s.root().join(".nvmod/journal");
    let backups: Vec<_> = std::fs::read_dir(&journals).unwrap().flatten().collect();
    assert_eq!(backups.len(), 1);
    let saved = std::fs::read(backups[0].path().join("backup/nvmod-instance.json")).unwrap();
    assert_eq!(saved, old);
}

#[test]
fn rollback_restores_the_schema_1_file_byte_for_byte() {
    let s = Scratch::new();
    let old = s.downgrade_to_v1();
    assert_eq!(s.stage().0, 0);
    let (code, out, err) = nvmod(&["rollback", "--instance", path_str(&s.root())]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("schema"), "{out}");
    assert_eq!(std::fs::read(s.file()).unwrap(), old);
    // Nothing left to undo, and the old file still opens (and migrates again).
    assert_eq!(nvmod(&["rollback", "--instance", path_str(&s.root())]).0, 2);
    assert_eq!(s.stage().0, 0);
    assert_eq!(s.json()["schema_version"], 2);
}

#[test]
fn the_doctor_opens_a_schema_1_instance_without_writing() {
    let s = Scratch::new();
    let old = s.downgrade_to_v1();
    let (code, _, err) = nvmod(&["doctor", "--instance", path_str(&s.root()), "--json"]);
    assert!(code == 0 || code == 1, "{code} {err}");
    assert_eq!(std::fs::read(s.file()).unwrap(), old);
    assert!(!s.root().join(".nvmod").exists());
}

#[test]
fn a_newer_schema_is_refused_with_exit_2_and_a_reason_and_left_alone() {
    let s = Scratch::new();
    let newer = br#"{"schema_version":3,"base":"/x","storefront":"steam","target":"vanilla"}"#;
    std::fs::write(s.file(), newer).unwrap();
    for args in [
        vec!["stage", "--instance", path_str(&s.root())],
        vec!["doctor", "--instance", path_str(&s.root())],
        vec!["rollback", "--instance", path_str(&s.root())],
    ] {
        let (code, _, err) = nvmod(&args);
        assert_eq!(code, 2, "{args:?}: {err}");
        assert!(
            err.contains("schema 3") && err.contains("newer"),
            "{args:?}: {err}"
        );
    }
    assert_eq!(std::fs::read(s.file()).unwrap(), newer);
}
