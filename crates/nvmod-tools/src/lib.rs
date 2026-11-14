// SPDX-License-Identifier: GPL-3.0-only
//! The external tools nvmod drives and how it trusts what they say: versioned contracts (a JSON Schema snapshot
//! per contract and version, with the rule that the current and the previous version are accepted and a newer
//! one is refused), and the one subprocess runner every tool call goes through (an argument vector, a timeout, an
//! output cap, a scrubbed environment, the tool's checksum re-verified before each run).

mod contract;
mod runner;
mod sha256;

pub use contract::{Contract, ContractError, SNAPSHOTS, Snapshot, builtin};
pub use runner::{Call, Output, Runner, ToolError, call_contract};
pub use sha256::sha256_hex;

/// Exit code of a command that was refused before it changed anything (the exit code table).
pub const EXIT_REFUSED: u8 = 2;
/// Exit code of a verification failure, such as a tool that cannot be verified.
pub const EXIT_VERIFICATION: u8 = 3;
/// Exit code of a tool that failed.
pub const EXIT_TOOL_FAILED: u8 = 4;
