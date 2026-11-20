// SPDX-License-Identifier: GPL-3.0-only
//! Installs nv-rs into an nvmod Instance: builds it from source when a Rust toolchain exists, otherwise fetches the
//! fork's release and verifies its checksum and attestation, then runs the installation check, `nv-viewer <data> --check`. Every step is
//! staged and rolled back on failure, and the game executable is never copied (see `docs/DECISIONS.md`).

// `Curl` is deprecated for callers (nvmod-rs#188); nvmod still uses it until it is removed.
#![allow(deprecated)]

mod build;
mod check;
mod install;
mod release;

pub use build::resolve_cargo;
pub use check::Check;
pub use install::{
    DEFAULT_ASSET, DEFAULT_RELEASE_URL, DEFAULT_REPO, InstallError, Installed, Method, Options,
    install,
};
pub use nvmod_tools::sha256_hex;
pub use nvmod_tools::{Curl, FetchError, Fetcher, RETRIES, TIMEOUT_SECS, get_with_retries};
