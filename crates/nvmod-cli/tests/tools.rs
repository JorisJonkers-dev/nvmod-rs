// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod tools list|verify|fetch|trust|run`, run as a program. The release server is a local fake, `gh` is a script
//! that records how it was called, and the tool is a tiny shell script; the lock file is written by each test and
//! points at the fake server. No network, no game data.
#![cfg(unix)]
use nvmod_testkit::{BaseBuilder, FakeServer, Reply, TempDir, write_script};
use nvmod_tools::sha256_hex;
use serde_json::Value;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

fn nvmod(args: &[&str]) -> (i32, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_nvmod"))
        .args(args)
        .env_remove("NVMOD_TOOLS_DIR")
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8(out.stdout).unwrap(),
        String::from_utf8(out.stderr).unwrap(),
    )
}

fn platform() -> String {
    nvmod_tools::platform()
}

fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        if entry.path().is_dir() {
            walk(&entry.path(), files);
        } else {
            files.push(entry.path());
        }
    }
}

struct Rig {
    dir: TempDir,
    /// The bytes the fake server serves as the tool.
    tool: Vec<u8>,
}

impl Rig {
    fn new() -> Rig {
        let dir = TempDir::new("tools");
        let log = dir.path().join("tool-ran.log");
        let tool = format!(
            "#!/bin/sh\necho ran >> {}\necho '{{\"worlds\":[]}}'\n",
            log.display()
        )
        .into_bytes();
        Rig { dir, tool }
    }

    fn store(&self) -> String {
        self.dir.path().join("store").to_str().unwrap().to_owned()
    }

    fn lock_path(&self) -> PathBuf {
        self.dir.path().join("tools.lock.toml")
    }

    fn lock_arg(&self) -> String {
        self.lock_path().to_str().unwrap().to_owned()
    }

    /// A server that serves `body` at `/tool` and 404 elsewhere.
    fn server(body: &[u8]) -> FakeServer {
        let body = body.to_vec();
        FakeServer::start(move |path, _| {
            if path == "tool" {
                Reply::Body(body.clone())
            } else {
                Reply::Status(404)
            }
        })
    }

    /// A lock pinning `lodwright-rs` at `version` to the bytes `pinned`, served from `server`.
    fn write_lock(&self, server: &FakeServer, version: &str, pinned: &[u8]) {
        let text = format!(
            "schema_version = 1\n\n[[tool]]\nname = \"lodwright-rs\"\nversion = \"{version}\"\ncontracts = {{ lodwright-plan = 1 }}\n\n\
             [tool.attestation]\nrepository = \"o/lodwright-rs\"\nworkflow = \".github/workflows/release.yml\"\n\n\
             [tool.assets.{}]\nurl = \"{}/tool\"\nsha256 = \"{}\"\n\n\
             [[tool]]\nname = \"loot-rs\"\nversion = \"0.1.0\"\nkind = \"linked\"\n[tool.source]\ngit = \"https://github.com/o/loot-rs\"\ntag = \"v0.1.0\"\n",
            platform(),
            server.url(),
            sha256_hex(pinned)
        );
        std::fs::write(self.lock_path(), text).unwrap();
    }

    /// A third-party tool with no pinned checksum.
    fn write_third_party_lock(&self, server: &FakeServer, version: &str) {
        let text = format!(
            "schema_version = 1\n\n[[tool]]\nname = \"ttw-installer\"\nversion = \"{version}\"\nkind = \"third-party\"\n\n\
             [tool.assets.{}]\nurl = \"{}/tool\"\n",
            platform(),
            server.url()
        );
        std::fs::write(self.lock_path(), text).unwrap();
    }

    fn gh(&self, code: i32) -> String {
        let log = self.dir.path().join("gh.log");
        write_script(
            self.dir.path(),
            "gh",
            &format!("echo \"$@\" >> {}\nexit {code}", log.display()),
        )
        .to_str()
        .unwrap()
        .to_owned()
    }

    fn gh_calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("gh.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn tool_runs(&self) -> usize {
        std::fs::read_to_string(self.dir.path().join("tool-ran.log"))
            .unwrap_or_default()
            .lines()
            .count()
    }

    fn run(&self, verb: &str, extra: &[&str]) -> (i32, String, String) {
        let (lock, store) = (self.lock_arg(), self.store());
        let mut args = vec![
            "tools",
            verb,
            "--lock",
            &lock,
            "--store",
            &store,
            "--retry-delay-ms",
            "0",
        ];
        args.extend_from_slice(extra);
        nvmod(&args)
    }

    fn stored(&self) -> Vec<PathBuf> {
        let mut files = Vec::new();
        walk(&self.dir.path().join("store"), &mut files);
        files
    }

    fn tool_path(&self, bytes: &[u8]) -> PathBuf {
        self.dir
            .path()
            .join("store")
            .join(sha256_hex(bytes))
            .join("lodwright-rs")
    }
}

#[test]
fn a_pinned_tool_is_fetched_verified_attested_and_stored_by_hash() {
    let rig = Rig::new();
    let server = Rig::server(&rig.tool);
    rig.write_lock(&server, "1.0.0", &rig.tool);
    let gh = rig.gh(0);
    let (code, out, err) = rig.run("fetch", &["lodwright-rs", "--gh", &gh]);
    assert_eq!(code, 0, "{out} {err}");
    let stored = rig.tool_path(&rig.tool);
    assert_eq!(std::fs::read(&stored).unwrap(), rig.tool);
    assert_eq!(
        std::fs::metadata(&stored).unwrap().permissions().mode() & 0o111,
        0o111
    );
    assert_eq!(rig.stored(), [stored]);
    // Fetched from the lock's URL and nowhere else.
    assert_eq!(server.requests(), ["tool"]);
    let calls = rig.gh_calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(
        calls[0].starts_with("attestation verify ")
            && calls[0].contains("--repo o/lodwright-rs")
            && calls[0].contains("--signer-workflow o/lodwright-rs/.github/workflows/release.yml"),
        "{calls:?}"
    );
    assert!(
        out.contains("SHA-256 matches") && out.contains("attestation names o/lodwright-rs"),
        "{out}"
    );
    // A second fetch has the file already, checks it again and does not go to the network.
    let (code, out, _) = rig.run("fetch", &["lodwright-rs", "--gh", &gh]);
    assert_eq!(code, 0);
    assert!(out.contains("already stored"), "{out}");
    assert_eq!(server.requests(), ["tool"]);
}

#[test]
fn a_checksum_mismatch_exits_3_installs_nothing_and_never_asks_for_the_attestation() {
    let rig = Rig::new();
    let server = Rig::server(b"not the pinned bytes");
    rig.write_lock(&server, "1.0.0", &rig.tool);
    let gh = rig.gh(0);
    let (code, _, err) = rig.run("fetch", &["lodwright-rs", "--gh", &gh]);
    assert_eq!(code, 3, "{err}");
    assert!(
        err.contains("checksum mismatch") && err.contains("nothing was installed"),
        "{err}"
    );
    assert!(rig.stored().is_empty(), "{:?}", rig.stored());
    assert_eq!(rig.gh_calls().len(), 0);
}

#[test]
fn a_failed_attestation_exits_3_and_leaves_no_file_behind() {
    let rig = Rig::new();
    let server = Rig::server(&rig.tool);
    rig.write_lock(&server, "1.0.0", &rig.tool);
    let gh = rig.gh(1);
    let (code, _, err) = rig.run("fetch", &["lodwright-rs", "--gh", &gh]);
    assert_eq!(code, 3, "{err}");
    assert!(err.contains("attestation"), "{err}");
    assert!(rig.stored().is_empty(), "{:?}", rig.stored());
    assert_eq!(rig.gh_calls().len(), 1);
    // gh not installed at all: fails closed, says so.
    let missing = rig.dir.path().join("no-such-gh");
    let (code, _, err) = rig.run(
        "fetch",
        &["lodwright-rs", "--gh", missing.to_str().unwrap()],
    );
    assert_eq!(code, 3, "{err}");
    assert!(err.contains("GitHub CLI"), "{err}");
    assert_eq!(rig.stored().len(), 0);
}

#[test]
fn a_malformed_response_is_not_retried_and_a_network_failure_is_retried_three_times() {
    let rig = Rig::new();
    let server = FakeServer::start(|_, _| Reply::Garbage);
    rig.write_lock(&server, "1.0.0", &rig.tool);
    let gh = rig.gh(0);
    let (code, _, err) = rig.run("fetch", &["lodwright-rs", "--gh", &gh]);
    assert_eq!(code, 3, "{err}");
    assert!(err.contains("malformed"), "{err}");
    assert_eq!(server.requests().len(), 1);

    let server = FakeServer::start(|_, _| Reply::Status(500));
    rig.write_lock(&server, "1.0.0", &rig.tool);
    let (code, _, err) = rig.run("fetch", &["lodwright-rs", "--gh", &gh]);
    assert_eq!(code, 4, "{err}");
    assert_eq!(server.requests().len(), 4, "one try and three retries");
    assert_eq!(rig.stored().len(), 0);
}

#[test]
fn a_pin_bump_never_overwrites_the_tool_in_use() {
    let rig = Rig::new();
    let gh = rig.gh(0);
    let server = Rig::server(&rig.tool);
    rig.write_lock(&server, "1.0.0", &rig.tool);
    assert_eq!(rig.run("fetch", &["lodwright-rs", "--gh", &gh]).0, 0);

    let newer = b"#!/bin/sh\necho newer\n".to_vec();
    let server2 = Rig::server(&newer);
    rig.write_lock(&server2, "1.1.0", &newer);
    assert_eq!(rig.run("fetch", &["lodwright-rs", "--gh", &gh]).0, 0);
    assert_eq!(std::fs::read(rig.tool_path(&rig.tool)).unwrap(), rig.tool);
    assert_eq!(std::fs::read(rig.tool_path(&newer)).unwrap(), newer);
    assert_eq!(rig.stored().len(), 2);
}

#[test]
fn list_shows_each_tool_with_its_pinned_version_and_state_and_verify_needs_no_network() {
    let rig = Rig::new();
    let server = Rig::server(&rig.tool);
    rig.write_lock(&server, "1.0.0", &rig.tool);
    let gh = rig.gh(0);
    let (code, out, _) = rig.run("list", &[]);
    assert_eq!(code, 0);
    assert!(out.contains("lodwright-rs 1.0.0 not fetched"), "{out}");
    assert!(out.contains("loot-rs 0.1.0 linked (tag v0.1.0)"), "{out}");

    assert_eq!(rig.run("fetch", &["lodwright-rs", "--gh", &gh]).0, 0);
    let (_, out, _) = rig.run("list", &[]);
    assert!(out.contains("lodwright-rs 1.0.0 verified"), "{out}");
    let (_, json, _) = rig.run("list", &["--json"]);
    let rows: Value = serde_json::from_str(&json).unwrap();
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "lodwright-rs")
        .unwrap();
    assert_eq!(
        (row["version"].as_str(), row["state"].as_str()),
        (Some("1.0.0"), Some("verified"))
    );

    // `verify` re-checks the stored files without the network: the server is gone, and so is curl.
    drop(server);
    let (code, out, err) = rig.run("verify", &["--curl", "/nonexistent/curl"]);
    assert_eq!(code, 0, "{out} {err}");
    assert!(
        out.contains("lodwright-rs") && out.contains("verified"),
        "{out}"
    );

    // A file changed in the store is caught by both.
    std::fs::write(rig.tool_path(&rig.tool), b"tampered").unwrap();
    let (code, _, err) = rig.run("verify", &[]);
    assert_eq!(code, 3, "{err}");
    assert!(
        err.contains("lodwright-rs") && err.contains("corrupt"),
        "{err}"
    );
    let (_, out, _) = rig.run("list", &[]);
    assert!(out.contains("lodwright-rs 1.0.0 corrupt"), "{out}");
}

#[test]
fn the_runner_rechecks_the_checksum_on_every_call_and_exits_3_when_the_binary_changed() {
    let rig = Rig::new();
    let server = Rig::server(&rig.tool);
    rig.write_lock(&server, "1.0.0", &rig.tool);
    let gh = rig.gh(0);
    assert_eq!(rig.run("fetch", &["lodwright-rs", "--gh", &gh]).0, 0);
    let (code, out, err) = rig.run("run", &["lodwright-rs", "--", "plan"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("worlds"), "{out}");
    assert_eq!(rig.tool_runs(), 1);

    std::fs::write(
        rig.tool_path(&rig.tool),
        b"#!/bin/sh\necho evil >> /dev/null\n",
    )
    .unwrap();
    let (code, _, err) = rig.run("run", &["lodwright-rs", "--", "plan"]);
    assert_eq!(code, 3, "{err}");
    assert!(err.contains("could not be verified"), "{err}");
    assert_eq!(rig.tool_runs(), 1, "the changed binary was not run");
}

#[test]
fn a_tool_that_is_not_fetched_or_not_in_the_lock_is_refused() {
    let rig = Rig::new();
    let server = Rig::server(&rig.tool);
    rig.write_lock(&server, "1.0.0", &rig.tool);
    let (code, _, err) = rig.run("run", &["lodwright-rs", "--", "plan"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("nvmod tools fetch"), "{err}");
    let (code, _, err) = rig.run("fetch", &["nope"]);
    assert_eq!(code, 2, "{err}");
    let (code, _, err) = rig.run("fetch", &["loot-rs"]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("linked"), "{err}");
}

#[test]
fn a_third_party_installer_without_a_pinned_checksum_is_refused_until_the_owner_trusts_it() {
    let rig = Rig::new();
    let server = Rig::server(&rig.tool);
    rig.write_third_party_lock(&server, "3.4");
    let inst = TempDir::new("tools-inst");
    let base = TempDir::new("tools-base");
    BaseBuilder::steam().write(base.path());
    let root = inst.path().join("i");
    let (code, _, err) = nvmod(&[
        "init",
        "--base",
        base.path().to_str().unwrap(),
        "--instance",
        root.to_str().unwrap(),
    ]);
    assert_eq!(code, 0, "{err}");
    let root = root.to_str().unwrap();
    let gh = rig.gh(0);

    // Refused, and nothing was asked of the network.
    let (code, _, err) = rig.run("fetch", &["ttw-installer", "--instance", root, "--gh", &gh]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("nvmod tools trust"), "{err}");
    assert_eq!(server.requests().len(), 0);

    // Without the checksum the owner saw, `trust` only shows the checksum and records nothing.
    let (code, out, err) = rig.run("trust", &["ttw-installer", "3.4", "--instance", root]);
    assert_eq!(code, 2, "{err}");
    let expected = sha256_hex(&rig.tool);
    assert!(
        out.contains(&expected) || err.contains(&expected),
        "{out}{err}"
    );
    assert!(!Path::new(root).join("tools.trust.toml").exists());

    // A checksum that is not the download's is a verification failure.
    let wrong = "0".repeat(64);
    let (code, _, err) = rig.run(
        "trust",
        &[
            "ttw-installer",
            "3.4",
            "--instance",
            root,
            "--sha256",
            &wrong,
        ],
    );
    assert_eq!(code, 3, "{err}");
    assert!(!Path::new(root).join("tools.trust.toml").exists());

    // The confirmed one is recorded, and the fetch then goes through (no attestation exists for such a tool).
    let (code, _, err) = rig.run(
        "trust",
        &[
            "ttw-installer",
            "3.4",
            "--instance",
            root,
            "--sha256",
            &expected,
        ],
    );
    assert_eq!(code, 0, "{err}");
    let trust = std::fs::read_to_string(Path::new(root).join("tools.trust.toml")).unwrap();
    assert!(
        trust.contains("ttw-installer") && trust.contains("3.4") && trust.contains(&expected),
        "{trust}"
    );
    let (code, out, err) = rig.run("fetch", &["ttw-installer", "--instance", root, "--gh", &gh]);
    assert_eq!(code, 0, "{out} {err}");
    assert!(out.contains("no attestation pinned"), "{out}");
    assert_eq!(rig.gh_calls().len(), 0);

    // Another version needs its own confirmation.
    rig.write_third_party_lock(&server, "3.5");
    let (code, _, err) = rig.run("fetch", &["ttw-installer", "--instance", root, "--gh", &gh]);
    assert_eq!(code, 2, "{err}");
}

#[test]
fn trusting_a_tool_that_has_a_pinned_checksum_is_refused() {
    let rig = Rig::new();
    let server = Rig::server(&rig.tool);
    rig.write_lock(&server, "1.0.0", &rig.tool);
    let inst = TempDir::new("tools-inst2");
    let base = TempDir::new("tools-base2");
    BaseBuilder::steam().write(base.path());
    let root = inst.path().join("i");
    assert_eq!(
        nvmod(&[
            "init",
            "--base",
            base.path().to_str().unwrap(),
            "--instance",
            root.to_str().unwrap()
        ])
        .0,
        0
    );
    let (code, _, err) = rig.run(
        "trust",
        &[
            "lodwright-rs",
            "1.0.0",
            "--instance",
            root.to_str().unwrap(),
        ],
    );
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("pinned"), "{err}");
}

#[test]
fn a_lock_that_breaks_the_rules_is_refused_with_the_reason() {
    let rig = Rig::new();
    std::fs::write(
        rig.lock_path(),
        format!(
            "schema_version = 1\n[[tool]]\nname = \"x\"\nversion = \"1\"\n[tool.assets.{}]\nurl = \"https://example.com/x\"\n",
            platform()
        ),
    )
    .unwrap();
    let (code, _, err) = rig.run("list", &[]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("no sha256"), "{err}");
}

#[test]
fn the_shipped_lock_lists_loot_rs_as_linked() {
    let (code, out, err) = nvmod(&["tools", "list"]);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("loot-rs 0.2.0 linked (tag v0.2.0)"), "{out}");
}

#[test]
fn usage_errors_exit_with_five() {
    for args in [
        vec!["tools"],
        vec!["tools", "nonsense"],
        vec!["tools", "fetch"],
        vec!["tools", "trust", "only-one"],
        vec!["tools", "list", "--nope"],
        vec!["tools", "run", "lodwright-rs", "no-separator"],
    ] {
        assert_eq!(nvmod(&args).0, 5, "{args:?}");
    }
}

#[test]
fn contracts_lists_what_this_nvmod_accepts() {
    let (code, out, err) = nvmod(&["tools", "contracts"]);
    assert_eq!(code, 0, "{err}");
    assert!(
        out.contains("loot-sort (loot-rs) current v1 accepts v1"),
        "{out}"
    );
    assert!(out.contains("lodwright-plan (lodwright-rs)"), "{out}");
    let (code, out, _) = nvmod(&["tools", "contracts", "--json"]);
    assert_eq!(code, 0);
    let json: Value = serde_json::from_str(&out).unwrap();
    assert!(json.as_array().unwrap().iter().any(|r| {
        r["contract"] == "ttw-installer-result" && r["accepted"] == serde_json::json!([1])
    }));
}
