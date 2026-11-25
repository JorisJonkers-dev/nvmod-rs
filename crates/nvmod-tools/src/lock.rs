// SPDX-License-Identifier: GPL-3.0-only
//! `tools.lock.toml`: every external tool nvmod fetches or links, pinned. A tool entry carries its version, the asset
//! URL and SHA-256 per platform, the attestation identity (repository and workflow) and the contract versions it
//! speaks. The file is read strictly: a download that lacks a checksum or an attestation pin is refused when the
//! lock is read, unless the tool is marked third-party (see `docs/architecture/integration.md`, section 5).

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The schema version of the lock file this build reads.
pub const LOCK_SCHEMA_VERSION: u32 = 1;

/// What kind of tool an entry is.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// Fetched from the pinned URL; the checksum and the attestation identity are required.
    #[default]
    Download,
    /// Fetched from the pinned URL, but built by someone else: the attestation may be missing, and the checksum
    /// may be left to `nvmod tools trust`.
    ThirdParty,
    /// Linked into nvmod, not fetched; the entry records the version and git tag for the compatibility matrix.
    Linked,
}

/// The identity a build attestation must name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttestationPin {
    /// The repository, `owner/name`.
    pub repository: String,
    /// The workflow file that built it, such as `.github/workflows/release.yml`.
    #[serde(default)]
    pub workflow: Option<String>,
}

/// One downloadable file of a tool, for one platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    /// The only URL the file is fetched from.
    pub url: String,
    /// The SHA-256 of the file, 64 lower-case hex digits. May be missing only for a third-party tool.
    #[serde(default)]
    pub sha256: Option<String>,
}

/// A source pin: a git tag and the hash of the tree it points at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcePin {
    /// The repository URL.
    pub git: String,
    /// The release tag.
    pub tag: String,
    /// The git tree hash of that tag (`git rev-parse <tag>^{tree}`). Absent for a linked tool.
    #[serde(default)]
    pub tree_hash: Option<String>,
}

/// One tool of the lock.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolEntry {
    /// The tool's name.
    pub name: String,
    /// The pinned version.
    pub version: String,
    /// What kind of tool it is.
    #[serde(default)]
    pub kind: Kind,
    /// The contracts the tool speaks: contract name to schema version.
    #[serde(default)]
    pub contracts: BTreeMap<String, u32>,
    /// The first release of the tool that answers `contracts --json`. A pin from this release on must answer the
    /// handshake; an earlier pin is grandfathered and its `contracts` entry implies what it speaks (ADR-0006).
    #[serde(default)]
    pub handshake_from: Option<String>,
    /// The attestation identity.
    #[serde(default)]
    pub attestation: Option<AttestationPin>,
    /// The files, by platform (`linux-x86_64`, `windows-x86_64`, ...).
    #[serde(default)]
    pub assets: BTreeMap<String, Asset>,
    /// The source pin (a linked tool's tag, or the tag a source build is checked against).
    #[serde(default)]
    pub source: Option<SourcePin>,
}

/// The whole lock file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lock {
    /// The layout version of this file.
    pub schema_version: u32,
    /// The tools.
    #[serde(default, rename = "tool")]
    pub tools: Vec<ToolEntry>,
}

/// The platform key of this machine, such as `linux-x86_64`.
#[must_use]
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

fn is_sha256(text: &str) -> bool {
    text.len() == 64 && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Only `https`, or `http` to this machine (for the fake release server of the tests), may be a tool's URL.
fn url_allowed(url: &str) -> bool {
    url.starts_with("https://")
        || url.starts_with("http://127.0.0.1:")
        || url.starts_with("http://127.0.0.1/")
        || url.starts_with("http://localhost:")
}

impl Lock {
    /// Read a lock file.
    ///
    /// # Errors
    /// A reason, when the text is not a lock of this schema or breaks one of the rules above.
    pub fn parse(text: &str) -> Result<Lock, String> {
        let lock: Lock = toml::from_str(text).map_err(|e| format!("tools.lock.toml: {e}"))?;
        if lock.schema_version > LOCK_SCHEMA_VERSION {
            return Err(format!(
                "tools.lock.toml is of schema {}, which is newer than this nvmod reads (schema {LOCK_SCHEMA_VERSION})",
                lock.schema_version
            ));
        }
        if lock.schema_version != LOCK_SCHEMA_VERSION {
            return Err(format!(
                "tools.lock.toml: schema {} is not known to this nvmod",
                lock.schema_version
            ));
        }
        let mut seen = Vec::new();
        for tool in &lock.tools {
            if seen.contains(&&tool.name) {
                return Err(format!(
                    "tools.lock.toml: tool {} is listed twice",
                    tool.name
                ));
            }
            seen.push(&tool.name);
            tool.check()?;
        }
        Ok(lock)
    }

    /// The tool called `name`.
    #[must_use]
    pub fn tool(&self, name: &str) -> Option<&ToolEntry> {
        self.tools.iter().find(|t| t.name == name)
    }
}

impl ToolEntry {
    /// Whether the pinned release must answer the handshake: `handshake_from` is set and the pinned version is that
    /// release or a later one. Versions compare as dot-separated integers (a prerelease suffix is ignored).
    #[must_use]
    pub fn handshake_required(&self) -> bool {
        fn parts(version: &str) -> Vec<u64> {
            version
                .trim_start_matches('v')
                .split(['-', '+'])
                .next()
                .unwrap_or("")
                .split('.')
                .map(|p| p.parse().unwrap_or(0))
                .collect()
        }
        self.handshake_from
            .as_deref()
            .is_some_and(|from| parts(&self.version) >= parts(from))
    }

    fn check(&self) -> Result<(), String> {
        let bad = |why: &str| Err(format!("tools.lock.toml: tool {}: {why}", self.name));
        if self.name.is_empty() || self.name.contains(['/', '\\']) || self.name.starts_with('.') {
            return bad("the name is not a plain file name");
        }
        if self.version.is_empty() {
            return bad("no version");
        }
        match self.kind {
            Kind::Linked => {
                if !self.assets.is_empty() {
                    return bad("a linked tool has no assets");
                }
                if self.source.is_none() {
                    return bad("a linked tool needs a source with its git tag");
                }
            }
            Kind::Download | Kind::ThirdParty => {
                if self.assets.is_empty() {
                    return bad("no assets");
                }
                for (platform, asset) in &self.assets {
                    if !url_allowed(&asset.url) {
                        return bad(&format!("the {platform} URL must be https"));
                    }
                    match (&asset.sha256, self.kind) {
                        (Some(hash), _) if !is_sha256(hash) => {
                            return bad(&format!(
                                "the {platform} sha256 is not 64 lower-case hex digits"
                            ));
                        }
                        (None, Kind::Download) => {
                            return bad(&format!(
                                "the {platform} asset has no sha256 (only a third-party tool may leave it to `nvmod tools trust`)"
                            ));
                        }
                        _ => {}
                    }
                }
                if self.kind == Kind::Download && self.attestation.is_none() {
                    return bad("no attestation identity (only a third-party tool may go without)");
                }
            }
        }
        if let Some(hash) = self.source.as_ref().and_then(|s| s.tree_hash.as_ref())
            && !(hash.len() >= 40 && hash.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
        {
            return bad("the source tree_hash is not a git hash");
        }
        Ok(())
    }

    /// The file of this machine's platform, or the one under the key `any` (a data file such as a masterlist).
    #[must_use]
    pub fn asset_here(&self) -> Option<&Asset> {
        self.assets
            .get(&platform())
            .or_else(|| self.assets.get("any"))
    }
}

/// The lock that ships with nvmod.
///
/// # Panics
/// Never: a test parses the compiled-in file.
#[must_use]
pub fn builtin_lock() -> Lock {
    Lock::parse(include_str!("../../../tools.lock.toml")).expect("the shipped lock file is valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{covers, ids};

    const HASH: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    fn download(extra: &str) -> String {
        format!(
            r#"schema_version = 1
[[tool]]
name = "lodwright-rs"
version = "1.2.3"
contracts = {{ lodwright-plan = 1 }}
[tool.attestation]
repository = "o/lodwright-rs"
workflow = ".github/workflows/release.yml"
[tool.assets.linux-x86_64]
url = "https://example.com/lodwright"
sha256 = "{HASH}"
{extra}"#
        )
    }

    #[test]
    fn a_complete_download_entry_parses() {
        let lock = Lock::parse(&download("")).unwrap();
        let tool = lock.tool("lodwright-rs").unwrap();
        assert_eq!(tool.version, "1.2.3");
        assert_eq!(tool.kind, Kind::Download);
        assert_eq!(tool.contracts["lodwright-plan"], 1);
        assert_eq!(
            tool.attestation.as_ref().unwrap().repository,
            "o/lodwright-rs"
        );
        assert_eq!(tool.assets["linux-x86_64"].sha256.as_deref(), Some(HASH));
        assert!(lock.tool("nope").is_none());
    }

    #[test]
    fn a_download_without_a_checksum_or_attestation_is_refused() {
        let no_hash = download("").replace(&format!("sha256 = \"{HASH}\"\n"), "");
        assert!(Lock::parse(&no_hash).unwrap_err().contains("no sha256"));
        let no_att = download("").replace("[tool.attestation]\nrepository = \"o/lodwright-rs\"\nworkflow = \".github/workflows/release.yml\"\n", "");
        assert!(Lock::parse(&no_att).unwrap_err().contains("no attestation"));
        let bad_hash = download("").replace(HASH, "XYZ");
        assert!(Lock::parse(&bad_hash).unwrap_err().contains("sha256"));
        let upper = download("").replace(HASH, &HASH.to_ascii_uppercase());
        assert!(Lock::parse(&upper).is_err());
    }

    #[test]
    fn a_third_party_tool_may_lack_both() {
        let text = r#"schema_version = 1
[[tool]]
name = "third-party-tool"
version = "3.4"
kind = "third-party"
[tool.assets.linux-x86_64]
url = "https://example.com/tool"
"#;
        let lock = Lock::parse(text).unwrap();
        assert_eq!(lock.tools[0].assets["linux-x86_64"].sha256, None);
    }

    #[test]
    fn only_https_or_loopback_urls_are_allowed() {
        for url in [
            "http://example.com/x",
            "ftp://example.com/x",
            "file:///tmp/x",
            "/tmp/x",
        ] {
            let text = download("").replace("https://example.com/lodwright", url);
            assert!(Lock::parse(&text).unwrap_err().contains("https"), "{url}");
        }
        let local =
            download("").replace("https://example.com/lodwright", "http://127.0.0.1:8080/x");
        assert!(Lock::parse(&local).is_ok());
    }

    #[test]
    fn schema_names_and_duplicates_are_checked() {
        assert!(
            Lock::parse("schema_version = 2")
                .unwrap_err()
                .contains("newer")
        );
        assert!(
            Lock::parse("schema_version = 0")
                .unwrap_err()
                .contains("not known")
        );
        assert!(Lock::parse("tool = 5").is_err());
        let twice = format!(
            "{}{}",
            download(""),
            download("").replace("schema_version = 1\n", "")
        );
        assert!(Lock::parse(&twice).unwrap_err().contains("twice"));
        for name in ["", "a/b", ".hidden"] {
            let text = download("").replace("\"lodwright-rs\"", &format!("\"{name}\""));
            assert!(Lock::parse(&text).is_err(), "{name}");
        }
        assert!(Lock::parse(&download("").replace("\"1.2.3\"", "\"\"")).is_err());
    }

    #[test]
    fn a_linked_tool_has_a_tag_and_no_assets() {
        let ok = r#"schema_version = 1
[[tool]]
name = "loot-rs"
version = "0.1.0"
kind = "linked"
[tool.source]
git = "https://github.com/o/loot-rs"
tag = "v0.1.0"
"#;
        assert!(Lock::parse(ok).is_ok());
        assert!(
            Lock::parse(&ok.replace(
                "[tool.source]\ngit = \"https://github.com/o/loot-rs\"\ntag = \"v0.1.0\"\n",
                ""
            ))
            .unwrap_err()
            .contains("git tag")
        );
        let with_asset =
            format!("{ok}[tool.assets.linux-x86_64]\nurl = \"https://example.com/x\"\n");
        assert!(Lock::parse(&with_asset).unwrap_err().contains("no assets"));
        let bad_tree = format!("{ok}tree_hash = \"zz\"\n");
        assert!(Lock::parse(&bad_tree).unwrap_err().contains("tree_hash"));
    }

    #[test]
    fn the_shipped_lock_is_valid_and_records_the_loot_rs_tag_of_cargo_toml() {
        covers(&[ids::tools::pins::LINKED_LOOT_RS]);
        let lock = builtin_lock();
        let loot = lock.tool("loot-rs").unwrap();
        assert_eq!(loot.kind, Kind::Linked);
        let cargo = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.toml"),
        )
        .unwrap();
        let tag = loot.source.as_ref().unwrap().tag.as_str();
        assert!(
            cargo.contains(&format!("loot-rs = {{ git = \"https://github.com/JorisJonkers-dev/loot-rs\", tag = \"{tag}\" }}")),
            "tools.lock.toml records tag {tag}, which Cargo.toml does not use"
        );
        assert_eq!(format!("v{}", loot.version), tag);
    }

    #[test]
    fn the_handshake_is_required_from_the_release_that_has_it() {
        let entry = |version: &str, from: Option<&str>| {
            let mut tool = builtin_lock().tools.remove(0);
            tool.version = version.to_owned();
            tool.handshake_from = from.map(str::to_owned);
            tool
        };
        assert!(!entry("0.2.0", None).handshake_required());
        assert!(!entry("0.2.0", Some("0.3.0")).handshake_required());
        assert!(entry("0.3.0", Some("0.3.0")).handshake_required());
        assert!(entry("v0.10.1", Some("0.3.0")).handshake_required());
        assert!(entry("1.0.0-rc.1", Some("0.3.0")).handshake_required());
    }
}
