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
const CASES: [(&str, &str, &str); 4] = [
    ("lodwright-plan", "lodwright-plan", "worlds"),
    ("loot-sort", "loot-sort", "sorted"),
    (
        "nv-rs-release-manifest",
        "nv-rs-release-manifest",
        "executable_hashes",
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
    for contract in ["nv-rs-release-manifest", "ttw-installer-result"] {
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
