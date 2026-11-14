// SPDX-License-Identifier: GPL-3.0-only
//! The thin wrapper of the TTW installers: it reads what an installer left (its exit code and, when it wrote one, a
//! report file) into the normalised `ttw-installer-result` record, and checks the record against its contract. The
//! installers cannot be changed, so nvmod owns the record (`docs/architecture/integration.md`, section 4).
//!
//! The report file is optional and read leniently: every field of it is a field of the record, and a field the
//! installer left out is filled in here. Whatever the installer does write goes through the contract unchanged, so
//! a field of the wrong type is a malformed response, not a guess.

use crate::{ToolError, builtin};
use serde_json::{Map, Value, json};
use std::path::Path;

/// The tool name and the contract of the TTW installers.
pub const TOOL: &str = "ttw-installer";
/// The contract the record must meet.
pub const CONTRACT: &str = "ttw-installer-result";

/// What the wrapper answers to the handshake for the installers, which cannot answer it themselves: the one contract
/// it writes and the versions of it (the same document `contracts --json` prints for a tool that has the command).
///
/// # Panics
/// Never: the contract's snapshot is compiled in.
#[must_use]
pub fn handshake() -> Value {
    let versions = builtin(CONTRACT)
        .expect("the ttw-installer-result snapshot is compiled in")
        .accepted();
    json!({"contracts": [{"name": CONTRACT, "versions": versions}]})
}

/// What nvmod knows about the run besides the installer's own words.
#[derive(Debug, Clone, Copy)]
pub struct Facts<'a> {
    /// The installer's name and version as pinned in the lock, used when its report names none.
    pub installer: (&'a str, &'a str),
    /// The installer's exit code.
    pub exit_code: i32,
    /// The folder the installer wrote into.
    pub output_dir: &'a Path,
    /// The files nvmod found there.
    pub files_found: u64,
}

fn malformed(reason: String) -> ToolError {
    ToolError::Malformed {
        tool: TOOL.to_owned(),
        reason,
    }
}

/// Build the normalised record from the facts and the report text (if the installer wrote one) and check it
/// against the contract.
///
/// Without a report, a run that exited 0 is a success with the Fallout 3 verification taken as passed: the
/// installers check the games before they build and exit with an error when a check fails (an unverified
/// assumption, see `docs/DECISIONS.md`). A report may say otherwise, and then it wins.
///
/// # Errors
/// `Malformed` when the report is not a JSON object or the record breaks the contract.
///
/// # Panics
/// Never: the contract's snapshot is compiled in.
pub fn normalise(facts: &Facts, report: Option<&str>) -> Result<Value, ToolError> {
    let mut record: Map<String, Value> = match report {
        None => Map::new(),
        Some(text) => match serde_json::from_str::<Value>(text) {
            Ok(Value::Object(map)) => map,
            Ok(_) => return Err(malformed("the report is not a JSON object".to_owned())),
            Err(e) => return Err(malformed(format!("the report is not JSON: {e}"))),
        },
    };
    let ok = facts.exit_code == 0;
    record.insert("schema_version".to_owned(), json!(1));
    record.insert("contract".to_owned(), json!(CONTRACT));
    record
        .entry("installer")
        .or_insert_with(|| json!({"name": facts.installer.0, "version": facts.installer.1}));
    record.insert("exit_code".to_owned(), json!(facts.exit_code));
    record
        .entry("outcome")
        .or_insert_with(|| json!(if ok { "success" } else { "failed" }));
    record
        .entry("fallout3_verified")
        .or_insert_with(|| json!(ok));
    record.insert(
        "output_dir".to_owned(),
        json!(facts.output_dir.display().to_string()),
    );
    record
        .entry("files_written")
        .or_insert_with(|| json!(facts.files_found));
    record.entry("messages").or_insert_with(|| json!([]));
    let record = Value::Object(record);
    let contract = builtin(CONTRACT).expect("the ttw-installer-result snapshot is compiled in");
    match contract.check(&record) {
        Ok(_) => Ok(record),
        Err(e) => Err(malformed(e.to_string())),
    }
}

/// Why a record that meets the contract is still not a good install, or `None` when it is one.
#[must_use]
pub fn failure(record: &Value) -> Option<String> {
    let said = record["messages"]
        .as_array()
        .map(|m| {
            m.iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("; ")
        })
        .unwrap_or_default();
    let why = if record["outcome"] != "success" {
        "the installer reports that it failed"
    } else if record["fallout3_verified"] != true {
        "the installer did not verify Fallout 3"
    } else if record["exit_code"] != 0 {
        "the installer exited with an error"
    } else {
        return None;
    };
    Some(if said.is_empty() {
        why.to_owned()
    } else {
        format!("{why}: {said}")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn facts(exit_code: i32) -> Facts<'static> {
        Facts {
            installer: ("ttw", "1"),
            exit_code,
            output_dir: Path::new("/out"),
            files_found: 3,
        }
    }

    #[test]
    fn no_report_and_exit_0_is_a_success() {
        let r = normalise(&facts(0), None).unwrap();
        assert_eq!(failure(&r), None);
        assert_eq!(r["files_written"], 3);
        assert_eq!(r["installer"]["name"], "ttw");
    }

    #[test]
    fn a_report_can_fail_a_run_that_exited_0() {
        let r = normalise(
            &facts(0),
            Some(r#"{"fallout3_verified":false,"messages":["bad edition"]}"#),
        )
        .unwrap();
        assert_eq!(
            failure(&r).unwrap(),
            "the installer did not verify Fallout 3: bad edition"
        );
    }

    #[test]
    fn a_nonzero_exit_is_a_failure_and_wrong_types_are_malformed() {
        let r = normalise(&facts(2), None).unwrap();
        assert!(failure(&r).is_some());
        for bad in ["[]", "x", r#"{"messages":"no"}"#] {
            assert!(matches!(
                normalise(&facts(0), Some(bad)),
                Err(ToolError::Malformed { .. })
            ));
        }
    }
}
