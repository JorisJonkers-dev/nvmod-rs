// SPDX-License-Identifier: GPL-3.0-only
//! Builders for test data. Every file the tests, golden files and fuzz seeds use is made here, in code: real game
//! files and mod files never enter the repository (`scripts/repo_guards.py` enforces it).

mod archive;
mod base;
mod mo2;
mod nvrs;
mod pe;
mod plugin;

pub use archive::{TarBuilder, ZipBuilder};
pub use base::{BaseBuilder, Store, snapshot};
pub use mo2::{Mo2Builder, ModSpec, TempDir, write_file};
pub use nvrs::{
    FAKE_ASSET, FAKE_BUILT_BANNER, FAKE_CHECK_REFUSED, FAKE_FAILING_RELEASE_BINARY,
    FAKE_RELEASE_BINARY, FakeNvRs, FakeRelease, FakeServer, Reply, capabilities_json, write_script,
};
pub use pe::{ExportSpec, PeBuilder};
pub use plugin::{PluginBuilder, ScdaBuilder};
