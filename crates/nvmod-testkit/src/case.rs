// SPDX-License-Identifier: GPL-3.0-only
//! Whether a folder's file system tells names apart by case. Most Linux file systems do; the defaults of macOS
//! (APFS) and Windows (NTFS) do not, and there `Textures/Wall.dds` and `textures/wall.dds` are one file.

use std::path::Path;

/// Whether the file system of `dir` (which must exist) keeps `a` and `A` apart. It writes and removes a probe file.
///
/// # Panics
/// When the probe file cannot be written.
#[must_use]
pub fn case_sensitive(dir: &Path) -> bool {
    let probe = dir.join(format!(".nvmod-Case-Probe-{}", std::process::id()));
    std::fs::write(&probe, b"").unwrap_or_else(|e| panic!("{}: {e}", probe.display()));
    let lower = dir.join(
        probe
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_lowercase(),
    );
    let sensitive = !lower.exists();
    let _ = std::fs::remove_file(&probe);
    sensitive
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TempDir;

    #[test]
    fn the_answer_matches_what_the_file_system_does() {
        let dir = TempDir::new("case");
        let sensitive = case_sensitive(dir.path());
        std::fs::write(dir.path().join("Wall.dds"), b"one").unwrap();
        std::fs::write(dir.path().join("wall.dds"), b"two").unwrap();
        let names = std::fs::read_dir(dir.path()).unwrap().count();
        assert_eq!(names, if sensitive { 2 } else { 1 });
        assert_eq!(
            std::fs::read(dir.path().join("Wall.dds")).unwrap(),
            if sensitive { b"one" } else { b"two" }
        );
    }
}
