// SPDX-License-Identifier: GPL-3.0-only
//! Contract tests on recorded synthetic outputs, no network: each vendored contract accepts a recorded sample of
//! what its tool emits, ignores fields it does not know, refuses a newer schema version and rejects a document
//! with a required field missing or mistyped. The samples are in `tests/recorded/` and hold no game data.

use nvmod_tools::{ContractError, builtin};
use serde_json::{Value, json};

fn sample(name: &str) -> Value {
    let path = format!("{}/tests/recorded/{name}.json", env!("CARGO_MANIFEST_DIR"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}")))
        .unwrap()
}

/// Each contract, its recorded sample, and a field whose removal must be rejected.
const CASES: [(&str, &str, &str); 11] = [
    ("lodwright-bands", "lodwright-bands", "bands"),
    ("lodwright-defaults", "lodwright-defaults", "options"),
    ("lodwright-events", "lodwright-events", "world"),
    ("lodwright-manifest", "lodwright-manifest", "files"),
    ("lodwright-plan", "lodwright-plan", "worlds"),
    ("lodwright-request", "lodwright-request", "plugins"),
    ("lodwright-result", "lodwright-result", "status"),
    ("lodwright-verify", "lodwright-verify", "errors"),
    ("loot-sort", "loot-sort", "sorted"),
    ("nv-rs-capabilities", "nv-rs-capabilities", "extenders"),
    (
        "nv-rs-release-manifest",
        "nv-rs-release-manifest",
        "executable",
    ),
];

#[test]
fn every_contract_accepts_its_recorded_sample() {
    for (contract, recorded, _) in CASES {
        assert_eq!(
            builtin(contract).unwrap().check(&sample(recorded)),
            Ok(1),
            "{contract}"
        );
    }
}

#[test]
fn unknown_fields_are_ignored() {
    for (contract, recorded, _) in CASES {
        let mut doc = sample(recorded);
        doc["a_field_added_by_a_newer_tool"] = json!({"nested": [1, 2]});
        assert_eq!(builtin(contract).unwrap().check(&doc), Ok(1), "{contract}");
    }
}

#[test]
fn a_newer_schema_version_is_refused_with_a_reason() {
    for (contract, recorded, _) in CASES {
        let mut doc = sample(recorded);
        doc["schema_version"] = json!(2);
        let why = builtin(contract).unwrap().check(&doc).unwrap_err();
        assert!(
            matches!(
                why,
                ContractError::Newer {
                    found: 2,
                    current: 1,
                    ..
                }
            ),
            "{contract}: {why}"
        );
        assert!(why.to_string().contains(contract));
    }
}

#[test]
fn a_missing_required_field_is_rejected_with_the_contract_named() {
    for (contract, recorded, field) in CASES {
        let mut doc = sample(recorded);
        doc.as_object_mut().unwrap().remove(field);
        let why = builtin(contract).unwrap().check(&doc).unwrap_err();
        assert!(
            matches!(why, ContractError::Invalid { .. }),
            "{contract}: {why}"
        );
        // A `oneOf` (the events schema) reports the whole line, not the field that is missing.
        assert!(why.to_string().contains(contract), "{why}");
        if contract != "lodwright-events" {
            assert!(why.to_string().contains(field), "{why}");
        }
    }
}

#[test]
fn contracts_without_a_legacy_reading_need_a_schema_version() {
    for contract in ["nv-rs-capabilities", "nv-rs-release-manifest"] {
        let mut doc = sample(contract);
        doc.as_object_mut().unwrap().remove("schema_version");
        assert!(matches!(
            builtin(contract).unwrap().check(&doc),
            Err(ContractError::NoVersion { .. })
        ));
    }
    // lodwright-rs writes `schema_version` in every document and its schemas require it: no legacy reading.
    for contract in [
        "lodwright-bands",
        "lodwright-defaults",
        "lodwright-manifest",
        "lodwright-plan",
        "lodwright-request",
        "lodwright-result",
        "lodwright-verify",
    ] {
        let mut doc = sample(contract);
        doc.as_object_mut().unwrap().remove("schema_version");
        assert!(
            matches!(
                builtin(contract).unwrap().check(&doc),
                Err(ContractError::NoVersion { .. })
            ),
            "{contract}"
        );
    }
    // The tools that did not write one at first, and a progress line that carries none, are read as version 1.
    for contract in ["lodwright-events", "loot-sort"] {
        let mut doc = sample(contract);
        doc.as_object_mut().unwrap().remove("schema_version");
        assert_eq!(builtin(contract).unwrap().check(&doc), Ok(1));
    }
}

/// The manifest as the nv-rs fork's `release-files` tool writes it (shape copied from its generator; the hashes
/// here are synthetic): a release's real `release-manifest.json` must validate under the vendored schema.
fn fork_manifest() -> Value {
    json!({
        "schema_version": 1,
        "version": "2026-01-01.1",
        "upstream_base_commit": "a".repeat(40),
        "source_commit": "b".repeat(40),
        "source_tree_hash": "c".repeat(40),
        "executable": {
            "file_name": "FalloutNV.exe",
            "game_version": "1.4.0.525",
            "hash_algorithm": "fnv1a-64",
            "hashed_bytes": "raw bytes of each listed section, as the file stores them",
            "sections": [
                {"name": ".text", "virtual_address": "0x00401000", "virtual_size": "0x00a2b000", "hash": "00112233445566ff"},
                {"name": ".rdata", "virtual_address": "0x00e2c000", "virtual_size": "0x00030000", "hash": "ffeeddccbbaa9988"}
            ]
        },
        "play_patch": {
            "standalone_step": false,
            "command": null,
            "arguments": [],
            "description": "not a program"
        },
        "installation_check": {
            "command": "nv-viewer",
            "arguments": ["<data folder>", "--check"],
            "description": "synthetic"
        },
        "plugins_txt_ordering_switch": false,
        "files": {"capabilities": "capabilities.json", "checksums": "SHA256SUMS"}
    })
}

#[test]
fn the_forks_real_manifest_shape_validates() {
    let contract = builtin("nv-rs-release-manifest").unwrap();
    assert_eq!(contract.check(&fork_manifest()), Ok(1));
}

#[test]
fn the_forks_manifest_is_rejected_when_its_own_required_parts_are_wrong() {
    let contract = builtin("nv-rs-release-manifest").unwrap();
    for field in ["executable", "play_patch", "plugins_txt_ordering_switch"] {
        let mut doc = fork_manifest();
        doc.as_object_mut().unwrap().remove(field);
        assert!(contract.check(&doc).is_err(), "{field}");
    }
    let mut doc = fork_manifest();
    doc["executable"]["sections"][0]["hash"] = json!("not-a-hash");
    assert!(contract.check(&doc).is_err());
    let mut doc = fork_manifest();
    doc["upstream_base_commit"] = json!("not-a-commit");
    assert!(contract.check(&doc).is_err());
}

#[test]
fn the_names_of_the_earlier_proposal_no_longer_satisfy_the_contract() {
    let mut doc = fork_manifest();
    let object = doc.as_object_mut().unwrap();
    object.remove("executable");
    object.remove("plugins_txt_ordering_switch");
    object.insert(
        "executable_hashes".into(),
        json!({"nv-viewer": "0".repeat(64)}),
    );
    object.insert("plugins_txt_ordering".into(), json!(false));
    assert!(
        builtin("nv-rs-release-manifest")
            .unwrap()
            .check(&doc)
            .is_err()
    );
}

#[test]
fn a_document_with_the_wrong_contract_name_is_refused() {
    for (contract, recorded, _) in CASES {
        let mut doc = sample(recorded);
        doc["contract"] = json!("some-other-contract");
        let why = builtin(contract).unwrap().check(&doc).unwrap_err();
        assert!(
            matches!(why, ContractError::WrongName { .. }),
            "{contract}: {why}"
        );
        assert!(why.to_string().contains("some-other-contract"), "{why}");
    }
}

#[test]
fn the_recorded_samples_name_their_contract_where_the_tool_does() {
    for contract in [
        "lodwright-bands",
        "lodwright-defaults",
        "lodwright-manifest",
        "lodwright-plan",
        "lodwright-request",
        "lodwright-result",
        "lodwright-verify",
        "loot-sort",
    ] {
        let doc = sample(contract);
        assert_eq!(doc["contract"], contract);
        assert_eq!(
            builtin(contract).unwrap().check_at(&doc, Some(1), true),
            Ok(1)
        );
    }
}

#[test]
fn a_name_is_only_required_after_the_handshake() {
    let mut doc = sample("loot-sort");
    doc.as_object_mut().unwrap().remove("contract");
    let c = builtin("loot-sort").unwrap();
    assert_eq!(c.check(&doc), Ok(1));
    assert!(matches!(
        c.check_at(&doc, Some(1), true),
        Err(ContractError::WrongName { found: None, .. })
    ));
}

#[test]
fn the_lock_agrees_with_what_the_built_in_contracts_accept() {
    // `tools.lock.toml` keeps the version each pin was tested with; the handshake clamp is what "consistent"
    // means, so every locked contract must be one nvmod reads at that version.
    for tool in &nvmod_tools::builtin_lock().tools {
        for (name, version) in &tool.contracts {
            let contract = builtin(name).unwrap_or_else(|| panic!("{name} is not built in"));
            assert!(contract.accepted().contains(version), "{name} {version}");
        }
    }
}

#[test]
fn a_document_above_the_negotiated_version_is_refused() {
    use nvmod_tools::Contract;
    let v1 = r#"{"type":"object","properties":{"schema_version":{"const":1}}}"#;
    let v2 = r#"{"type":"object","properties":{"schema_version":{"const":2}}}"#;
    let c = Contract::new("demo", None, &[(1, v1), (2, v2)]).unwrap();
    let doc = json!({"schema_version": 2, "contract": "demo"});
    assert_eq!(c.check_at(&doc, Some(2), true), Ok(2));
    assert!(matches!(
        c.check_at(&doc, Some(1), true),
        Err(ContractError::AboveNegotiated {
            found: 2,
            negotiated: 1,
            ..
        })
    ));
}

/// The names and versions lodwright-rs v0.14.0 lists under `lodwright contracts --json`, in its order. `lodwright-api`
/// is the in-process library API and has no schema file, so it is the one name nvmod does not vendor.
const LODWRIGHT_HANDSHAKE: [&str; 9] = [
    "lodwright-plan",
    "lodwright-result",
    "lodwright-request",
    "lodwright-manifest",
    "lodwright-verify",
    "lodwright-events",
    "lodwright-defaults",
    "lodwright-bands",
    "lodwright-api",
];

#[test]
fn every_schema_of_the_lodwright_handshake_is_vendored_at_version_1() {
    for name in LODWRIGHT_HANDSHAKE {
        if name == "lodwright-api" {
            assert!(builtin(name).is_none());
            continue;
        }
        let contract = builtin(name).unwrap_or_else(|| panic!("{name} is not built in"));
        assert_eq!((contract.current(), contract.accepted()), (1, vec![1]));
        let snapshot = nvmod_tools::SNAPSHOTS
            .iter()
            .find(|s| s.contract == name)
            .unwrap();
        assert_eq!(snapshot.tool, "lodwright-rs");
    }
    let vendored = nvmod_tools::SNAPSHOTS
        .iter()
        .filter(|s| s.tool == "lodwright-rs")
        .count();
    assert_eq!(vendored, LODWRIGHT_HANDSHAKE.len() - 1);
}

/// A broken document per lodwright contract: a field of the wrong type, which the schema must reject with the
/// contract named and the place in the document.
#[test]
fn a_mistyped_field_is_rejected_per_lodwright_contract() {
    let broken = [
        ("lodwright-bands", "runs", json!("three")),
        ("lodwright-defaults", "levels", json!("4,8")),
        ("lodwright-events", "done", json!("one")),
        ("lodwright-manifest", "files", json!({"path": "a"})),
        ("lodwright-plan", "worlds", json!("none")),
        ("lodwright-request", "plugins", json!("FalloutNV.esm")),
        ("lodwright-result", "status", json!("maybe")),
        ("lodwright-verify", "ok", json!("yes")),
    ];
    for (contract, field, value) in broken {
        let mut doc = sample(contract);
        doc[field] = value;
        let why = builtin(contract).unwrap().check(&doc).unwrap_err();
        assert!(
            matches!(&why, ContractError::Invalid { version: 1, .. }),
            "{contract}: {why}"
        );
        assert!(why.to_string().contains(contract), "{why}");
    }
}

#[test]
fn the_plan_reads_the_bounds_the_released_tool_writes() {
    // The first vendored plan wrote `min_x`..`max_y`; v0.7.0 writes the four compass names. A bounds object of the
    // old names no longer validates.
    let contract = builtin("lodwright-plan").unwrap();
    let mut doc = sample("lodwright-plan");
    doc["worlds"][0]["bounds"] = json!({"min_x": -1, "min_y": -1, "max_x": 0, "max_y": 0});
    let why = contract.check(&doc).unwrap_err();
    assert!(matches!(why, ContractError::Invalid { .. }), "{why}");
}

#[test]
fn the_result_status_is_one_of_the_three_the_tool_writes() {
    let contract = builtin("lodwright-result").unwrap();
    for status in ["ok", "unsupported", "failed"] {
        let mut doc = sample("lodwright-result");
        doc["status"] = json!(status);
        assert_eq!(contract.check(&doc), Ok(1), "{status}");
    }
}

#[test]
fn an_event_of_another_kind_is_rejected() {
    let contract = builtin("lodwright-events").unwrap();
    let mut doc = sample("lodwright-events");
    doc["event"] = json!("finished");
    assert!(contract.check(&doc).is_err());
}
