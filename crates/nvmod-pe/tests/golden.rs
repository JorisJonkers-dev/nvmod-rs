// SPDX-License-Identifier: GPL-3.0-only
//! Golden reports: the JSON of the inspector's output for synthetic files. `UPDATE_GOLDEN=1 cargo test` rewrites
//! them; review the diff like code.

use nvmod_pe::inspect_bytes;
use nvmod_testkit::PeBuilder;
use std::path::PathBuf;

fn check(name: &str, file: &[u8]) {
    let report = inspect_bytes(file).unwrap();
    let json = serde_json::to_string_pretty(&report).unwrap() + "\n";
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.json"));
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &json).unwrap();
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("missing golden file {}", path.display()));
    assert_eq!(
        json, expected,
        "golden file {name} differs; run with UPDATE_GOLDEN=1 and review"
    );
}

#[test]
fn legacy_nvse_plugin() {
    check(
        "legacy-plugin",
        &PeBuilder::nvse_plugin().import("MSVCP140.dll").build(),
    );
}

#[test]
fn modern_nvse_plugin_with_a_version_struct_and_resource() {
    let file = PeBuilder::dll()
        .export_code("NVSEPlugin_Load")
        .nvse_version(1, 42)
        .import("KERNEL32.dll")
        .versions([3, 1, 0, 7], [3, 1, 0, 7])
        .build();
    check("modern-plugin", &file);
}

#[test]
fn game_executable_with_the_large_address_flag() {
    check(
        "game-exe",
        &PeBuilder::exe()
            .versions([1, 4, 0, 525], [1, 4, 0, 525])
            .large_address_aware(true)
            .build(),
    );
}

#[test]
fn x64_dll() {
    check(
        "x64-dll",
        &PeBuilder::nvse_plugin().machine(0x8664).pe32_plus().build(),
    );
}
