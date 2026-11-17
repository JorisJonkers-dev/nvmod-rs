// SPDX-License-Identifier: GPL-3.0-only
//! Listing the files a mod provides under one subtree (for the doctor: `NVSE`), with fixed bounds so a hostile
//! mod folder (a deep tree, a huge directory, a symlink loop) cannot make the scan run away.

use std::path::Path;

/// Deepest directory level followed below the subtree root.
pub const MAX_DEPTH: usize = 16;
/// Most files listed for one mod.
pub const MAX_FILES: usize = 50_000;
/// Suffix MO2 gives a file it hides; such a file is not part of the merged view.
pub const HIDDEN_SUFFIX: &str = ".mohidden";

/// The files below the directories of `mod_dir` whose name equals `subtree` ignoring case (`nvse` and `NVSE` are
/// both found), as paths relative to `mod_dir` with forward slashes and the spelling on disk. Symbolic links are
/// not followed and not listed; hidden files are skipped. Unreadable directories are skipped.
#[must_use]
pub fn files_under(mod_dir: &Path, subtree: &str) -> Vec<String> {
    let mut found = Vec::new();
    let Ok(top) = std::fs::read_dir(mod_dir) else {
        return found;
    };
    let mut roots: Vec<_> = top
        .flatten()
        .filter(|e| {
            e.file_type().is_ok_and(|t| t.is_dir())
                && e.file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(subtree)
        })
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    roots.sort();
    for root in roots {
        walk(&mod_dir.join(&root), &root, 0, &mut found);
    }
    found
}

fn walk(dir: &Path, relative: &str, depth: usize, found: &mut Vec<String>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        if found.len() >= MAX_FILES {
            return;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        let path = format!("{relative}/{name}");
        if kind.is_dir() {
            if depth < MAX_DEPTH {
                walk(&entry.path(), &path, depth + 1, found);
            }
        } else if kind.is_file() && !name.to_lowercase().ends_with(HIDDEN_SUFFIX) {
            found.push(path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("nvmod-scan-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, b"x").unwrap();
    }

    #[test]
    fn both_spellings_of_the_subtree_are_listed_with_disk_spelling() {
        let dir = temp("spellings");
        touch(&dir.join("NVSE/Plugins/a.dll"));
        touch(&dir.join("nvse/plugins/b.dll"));
        touch(&dir.join("meshes/x.nif"));
        touch(&dir.join("NVSE/Plugins/hidden.dll.mohidden"));
        let listed = if nvmod_testkit::case_sensitive(&dir) {
            ["NVSE/Plugins/a.dll", "nvse/plugins/b.dll"]
        } else {
            // macOS and Windows: `nvse/plugins` is the folder `NVSE/Plugins`, which keeps the spelling it was made
            // with, and that is the one listed.
            ["NVSE/Plugins/a.dll", "NVSE/Plugins/b.dll"]
        };
        assert_eq!(files_under(&dir, "nvse"), listed);
        assert_eq!(
            files_under(&dir.join("missing"), "nvse"),
            Vec::<String>::new()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn symlinks_are_not_followed_and_depth_is_bounded() {
        let dir = temp("links");
        touch(&dir.join("nvse/real.dll"));
        std::os::unix::fs::symlink(&dir, dir.join("nvse/loop")).unwrap();
        std::os::unix::fs::symlink(dir.join("nvse/real.dll"), dir.join("nvse/link.dll")).unwrap();
        assert_eq!(files_under(&dir, "nvse"), ["nvse/real.dll"]);
        let mut deep = dir.join("deep");
        for n in 0..(MAX_DEPTH + 4) {
            deep = deep.join(format!("d{n}"));
        }
        touch(&deep.join("too-deep.dll"));
        touch(&dir.join("deep/shallow.dll"));
        assert_eq!(files_under(&dir, "deep"), ["deep/shallow.dll"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
