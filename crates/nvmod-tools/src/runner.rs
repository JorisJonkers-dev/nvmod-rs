// SPDX-License-Identifier: GPL-3.0-only
//! Calls of a tool whose standard output is a versioned contract document, with the handshake that settles the
//! version first. The subprocess itself is run by `nvmod-runner` (ADR-0005); this adds the schema check of the
//! answer and maps a newer contract version onto the refusal (exit code 2).

use crate::{Contract, ContractError, Handshake, negotiate};
use nvmod_runner::{Call, Runner, ToolError};
use serde_json::Value;

/// Run `call`, parse its standard output as JSON and check it against `contract`. Returns the contract version the
/// document speaks and the document.
///
/// # Errors
/// Everything [`Runner::run`] reports, plus `Malformed` when the output is not JSON or does not match the schema
/// of its version (the report names the tool and the first error), and `Refused` when the tool speaks a newer
/// version than nvmod reads.
pub fn call_contract(
    runner: &Runner,
    call: &Call,
    contract: &Contract,
) -> Result<(u32, Value), ToolError> {
    let output = runner.run(call)?;
    let malformed = |reason: String| ToolError::Malformed {
        tool: call.tool.to_owned(),
        reason,
    };
    let document: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| malformed(format!("the output is not JSON: {e}")))?;
    finish(call, contract.check(&document), document)
}

/// What a handshake asks of a call, see [`call_negotiated`].
#[derive(Debug, Clone, Copy)]
pub struct Negotiation {
    /// The version the lock records for the pin: the handshake never settles above it.
    pub lock_version: u32,
    /// Whether the pinned release must answer `contracts --json` (see `ToolEntry::handshake_required`). A pin made
    /// before the handshake existed is grandfathered: nothing is asked of the tool, and the lock implies its
    /// contract.
    pub required: bool,
}

/// Run the handshake first (`contracts --json` through the same program), settle on the highest version both sides
/// list that is not above the lock's, then run `call` and check its document against `contract` at that version,
/// with the contract's name required. Returns the version spoken and the document. With `negotiation.required`
/// false this is [`call_contract`].
///
/// # Errors
/// What [`call_contract`] reports, plus `Unverifiable` (exit code 3) when the handshake cannot be run or read, or
/// when the lock's version is not in the tool's list or no version is common, and `Refused` when the document is of
/// a version above the negotiated one.
pub fn call_negotiated(
    runner: &Runner,
    call: &Call,
    contract: &Contract,
    negotiation: Negotiation,
) -> Result<(u32, Value), ToolError> {
    if !negotiation.required {
        return call_contract(runner, call, contract);
    }
    let unverifiable = |reason: String| ToolError::Unverifiable {
        tool: call.tool.to_owned(),
        reason,
    };
    let words = ["contracts".to_owned(), "--json".to_owned()];
    let mut ask = Call::new(call.tool, call.program, &words);
    ask.expected_sha256 = call.expected_sha256;
    let answer = runner.run(&ask).map_err(|e| match e {
        ToolError::Failed { reason, .. } => {
            unverifiable(format!("it does not answer `contracts --json`: {reason}"))
        }
        other => other,
    })?;
    let text = String::from_utf8(answer.stdout)
        .map_err(|_| unverifiable("its handshake is not UTF-8".to_owned()))?;
    let handshake = Handshake::parse(&text).map_err(unverifiable)?;
    let version = negotiate(
        call.tool,
        contract.name(),
        negotiation.lock_version,
        &contract.accepted(),
        &handshake,
    )?;
    let output = runner.run(call)?;
    let document: Value =
        serde_json::from_slice(&output.stdout).map_err(|e| ToolError::Malformed {
            tool: call.tool.to_owned(),
            reason: format!("the output is not JSON: {e}"),
        })?;
    let checked = contract.check_at(&document, Some(version), true);
    finish(call, checked, document)
}

fn finish(
    call: &Call,
    checked: Result<u32, ContractError>,
    document: Value,
) -> Result<(u32, Value), ToolError> {
    let malformed = |reason: String| ToolError::Malformed {
        tool: call.tool.to_owned(),
        reason,
    };
    match checked {
        Ok(version) => Ok((version, document)),
        Err(
            e @ (ContractError::Newer { .. }
            | ContractError::TooOld { .. }
            | ContractError::AboveNegotiated { .. }),
        ) => Err(ToolError::Refused {
            tool: call.tool.to_owned(),
            reason: e.to_string(),
        }),
        Err(e) => Err(malformed(e.to_string())),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use nvmod_testkit::TempDir;
    use serde_json::json;
    use std::path::PathBuf;
    use std::time::Duration;

    fn script(dir: &TempDir, name: &str, body: &str) -> PathBuf {
        nvmod_testkit::write_script(dir.path(), name, body)
    }

    fn fast() -> Runner {
        Runner {
            timeout: Duration::from_secs(10),
            ..Runner::default()
        }
    }

    fn lodwright() -> Contract {
        crate::builtin("lodwright-plan").unwrap()
    }

    #[test]
    fn contract_calls_validate_the_output_and_name_the_tool_and_the_first_error() {
        let dir = TempDir::new("run-contract");
        let good = json!({"worlds": []}).to_string();
        let p = script(&dir, "good", &format!("echo '{good}'"));
        assert_eq!(
            call_contract(&fast(), &Call::new("lodwright-rs", &p, &[]), &lodwright())
                .unwrap()
                .0,
            1
        );

        let bad = json!({"worlds": [{"name": 3}]}).to_string();
        let p = script(&dir, "bad", &format!("echo '{bad}'"));
        let why =
            call_contract(&fast(), &Call::new("lodwright-rs", &p, &[]), &lodwright()).unwrap_err();
        assert!(
            matches!(&why, ToolError::Malformed { tool, reason } if tool == "lodwright-rs" && reason.contains("lodwright-plan")),
            "{why}"
        );
        assert_eq!(why.exit_code(), 4);

        let p = script(&dir, "text", "echo not json");
        let why =
            call_contract(&fast(), &Call::new("lodwright-rs", &p, &[]), &lodwright()).unwrap_err();
        assert!(matches!(why, ToolError::Malformed { .. }), "{why}");
    }

    #[test]
    fn a_newer_contract_version_is_refused_with_exit_2() {
        let dir = TempDir::new("run-newer");
        let p = script(&dir, "t", r#"echo '{"schema_version":2,"worlds":[]}'"#);
        let why =
            call_contract(&fast(), &Call::new("lodwright-rs", &p, &[]), &lodwright()).unwrap_err();
        assert!(
            matches!(&why, ToolError::Refused { reason, .. } if reason.contains("newer")),
            "{why}"
        );
        assert_eq!(why.exit_code(), 2);
    }
}
