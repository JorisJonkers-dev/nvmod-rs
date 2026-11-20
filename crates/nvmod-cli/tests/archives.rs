// SPDX-License-Identifier: GPL-3.0-only
#![allow(clippy::format_push_string, clippy::assert_is_empty)]
//! `nvmod conflicts --archives` and the Doctor's archive checks on a scratch Instance. The archives are
//! generated headers with fake names; nothing is game data.

use nvmod_testkit::{BaseBuilder, BsaBuilder, PeBuilder, TempDir, write_file};
use serde_json::{Value, json};
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

struct Scratch {
    _base: TempDir,
    inst: TempDir,
}

impl Scratch {
    fn new() -> Scratch {
        let base = TempDir::new("arch-base");
        BaseBuilder::steam().write(base.path());
        write_file(base.path(), "nvse_1_4.dll", &PeBuilder::dll().build());
        let inst = TempDir::new("arch-inst");
        let root = inst.path().join("fnv");
        let (code, _, err) = nvmod(&[
            "init",
            "--base",
            base.path().to_str().unwrap(),
            "--instance",
            root.to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "{err}");
        Scratch { _base: base, inst }
    }

    fn root(&self) -> PathBuf {
        self.inst.path().join("fnv")
    }

    fn profile(&self, file: &str, text: &str) {
        write_file(
            &self.root(),
            &format!("profiles/Default/{file}"),
            text.as_bytes(),
        );
    }

    fn modlist(&self, lines: &str) {
        self.profile("modlist.txt", &format!("# generated\r\n{lines}"));
    }

    fn plugins(&self, active: &[&str], inactive: &[&str]) {
        let mut order = String::from("FalloutNV.esm\r\n");
        let mut on = order.clone();
        for p in active {
            order += &format!("{p}\r\n");
            on += &format!("{p}\r\n");
        }
        for p in inactive {
            order += &format!("{p}\r\n");
        }
        self.profile("loadorder.txt", &order);
        self.profile("plugins.txt", &on);
    }

    fn ini(&self, archive_list: &str, invalidate: Option<&str>) {
        let mut text = format!("[Archive]\r\nSArchiveList={archive_list}\r\n");
        if let Some(v) = invalidate {
            text += &format!("bInvalidateOlderFiles={v}\r\n");
        }
        self.profile("fallout.ini", &text);
    }

    fn file(&self, name: &str, path: &str, bytes: &[u8]) {
        write_file(&self.root(), &format!("mods/{name}/{path}"), bytes);
    }

    fn bsa(&self, name: &str, file: &str, paths: &[&str]) {
        let mut b = BsaBuilder::new();
        for p in paths {
            b = b.file(p, 4);
        }
        self.file(name, file, &b.bytes());
    }

    fn json(&self, args: &[&str]) -> (i32, Value) {
        let root = self.root();
        let mut all = vec![args[0], "--instance", root.to_str().unwrap(), "--json"];
        all.extend_from_slice(&args[1..]);
        let (code, out, err) = nvmod(&all);
        (
            code,
            serde_json::from_str(&out).unwrap_or_else(|e| panic!("{e}: {out} / {err}")),
        )
    }

    /// The findings with the given code from the Doctor.
    fn findings(&self, code: &str) -> Vec<Value> {
        let (_, report) = self.json(&["doctor"]);
        report["findings"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|f| f["code"] == code)
            .cloned()
            .collect()
    }
}

/// A profile with three Mods: Alpha (archive and plugin), Beta (loose file and archive) and Gamma (archive).
fn conflict_scratch() -> Scratch {
    let s = Scratch::new();
    s.modlist("+Alpha\r\n+Beta\r\n+Gamma\r\n");
    s.plugins(&["Alpha.esp", "Beta.esp"], &[]);
    s.ini("Gamma - Main.bsa", Some("1"));
    s.bsa(
        "Alpha",
        "Alpha.bsa",
        &["meshes/fake/rock.nif", "textures/fake/a.dds"],
    );
    s.bsa(
        "Beta",
        "Beta.bsa",
        &["meshes/fake/rock.nif", "sound/fake/x.wav"],
    );
    s.file("Beta", "Textures/Fake/A.dds", b"loose");
    s.bsa(
        "Gamma",
        "Gamma - Main.bsa",
        &["meshes/fake/rock.nif", "gamma.txt"],
    );
    s
}

#[test]
fn archives_lists_the_loose_and_archive_losers_with_provider_names() {
    let s = conflict_scratch();
    let (code, r) = s.json(&["conflicts", "--archives"]);
    assert_eq!(code, 0);
    let a = &r["archives"];
    assert_eq!(a["invalidate_older_files"], true);
    let names: Vec<_> = a["archives"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| {
            (
                x["name"].as_str().unwrap(),
                x["loaded_by"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        names,
        [
            ("Gamma - Main.bsa", "SArchiveList #1"),
            ("Alpha.bsa", "plugin Alpha.esp"),
            ("Beta.bsa", "plugin Beta.esp")
        ]
    );
    let conflicts = a["conflicts"].as_array().unwrap();
    let by_path = |p: &str| conflicts.iter().find(|c| c["path"] == p).unwrap();
    // Archive against archive: the first loaded wins.
    let rock = by_path("meshes/fake/rock.nif");
    assert_eq!(rock["rule"], "archive-load-order");
    assert_eq!(rock["winner"]["archive"], "Gamma - Main.bsa");
    assert_eq!(rock["losers"][0]["archive"], "Alpha.bsa");
    assert_eq!(rock["losers"][1]["archive"], "Beta.bsa");
    assert_eq!(rock["unverified"], true);
    // Loose against archive.
    let dds = by_path("textures/fake/a.dds");
    assert_eq!(dds["rule"], "loose-over-archive");
    assert_eq!(dds["winner"], json!({"mod": "Beta", "kind": "loose"}));
    assert_eq!(dds["losers"][0]["mod"], "Alpha");
    // Paths only one archive holds are not conflicts.
    assert_eq!(conflicts.len(), 2);
}

#[test]
fn without_the_flag_the_report_has_no_archive_section() {
    let s = conflict_scratch();
    let (_, r) = s.json(&["conflicts"]);
    assert!(r.get("archives").is_none());
}

#[test]
fn the_archive_json_matches_the_golden_file() {
    let s = conflict_scratch();
    let root = s.root();
    let (_, out, _) = nvmod(&[
        "conflicts",
        "--archives",
        "--instance",
        root.to_str().unwrap(),
        "--json",
    ]);
    golden("conflicts-archives.json", &out);
}

#[test]
fn the_text_output_names_the_rules() {
    let s = conflict_scratch();
    let root = s.root();
    let (code, out, _) = nvmod(&[
        "conflicts",
        "--archives",
        "--instance",
        root.to_str().unwrap(),
    ]);
    assert_eq!(code, 0);
    assert!(
        out.contains("Beta (loose) beats Alpha (Alpha.bsa)"),
        "{out}"
    );
}

fn golden(name: &str, actual: &str) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, actual).unwrap();
    }
    let expected = std::fs::read_to_string(&path).expect("golden file; run with UPDATE_GOLDEN=1");
    assert_eq!(
        actual, expected,
        "run with UPDATE_GOLDEN=1 and review the diff"
    );
}

/// Pairs the positive and the negative findings of one check into a pretty JSON text for a golden file.
fn pair(positive: &[Value], negative: &[Value]) -> String {
    let mut text =
        serde_json::to_string_pretty(&json!({"positive": positive, "negative": negative})).unwrap();
    text.push('\n');
    text
}

#[test]
fn archive_not_listed_has_a_positive_and_a_negative_fixture() {
    let s = Scratch::new();
    s.modlist("+Named\r\n+Stray\r\n+Listed\r\n+Dormant\r\n");
    s.plugins(&["Named.esp"], &["Dormant.esp"]);
    s.ini("Listed.bsa", None);
    s.bsa("Named", "Named.bsa", &["a/a.txt"]);
    s.bsa("Named", "Named - Textures.bsa", &["a/b.txt"]);
    s.bsa("Stray", "Stray.bsa", &["a/c.txt"]);
    s.bsa("Listed", "Listed.bsa", &["a/d.txt"]);
    s.bsa("Dormant", "Dormant.bsa", &["a/e.txt"]);
    let positive = s.findings("archive.not-listed");
    let subjects: Vec<_> = positive
        .iter()
        .map(|f| f["subject"].as_str().unwrap())
        .collect();
    assert_eq!(subjects, ["Stray: Stray.bsa", "Dormant: Dormant.bsa"]);
    assert_eq!(positive[0]["severity"], "warning");
    assert_eq!(positive[1]["severity"], "info");
    // Negative: the same Instance with the stray and the dormant archives removed.
    std::fs::remove_file(s.root().join("mods/Stray/Stray.bsa")).unwrap();
    std::fs::remove_file(s.root().join("mods/Dormant/Dormant.bsa")).unwrap();
    let negative = s.findings("archive.not-listed");
    assert!(negative.is_empty(), "{negative:?}");
    golden("archive-not-listed.json", &pair(&positive, &negative));
}

#[test]
fn archive_listed_missing_has_a_positive_and_a_negative_fixture() {
    let s = Scratch::new();
    s.modlist("+Alpha\r\n");
    s.plugins(&[], &[]);
    s.ini("Alpha.bsa, Absent.bsa", None);
    s.bsa("Alpha", "Alpha.bsa", &["a/a.txt"]);
    let positive = s.findings("archive.listed-missing");
    assert_eq!(positive.len(), 1);
    assert_eq!(positive[0]["subject"], "Absent.bsa");
    s.ini("Alpha.bsa", None);
    let negative = s.findings("archive.listed-missing");
    assert!(negative.is_empty());
    golden("archive-listed-missing.json", &pair(&positive, &negative));
}

#[test]
fn archive_invalidation_has_a_positive_and_a_negative_fixture() {
    let s = Scratch::new();
    s.modlist("");
    s.plugins(&[], &[]);
    s.profile(
        "settings.ini",
        "[General]\r\nAutomaticArchiveInvalidation=true\r\n",
    );
    s.ini("Fallout - Meshes.bsa", None);
    let positive = s.findings("archive.invalidation");
    assert_eq!(positive.len(), 2, "{positive:?}");
    s.ini(
        "Fallout - Invalidation.bsa, Fallout - Meshes.bsa",
        Some("1"),
    );
    let negative = s.findings("archive.invalidation");
    assert!(negative.is_empty(), "{negative:?}");
    // With the profile setting off nothing is asked of the INI.
    s.profile(
        "settings.ini",
        "[General]\r\nAutomaticArchiveInvalidation=false\r\n",
    );
    s.ini("Fallout - Meshes.bsa", None);
    assert!(s.findings("archive.invalidation").is_empty());
    golden("archive-invalidation.json", &pair(&positive, &negative));
}

#[test]
fn archive_corrupt_header_has_a_positive_and_a_negative_fixture() {
    let s = Scratch::new();
    s.modlist("+Alpha\r\n+Beta\r\n");
    s.plugins(&["Alpha.esp", "Beta.esp"], &[]);
    s.ini("", None);
    s.bsa("Alpha", "Alpha.bsa", &["a/a.txt"]);
    let mut bad = BsaBuilder::new().file("a/b.txt", 4).bytes();
    bad.truncate(40);
    s.file("Beta", "Beta.bsa", &bad);
    let positive = s.findings("archive.corrupt-header");
    assert_eq!(positive.len(), 1);
    assert_eq!(positive[0]["subject"], "Beta: Beta.bsa");
    assert_eq!(positive[0]["severity"], "error");
    s.file(
        "Beta",
        "Beta.bsa",
        &BsaBuilder::new().file("a/b.txt", 4).bytes(),
    );
    let negative = s.findings("archive.corrupt-header");
    assert!(negative.is_empty());
    golden("archive-corrupt-header.json", &pair(&positive, &negative));
}

#[test]
fn an_unsupported_archive_version_is_not_a_corrupt_header() {
    let s = Scratch::new();
    s.modlist("+Alpha\r\n+Beta\r\n+Gamma\r\n+Delta\r\n");
    s.plugins(&["Alpha.esp", "Beta.esp", "Gamma.esp", "Delta.esp"], &[]);
    s.ini("", None);
    s.file(
        "Alpha",
        "Alpha.bsa",
        &BsaBuilder::new().file("a/a.txt", 4).version(103).bytes(),
    );
    s.file(
        "Beta",
        "Beta.bsa",
        &BsaBuilder::new().file("a/b.txt", 4).version(9).bytes(),
    );
    s.bsa("Gamma", "Gamma.bsa", &["a/c.txt"]);
    let mut bad = BsaBuilder::new().file("a/d.txt", 4).bytes();
    bad.truncate(40);
    s.file("Delta", "Delta.bsa", &bad);

    let positive = s.findings("archive.unsupported-version");
    assert_eq!(positive.len(), 2, "{positive:?}");
    assert_eq!(positive[0]["subject"], "Alpha: Alpha.bsa");
    assert_eq!(positive[0]["severity"], "warning");
    assert!(
        positive[0]["message"]
            .as_str()
            .unwrap()
            .contains("version 103")
    );
    assert_eq!(positive[1]["subject"], "Beta: Beta.bsa");
    assert_eq!(positive[1]["severity"], "error");
    assert!(
        positive[1]["message"]
            .as_str()
            .unwrap()
            .contains("version 9")
    );
    // Only the truly damaged header is a corrupt header; version 104 raises neither.
    let corrupt = s.findings("archive.corrupt-header");
    assert_eq!(corrupt.len(), 1, "{corrupt:?}");
    assert_eq!(corrupt[0]["subject"], "Delta: Delta.bsa");
    golden("archive-unsupported-version.json", &pair(&positive, &[]));
}
