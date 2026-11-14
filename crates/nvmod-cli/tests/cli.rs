// SPDX-License-Identifier: GPL-3.0-only
//! The binary, run as a program.

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

#[test]
fn version_prints_the_package_version() {
    let (code, out, _) = nvmod(&["--version"]);
    assert_eq!(code, 0);
    assert_eq!(out.trim(), concat!("nvmod ", env!("CARGO_PKG_VERSION")));
}

#[test]
fn no_arguments_print_the_help() {
    let (code, out, _) = nvmod(&[]);
    assert_eq!(code, 0);
    assert!(out.contains("nvmod pe <file>"));
}

#[test]
fn an_unknown_command_is_a_usage_error() {
    let (code, _, err) = nvmod(&["frobnicate"]);
    assert_eq!(code, 2);
    assert!(err.contains("unknown command `frobnicate`"));
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("nvmod-cli-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn pe_describes_a_plugin_as_text_and_as_json() {
    let dir = temp_dir("pe");
    let path = dir.join("plugin.dll");
    std::fs::write(
        &path,
        nvmod_testkit::PeBuilder::nvse_plugin()
            .nvse_version(1, 3)
            .versions([1, 2, 3, 4], [1, 2, 3, 4])
            .build(),
    )
    .unwrap();
    let file = path.to_str().unwrap();
    let (code, out, _) = nvmod(&["pe", file]);
    assert_eq!(code, 0);
    assert!(
        out.contains("NVSE plugin: query yes, load yes, version struct info 1 version 3"),
        "{out}"
    );
    assert!(out.contains("file version: 1.2.3.4") && out.contains("imports: KERNEL32.dll"));
    let (code, out, _) = nvmod(&["pe", file, "--json"]);
    assert_eq!(code, 0);
    let json: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(json["report"]["nvse"]["version_struct"]["version"], 3);
    assert_eq!(json["report"]["file_version"], "1.2.3.4");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn pe_says_when_a_file_is_not_an_nvse_plugin() {
    let dir = temp_dir("pe-text");
    let plain = dir.join("plain.dll");
    std::fs::write(&plain, nvmod_testkit::PeBuilder::dll().build()).unwrap();
    let (_, out, _) = nvmod(&["pe", plain.to_str().unwrap()]);
    assert!(out.contains("NVSE plugin: no"), "{out}");
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn pe_on_a_file_that_is_not_a_pe_file_exits_with_one() {
    let dir = temp_dir("pe-bad");
    let path = dir.join("bad.dll");
    std::fs::write(
        &path,
        b"not a pe file at all, just text that is long enough to have a DOS header..........",
    )
    .unwrap();
    let file = path.to_str().unwrap();
    let (code, _, err) = nvmod(&["pe", file]);
    assert_eq!(code, 1);
    assert!(err.contains("not a PE file"), "{err}");
    let (code, out, _) = nvmod(&["pe", file, "--json"]);
    assert_eq!(code, 1);
    assert!(
        serde_json::from_str::<serde_json::Value>(&out).unwrap()["error"]
            .as_str()
            .unwrap()
            .contains("not a PE file")
    );
    let (code, _, err) = nvmod(&["pe", dir.join("missing.dll").to_str().unwrap()]);
    assert_eq!(code, 1);
    assert!(err.contains("cannot read the file"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn pe_usage_errors_exit_with_two() {
    assert_eq!(nvmod(&["pe"]).0, 2);
    assert_eq!(nvmod(&["pe", "a", "b"]).0, 2);
    let (code, _, err) = nvmod(&["pe", "--bogus"]);
    assert_eq!(code, 2);
    assert!(err.contains("unknown option"));
}
