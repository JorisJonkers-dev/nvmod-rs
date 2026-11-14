// SPDX-License-Identifier: GPL-3.0-only
//! The external tools nvmod drives and how it trusts what they say: versioned contracts (a JSON Schema snapshot
//! per contract and version, with the rule that the current and the previous version are accepted and a newer
//! one is refused), and the one subprocess runner every tool call goes through (an argument vector, a timeout, an
//! output cap, a scrubbed environment, the tool's checksum re-verified before each run).

mod acquire;
mod capabilities;
mod contract;
mod fetch;
mod lock;
pub mod lodwright;
mod negotiate;
mod runner;
mod sha256;
mod source;
mod store;
mod trust;
pub mod ttw;
mod verify;

pub use acquire::{Acquire, AcquireError, Acquired, Pin, pin_of};
pub use capabilities::{Capabilities, CapabilitiesError, Coverage};
pub use contract::{Contract, ContractError, SNAPSHOTS, Snapshot, builtin};
pub use fetch::{Curl, FetchError, Fetcher, RETRIES, TIMEOUT_SECS, get_with_retries};
pub use lock::{
    Asset, AttestationPin, Kind, LOCK_SCHEMA_VERSION, Lock, SourcePin, ToolEntry, builtin_lock,
    platform,
};
pub use negotiate::{Handshake, negotiate};
pub use runner::{Call, Negotiation, Output, Runner, ToolError, call_contract, call_negotiated};
pub use sha256::sha256_hex;
pub use source::{SourceCheck, verify_source_build};
pub use store::{Store, StoreState, exe_file};
pub use trust::{Confirmed, TRUST_FILE, Trust};
pub use verify::{Attestation, VerifyError, check_sha256, fetch_bytes, verify_attestation};

/// Exit code of a command that was refused before it changed anything (the exit code table).
pub const EXIT_REFUSED: u8 = 2;
/// Exit code of a verification failure, such as a tool that cannot be verified.
pub const EXIT_VERIFICATION: u8 = 3;
/// Exit code of a tool that failed.
pub const EXIT_TOOL_FAILED: u8 = 4;
