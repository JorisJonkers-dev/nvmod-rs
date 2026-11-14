// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod install-nv-rs`, run as a program against a scratch Instance made by `nvmod init` from a synthetic base
//! copy. The nv-rs source is a tiny cargo workspace written by the test, the release is a fake local one, and
//! `gh` and `curl` are shell scripts that record how they were called, and the fake `nv-viewer` answers `--check`. No network, no game.

// Tests start the built `nvmod` binary itself; the runner rule (ADR-0005) is for the product code.
#![allow(clippy::disallowed_methods)]

use nvmod_install::sha256_hex;
use nvmod_testkit::{
    BaseBuilder, FAKE_ASSET, FAKE_BUILT_BANNER, FAKE_CHECK_REFUSED, FAKE_RELEASE_BINARY, FakeNvRs,
    FakeRelease, FakeServer, Reply, TempDir, snapshot, write_script,
};
use std::path::{Path, PathBuf};
use std::process::Command;

const CARGO: &str = env!("CARGO");

fn nvmod(args: &[&str]) -> (i32, String, String) {
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

/// A base copy, an Instance made from it, a nv-rs source, and a folder for scripts and logs.
struct Rig {
    base: TempDir,
    instance: TempDir,
    source: TempDir,
    tools: TempDir,
}

impl Rig {
    fn new() -> Rig {
        let rig = Rig {
            base: TempDir::new("nvrs-base"),
            instance: TempDir::new("nvrs-inst"),
            source: TempDir::new("nvrs-src"),
            tools: TempDir::new("nvrs-tools"),
        };
        BaseBuilder::steam().write(rig.base.path());
        let root = rig.root();
        let (code, _, err) = nvmod(&[
            "init",
            "--base",
            rig.base.path().to_str().unwrap(),
            "--instance",
            root.to_str().unwrap(),
        ]);
        assert_eq!(code, 0, "{err}");
        FakeNvRs::new().write(rig.source.path());
        rig
    }

    fn root(&self) -> PathBuf {
        self.instance.path().join("i")
    }

    fn install(&self, extra: &[&str]) -> (i32, String, String) {
        let root = self.root();
        let mut args = vec!["install-nv-rs", "--instance", root.to_str().unwrap()];
        args.extend_from_slice(extra);
        nvmod(&args)
    }

    /// A `gh` that records its arguments and exits with `code`.
    fn gh(&self, code: i32) -> String {
        let log = self.tools.path().join("gh.log");
        let body = format!("echo \"$@\" >> {}\nexit {code}", log.display());
        write_script(self.tools.path(), "gh", &body)
            .to_str()
            .unwrap()
            .to_owned()
    }

    fn gh_calls(&self) -> Vec<String> {
        log_lines(&self.tools.path().join("gh.log"))
    }

    fn release(&self, release: &FakeRelease) -> String {
        let dir = self.tools.path().join("release");
        std::fs::create_dir_all(&dir).unwrap();
        release.write(&dir, sha256_hex);
        format!("file://{}", dir.display())
    }

    fn bin(&self) -> PathBuf {
        self.root().join("nv-rs/bin/nv-viewer")
    }

    fn source(&self) -> &str {
        self.source.path().to_str().unwrap()
    }
}

fn log_lines(path: &Path) -> Vec<String> {
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(str::to_owned)
        .collect()
}

fn absent_cargo(rig: &Rig) -> String {
    rig.tools
        .path()
        .join("no-such-cargo")
        .to_str()
        .unwrap()
        .to_owned()
}

#[test]
fn with_a_toolchain_nv_rs_is_built_from_the_fake_workspace() {
    let rig = Rig::new();
    let (code, out, err) = rig.install(&["--source", rig.source(), "--cargo", CARGO]);
    assert_eq!(code, 0, "{out}\n{err}");
    let run = Command::new(rig.bin()).output().unwrap();
    assert!(
        String::from_utf8(run.stdout)
            .unwrap()
            .contains(FAKE_BUILT_BANNER)
    );
    let record = std::fs::read_to_string(rig.root().join("nv-rs/install.json")).unwrap();
    let record: serde_json::Value = serde_json::from_str(&record).unwrap();
    assert_eq!(record["method"], "source");
    assert_eq!(
        record["sha256"],
        sha256_hex(&std::fs::read(rig.bin()).unwrap())
    );
    assert_eq!(record["check"]["ran"], true);
    assert_eq!(record["check"]["exit_code"], 0);
}

#[test]
fn the_toolchain_can_come_from_the_environment() {
    let rig = Rig::new();
    let root = rig.root();
    let out = Command::new(env!("CARGO_BIN_EXE_nvmod"))
        .args([
            "install-nv-rs",
            "--instance",
            root.to_str().unwrap(),
            "--source",
            rig.source(),
        ])
        .env("NVMOD_CARGO", CARGO)
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(rig.bin().is_file());
}

#[test]
fn the_check_is_run_against_the_data_folder_of_the_base_copy_and_reported() {
    let rig = Rig::new();
    let (code, out, err) = rig.install(&["--source", rig.source(), "--cargo", CARGO]);
    assert_eq!(code, 0, "{out}\n{err}");
    let data = std::fs::canonicalize(rig.base.path()).unwrap().join("Data");
    assert!(
        out.contains(&format!("fake check passed {} --check", data.display())),
        "{out}"
    );
    assert!(out.contains("check passed"), "{out}");
    let record = std::fs::read_to_string(rig.root().join("nv-rs/install.json")).unwrap();
    assert!(record.contains("\"command\": \"nv-viewer\""), "{record}");
    assert!(!record.contains("play_patch"), "{record}");
}

#[test]
fn the_play_patch_option_is_gone() {
    let rig = Rig::new();
    let before = snapshot(&rig.root());
    let (code, ..) = rig.install(&[
        "--source",
        rig.source(),
        "--cargo",
        CARGO,
        "--play-patch",
        "x",
    ]);
    assert_eq!(code, 5);
    assert_eq!(snapshot(&rig.root()), before);
}

#[test]
fn a_failing_check_of_a_source_build_exits_4_shows_its_output_and_rolls_back() {
    let rig = Rig::new();
    FakeNvRs::new().failing_check().write(rig.source.path());
    let before = snapshot(&rig.root());
    let (code, _, err) = rig.install(&["--source", rig.source(), "--cargo", CARGO]);
    assert_eq!(code, 4, "{err}");
    assert!(err.contains(FAKE_CHECK_REFUSED), "{err}");
    assert!(err.contains("check"), "{err}");
    assert_eq!(snapshot(&rig.root()), before);
}

#[test]
fn a_failing_check_of_a_release_exits_4_and_rolls_back() {
    let rig = Rig::new();
    let release = rig.release(&FakeRelease::new().failing_check());
    let (gh, cargo) = (rig.gh(0), absent_cargo(&rig));
    let before = snapshot(&rig.root());
    let (code, _, err) = rig.install(&["--cargo", &cargo, "--release-url", &release, "--gh", &gh]);
    assert_eq!(code, 4, "{err}");
    assert!(err.contains(FAKE_CHECK_REFUSED), "{err}");
    assert_eq!(snapshot(&rig.root()), before);
}

const MANIFEST: &str = r#"{"schema_version":1,"version":"0.0.0-synthetic",
  "executable":{"file_name":"FalloutNV.exe","game_version":"1.4.0.525","hash_algorithm":"fnv1a-64","sections":[{"name":".text","virtual_address":"0x00001000","virtual_size":"0x00200000","hash":"0123456789abcdef"}]},
  "upstream_base_commit":"0123456789abcdef0123456789abcdef01234567","plugins_txt_ordering_switch":false,
  "play_patch":{"standalone_step":false,"command":null,"arguments":[]},
  "installation_check":{"command":"nv-viewer","arguments":["<data folder>","--check","--from-manifest"]}}"#;

#[test]
fn a_release_manifest_gives_the_check_command() {
    let rig = Rig::new();
    let release = rig.release(&FakeRelease::new().with_manifest(MANIFEST));
    let (gh, cargo) = (rig.gh(0), absent_cargo(&rig));
    let (code, out, err) =
        rig.install(&["--cargo", &cargo, "--release-url", &release, "--gh", &gh]);
    assert_eq!(code, 0, "{out}\n{err}");
    let data = std::fs::canonicalize(rig.base.path()).unwrap().join("Data");
    assert!(
        out.contains(&format!("{} --check --from-manifest", data.display())),
        "{out}"
    );
}

#[test]
fn a_release_without_a_manifest_uses_the_default_check() {
    let rig = Rig::new();
    let release = rig.release(&FakeRelease::new());
    let (gh, cargo) = (rig.gh(0), absent_cargo(&rig));
    let (code, out, err) =
        rig.install(&["--cargo", &cargo, "--release-url", &release, "--gh", &gh]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert!(!out.contains("--from-manifest"), "{out}");
    assert!(out.contains("--check"), "{out}");
}

#[test]
fn a_manifest_that_names_another_program_or_is_unreadable_installs_nothing() {
    for manifest in [
        r#"{"installation_check":{"command":"/bin/sh","arguments":["-c","true"]}}"#,
        r#"{"installation_check":{"command":"nv-viewer","arguments":"--check"}}"#,
        "not json",
    ] {
        let rig = Rig::new();
        let release = rig.release(&FakeRelease::new().with_manifest(manifest));
        let (gh, cargo) = (rig.gh(0), absent_cargo(&rig));
        let before = snapshot(&rig.root());
        let (code, _, err) =
            rig.install(&["--cargo", &cargo, "--release-url", &release, "--gh", &gh]);
        assert!(code == 3 || code == 4, "{code} {err}");
        assert!(err.contains("release-manifest.json"), "{err}");
        assert_eq!(snapshot(&rig.root()), before);
    }
}

#[test]
fn a_failed_build_exits_4_with_the_log_and_leaves_the_instance_alone() {
    let rig = Rig::new();
    FakeNvRs::new().failing_build().write(rig.source.path());
    let release = rig.release(&FakeRelease::new());
    let gh = rig.gh(0);
    let before = snapshot(&rig.root());
    let (code, _, err) = rig.install(&[
        "--source",
        rig.source(),
        "--cargo",
        CARGO,
        "--release-url",
        &release,
        "--gh",
        &gh,
    ]);
    assert_eq!(code, 4, "{err}");
    assert!(
        err.contains("fake-compile-error") || err.contains("could not compile"),
        "{err}"
    );
    assert_eq!(snapshot(&rig.root()), before);
    assert!(
        rig.gh_calls().is_empty(),
        "no fallback to the release binary"
    );
}

#[test]
fn without_a_toolchain_the_release_is_fetched_verified_and_installed() {
    let rig = Rig::new();
    let release = rig.release(&FakeRelease::new());
    let gh = rig.gh(0);
    let cargo = absent_cargo(&rig);
    let (code, out, err) =
        rig.install(&["--cargo", &cargo, "--release-url", &release, "--gh", &gh]);
    assert_eq!(code, 0, "{out}\n{err}");
    assert_eq!(std::fs::read(rig.bin()).unwrap(), FAKE_RELEASE_BINARY);
    let calls = rig.gh_calls();
    assert_eq!(calls.len(), 1);
    assert!(calls[0].starts_with("attestation verify "), "{calls:?}");
    assert!(
        calls[0].ends_with(&format!("{FAKE_ASSET} --repo JorisJonkers-dev/nv-rs")),
        "{calls:?}"
    );
    let record = std::fs::read_to_string(rig.root().join("nv-rs/install.json")).unwrap();
    assert!(record.contains("\"method\": \"release\""), "{record}");
    assert!(!rig.root().join("nv-rs/bin/play-patch").exists());
}

#[test]
fn the_capabilities_file_of_a_release_is_installed_next_to_the_binaries() {
    let rig = Rig::new();
    let release = rig.release(&FakeRelease::new().with_capabilities());
    let gh = rig.gh(0);
    let (code, out, err) = rig.install(&["--release", "--release-url", &release, "--gh", &gh]);
    assert_eq!(code, 0, "{out}\n{err}");
    let text = std::fs::read_to_string(rig.root().join("nv-rs/capabilities.json")).unwrap();
    assert!(text.contains("\"synthetic\""), "{text}");
    // A release without one installs without one; the Doctor then warns that no coverage is known.
    let rig = Rig::new();
    let release = rig.release(&FakeRelease::new());
    let gh = rig.gh(0);
    let (code, ..) = rig.install(&["--release", "--release-url", &release, "--gh", &gh]);
    assert_eq!(code, 0);
    assert!(!rig.root().join("nv-rs/capabilities.json").exists());
}

#[test]
fn the_capabilities_file_of_the_source_tree_is_installed_when_it_has_one() {
    let rig = Rig::new();
    nvmod_testkit::write_file(
        rig.source.path(),
        ".fork/capabilities.json",
        nvmod_testkit::capabilities_json(&[("synthetic", "Synthetic", 1, 2)]).as_bytes(),
    );
    let (code, out, err) = rig.install(&["--source", rig.source(), "--cargo", CARGO]);
    assert_eq!(code, 0, "{out}\n{err}");
    let text = std::fs::read_to_string(rig.root().join("nv-rs/capabilities.json")).unwrap();
    assert!(text.contains("\"synthetic\""), "{text}");
}

#[test]
fn the_release_can_be_requested_even_with_a_toolchain() {
    let rig = Rig::new();
    let release = rig.release(&FakeRelease::new());
    let gh = rig.gh(0);
    let (code, _, err) = rig.install(&[
        "--release",
        "--cargo",
        CARGO,
        "--source",
        rig.source(),
        "--release-url",
        &release,
        "--gh",
        &gh,
    ]);
    assert_eq!(code, 0, "{err}");
    assert_eq!(std::fs::read(rig.bin()).unwrap(), FAKE_RELEASE_BINARY);
}

fn verification_fails(rig: &Rig, release: &FakeRelease, gh_code: i32) -> String {
    let url = rig.release(release);
    let gh = rig.gh(gh_code);
    let cargo = absent_cargo(rig);
    let before = snapshot(&rig.root());
    let (code, _, err) = rig.install(&["--cargo", &cargo, "--release-url", &url, "--gh", &gh]);
    assert_eq!(code, 3, "{err}");
    assert_eq!(snapshot(&rig.root()), before, "nothing is installed");
    err
}

#[test]
fn a_wrong_checksum_exits_3_and_installs_nothing() {
    let rig = Rig::new();
    let err = verification_fails(&rig, &FakeRelease::new().wrong_checksum(), 0);
    assert!(err.contains("checksum"), "{err}");
    assert!(
        rig.gh_calls().is_empty(),
        "attestation is not asked for a file that failed its checksum"
    );
}

#[test]
fn an_asset_missing_from_the_checksum_list_exits_3() {
    let rig = Rig::new();
    let err = verification_fails(&rig, &FakeRelease::new().asset_not_listed(), 0);
    assert!(err.contains(FAKE_ASSET), "{err}");
}

#[test]
fn a_malformed_checksum_list_exits_3() {
    let rig = Rig::new();
    let err = verification_fails(&rig, &FakeRelease::new().malformed_sums(), 0);
    assert!(err.contains("malformed"), "{err}");
}

#[test]
fn a_failed_or_missing_attestation_exits_3_and_installs_nothing() {
    let rig = Rig::new();
    let err = verification_fails(&rig, &FakeRelease::new(), 1);
    assert!(err.contains("attestation"), "{err}");

    // `gh` itself not found counts as a failed attestation too.
    let url = rig.release(&FakeRelease::new());
    let missing = rig.tools.path().join("no-such-gh");
    let cargo = absent_cargo(&rig);
    let (code, _, err) = rig.install(&[
        "--cargo",
        &cargo,
        "--release-url",
        &url,
        "--gh",
        missing.to_str().unwrap(),
    ]);
    assert_eq!(code, 3, "{err}");
    assert!(!rig.root().join("nv-rs").exists());
}

#[test]
fn an_archive_member_that_escapes_the_folder_exits_3() {
    let rig = Rig::new();
    let err = verification_fails(&rig, &FakeRelease::new().escaping_member(), 0);
    assert!(err.contains("outside"), "{err}");
}

fn serve_release(release: &FakeRelease, tools: &Path) -> FakeServer {
    let dir = tools.join("served");
    std::fs::create_dir_all(&dir).unwrap();
    release.write(&dir, sha256_hex);
    FakeServer::start(move |path, _| match std::fs::read(dir.join(path)) {
        Ok(bytes) => Reply::Body(bytes),
        Err(_) => Reply::Status(404),
    })
}

fn fetch_args<'a>(url: &'a str, gh: &'a str, cargo: &'a str) -> Vec<&'a str> {
    vec![
        "--cargo",
        cargo,
        "--release-url",
        url,
        "--gh",
        gh,
        "--retry-delay-ms",
        "0",
    ]
}

#[test]
fn a_release_served_over_http_is_installed() {
    let rig = Rig::new();
    let server = serve_release(&FakeRelease::new(), rig.tools.path());
    let (gh, cargo, url) = (rig.gh(0), absent_cargo(&rig), server.url());
    let (code, _, err) = rig.install(&fetch_args(&url, &gh, &cargo));
    assert_eq!(code, 0, "{err}");
    assert_eq!(server.count("SHA256SUMS"), 1);
    assert_eq!(server.count(FAKE_ASSET), 1);
}

#[test]
fn a_failing_server_is_asked_once_and_retried_three_times_then_exit_4() {
    let rig = Rig::new();
    let server = FakeServer::start(|_, _| Reply::Status(503));
    let (gh, cargo, url) = (rig.gh(0), absent_cargo(&rig), server.url());
    let before = snapshot(&rig.root());
    let (code, _, err) = rig.install(&fetch_args(&url, &gh, &cargo));
    assert_eq!(code, 4, "{err}");
    assert_eq!(server.count("SHA256SUMS"), 4, "1 try and 3 retries");
    assert_eq!(snapshot(&rig.root()), before);
}

#[test]
fn a_server_that_recovers_within_the_retries_gives_an_install() {
    let rig = Rig::new();
    let dir = rig.tools.path().join("served");
    std::fs::create_dir_all(&dir).unwrap();
    FakeRelease::new().write(&dir, sha256_hex);
    let server = FakeServer::start(move |path, nth| {
        if nth < 2 {
            return Reply::Status(500);
        }
        Reply::Body(std::fs::read(dir.join(path)).unwrap())
    });
    let (gh, cargo, url) = (rig.gh(0), absent_cargo(&rig), server.url());
    let (code, _, err) = rig.install(&fetch_args(&url, &gh, &cargo));
    assert_eq!(code, 0, "{err}");
    assert_eq!(server.count("SHA256SUMS"), 3);
}

#[test]
fn a_malformed_response_aborts_without_retrying() {
    for reply in [Reply::Garbage, Reply::Truncated] {
        let rig = Rig::new();
        let server = FakeServer::start(move |_, _| reply.clone());
        let (gh, cargo, url) = (rig.gh(0), absent_cargo(&rig), server.url());
        let before = snapshot(&rig.root());
        let (code, _, err) = rig.install(&fetch_args(&url, &gh, &cargo));
        assert_eq!(code, 3, "{err}");
        assert!(err.contains("malformed"), "{err}");
        assert_eq!(
            server.count("SHA256SUMS"),
            1,
            "no retry of a malformed response"
        );
        assert_eq!(snapshot(&rig.root()), before);
    }
}

#[test]
fn every_fetch_has_a_60_second_timeout_and_a_timeout_is_retried() {
    let rig = Rig::new();
    let log = rig.tools.path().join("curl.log");
    let curl = write_script(
        rig.tools.path(),
        "curl",
        &format!("echo \"$@\" >> {}\nexit 28", log.display()),
    );
    let (gh, cargo) = (rig.gh(0), absent_cargo(&rig));
    let mut args = fetch_args("http://example.invalid", &gh, &cargo);
    args.extend(["--curl", curl.to_str().unwrap()]);
    let (code, _, err) = rig.install(&args);
    assert_eq!(code, 4, "{err}");
    let calls = log_lines(&log);
    assert_eq!(calls.len(), 4, "{calls:?}");
    assert!(
        calls.iter().all(|c| c.contains("--max-time 60")),
        "{calls:?}"
    );
}

#[test]
fn no_executable_bytes_are_copied_and_the_base_copy_is_untouched() {
    let rig = Rig::new();
    let base_before = snapshot(rig.base.path());
    let exe = std::fs::read(rig.base.path().join("FalloutNV.exe")).unwrap();
    let (code, ..) = rig.install(&["--source", rig.source(), "--cargo", CARGO]);
    assert_eq!(code, 0);
    assert_eq!(snapshot(rig.base.path()), base_before);
    for (name, bytes) in snapshot(&rig.root()) {
        assert!(!name.to_lowercase().contains("falloutnv"), "{name}");
        assert!(
            !bytes.windows(exe.len()).any(|w| w == exe.as_slice()),
            "{name} holds the game executable"
        );
    }
    for (name, bytes) in snapshot(&std::fs::canonicalize(rig.source.path()).unwrap()) {
        assert!(!name.to_lowercase().contains("falloutnv"), "{name}");
        assert_ne!(bytes, exe);
    }
}

#[test]
fn refusals_exit_2_and_change_nothing() {
    let rig = Rig::new();
    let before = snapshot(&rig.root());
    // A toolchain but no source and no --release.
    let (code, _, err) = rig.install(&["--cargo", CARGO]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("--source"), "{err}");
    // A folder that is not an Instance.
    let plain = TempDir::new("nvrs-plain");
    let (code, _, err) = nvmod(&[
        "install-nv-rs",
        "--instance",
        plain.path().to_str().unwrap(),
    ]);
    assert_eq!(code, 2, "{err}");
    // A command line that is not understood.
    assert_eq!(rig.install(&["--nope"]).0, 5);
    assert_eq!(nvmod(&["install-nv-rs"]).0, 5);
    assert_eq!(snapshot(&rig.root()), before);
}

#[test]
fn an_installed_nv_rs_is_not_overwritten() {
    let rig = Rig::new();
    let (code, ..) = rig.install(&["--source", rig.source(), "--cargo", CARGO]);
    assert_eq!(code, 0);
    let before = snapshot(&rig.root());
    let (code, _, err) = rig.install(&["--source", rig.source(), "--cargo", CARGO]);
    assert_eq!(code, 2, "{err}");
    assert!(err.contains("already installed"), "{err}");
    assert_eq!(snapshot(&rig.root()), before);
}

#[test]
fn the_help_lists_the_command() {
    let (code, out, _) = nvmod(&["--help"]);
    assert_eq!(code, 0);
    assert!(out.contains("install-nv-rs"), "{out}");
}
