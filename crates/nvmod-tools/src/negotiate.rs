// SPDX-License-Identifier: GPL-3.0-only
//! The contract handshake (ADR-0006): a tool answers `<tool> contracts --json` with
//! `{"contracts":[{"name":..,"versions":[..]}]}`, and nvmod picks the highest version both sides list that is not
//! above the version the lock records for the pin, so a handshake finds a mismatch and never upgrades past what the
//! compatibility matrix tested. A lock version the tool does not list, or no common version, is a tool that cannot
//! be verified (exit code 3). A pin made before the handshake exists is grandfathered, see
//! [`ToolEntry::handshake_required`](crate::ToolEntry::handshake_required).

use crate::ToolError;
use serde_json::Value;
use std::collections::BTreeMap;

/// What a tool says it speaks: contract name to the schema versions it can emit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Handshake {
    contracts: BTreeMap<String, Vec<u32>>,
}

impl Handshake {
    /// Read the output of `contracts --json`.
    ///
    /// # Errors
    /// A reason, when the text is not `{"contracts":[{"name":<string>,"versions":[<integer>..]}]}`.
    pub fn parse(text: &str) -> Result<Handshake, String> {
        let document: Value =
            serde_json::from_str(text).map_err(|e| format!("the handshake is not JSON: {e}"))?;
        Handshake::from_value(&document)
    }

    /// Read a handshake document that is already parsed.
    ///
    /// # Errors
    /// A reason, when the document has the wrong shape.
    pub fn from_value(document: &Value) -> Result<Handshake, String> {
        let list = document
            .get("contracts")
            .and_then(Value::as_array)
            .ok_or("the handshake has no `contracts` list")?;
        let mut contracts = BTreeMap::new();
        for entry in list {
            let name = entry
                .get("name")
                .and_then(Value::as_str)
                .ok_or("a handshake entry has no `name`")?;
            let versions = entry
                .get("versions")
                .and_then(Value::as_array)
                .ok_or_else(|| format!("handshake entry `{name}` has no `versions` list"))?
                .iter()
                .map(|v| {
                    v.as_u64()
                        .and_then(|v| u32::try_from(v).ok())
                        .ok_or_else(|| {
                            format!("handshake entry `{name}` lists a non-integer version")
                        })
                })
                .collect::<Result<Vec<u32>, String>>()?;
            contracts.insert(name.to_owned(), versions);
        }
        Ok(Handshake { contracts })
    }

    /// The versions the tool lists for `contract`, if it lists it.
    #[must_use]
    pub fn versions(&self, contract: &str) -> Option<&[u32]> {
        self.contracts.get(contract).map(Vec::as_slice)
    }
}

/// Pick the version to speak for `contract`: the highest that the tool lists and nvmod reads (`readable`), not above
/// `lock_version`.
///
/// # Errors
/// `Unverifiable` (exit code 3) when the tool does not list the contract, does not list `lock_version`, or shares no
/// version with nvmod at or below it.
pub fn negotiate(
    tool: &str,
    contract: &str,
    lock_version: u32,
    readable: &[u32],
    handshake: &Handshake,
) -> Result<u32, ToolError> {
    let refuse = |reason: String| ToolError::Unverifiable {
        tool: tool.to_owned(),
        reason,
    };
    let Some(listed) = handshake.versions(contract) else {
        return Err(refuse(format!(
            "its handshake does not list the contract `{contract}`"
        )));
    };
    if !listed.contains(&lock_version) {
        return Err(refuse(format!(
            "the lock records `{contract}` version {lock_version}, which the tool does not list (it lists {listed:?})"
        )));
    }
    listed
        .iter()
        .copied()
        .filter(|v| *v <= lock_version && readable.contains(v))
        .max()
        .ok_or_else(|| {
            refuse(format!(
                "`{contract}` has no version that both sides list at or below {lock_version} \
                 (the tool lists {listed:?}, this nvmod reads {readable:?})"
            ))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{covers, ids};

    fn hs(versions: &str) -> Handshake {
        Handshake::parse(&format!(
            r#"{{"contracts":[{{"name":"demo","versions":{versions}}}]}}"#
        ))
        .unwrap()
    }

    #[test]
    fn the_highest_common_version_not_above_the_lock_is_picked() {
        covers(&[ids::tools::contracts::VERSION_NEGOTIATION]);
        assert_eq!(negotiate("t", "demo", 2, &[1, 2], &hs("[1,2,3]")), Ok(2));
        // A tool that lists a newer version is never moved up to it.
        assert_eq!(negotiate("t", "demo", 1, &[1, 2], &hs("[1,2]")), Ok(1));
        assert_eq!(negotiate("t", "demo", 2, &[1, 2], &hs("[2]")), Ok(2));
    }

    #[test]
    fn a_lock_version_the_tool_does_not_list_is_unverifiable() {
        let why = negotiate("t", "demo", 2, &[1, 2], &hs("[1]")).unwrap_err();
        assert!(matches!(why, ToolError::Unverifiable { .. }), "{why}");
        assert_eq!(why.exit_code(), 3);
        assert!(why.to_string().contains("version 2"), "{why}");
    }

    #[test]
    fn no_common_version_is_unverifiable() {
        let why = negotiate("t", "demo", 3, &[1, 2], &hs("[3]")).unwrap_err();
        assert_eq!(why.exit_code(), 3);
        assert!(why.to_string().contains("no version"), "{why}");
    }

    #[test]
    fn a_contract_the_tool_does_not_list_is_unverifiable() {
        let why = negotiate("t", "other", 1, &[1], &hs("[1]")).unwrap_err();
        assert_eq!(why.exit_code(), 3);
    }

    #[test]
    fn a_malformed_handshake_is_refused_with_a_reason() {
        for text in [
            "not json",
            "{}",
            r#"{"contracts":[{"versions":[1]}]}"#,
            r#"{"contracts":[{"name":"a"}]}"#,
            r#"{"contracts":[{"name":"a","versions":["1"]}]}"#,
        ] {
            assert!(Handshake::parse(text).is_err(), "{text}");
        }
    }
}
