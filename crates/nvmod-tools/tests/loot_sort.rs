// SPDX-License-Identifier: GPL-3.0-only
//! The second contract of loot-rs: its `sort --json` output must say what the library says. nvmod links the library
//! and keeps the command line form only for the differential check and for debugging, so the two are compared on a
//! synthetic plugin set, and the command line output is checked against the vendored `loot-sort` schema.

use loot_rs::api::{Masterlist, PluginSlot, SortOptions, sort};
use nvmod_testkit::{PluginBuilder, TempDir, write_file};
use nvmod_tools::builtin;
use serde_json::Value;
use std::path::Path;

const MASTERLIST: &str = r"
globals:
  - {type: say, content: 'Masterlist for tests'}
plugins:
  - name: 'Other\.esp'
    msg:
      - {type: warn, content: 'Other is only a test plugin'}
    after: ['Patch\.esp']
";

const ORDER: [&str; 4] = ["FalloutNV.esm", "Patch.esp", "Base.esp", "Other.esp"];

fn fixture(dir: &Path) {
    write_file(dir, "Data/FalloutNV.esm", &PluginBuilder::master().build());
    write_file(
        dir,
        "Data/Patch.esp",
        &PluginBuilder::new().requires("Base.esp").build(),
    );
    write_file(dir, "Data/Base.esp", &PluginBuilder::new().build());
    write_file(dir, "Data/Other.esp", &PluginBuilder::new().build());
    write_file(dir, "masterlist.yaml", MASTERLIST.as_bytes());
    // plugins.txt lists the active plugins in order; the patch is listed before its master on purpose.
    write_file(
        dir,
        "plugins.txt",
        ORDER.map(|n| format!("{n}\r\n")).concat().as_bytes(),
    );
}

fn command_line_json(dir: &Path) -> (u8, String) {
    let args: Vec<String> = [
        "sort",
        "--masterlist",
        dir.join("masterlist.yaml").to_str().unwrap(),
        "--plugins",
        dir.join("plugins.txt").to_str().unwrap(),
        "--data",
        dir.join("Data").to_str().unwrap(),
        "--json",
    ]
    .map(str::to_owned)
    .to_vec();
    let (mut out, mut err) = (Vec::new(), Vec::new());
    let code = loot_rs::cli::main_with(&args, &mut out, &mut err);
    assert!(err.is_empty(), "{}", String::from_utf8_lossy(&err));
    (code, String::from_utf8(out).unwrap())
}

#[test]
fn the_library_result_and_the_sort_json_output_are_equal() {
    let dir = TempDir::new("loot-contract");
    fixture(dir.path());

    // Same input as the command line reads from plugins.txt: every line is a slot, active when listed.
    let slots: Vec<PluginSlot> = ORDER
        .iter()
        .map(|name| PluginSlot::new((*name).to_owned(), true))
        .collect();
    let masterlist = Masterlist::parse(MASTERLIST).unwrap();
    let library = sort(
        &slots,
        &masterlist,
        &dir.path().join("Data"),
        &SortOptions::default(),
    )
    .unwrap();

    let (code, text) = command_line_json(dir.path());
    let cli: Value = serde_json::from_str(&text).unwrap();

    // The whole document equals the library's own rendering of its result.
    let rendered: Value =
        serde_json::from_str(&loot_rs::render::render_sort_json(&library)).unwrap();
    assert_eq!(cli, rendered);

    // And the fields nvmod acts on equal the library result field by field.
    let sorted: Vec<&str> = cli["sorted"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(sorted, library.order);
    let place = |name: &str| sorted.iter().position(|n| *n == name);
    assert!(
        place("Base.esp") < place("Patch.esp"),
        "the master sorts before its patch: {sorted:?}"
    );
    let plugins = cli["plugins"].as_array().unwrap();
    assert_eq!(plugins.len(), library.plugins.len());
    for (json, lib) in plugins.iter().zip(&library.plugins) {
        assert_eq!(json["name"], lib.name.as_str());
        assert_eq!(json["position"], lib.position);
        assert_eq!(json["active"], lib.active);
    }
    let hard = library
        .plugins
        .iter()
        .flat_map(|p| &p.findings)
        .filter(|f| f.hard)
        .count();
    assert_eq!(cli["hard_violations"], hard);
    assert_eq!(code, u8::from(hard > 0));
}

#[test]
fn the_sort_json_output_matches_the_vendored_schema() {
    let dir = TempDir::new("loot-contract-schema");
    fixture(dir.path());
    let (_, text) = command_line_json(dir.path());
    let document: Value = serde_json::from_str(&text).unwrap();
    // loot-rs 0.1.0 writes no schema_version: it is read as version 1.
    assert_eq!(builtin("loot-sort").unwrap().check(&document), Ok(1));

    // With NVMOD_BLESS=1 this output is recorded as the synthetic sample the offline contract tests read.
    let recorded = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/recorded/loot-sort.json");
    if std::env::var_os("NVMOD_BLESS").is_some() {
        std::fs::write(&recorded, text).unwrap();
    }
}
