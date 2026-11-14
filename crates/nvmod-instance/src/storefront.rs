// SPDX-License-Identifier: GPL-3.0-only
//! Storefront detection of a base copy, from marker files. The markers are the ones the research notes name; they
//! are unverified against real copies of every edition (see `docs/DECISIONS.md`).

use serde::{Deserialize, Serialize};
use std::fmt;
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

/// Where a base copy came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Storefront {
    /// Steam: `steam_api.dll` beside the executable.
    Steam,
    /// GOG: `goggame-1454587428.info` or `goglog.ini`.
    Gog,
    /// Epic: a `.egstore` folder. Not supported.
    Epic,
    /// No marker of a known store. Not supported.
    Unknown,
}

impl Storefront {
    /// Detect the storefront of the base copy in `dir`. Epic is checked first because its copies can carry a
    /// Steam-like library too; then GOG, then Steam.
    #[must_use]
    pub fn detect(dir: &Path) -> Storefront {
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

    /// The name for people.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Storefront::Steam => "Steam",
            Storefront::Gog => "GOG",
            Storefront::Epic => "Epic",
            Storefront::Unknown => "unknown storefront",
        }
    }

    /// Why this base copy is refused, or `None` when it is supported.
    #[must_use]
    pub fn refusal(self) -> Option<&'static str> {
        match self {
            Storefront::Steam | Storefront::Gog => None,
            Storefront::Epic => Some(
                "this is an Epic copy of the game; nvmod installs from a Steam or GOG base copy only",
            ),
            Storefront::Unknown => Some(
                "the storefront of this copy could not be recognised; nvmod installs from a Steam or GOG base copy only",
            ),
        }
    }
}

impl fmt::Display for Storefront {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{BaseBuilder, TempDir};

    #[test]
    fn each_fixture_is_detected_and_only_steam_and_gog_are_supported() {
        for (builder, store, ok) in [
            (BaseBuilder::steam(), Storefront::Steam, true),
            (BaseBuilder::gog(), Storefront::Gog, true),
            (BaseBuilder::epic(), Storefront::Epic, false),
            (BaseBuilder::unknown(), Storefront::Unknown, false),
        ] {
            let dir = TempDir::new("store");
            builder.write(dir.path());
            assert_eq!(Storefront::detect(dir.path()), store);
            assert_eq!(store.refusal().is_none(), ok);
            assert_eq!(store.to_string(), store.label());
        }
    }

    #[test]
    fn markers_are_found_ignoring_case() {
        let dir = TempDir::new("store-case");
        std::fs::write(dir.path().join("STEAM_API.DLL"), b"x").unwrap();
        assert_eq!(Storefront::detect(dir.path()), Storefront::Steam);
    }
}
