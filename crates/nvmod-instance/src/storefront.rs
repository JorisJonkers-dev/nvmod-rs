// SPDX-License-Identifier: GPL-3.0-only
//! Storefront detection of a base copy, from marker files. The markers are the ones the research notes name; they
//! are unverified against real copies of every edition (see `docs/DECISIONS.md`).

use nvmod_core::Storefront;
use std::path::{Path, PathBuf};

/// The entry in `dir` called `name`, ignoring case (the game's folders are case-insensitive on Windows).
#[must_use]
pub fn find_ci(dir: &Path, name: &str) -> Option<PathBuf> {
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
}

/// Detect the storefront of the base copy in `dir`, from marker files. Epic is checked first because its copies
/// can carry a Steam-like library too; then GOG, then Steam.
#[must_use]
pub fn detect_storefront(dir: &Path) -> Storefront {
    let has = |name| find_ci(dir, name).is_some();
    if has(".egstore") {
        Storefront::Epic
    } else if has("goggame-1454587428.info") || has("goglog.ini") {
        Storefront::Gog
    } else if has("steam_api.dll") {
        Storefront::Steam
    } else {
        Storefront::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{BaseBuilder, TempDir};
    use nvmod_testkit::{covers, ids};

    #[test]
    fn each_fixture_is_detected_and_only_steam_and_gog_are_supported() {
        covers(&[ids::instance::init::STOREFRONTS]);
        for (builder, store, ok) in [
            (BaseBuilder::steam(), Storefront::Steam, true),
            (BaseBuilder::gog(), Storefront::Gog, true),
            (BaseBuilder::epic(), Storefront::Epic, false),
            (BaseBuilder::unknown(), Storefront::Unknown, false),
        ] {
            let dir = TempDir::new("store");
            builder.write(dir.path());
            assert_eq!(detect_storefront(dir.path()), store);
            assert_eq!(store.refusal().is_none(), ok);
        }
    }

    #[test]
    fn markers_are_found_ignoring_case() {
        let dir = TempDir::new("store-case");
        std::fs::write(dir.path().join("STEAM_API.DLL"), b"x").unwrap();
        assert_eq!(detect_storefront(dir.path()), Storefront::Steam);
    }
}
