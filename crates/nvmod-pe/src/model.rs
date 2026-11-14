// SPDX-License-Identifier: GPL-3.0-only
//! What the inspector reports.

use serde::{Serialize, Serializer};
use std::fmt;

/// A four-part file version such as `1.4.0.525`. Compares part by part, most significant first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Version4(pub [u16; 4]);

impl Version4 {
    /// Parse `a.b.c.d` (missing trailing parts count as zero, at most four parts).
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = [0u16; 4];
        for (n, piece) in text.trim().split('.').enumerate() {
            *parts.get_mut(n)? = piece.parse().ok()?;
        }
        Some(Version4(parts))
    }
}

impl fmt::Display for Version4 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [major, minor, patch, build] = self.0;
        write!(f, "{major}.{minor}.{patch}.{build}")
    }
}

impl Serialize for Version4 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

/// The two layouts of the optional header.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PeFormat {
    /// 32-bit (`PE32`, magic `0x10b`).
    Pe32,
    /// 64-bit (`PE32+`, magic `0x20b`).
    Pe32Plus,
}

/// One named export.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Export {
    /// The exported name (a C symbol, case sensitive).
    pub name: String,
    /// The relative virtual address it points at.
    pub rva: u32,
    /// True when the address is a forwarder string, not code or data.
    pub forwarded: bool,
}

/// The exported `NVSEPlugin_Version` data, as far as the doctor reads it. See `docs/DECISIONS.md`, "Unverified
/// assumptions": the layout (two little-endian `u32` first) is from memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct VersionStruct {
    /// First field: the version of the struct itself.
    pub info_version: u32,
    /// Second field: the plugin's own version number.
    pub version: u32,
}

/// What the exports say about NVSE.
#[allow(clippy::struct_excessive_bools)] // independent facts, reported as such in JSON
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize)]
pub struct NvseInfo {
    /// Any of the three NVSE entry points is exported.
    pub is_plugin: bool,
    /// Exports `NVSEPlugin_Query` (the older way of declaring a plugin).
    pub has_query: bool,
    /// Exports `NVSEPlugin_Load`.
    pub has_load: bool,
    /// Exports the data symbol `NVSEPlugin_Version` (the newer way).
    pub has_version_export: bool,
    /// The two leading fields of that symbol, when they could be read.
    pub version_struct: Option<VersionStruct>,
}

/// Everything the inspector learned about one PE file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PeReport {
    /// COFF machine type (`0x14c` is 32-bit x86).
    pub machine: u16,
    /// Name of the machine type.
    pub machine_name: String,
    /// 32-bit or 64-bit optional header.
    pub format: PeFormat,
    /// The DLL flag is set.
    pub is_dll: bool,
    /// The large-address-aware flag is set (the process may use more than 2 GiB).
    pub large_address_aware: bool,
    /// Optional header subsystem.
    pub subsystem: u16,
    /// Link time stamp (seconds since 1970; not trustworthy, reported as is).
    pub timestamp: u32,
    /// Number of sections.
    pub sections: u16,
    /// File version from the version resource, when there is one.
    pub file_version: Option<Version4>,
    /// Product version from the version resource, when there is one.
    pub product_version: Option<Version4>,
    /// Named exports.
    pub exports: Vec<Export>,
    /// Names of the DLLs the file imports from.
    pub imports: Vec<String>,
    /// The NVSE view of the exports.
    pub nvse: NvseInfo,
    /// Parts of the file that could not be read, one line each. The rest of the report is still valid.
    pub issues: Vec<String>,
}

impl PeReport {
    /// True for a 32-bit x86 image, which is what the game and its plugins are.
    #[must_use]
    pub fn is_x86_32bit(&self) -> bool {
        self.machine == 0x014c && self.format == PeFormat::Pe32
    }
}

/// Why a file is not a PE file the inspector can use at all.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeError {
    /// The file could not be opened or read.
    Io(String),
    /// The file is not a PE file (the string says which check failed).
    NotPe(&'static str),
    /// The file ends inside a header.
    Truncated(&'static str),
    /// A header field is beyond what the inspector accepts.
    Unsupported(&'static str),
}

impl fmt::Display for PeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PeError::Io(why) => write!(f, "cannot read the file: {why}"),
            PeError::NotPe(why) => write!(f, "not a PE file: {why}"),
            PeError::Truncated(what) => write!(f, "file is cut off inside the {what}"),
            PeError::Unsupported(why) => write!(f, "unsupported PE file: {why}"),
        }
    }
}

impl std::error::Error for PeError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_parse_print_and_compare() {
        let v = Version4::parse("1.4.0.525").unwrap();
        assert_eq!(v.to_string(), "1.4.0.525");
        assert_eq!(Version4::parse(" 2.1 ").unwrap(), Version4([2, 1, 0, 0]));
        assert!(Version4::parse("1.2.3.4.5").is_none());
        assert!(Version4::parse("a.b").is_none());
        assert!(Version4::parse("70000").is_none());
        assert!(Version4([1, 4, 0, 525]) < Version4([1, 4, 1, 0]));
        assert_eq!(serde_json::to_string(&v).unwrap(), "\"1.4.0.525\"");
    }

    #[test]
    fn errors_say_what_failed() {
        assert!(PeError::Io("x".into()).to_string().contains('x'));
        assert!(PeError::NotPe("no MZ").to_string().contains("no MZ"));
        assert!(
            PeError::Truncated("COFF header")
                .to_string()
                .contains("COFF header")
        );
        assert!(
            PeError::Unsupported("too big")
                .to_string()
                .contains("too big")
        );
    }
}
