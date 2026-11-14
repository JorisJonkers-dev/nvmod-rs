// SPDX-License-Identifier: GPL-3.0-only
//! Builders for test data. Every file the tests, golden files and fuzz seeds use is made here, in code: real game
//! files and mod files never enter the repository (`scripts/repo_guards.py` enforces it).

mod pe;

pub use pe::{ExportSpec, PeBuilder};
