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
const CASES: [(&str, &str, &str); 5] = [
    ("lodwright-plan", "lodwright-plan", "worlds"),
    ("loot-sort", "loot-sort", "sorted"),
    ("nv-rs-capabilities", "nv-rs-capabilities", "extenders"),
    (
        "nv-rs-release-manifest",
        "nv-rs-release-manifest",
        "executable",
    ),
    ("ttw-installer-result", "ttw-installer-result", "outcome"),
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
        assert!(
            why.to_string().contains(contract) && why.to_string().contains(field),
            "{why}"
        );
    }
}

#[test]
fn contracts_without_a_legacy_reading_need_a_schema_version() {
    for contract in [
        "nv-rs-capabilities",
        "nv-rs-release-manifest",
        "ttw-installer-result",
    ] {
        let recorded = if contract == "ttw-installer-result" {
            "ttw-installer-result"
        } else {
            contract
        };
        let mut doc = sample(recorded);
        doc.as_object_mut().unwrap().remove("schema_version");
        assert!(matches!(
            builtin(contract).unwrap().check(&doc),
            Err(ContractError::NoVersion { .. })
        ));
    }
    // The tools that did not write one at first are read as version 1.
    for contract in ["lodwright-plan", "loot-sort"] {
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
