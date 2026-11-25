// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod profile apply|finish`, run as a program against a scratch Instance and a synthetic prefix.

use nvmod_testkit::{BaseBuilder, TempDir, snapshot, write_file};
use nvmod_testkit::{covers, ids};
use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn nvmod_env(args: &[&str], env: &[(&str, &str)]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_nvmod"))
        .args(args)
        .envs(env.iter().copied())
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn nvmod(args: &[&str]) -> (i32, String, String) {
    nvmod_env(args, &[])
}

struct Scratch {
    base: TempDir,
    dir: TempDir,
}

impl Scratch {
    fn new() -> Scratch {
        let base = TempDir::new("pa-base");
        BaseBuilder::steam().write(base.path());
        let dir = TempDir::new("pa");
        let s = Scratch { base, dir };
        let (code, _, err) = nvmod(&[
            "init",
            "--base",
            s.base.path().to_str().unwrap(),
            "--instance",
            s.root().to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "{err}");
        let (code, _, err) = nvmod(&[
            "profile",
            "create",
            "--instance",
            s.root().to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "{err}");
        fs::create_dir_all(s.prefix().join("pfx/drive_c/users/steamuser/Documents")).unwrap();
        write_file(
            &s.root(),
            "profiles/Default/settings.ini",
            b"[General]\r\nLocalSettings=true\r\nLocalSaves=true\r\nAutomaticArchiveInvalidation=true\r\n",
        );
        write_file(
            &s.root(),
            "profiles/Default/fallout.ini",
            b"[Archive]\r\nSArchiveList=A.bsa\r\n",
        );
        s
    }
    fn root(&self) -> PathBuf {
        self.dir.path().join("fnv")
    }
    fn prefix(&self) -> PathBuf {
        self.dir.path().join("prefix")
    }
    fn args<'a>(&'a self, sub: &'a [&'a str]) -> Vec<String> {
        let mut v: Vec<String> = ["profile"].iter().map(|s| (*s).to_owned()).collect();
        v.extend(sub.iter().map(|s| (*s).to_owned()));
        v.extend(
            [
                "--instance",
                self.root().to_str().unwrap(),
                "--prefix",
                self.prefix().to_str().unwrap(),
            ]
            .iter()
            .map(|s| (*s).to_owned()),
        );
        v
    }
    fn run(&self, sub: &[&str]) -> (i32, String, String) {
        let args = self.args(sub);
        let refs: Vec<&str> = args.iter().map(String::as_str).collect();
        nvmod(&refs)
    }
    fn games(&self) -> PathBuf {
        self.prefix()
            .join("pfx/drive_c/users/steamuser/Documents/My Games/FalloutNV")
    }
}

#[test]
fn apply_reports_json_and_places_the_profile() {
    covers(&[ids::run::prefix::APPLY_PLACES_PROFILE]);
    let s = Scratch::new();
    let (code, out, err) = s.run(&["apply", "--json"]);
    assert_eq!(code, 0, "{err}");
    let v: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(v["status"], "applied");
    assert_eq!(v["schema_version"], 1);
    assert_eq!(v["settings"]["local_saves"], true);
    assert!(
        fs::read_to_string(s.games().join("fallout.ini"))
            .unwrap()
            .contains("sLocalSavePath=__MO_Saves\\")
    );
    let (code, out, _) = s.run(&["apply", "--json"]);
    assert_eq!(code, 0);
    assert_eq!(
        serde_json::from_str::<Value>(&out).unwrap()["status"],
        "unchanged"
    );
}

#[test]
fn the_default_prefix_is_the_one_in_the_launch_plan() {
    let s = Scratch::new();
    fs::rename(s.prefix(), s.root().join("compatdata")).unwrap();
    let (code, _, err) = nvmod(&["profile", "apply", "--instance", s.root().to_str().unwrap()]);
    assert_eq!(code, 0, "{err}");
    assert!(
        s.root()
            .join("compatdata/pfx/drive_c/users/steamuser/Documents/My Games/FalloutNV/fallout.ini")
            .is_file()
    );
}

#[test]
fn a_dry_run_writes_nothing_and_a_missing_prefix_is_refused() {
    covers(&[ids::run::prefix::DRY_RUN]);
    let s = Scratch::new();
    let before = snapshot(s.dir.path());
    let (code, out, err) = s.run(&["apply", "--dry-run"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("dry run"), "{out}");
    assert_eq!(snapshot(s.dir.path()), before);
    let (code, _, err) = nvmod(&[
        "profile",
        "apply",
        "--instance",
        s.root().to_str().unwrap(),
        "--prefix",
        s.dir.path().join("none").to_str().unwrap(),
    ]);
    assert_eq!(code, 2);
    assert!(err.contains("not a Wine or Proton prefix"), "{err}");
}

#[test]
fn a_read_only_profile_ini_exits_2_before_any_write() {
    let s = Scratch::new();
    let ini = s.root().join("profiles/Default/fallout.ini");
    let mut p = fs::metadata(&ini).unwrap().permissions();
    p.set_readonly(true);
    fs::set_permissions(&ini, p).unwrap();
    let before = snapshot(s.dir.path());
    let (code, _, err) = s.run(&["apply"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("read-only"), "{err}");
    assert_eq!(snapshot(s.dir.path()), before);
}

#[test]
fn a_failing_step_exits_4_and_leaves_everything_as_it_was() {
    let s = Scratch::new();
    let before = snapshot(s.dir.path());
    let args = s.args(&["apply"]);
    let refs: Vec<&str> = args.iter().map(String::as_str).collect();
    for step in ["profile-ini", "initweaks", "place", "saves"] {
        let (code, _, err) = nvmod_env(&refs, &[("NVMOD_FAIL_AT", step)]);
        assert_eq!(code, 4, "{step}: {err}");
        assert_eq!(snapshot(s.dir.path()), before, "after a failure at {step}");
    }
}

#[test]
fn finish_collects_what_the_game_wrote() {
    covers(&[ids::run::prefix::FINISH_COLLECTS]);
    let s = Scratch::new();
    assert_eq!(s.run(&["apply"]).0, 0);
    fs::write(s.games().join("fallout.ini"), b"[General]\r\nplayed=1\r\n").unwrap();
    let (code, out, err) = s.run(&["finish", "--json"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(
        serde_json::from_str::<Value>(&out).unwrap()["status"],
        "collected"
    );
    assert_eq!(
        fs::read(s.root().join("profiles/Default/fallout.ini")).unwrap(),
        b"[General]\r\nplayed=1\r\n"
    );
}

#[test]
fn the_invalidation_archive_reaches_a_staged_tree() {
    covers(&[ids::run::prefix::ARCHIVE_INVALIDATION]);
    let s = Scratch::new();
    write_file(&s.root(), "mods/M/Textures/a.dds", b"m");
    write_file(
        &s.root(),
        "profiles/Default/modlist.txt",
        b"# This file was automatically generated by Mod Organizer.\r\n+M\r\n",
    );
    assert_eq!(
        nvmod(&["stage", "--instance", s.root().to_str().unwrap()]).0,
        0
    );
    let (code, out, err) = s.run(&["apply", "--json"]);
    assert_eq!(code, 0, "{err}");
    assert!(serde_json::from_str::<Value>(&out).unwrap()["staged_files"].is_number());
    assert!(
        s.root()
            .join("staging/Default/data/fallout - invalidation.bsa")
            .is_file()
    );
}

#[test]
fn usage_errors_exit_5() {
    for args in [
        vec!["profile", "apply"],
        vec!["profile", "finish", "--bogus"],
        vec!["profile", "apply", "extra", "--instance", "x"],
    ] {
        assert_eq!(nvmod(&args).0, 5, "{args:?}");
    }
}
