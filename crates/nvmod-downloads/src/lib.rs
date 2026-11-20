// SPDX-License-Identifier: GPL-3.0-only
//! The downloads library of an Instance: the archives in its downloads folder, read together with the `.meta`
//! file Mod Organizer 2 keeps beside each one (`<archive>.meta`, an INI file) and matched to the Mods installed in
//! the Instance.
//!
//! * A missing `.meta` is unknown provenance, not corruption: the archive is listed, its Nexus ids come from the
//!   Nexus file name pattern when the name has one, and its state is read from the Mods alone.
//! * A `.meta` is never re-serialised. The few keys nvmod sets (`installed`, `uninstalled`, `removed`) are edited
//!   in place with [`nvmod_formats::metaini::MetaIni`], so every other byte of the file stays as MO2 wrote it.
//! * Every file name this crate prints or takes is a bare name (never a directory, never a Windows path).
//! * Nothing here uses the network or starts a process. Hashing reads a file; deleting an archive moves it to the
//!   Instance's trash.

mod hash;
mod ops;
mod scan;

pub use hash::{Digests, HashCache, digests_of};
pub use ops::{MetaChange, change_meta, check_file_name, trash};
pub use scan::{
    Archive, IdSource, InstalledMod, ListOptions, ListReport, MatchedBy, MatchedMod,
    ModWithoutArchive, ShowReport, State, installed_mods, list, show,
};

use std::fmt;

/// The version of the `--json` layouts of [`ListReport`] and [`ShowReport`].
pub const SCHEMA_VERSION: u32 = 1;
/// What MO2 appends to an archive's file name for its meta file.
pub const META_SUFFIX: &str = ".meta";
/// What MO2 appends to the file name of a download that is not finished.
pub const UNFINISHED_SUFFIX: &str = ".unfinished";
/// The progress step of reading the folder.
pub const STEP_READ: &str = "read-downloads";
/// The progress step of hashing.
pub const STEP_HASH: &str = "hash";

/// Why a downloads call gave no result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    /// The caller asked to stop.
    Cancelled,
    /// Refused before anything was written: a name that is not a plain file name, a file that is not there.
    Refused(String),
    /// A read or write failed.
    Failed(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Cancelled => f.write_str("cancelled"),
            Error::Refused(why) | Error::Failed(why) => f.write_str(why),
        }
    }
}

impl std::error::Error for Error {}

/// The JSON Schema of [`ListReport`], as pretty JSON with a trailing newline (`schemas/downloads-list.v1.json`).
///
/// # Panics
/// Never in practice: a generated schema always serialises.
#[must_use]
pub fn list_schema() -> String {
    schema_text(&schemars::schema_for!(ListReport))
}

/// The JSON Schema of [`ShowReport`], as pretty JSON with a trailing newline (`schemas/downloads-show.v1.json`).
///
/// # Panics
/// Never in practice: a generated schema always serialises.
#[must_use]
pub fn show_schema() -> String {
    schema_text(&schemars::schema_for!(ShowReport))
}

fn schema_text(schema: &schemars::Schema) -> String {
    let mut text = serde_json::to_string_pretty(schema).expect("a schema serialises");
    text.push('\n');
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(file: &str, generated: &str) {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas")
            .join(file);
        if std::env::var_os("NVMOD_BLESS").is_some() {
            std::fs::write(&path, generated).unwrap();
        }
        let checked_in = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            checked_in == generated,
            "schemas/{file} differs from the generated schema; bump the schema version if the change is \
             intended and run the tests with NVMOD_BLESS=1 to rewrite it"
        );
    }

    #[test]
    fn the_checked_in_list_schema_is_the_generated_one() {
        check("downloads-list.v1.json", &list_schema());
    }

    #[test]
    fn the_checked_in_show_schema_is_the_generated_one() {
        check("downloads-show.v1.json", &show_schema());
    }
}
