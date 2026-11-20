// SPDX-License-Identifier: GPL-3.0-only
//! The masters of the base game, which a new Profile lists: `FalloutNV.esm`, the five official DLC masters and the
//! four pre-order and bonus pack masters. The game loads the DLC through its own `.nam` files, so they are not in
//! any list the player wrote; the Profile lists them, because everything that reads the list (sort, the Doctor,
//! LOD planning, save checks) would otherwise not see them.

use crate::find_ci;
use std::collections::HashSet;
use std::path::Path;

/// The masters of the base game in the order the game loads them: `FalloutNV.esm`, then the DLC in release order,
/// then the pack masters. `nvmod-loadorder` keeps the same relative order in its Tale of Two Wastelands list.
pub const BASE_MASTERS: [&str; 10] = [
    "FalloutNV.esm",
    "DeadMoney.esm",
    "HonestHearts.esm",
    "OldWorldBlues.esm",
    "LonesomeRoad.esm",
    "GunRunnersArsenal.esm",
    "CaravanPack.esm",
    "ClassicPack.esm",
    "MercenaryPack.esm",
    "TribalPack.esm",
];

/// The base masters that the `Data` folder of the base copy `base` holds, in [`BASE_MASTERS`] order and spelled as
/// that list spells them (the file name's case is ignored when looking). `FalloutNV.esm` is always the first, even
/// when `Data` lacks it: a base copy without it is the Doctor's business, and a Profile always starts with it.
#[must_use]
pub fn base_masters_present(base: &Path) -> Vec<&'static str> {
    let on_disk: HashSet<String> = find_ci(base, "Data")
        .and_then(|data| std::fs::read_dir(data).ok())
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().to_ascii_lowercase())
        .collect();
    BASE_MASTERS
        .iter()
        .copied()
        .enumerate()
        .filter(|(i, name)| *i == 0 || on_disk.contains(&name.to_ascii_lowercase()))
        .map(|(_, name)| name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{BaseBuilder, TempDir, write_file};

    fn base_with(files: &[&str]) -> TempDir {
        let base = TempDir::new("masters-base");
        BaseBuilder::steam().write(base.path());
        for f in files {
            write_file(base.path(), &format!("Data/{f}"), b"synthetic master");
        }
        base
    }

    #[test]
    fn none_present_gives_only_the_base_game() {
        let base = base_with(&[]);
        assert_eq!(base_masters_present(base.path()), ["FalloutNV.esm"]);
    }

    #[test]
    fn a_data_folder_without_the_game_master_still_lists_it_first() {
        let base = TempDir::new("masters-nodata");
        assert_eq!(base_masters_present(base.path()), ["FalloutNV.esm"]);
        write_file(base.path(), "Data/DeadMoney.esm", b"x");
        assert_eq!(
            base_masters_present(base.path()),
            ["FalloutNV.esm", "DeadMoney.esm"]
        );
    }

    #[test]
    fn some_present_in_mixed_case_are_found_and_spelled_canonically() {
        let base = base_with(&["deadmoney.ESM", "OLDWORLDBLUES.esm", "tribalpack.esm"]);
        // Not masters of the base game, and a folder that has a master's name.
        write_file(base.path(), "Data/Other.esm", b"x");
        std::fs::create_dir_all(base.path().join("Data/HonestHearts.esm")).unwrap();
        assert_eq!(
            base_masters_present(base.path()),
            [
                "FalloutNV.esm",
                "DeadMoney.esm",
                "OldWorldBlues.esm",
                "TribalPack.esm"
            ]
        );
    }

    #[test]
    fn a_lowercase_data_folder_is_found() {
        let base = TempDir::new("masters-lowercase");
        BaseBuilder::steam().without_data().write(base.path());
        write_file(base.path(), "data/falloutnv.esm", b"x");
        write_file(base.path(), "data/lonesomeroad.esm", b"x");
        assert_eq!(
            base_masters_present(base.path()),
            ["FalloutNV.esm", "LonesomeRoad.esm"]
        );
    }

    #[test]
    fn all_present_come_in_the_games_order_whatever_order_they_were_written_in() {
        let mut files = BASE_MASTERS.to_vec();
        files.reverse();
        let base = base_with(&files);
        assert_eq!(base_masters_present(base.path()), BASE_MASTERS);
        assert_eq!(BASE_MASTERS[0], "FalloutNV.esm");
        assert_eq!(BASE_MASTERS[5], "GunRunnersArsenal.esm");
    }
}
