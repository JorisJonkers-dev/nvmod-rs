// SPDX-License-Identifier: GPL-3.0-only
//! A case-insensitive index of the files that mods provide (requirement D2: Fallout mods mix `Meshes/` and
//! `meshes/`, and the engine does not care).

use std::collections::BTreeMap;

/// The key a path is stored under: forward slashes, no leading slash, lower case. Idempotent.
#[must_use]
pub fn normalize_path(path: &str) -> String {
    path.replace('\\', "/")
        .trim_start_matches('/')
        .to_lowercase()
}

/// One source of a file: the mod (or other layer) and the spelling it uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provider {
    /// The mod or layer that provides the file.
    pub source: String,
    /// The path as that source spells it, with forward slashes.
    pub path: String,
}

/// Files by normalised path, each with its providers from highest to lowest priority (the order of insertion).
#[derive(Debug, Default, Clone)]
pub struct FileIndex {
    files: BTreeMap<String, Vec<Provider>>,
}

impl FileIndex {
    /// An empty index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that `source` provides `path`. Insert sources from highest to lowest priority.
    pub fn insert(&mut self, source: &str, path: &str) {
        let spelled = path.replace('\\', "/");
        self.files
            .entry(normalize_path(path))
            .or_default()
            .push(Provider {
                source: source.to_owned(),
                path: spelled.trim_start_matches('/').to_owned(),
            });
    }

    /// Every provider of `path`, winner first. Empty when nobody provides it.
    #[must_use]
    pub fn providers(&self, path: &str) -> &[Provider] {
        self.files
            .get(&normalize_path(path))
            .map_or(&[], Vec::as_slice)
    }

    /// The provider that wins `path`.
    #[must_use]
    pub fn winner(&self, path: &str) -> Option<&Provider> {
        self.providers(path).first()
    }

    /// Number of distinct files.
    #[must_use]
    pub fn len(&self) -> usize {
        self.files.len()
    }

    /// True when no file is indexed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.files.is_empty()
    }

    /// The files directly inside `dir` whose name ends with `suffix` (both compared without case), with their
    /// providers, in path order. `dir` has no trailing slash, for example `nvse/plugins`.
    pub fn in_dir<'a>(
        &'a self,
        dir: &str,
        suffix: &'a str,
    ) -> impl Iterator<Item = (&'a str, &'a [Provider])> + 'a {
        let prefix = format!("{}/", normalize_path(dir));
        let suffix = suffix.to_lowercase();
        self.files.iter().filter_map(move |(key, providers)| {
            let rest = key.strip_prefix(&prefix)?;
            (!rest.contains('/') && rest.ends_with(&suffix))
                .then_some((key.as_str(), providers.as_slice()))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn lookup_ignores_case_and_slash_style() {
        let mut index = FileIndex::new();
        index.insert("A", "Meshes\\Armor\\x.nif");
        assert_eq!(
            index.winner("meshes/armor/X.NIF").unwrap().path,
            "Meshes/Armor/x.nif"
        );
        assert!(index.winner("meshes/armor/y.nif").is_none());
        assert_eq!(index.len(), 1);
        assert!(!index.is_empty());
    }

    #[test]
    fn first_insert_wins_and_later_ones_are_shadowed() {
        let mut index = FileIndex::new();
        index.insert("high", "NVSE/Plugins/a.dll");
        index.insert("low", "nvse/plugins/A.dll");
        let sources: Vec<_> = index
            .providers("nvse/plugins/a.dll")
            .iter()
            .map(|p| p.source.as_str())
            .collect();
        assert_eq!(sources, ["high", "low"]);
    }

    #[test]
    fn in_dir_lists_direct_children_with_the_suffix_only() {
        let mut index = FileIndex::new();
        for p in [
            "NVSE/Plugins/a.dll",
            "nvse/plugins/b.INI",
            "nvse/plugins/sub/c.dll",
            "other/d.dll",
            "nvse/plugins/e.DLL",
        ] {
            index.insert("m", p);
        }
        let found: Vec<_> = index
            .in_dir("NVSE/Plugins", ".dll")
            .map(|(k, _)| k)
            .collect();
        assert_eq!(found, ["nvse/plugins/a.dll", "nvse/plugins/e.dll"]);
    }

    #[test]
    fn an_empty_index_has_no_providers() {
        let index = FileIndex::new();
        assert!(index.is_empty());
        assert_eq!(index.providers("x"), []);
    }

    proptest! {
        #[test]
        fn normalising_twice_changes_nothing(path in "[ -~]{0,40}") {
            let once = normalize_path(&path);
            prop_assert_eq!(normalize_path(&once), once);
        }

        #[test]
        fn any_spelling_of_a_path_finds_it(path in "[A-Za-z0-9_/\\\\ .]{1,40}") {
            let mut index = FileIndex::new();
            index.insert("m", &path);
            prop_assert_eq!(index.providers(&path.to_uppercase()).len(), 1);
            prop_assert_eq!(index.providers(&path.to_lowercase()).len(), 1);
        }
    }
}
