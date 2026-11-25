// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod saves`, run as a program against a scratch Instance with synthetic saves.

use nvmod_testkit::{BaseBuilder, SaveBuilder, TempDir, snapshot, write_file};
use nvmod_testkit::{covers, ids};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn nvmod(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_nvmod"))
        .args(args)
        .env("NVMOD_NOW", "1794657600")
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

struct Scratch {
    base: TempDir,
    dir: TempDir,
}

impl Scratch {
    fn new() -> Scratch {
        let base = TempDir::new("sc-base");
        BaseBuilder::steam().write(base.path());
        let s = Scratch {
            base,
            dir: TempDir::new("sc"),
        };
        let r = s.root();
        let r = r.to_str().unwrap();
        assert_eq!(
            nvmod(&[
                "init",
                "--base",
                s.base.path().to_str().unwrap(),
                "--instance",
                r
            ])
            .0,
            0
        );
        for name in ["A", "B"] {
            assert_eq!(
                nvmod(&["profile", "create", "--instance", r, "--name", name]).0,
                0
            );
            write_file(
                &s.root(),
                &format!("profiles/{name}/settings.ini"),
                b"[General]\r\nLocalSaves=true\r\n",
            );
        }
        s
    }
    fn root(&self) -> PathBuf {
        self.dir.path().join("fnv")
    }
    fn save(&self, profile: &str, name: &str, bytes: &[u8]) {
        write_file(
            &self.root(),
            &format!("profiles/{profile}/saves/{name}"),
            bytes,
        );
    }
    fn run(&self, args: &[&str]) -> (i32, String, String) {
        let root = self.root();
        let mut v: Vec<&str> = args.to_vec();
        v.extend(["--instance", root.to_str().unwrap()]);
        nvmod(&v)
    }
}

#[test]
fn list_shows_saves_with_plugin_counts_and_unreadable_ones() {
    covers(&[ids::safety::saves::LIST]);
    let s = Scratch::new();
    s.save(
        "A",
        "Save 1.fos",
        &SaveBuilder::new(1).plugins(&["FalloutNV.esm"]).build(),
    );
    s.save("A", "Cut.fos", b"FO3SAVEGAME\0");
    let (code, out, err) = s.run(&["saves", "list", "--profile", "A", "--json"]);
    assert_eq!(code, 0, "{err}");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["saves"][0]["name"], "Cut.fos");
    assert_eq!(v["saves"][0]["status"], "unreadable");
    assert_eq!(v["saves"][1]["plugins"], 1);
    let (code, out, _) = s.run(&["saves", "list", "--profile", "A"]);
    assert_eq!(code, 0);
    assert!(
        out.contains("1 plugin(s)") && out.contains("unreadable"),
        "{out}"
    );
}

#[test]
fn backup_delete_restore_round_trips_through_the_commands() {
    covers(&[ids::safety::saves::BACKUP_RESTORE]);
    let s = Scratch::new();
    s.save("A", "Save 1.fos", b"fos");
    s.save("A", "Save 1.nvse", b"nvse");
    let dir = s.root().join("profiles/A/saves");
    let before = snapshot(&dir);
    let (code, out, err) = s.run(&["saves", "backup", "--profile", "A", "--json"]);
    assert_eq!(code, 0, "{err}");
    let id = serde_json::from_str::<Value>(&out).unwrap()["backup"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    assert_eq!(id, "20261114T120000Z");
    fs::remove_dir_all(&dir).unwrap();
    let (code, out, err) = s.run(&["saves", "restore", "latest", "--profile", "A"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("restored"), "{out}");
    assert_eq!(snapshot(&dir), before);
    // A damaged backup exits 3.
    fs::write(
        s.root()
            .join(format!(".nvmod/saves-backup/{id}/files/Save 1.fos")),
        b"x",
    )
    .unwrap();
    assert_eq!(s.run(&["saves", "restore", &id, "--profile", "A"]).0, 3);
}

#[test]
fn transfer_refuses_without_the_cosave_and_force_copies() {
    covers(&[ids::safety::saves::TRANSFER]);
    let s = Scratch::new();
    s.save("A", "Save 1.fos", b"fos");
    let (code, _, err) = s.run(&["saves", "transfer", "A", "B"]);
    assert_eq!(code, 2);
    assert!(err.contains("--force"), "{err}");
    assert!(!s.root().join("profiles/B/saves").exists());
    let (code, _, err) = s.run(&["saves", "transfer", "A", "B", "--force"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        fs::read(s.root().join("profiles/B/saves/Save 1.fos")).unwrap(),
        b"fos"
    );
}

#[test]
fn check_exits_1_for_a_save_that_needs_a_missing_plugin() {
    covers(&[ids::safety::saves::CHECK_MISSING_PLUGIN]);
    let s = Scratch::new();
    write_file(&s.root(), "profiles/A/plugins.txt", b"FalloutNV.esm\r\n");
    s.save(
        "A",
        "Fine.fos",
        &SaveBuilder::new(1).plugins(&["FalloutNV.esm"]).build(),
    );
    assert_eq!(s.run(&["saves", "check", "--profile", "A"]).0, 0);
    s.save(
        "A",
        "Bad.fos",
        &SaveBuilder::new(2)
            .plugins(&["FalloutNV.esm", "Gone.esp"])
            .build(),
    );
    let (code, out, _) = s.run(&["saves", "check", "--profile", "A"]);
    assert_eq!(code, 1);
    assert!(out.contains("Bad.fos") && out.contains("Gone.esp"), "{out}");
}

#[test]
fn checksum_lists_hashes_and_a_shared_profile_needs_a_prefix() {
    covers(&[ids::safety::saves::CHECKSUMS]);
    let s = Scratch::new();
    s.save("A", "Save 1.fos", b"abc");
    let (code, out, _) = s.run(&["saves", "checksum", "--profile", "A", "--json"]);
    assert_eq!(code, 0);
    assert_eq!(
        serde_json::from_str::<Value>(&out).unwrap()["files"][0]["sha256"],
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    write_file(
        &s.root(),
        "profiles/B/settings.ini",
        b"[General]\r\nLocalSaves=false\r\n",
    );
    let (code, _, err) = s.run(&["saves", "list", "--profile", "B"]);
    assert_eq!(code, 2);
    assert!(err.contains("prefix"), "{err}");
}

#[test]
fn usage_errors_exit_5() {
    for args in [
        vec!["saves"],
        vec!["saves", "list"],
        vec!["saves", "restore", "--instance", "x"],
        vec!["saves", "transfer", "A", "--instance", "x"],
        vec!["saves", "list", "--force", "--instance", "x"],
        vec!["saves", "wat", "--instance", "x"],
    ] {
        assert_eq!(nvmod(&args).0, 5, "{args:?}");
    }
}
