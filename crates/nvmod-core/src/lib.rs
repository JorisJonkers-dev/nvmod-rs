// SPDX-License-Identifier: GPL-3.0-only
//! Shared types of nvmod: how serious a finding is, what a finding looks like, and the case-insensitive index
//! of the files that mods provide. No input or output happens here; the crates that read files fill these types.

mod finding;
mod index;

pub use finding::{Finding, ParseSeverityError, Severity};
pub use index::{FileIndex, Provider, normalize_path};
