// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod sort --fetch-masterlist`: the masterlist comes from the source pinned in `tools.lock.toml`, fetched
//! from a local fake server with its checksum verified; a missing pin is a refusal that says so.

use nvmod_testkit::{BaseBuilder, FakeServer, PluginBuilder, Reply, TempDir, write_file};
use nvmod_tools::sha256_hex;
use std::path::PathBuf;
use std::process::Command;

const MASTERLIST: &[u8] = b"plugins: []\n";
const TOOL: &str = "loot-masterlist-falloutnv";

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
    _base: TempDir,
    dir: TempDir,
}

impl Rig {
    fn new() -> Rig {
        let base = TempDir::new("ml-base");
        BaseBuilder::steam().write(base.path());
        write_file(
            base.path(),
            "nvse_1_4.dll",
            &nvmod_testkit::PeBuilder::dll().build(),
        );
        write_file(
            base.path(),
            "Data/FalloutNV.esm",
            &PluginBuilder::master().build(),
        );
        let dir = TempDir::new("ml-inst");
        let root = dir.path().join("fnv");
        let (code, _, err) = nvmod(&[
            "init",
            "--base",
            base.path().to_str().unwrap(),
            "--instance",
            root.to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "{err}");
        write_file(&root, "profiles/Default/modlist.txt", b"# header\r\n");
        write_file(
            &root,
            "profiles/Default/plugins.txt",
            b"# header\r\nFalloutNV.esm\r\n",
        );
        write_file(
            &root,
            "profiles/Default/loadorder.txt",
            b"# header\r\nFalloutNV.esm\r\n",
        );
        let (code, _, err) = nvmod(&["stage", "--instance", root.to_str().unwrap()]);
        assert_eq!(code, 0, "{err}");
        Rig { _base: base, dir }
    }

    fn lock(&self, server: Option<&FakeServer>, pinned: &[u8]) -> String {
        let text = match server {
            Some(s) => format!(
                "schema_version = 1\n\n[[tool]]\nname = \"{TOOL}\"\nversion = \"v1\"\nkind = \"third-party\"\n\n\
                 [tool.assets.any]\nurl = \"{}/masterlist.yaml\"\nsha256 = \"{}\"\n",
                s.url(),
                sha256_hex(pinned)
            ),
            None => "schema_version = 1\n".to_owned(),
        };
        let path = self.dir.path().join("tools.lock.toml");
        std::fs::write(&path, text).unwrap();
        path.to_str().unwrap().to_owned()
    }

    fn sort(&self, lock: &str, extra: &[&str]) -> (i32, String, String) {
        let root: PathBuf = self.dir.path().join("fnv");
        let store = self.dir.path().join("store");
        let mut args = vec![
            "sort",
            "--instance",
            root.to_str().unwrap(),
            "--fetch-masterlist",
            "--lock",
            lock,
            "--store",
            store.to_str().unwrap(),
            "--retry-delay-ms",
            "0",
        ];
        args.extend_from_slice(extra);
        nvmod(&args)
    }
}

fn server(body: &[u8]) -> FakeServer {
    let body = body.to_vec();
    FakeServer::start(move |path, _| {
        if path == "masterlist.yaml" {
            Reply::Body(body.clone())
        } else {
            Reply::Status(404)
        }
    })
}

#[test]
fn the_pinned_masterlist_is_fetched_verified_stored_and_used() {
    let rig = Rig::new();
    let server = server(MASTERLIST);
    let lock = rig.lock(Some(&server), MASTERLIST);
    let (code, out, err) = rig.sort(&lock, &["--json"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\"order\""), "{out}");
    assert_eq!(server.count("masterlist.yaml"), 1);
    // The second run finds the verified file in the store and does not go to the network.
    let (code, _, err) = rig.sort(&lock, &["--json"]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(server.count("masterlist.yaml"), 1);
}

#[test]
fn a_missing_pin_is_a_refusal_that_says_so() {
    let rig = Rig::new();
    let lock = rig.lock(None, b"");
    let (code, _, err) = rig.sort(&lock, &[]);
    assert_eq!(code, 2);
    assert!(err.contains(TOOL) && err.contains("pins no tool"), "{err}");
}

#[test]
fn a_masterlist_that_does_not_match_its_checksum_exits_3_and_is_not_used() {
    let rig = Rig::new();
    let server = server(b"plugins: []\n# tampered\n");
    let lock = rig.lock(Some(&server), MASTERLIST);
    let (code, _, err) = rig.sort(&lock, &[]);
    assert_eq!(code, 3, "{err}");
    assert!(
        !rig.dir.path().join("store").exists()
            || std::fs::read_dir(rig.dir.path().join("store"))
                .unwrap()
                .count()
                == 0
    );
}

#[test]
fn a_failed_fetch_exits_4_and_one_masterlist_source_is_required() {
    let rig = Rig::new();
    let server = FakeServer::start(|_, _| Reply::Status(404));
    let lock = rig.lock(Some(&server), MASTERLIST);
    assert_eq!(rig.sort(&lock, &[]).0, 4);
    // Both sources, or neither, is a usage error.
    let root = rig.dir.path().join("fnv");
    let r = root.to_str().unwrap();
    assert_eq!(nvmod(&["sort", "--instance", r]).0, 5);
    assert_eq!(
        nvmod(&[
            "sort",
            "--instance",
            r,
            "--masterlist",
            "x.yaml",
            "--fetch-masterlist"
        ])
        .0,
        5
    );
}

#[test]
fn the_shipped_lock_pins_the_masterlist_by_commit_and_checksum() {
    let lock = nvmod_tools::builtin_lock();
    let entry = lock
        .tool(TOOL)
        .expect("the shipped lock pins the masterlist");
    let asset = entry.asset_here().expect("an asset for every platform");
    assert!(
        asset
            .url
            .starts_with("https://raw.githubusercontent.com/loot/falloutnv/")
    );
    assert!(asset.sha256.as_deref().is_some_and(|h| h.len() == 64));
    // A commit, not a branch name, so the bytes never change under the checksum.
    let segment = asset.url.split('/').nth(5).unwrap();
    assert_eq!(segment.len(), 40, "{}", asset.url);
}
