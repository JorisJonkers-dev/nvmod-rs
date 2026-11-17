// SPDX-License-Identifier: GPL-3.0-only
//! Hard links, for assertions that hold on every platform: how many names a file has, and whether two paths are
//! the same file. Unix reads the inode; Windows reads the file's handle (volume serial number and file index).

use std::path::Path;

/// How many names (hard links) the file at `path` has.
///
/// # Panics
/// When the file cannot be read.
#[must_use]
pub fn link_count(path: &Path) -> u64 {
    identity(path).2
}

/// Whether `a` and `b` are the same file (two hard links of one file, or one path twice).
///
/// # Panics
/// When either file cannot be read.
#[must_use]
pub fn same_file(a: &Path, b: &Path) -> bool {
    let (a, b) = (identity(a), identity(b));
    (a.0, a.1) == (b.0, b.1)
}

/// The device, the file number and the link count of `path`.
#[cfg(unix)]
fn identity(path: &Path) -> (u64, u64, u64) {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    (meta.dev(), meta.ino(), meta.nlink())
}

/// The volume, the file index and the link count of `path`.
#[cfg(windows)]
fn identity(path: &Path) -> (u64, u64, u64) {
    let info = std::fs::File::open(path)
        .and_then(|file| winapi_util::file::information(&file))
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    (
        info.volume_serial_number(),
        info.file_index(),
        info.number_of_links(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TempDir;

    #[test]
    fn a_hard_link_is_the_same_file_and_a_copy_is_not() {
        let dir = TempDir::new("links");
        let a = dir.path().join("a");
        std::fs::write(&a, b"x").unwrap();
        assert_eq!(link_count(&a), 1);
        let b = dir.path().join("b");
        std::fs::hard_link(&a, &b).unwrap();
        assert!(same_file(&a, &b));
        assert_eq!((link_count(&a), link_count(&b)), (2, 2));
        let c = dir.path().join("c");
        std::fs::copy(&a, &c).unwrap();
        assert!(!same_file(&a, &c));
        assert_eq!(link_count(&c), 1);
        std::fs::remove_file(&b).unwrap();
        assert_eq!(link_count(&a), 1);
    }
}
