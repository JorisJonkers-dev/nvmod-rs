// SPDX-License-Identifier: GPL-3.0-only
//! Contracts: the schema of what a tool emits, vendored as snapshots under `contracts/<tool>/` with a hash file.
//! A document carries an integer `schema_version`. A [`Contract`] accepts its current version and the one before
//! it, and refuses a newer one with a reason (docs/architecture/integration.md, section "Contracts").

use serde_json::Value;
use std::fmt;

/// One vendored snapshot: where it lives under `contracts/`, and its text.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    /// The tool's folder under `contracts/`.
    pub tool: &'static str,
    /// The contract name, such as `loot-sort`.
    pub contract: &'static str,
    /// The schema version the file describes.
    pub version: u32,
    /// The file name under `contracts/<tool>/`.
    pub file: &'static str,
    /// The file's text.
    pub text: &'static str,
    /// A document without `schema_version` is read as this version (tools that did not emit one at first).
    pub legacy: bool,
}

macro_rules! snapshot {
    ($tool:literal, $contract:literal, $version:literal, $legacy:literal) => {
        Snapshot {
            tool: $tool,
            contract: $contract,
            version: $version,
            file: concat!($contract, ".v", $version, ".json"),
            text: include_str!(concat!(
                "../../../contracts/",
                $tool,
                "/",
                $contract,
                ".v",
                $version,
                ".json"
            )),
            legacy: $legacy,
        }
    };
}

/// Every snapshot compiled into nvmod.
pub const SNAPSHOTS: &[Snapshot] = &[
    snapshot!("lodwright-rs", "lodwright-plan", 1, true),
    snapshot!("loot-rs", "loot-sort", 1, true),
    snapshot!("nv-rs", "nv-rs-capabilities", 1, false),
    snapshot!("nv-rs", "nv-rs-release-manifest", 1, false),
    snapshot!("ttw-installer", "ttw-installer-result", 1, false),
];

/// Why a document was not accepted under a contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractError {
    /// The document is of a newer schema version than the contract's current one (exit code 2).
    Newer {
        /// The contract name.
        contract: String,
        /// The version found.
        found: u32,
        /// The contract's current version.
        current: u32,
    },
    /// The document is older than the previous version, which is the oldest still accepted.
    TooOld {
        /// The contract name.
        contract: String,
        /// The version found.
        found: u32,
        /// The oldest version accepted.
        oldest: u32,
    },
    /// The document has no usable `schema_version` and the contract has no legacy reading.
    NoVersion {
        /// The contract name.
        contract: String,
    },
    /// The document names a different contract than the one it was read as (a tool or file wired to the wrong place),
    /// or needs a name and has none (exit code 4).
    WrongName {
        /// The contract name asked for.
        expected: String,
        /// The `contract` the document carries, or `None` when it carries none.
        found: Option<String>,
    },
    /// The document is of a newer schema version than the handshake settled on, which is the version the lock
    /// records for the pin (exit code 2).
    AboveNegotiated {
        /// The contract name.
        contract: String,
        /// The version found.
        found: u32,
        /// The version negotiated.
        negotiated: u32,
    },
    /// The document does not validate against the snapshot of its version.
    Invalid {
        /// The contract name.
        contract: String,
        /// The version it was checked against.
        version: u32,
        /// The first validation error, with the place in the document.
        first_error: String,
    },
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContractError::Newer {
                contract,
                found,
                current,
            } => write!(
                f,
                "{contract}: the tool speaks schema {found}, which is newer than this nvmod reads \
                 (schema {current}); update nvmod or pin an older tool"
            ),
            ContractError::TooOld {
                contract,
                found,
                oldest,
            } => write!(
                f,
                "{contract}: the tool speaks schema {found}, which is older than the oldest this nvmod reads \
                 (schema {oldest}); pin a newer tool"
            ),
            ContractError::NoVersion { contract } => {
                write!(f, "{contract}: the document has no integer schema_version")
            }
            ContractError::WrongName { expected, found } => match found {
                Some(found) => write!(
                    f,
                    "{expected}: the document says it is `{found}`, not `{expected}`"
                ),
                None => write!(f, "{expected}: the document names no contract"),
            },
            ContractError::AboveNegotiated {
                contract,
                found,
                negotiated,
            } => write!(
                f,
                "{contract}: the tool wrote schema {found}, above the schema {negotiated} the handshake settled on \
                 (the version the lock records for the pin)"
            ),
            ContractError::Invalid {
                contract,
                version,
                first_error,
            } => write!(
                f,
                "{contract}: the document does not match schema {version}: {first_error}"
            ),
        }
    }
}

impl std::error::Error for ContractError {}

/// A contract: the schema snapshots of its versions.
pub struct Contract {
    name: String,
    legacy_version: Option<u32>,
    versions: Vec<(u32, jsonschema::Validator)>,
}

impl fmt::Debug for Contract {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Contract")
            .field("name", &self.name)
            .field("versions", &self.versions())
            .finish_non_exhaustive()
    }
}

impl Contract {
    /// Build a contract from the texts of its schemas, `(version, schema text)`. `legacy_version` is what a
    /// document without `schema_version` is read as, for a tool that did not emit one at first.
    ///
    /// # Errors
    /// A reason, when a schema is not JSON, is not a valid JSON Schema, or no schema was given.
    pub fn new(
        name: &str,
        legacy_version: Option<u32>,
        schemas: &[(u32, &str)],
    ) -> Result<Contract, String> {
        if schemas.is_empty() {
            return Err(format!("{name}: a contract needs at least one schema"));
        }
        let mut versions = Vec::new();
        for (version, text) in schemas {
            let value: Value = serde_json::from_str(text)
                .map_err(|e| format!("{name} v{version}: the schema is not JSON: {e}"))?;
            let validator = jsonschema::validator_for(&value)
                .map_err(|e| format!("{name} v{version}: the schema is not valid: {e}"))?;
            versions.push((*version, validator));
        }
        versions.sort_by_key(|(v, _)| *v);
        Ok(Contract {
            name: name.to_owned(),
            legacy_version,
            versions,
        })
    }

    /// The contract's name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The newest version this build knows.
    #[must_use]
    pub fn current(&self) -> u32 {
        self.versions.last().map_or(0, |(v, _)| *v)
    }

    /// The versions that are accepted: the current one and the one before it, when a snapshot of it exists.
    #[must_use]
    pub fn accepted(&self) -> Vec<u32> {
        let current = self.current();
        self.versions
            .iter()
            .map(|(v, _)| *v)
            .filter(|v| *v == current || *v + 1 == current)
            .collect()
    }

    /// Every version with a snapshot.
    #[must_use]
    pub fn versions(&self) -> Vec<u32> {
        self.versions.iter().map(|(v, _)| *v).collect()
    }

    /// Check `document` against its own schema version, and return that version.
    ///
    /// # Errors
    /// See [`ContractError`]: a newer version is refused, a version older than the previous one is refused, and
    /// anything that does not validate is reported with its first error.
    pub fn check(&self, document: &Value) -> Result<u32, ContractError> {
        self.check_at(document, None, false)
    }

    /// Like [`Contract::check`], with what a handshake settled: `ceiling` is the version negotiated, and a document
    /// of a higher version is refused; `require_name` makes a document without `contract` an error (a tool that has
    /// answered the handshake must name its contract). A document that names another contract is always refused.
    ///
    /// # Errors
    /// See [`ContractError`].
    pub fn check_at(
        &self,
        document: &Value,
        ceiling: Option<u32>,
        require_name: bool,
    ) -> Result<u32, ContractError> {
        match document.get("contract") {
            Some(Value::String(name)) if *name == self.name => {}
            Some(other) => {
                return Err(ContractError::WrongName {
                    expected: self.name.clone(),
                    found: Some(
                        other
                            .as_str()
                            .map_or_else(|| other.to_string(), str::to_owned),
                    ),
                });
            }
            None if require_name => {
                return Err(ContractError::WrongName {
                    expected: self.name.clone(),
                    found: None,
                });
            }
            None => {}
        }
        let version = self.check_version(document)?;
        if let Some(negotiated) = ceiling
            && version > negotiated
        {
            return Err(ContractError::AboveNegotiated {
                contract: self.name.clone(),
                found: version,
                negotiated,
            });
        }
        Ok(version)
    }

    fn check_version(&self, document: &Value) -> Result<u32, ContractError> {
        let found = match document.get("schema_version") {
            Some(v) => v
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .ok_or_else(|| ContractError::NoVersion {
                    contract: self.name.clone(),
                })?,
            None => self
                .legacy_version
                .ok_or_else(|| ContractError::NoVersion {
                    contract: self.name.clone(),
                })?,
        };
        let current = self.current();
        if found > current {
            return Err(ContractError::Newer {
                contract: self.name.clone(),
                found,
                current,
            });
        }
        let Some((_, validator)) = self
            .versions
            .iter()
            .find(|(v, _)| *v == found && (*v == current || *v + 1 == current))
        else {
            return Err(ContractError::TooOld {
                contract: self.name.clone(),
                found,
                oldest: self.accepted().first().copied().unwrap_or(current),
            });
        };
        match validator.iter_errors(document).next() {
            None => Ok(found),
            Some(error) => Err(ContractError::Invalid {
                contract: self.name.clone(),
                version: found,
                first_error: format!("{error} (at `{}`)", error.instance_path),
            }),
        }
    }
}

/// The built-in contract called `name` (see [`SNAPSHOTS`]), with every snapshot of that name.
///
/// # Panics
/// Never for a compiled-in snapshot: the tests check that each one is a valid schema.
#[must_use]
pub fn builtin(name: &str) -> Option<Contract> {
    let mine: Vec<&Snapshot> = SNAPSHOTS.iter().filter(|s| s.contract == name).collect();
    let first = mine.first()?;
    let schemas: Vec<(u32, &str)> = mine.iter().map(|s| (s.version, s.text)).collect();
    let legacy = first.legacy.then_some(1);
    Some(Contract::new(name, legacy, &schemas).expect("a compiled-in snapshot is a valid schema"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sha256_hex;
    use serde_json::json;
    use std::path::Path;

    const V1: &str = r#"{"type":"object","required":["schema_version","a"],"properties":{"schema_version":{"const":1},"a":{"type":"string"}}}"#;
    const V2: &str = r#"{"type":"object","required":["schema_version","b"],"properties":{"schema_version":{"const":2},"b":{"type":"integer"}}}"#;

    fn two() -> Contract {
        Contract::new("demo", None, &[(1, V1), (2, V2)]).unwrap()
    }

    #[test]
    fn the_current_and_the_previous_version_are_accepted() {
        let c = two();
        assert_eq!(c.current(), 2);
        assert_eq!(c.accepted(), [1, 2]);
        assert_eq!(c.check(&json!({"schema_version":2,"b":1})), Ok(2));
        assert_eq!(c.check(&json!({"schema_version":1,"a":"x"})), Ok(1));
    }

    #[test]
    fn a_newer_version_is_refused_with_a_reason() {
        let why = two().check(&json!({"schema_version":3})).unwrap_err();
        assert!(matches!(
            why,
            ContractError::Newer {
                found: 3,
                current: 2,
                ..
            }
        ));
        let text = why.to_string();
        assert!(text.contains("demo") && text.contains("schema 3") && text.contains("newer"));
    }

    #[test]
    fn a_version_before_the_previous_one_is_refused() {
        let three = r#"{"type":"object","properties":{"schema_version":{"const":3}}}"#;
        let c = Contract::new("demo", None, &[(1, V1), (2, V2), (3, three)]).unwrap();
        assert_eq!(c.accepted(), [2, 3]);
        let why = c.check(&json!({"schema_version":1,"a":"x"})).unwrap_err();
        assert!(
            matches!(
                why,
                ContractError::TooOld {
                    found: 1,
                    oldest: 2,
                    ..
                }
            ),
            "{why}"
        );
    }

    #[test]
    fn an_invalid_document_reports_the_contract_and_the_first_error() {
        let why = two()
            .check(&json!({"schema_version":2,"b":"no"}))
            .unwrap_err();
        let text = why.to_string();
        assert!(
            text.contains("demo") && text.contains("schema 2") && text.contains("/b"),
            "{text}"
        );
    }

    #[test]
    fn a_missing_version_is_read_as_the_legacy_one_or_refused() {
        let legacy = Contract::new("demo", Some(1), &[(1, V1)]).unwrap();
        // Legacy 1 passes the version gate; the schema then decides (it requires the field, so this fails).
        assert!(matches!(
            legacy.check(&json!({"a":"x"})),
            Err(ContractError::Invalid { version: 1, .. })
        ));
        let strict = Contract::new("demo", None, &[(1, V1)]).unwrap();
        for doc in [
            json!({"a":"x"}),
            json!({"schema_version":"1"}),
            json!({"schema_version":-1}),
        ] {
            assert!(
                matches!(strict.check(&doc), Err(ContractError::NoVersion { .. })),
                "{doc}"
            );
        }
    }

    #[test]
    fn bad_schemas_are_rejected_when_the_contract_is_built() {
        assert!(Contract::new("demo", None, &[]).is_err());
        assert!(Contract::new("demo", None, &[(1, "{")]).is_err());
        assert!(Contract::new("demo", None, &[(1, r#"{"type":"nonsense"}"#)]).is_err());
    }

    #[test]
    fn every_compiled_in_snapshot_is_a_valid_schema_and_a_builtin() {
        for s in SNAPSHOTS {
            let c = builtin(s.contract).unwrap();
            assert!(c.versions().contains(&s.version), "{}", s.file);
        }
        assert!(builtin("nonsense").is_none());
    }

    /// The hash file of each tool folder lists exactly the snapshots, with their SHA-256.
    #[test]
    fn the_hash_files_match_the_snapshots() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../contracts");
        let mut tools: Vec<&str> = SNAPSHOTS.iter().map(|s| s.tool).collect();
        tools.sort_unstable();
        tools.dedup();
        for tool in tools {
            let sums = std::fs::read_to_string(root.join(tool).join("SHA256SUMS")).unwrap();
            let mut listed: Vec<(String, String)> = sums
                .lines()
                .map(|l| {
                    let (hash, name) = l.split_once("  ").unwrap();
                    (name.to_owned(), hash.to_owned())
                })
                .collect();
            listed.sort();
            let mut actual: Vec<(String, String)> = SNAPSHOTS
                .iter()
                .filter(|s| s.tool == tool)
                .map(|s| (s.file.to_owned(), sha256_hex(s.text.as_bytes())))
                .collect();
            actual.sort();
            assert_eq!(listed, actual, "contracts/{tool}/SHA256SUMS");
        }
    }

    /// With `NVMOD_PUBLISHED_SCHEMAS=<dir>` (a folder holding each tool's published schema files, as
    /// `<dir>/<tool>/<file>`), every snapshot must be byte-identical to the published one. This is the check a
    /// tool bump pull request runs against the pinned release; without the variable it has nothing to compare.
    #[test]
    fn snapshots_equal_the_published_schemas_when_a_source_is_given() {
        let Some(dir) = std::env::var_os("NVMOD_PUBLISHED_SCHEMAS") else {
            return;
        };
        for s in SNAPSHOTS {
            let path = Path::new(&dir).join(s.tool).join(s.file);
            let published = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            assert_eq!(
                published, s.text,
                "contracts/{}/{} differs from the published schema",
                s.tool, s.file
            );
        }
    }
}
