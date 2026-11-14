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
    assert!(out.contains("usage: nvmod"));
}

#[test]
fn an_unknown_command_is_a_usage_error() {
    let (code, _, err) = nvmod(&["frobnicate"]);
    assert_eq!(code, 2);
    assert!(err.contains("unknown command `frobnicate`"));
}
