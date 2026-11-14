// SPDX-License-Identifier: GPL-3.0-only
//! A source build of a tool is authenticated differently from a fetched release: by the lock's git tag and tree
//! hash, and by the manifest's `source_tree_hash`. The executable section hashes of a release cannot match a local build, so
//! that check is skipped, and the report says which checks ran and which did not.

use crate::lock::ToolEntry;
use crate::verify::VerifyError;

/// What a source-build check did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceCheck {
    /// The checks that ran and passed.
    pub ran: Vec<String>,
    /// The checks that were skipped, with the reason.
    pub skipped: Vec<String>,
}

impl std::fmt::Display for SourceCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "source build authenticated by: {}; skipped: {}",
            self.ran.join(", "),
            self.skipped.join(", ")
        )
    }
}

/// Check a source build of `entry`: `tag` and `tree_hash` are what the checked-out source is (its git tag and
/// `git rev-parse HEAD^{tree}`), `manifest_tree_hash` is the `source_tree_hash` of the release manifest, if there
/// is one.
///
/// # Errors
/// `Verification` when the lock pins no source tag and tree hash, when the tag or tree hash differ from the pins,
/// or when the manifest's `source_tree_hash` differs from the tree hash.
pub fn verify_source_build(
    entry: &ToolEntry,
    tag: &str,
    tree_hash: &str,
    manifest_tree_hash: Option<&str>,
) -> Result<SourceCheck, VerifyError> {
    let fail = |why: String| Err(VerifyError::Verification(format!("{}: {why}", entry.name)));
    let Some(pin) = &entry.source else {
        return fail("the lock file pins no source for a source build".to_owned());
    };
    let Some(pinned_tree) = &pin.tree_hash else {
        return fail("the lock file pins no source tree hash".to_owned());
    };
    if tag != pin.tag {
        return fail(format!(
            "the source is at tag {tag}, the lock pins {}",
            pin.tag
        ));
    }
    if !tree_hash.eq_ignore_ascii_case(pinned_tree) {
        return fail(format!(
            "the source tree hash is {tree_hash}, the lock pins {pinned_tree}"
        ));
    }
    let mut ran = vec![format!("lock git tag {tag}"), "lock tree hash".to_owned()];
    match manifest_tree_hash {
        Some(from_manifest) if from_manifest.eq_ignore_ascii_case(tree_hash) => {
            ran.push("manifest source_tree_hash".to_owned());
        }
        Some(from_manifest) => {
            return fail(format!(
                "the manifest's source_tree_hash {from_manifest} is not the source tree hash {tree_hash}"
            ));
        }
        None => {
            return fail(
                "the manifest has no source_tree_hash, so the build cannot be tied to the source"
                    .to_owned(),
            );
        }
    }
    Ok(SourceCheck {
        ran,
        skipped: vec![
            "executable (the section hashes; a source build cannot match the release's hashes)"
                .to_owned(),
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lock::Lock;

    const TREE: &str = "0123456789abcdef0123456789abcdef01234567";

    fn entry() -> ToolEntry {
        let text = format!(
            "schema_version = 1\n[[tool]]\nname = \"nv-rs\"\nversion = \"1\"\nkind = \"third-party\"\n[tool.assets.linux-x86_64]\nurl = \"https://e.com/x\"\n[tool.source]\ngit = \"https://github.com/o/nv-rs\"\ntag = \"v1\"\ntree_hash = \"{TREE}\"\n"
        );
        Lock::parse(&text).unwrap().tools.remove(0)
    }

    #[test]
    fn a_matching_source_build_says_which_checks_ran_and_which_were_skipped() {
        let check = verify_source_build(&entry(), "v1", TREE, Some(TREE)).unwrap();
        assert_eq!(check.ran.len(), 3);
        let text = check.to_string();
        assert!(
            text.contains("lock git tag v1") && text.contains("manifest source_tree_hash"),
            "{text}"
        );
        assert!(text.contains("skipped: executable"), "{text}");
    }

    #[test]
    fn a_wrong_tag_tree_or_manifest_hash_fails_verification() {
        let other = "f".repeat(40);
        for (tag, tree, manifest) in [
            ("v2", TREE, Some(TREE)),
            ("v1", other.as_str(), Some(other.as_str())),
            ("v1", TREE, Some(other.as_str())),
            ("v1", TREE, None),
        ] {
            let why = verify_source_build(&entry(), tag, tree, manifest).unwrap_err();
            assert!(matches!(why, VerifyError::Verification(_)), "{why}");
        }
    }

    #[test]
    fn a_lock_without_a_source_pin_cannot_authenticate_a_source_build() {
        let mut bare = entry();
        bare.source = None;
        assert!(verify_source_build(&bare, "v1", TREE, Some(TREE)).is_err());
        let mut no_tree = entry();
        no_tree.source.as_mut().unwrap().tree_hash = None;
        assert!(verify_source_build(&no_tree, "v1", TREE, Some(TREE)).is_err());
    }
}
