// SPDX-License-Identifier: GPL-3.0-only
//! What the installed nv-rs says it has built in: its `capabilities.json`, read at run time and checked against the
//! vendored `nv-rs-capabilities` contract. Only names and counts are kept; the catalogue never holds a number.

use crate::{ContractError, builtin};
use serde::Deserialize;
use serde_json::Value;
use std::fmt;
use std::path::Path;

/// The contract's name.
pub const CONTRACT: &str = "nv-rs-capabilities";

/// How many commands of one built-in extender nv-rs implements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    /// The extender's id in the file (`xnvse`, `jip-ln`).
    pub id: String,
    /// Its display name.
    pub name: String,
    /// Commands implemented.
    pub implemented: u64,
    /// Commands that exist.
    pub total: u64,
}

impl Coverage {
    /// Commands that exist and are not implemented.
    #[must_use]
    pub fn missing(&self) -> u64 {
        self.total.saturating_sub(self.implemented)
    }
}

/// A parsed `capabilities.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capabilities {
    /// The schema version of the file.
    pub schema_version: u32,
    /// One entry per built-in extender, in file order.
    pub extenders: Vec<Coverage>,
}

/// Why a capabilities file was not used.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilitiesError {
    /// The file does not exist.
    Missing,
    /// It could not be read, is not JSON, or breaks the contract (newer, older, wrong shape, more implemented
    /// than exist). The reason is for people.
    Invalid(String),
}

impl fmt::Display for CapabilitiesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CapabilitiesError::Missing => f.write_str("the file does not exist"),
            CapabilitiesError::Invalid(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for CapabilitiesError {}

#[derive(Deserialize)]
struct File {
    schema_version: u32,
    extenders: Vec<Extender>,
}

#[derive(Deserialize)]
struct Extender {
    id: String,
    name: String,
    commands: Commands,
}

#[derive(Deserialize)]
struct Commands {
    implemented: u64,
    total: u64,
}

impl Capabilities {
    /// Read and check `path`.
    ///
    /// # Errors
    /// `Missing` when there is no file; `Invalid` with the reason otherwise.
    pub fn load(path: &Path) -> Result<Capabilities, CapabilitiesError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(CapabilitiesError::Missing);
            }
            Err(e) => return Err(CapabilitiesError::Invalid(format!("cannot read it: {e}"))),
        };
        Capabilities::parse(&text)
    }

    /// Check `text` against the contract.
    ///
    /// # Errors
    /// `Invalid` with the reason.
    pub fn parse(text: &str) -> Result<Capabilities, CapabilitiesError> {
        let invalid = |why: String| CapabilitiesError::Invalid(why);
        let value: Value =
            serde_json::from_str(text).map_err(|e| invalid(format!("it is not JSON: {e}")))?;
        let contract =
            builtin(CONTRACT).ok_or_else(|| invalid("no contract is built in".into()))?;
        let schema_version = contract
            .check(&value)
            .map_err(|e: ContractError| invalid(e.to_string()))?;
        let file: File = serde_json::from_value(value).map_err(|e| invalid(e.to_string()))?;
        let mut extenders = Vec::new();
        for e in file.extenders {
            if e.commands.implemented > e.commands.total {
                return Err(invalid(format!(
                    "{}: more commands implemented than exist",
                    e.id
                )));
            }
            extenders.push(Coverage {
                id: e.id,
                name: e.name,
                implemented: e.commands.implemented,
                total: e.commands.total,
            });
        }
        debug_assert_eq!(schema_version, file.schema_version);
        Ok(Capabilities {
            schema_version,
            extenders,
        })
    }

    /// The coverage of the extender called `id`.
    #[must_use]
    pub fn extender(&self, id: &str) -> Option<&Coverage> {
        self.extenders.iter().find(|e| e.id == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{covers, ids};

    const GOOD: &str = r#"{"schema_version":1,"extenders":[{"id":"a","name":"A","commands":{"implemented":2,"total":5,"implemented_names":["X","Y"]}}]}"#;

    #[test]
    fn counts_are_read_and_missing_is_derived() {
        covers(&[ids::tools::contracts::CAPABILITIES_FILE]);
        let caps = Capabilities::parse(GOOD).unwrap();
        let a = caps.extender("a").unwrap();
        assert_eq!((a.implemented, a.total, a.missing()), (2, 5, 3));
        assert!(caps.extender("b").is_none());
    }

    #[test]
    fn a_file_that_breaks_the_contract_is_refused_with_a_reason() {
        covers(&[ids::tools::contracts::CAPABILITIES_FILE]);
        for (text, want) in [
            ("nope", "not JSON"),
            (r#"{"schema_version":2,"extenders":[]}"#, "newer"),
            (r#"{"schema_version":1}"#, "extenders"),
            (
                r#"{"schema_version":1,"extenders":[{"id":"a","name":"A","commands":{"implemented":6,"total":5,"implemented_names":[]}}]}"#,
                "more commands implemented than exist",
            ),
        ] {
            let why = Capabilities::parse(text).unwrap_err().to_string();
            assert!(why.contains(want), "{text}: {why}");
        }
    }

    #[test]
    fn a_missing_file_is_its_own_case() {
        assert_eq!(
            Capabilities::load(Path::new("/nonexistent/capabilities.json")),
            Err(CapabilitiesError::Missing)
        );
    }

    /// With `NVMOD_NV_RS_CAPABILITIES` naming a real `capabilities.json` (nv-rs's checked-in file or a release's),
    /// the vendored contract must accept it. Does nothing without it, so CI needs no nv-rs checkout.
    #[test]
    fn a_real_capabilities_file_is_accepted_when_one_is_given() {
        let Some(path) = std::env::var_os("NVMOD_NV_RS_CAPABILITIES") else {
            return;
        };
        let caps = Capabilities::load(Path::new(&path)).unwrap();
        assert_ne!(caps.extenders.len(), 0);
    }
}
