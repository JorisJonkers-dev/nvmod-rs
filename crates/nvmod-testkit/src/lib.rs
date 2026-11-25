// SPDX-License-Identifier: GPL-3.0-only
//! Builders for test data. Every file the tests, golden files and fuzz seeds use is made here, in code: real game
//! files and mod files never enter the repository (`scripts/repo_guards.py` enforces it).

mod archive;
mod base;
mod bsa;
mod case;
mod links;
mod mo2;
mod nvrs;
mod pe;
mod plugin;
mod save;
mod standin;

// The ids of the feature catalogue (catalogue/, docs/TESTING.md "Feature catalogue"), written by build.rs: a test says
// which promises it proves with `covers(&[ids::area::group::ITEM])`, and an id the catalogue lacks does not compile.
#[allow(missing_docs)]
mod tally {
    include!(concat!(env!("OUT_DIR"), "/tally_ids.rs"));
}
pub use tally::{Id, covers, ids};

pub use archive::{TarBuilder, ZipBuilder};
pub use base::{BaseBuilder, Store, snapshot};
pub use bsa::BsaBuilder;
pub use case::case_sensitive;
pub use links::{link_count, same_file};
pub use mo2::{Mo2Builder, ModSpec, TempDir, write_file};
pub use nvrs::{
    FAKE_ASSET, FAKE_BUILT_BANNER, FAKE_CHECK_REFUSED, FAKE_FAILING_RELEASE_BINARY,
    FAKE_RELEASE_BINARY, FakeNvRs, FakeRelease, FakeServer, Reply, capabilities_json,
    fake_failing_release_binary, fake_release_binary, nv_viewer_name, write_executable,
    write_script,
};
pub use pe::{ExportSpec, PeBuilder};
pub use plugin::{PluginBuilder, ScdaBuilder};
pub use save::SaveBuilder;
pub use standin::Standin;
