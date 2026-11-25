// SPDX-License-Identifier: GPL-3.0-only
//! The paths of a `ModOrganizer.ini`, mapped to host paths. Where MO2 runs decides how it wrote them:
//!
//! - under Wine or Proton on Linux or macOS they are Wine paths (`Z:\home\user\...`). `Z:` is Wine's mapping of
//!   `/`; other drive letters need the prefix's `dosdevices` links, which the caller supplies (research, section
//!   2.12 item 6);
//! - on Windows itself they are already host paths (`C:\Games\...`), and every drive letter is a real drive.

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

/// The drive letter has no mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnmappedDrive(pub char);

impl fmt::Display for UnmappedDrive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "drive {}: is not mapped to a host directory (only Z: is, by default)",
            self.0.to_ascii_uppercase()
        )
    }
}

impl std::error::Error for UnmappedDrive {}

/// How the paths of an MO2 instance are written, which follows from where MO2 ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathStyle {
    /// MO2 under Wine or Proton on Linux or macOS: a drive path is a Wine path.
    Wine,
    /// MO2 on Windows: a drive path is a host path as it stands.
    Windows,
}

impl PathStyle {
    /// The style of the paths an MO2 on this machine writes: [`PathStyle::Windows`] on Windows, where MO2 runs
    /// natively, and [`PathStyle::Wine`] everywhere else, where it can only run under Wine or Proton.
    #[must_use]
    pub const fn host() -> PathStyle {
        if cfg!(windows) {
            PathStyle::Windows
        } else {
            PathStyle::Wine
        }
    }
}

/// Map `path` to a host path of this machine, in the style of [`PathStyle::host`]. See [`to_host_path_as`].
///
/// # Errors
/// Under Wine, the path names a drive that is neither `Z:` nor in `drives`.
pub fn to_host_path(
    path: &str,
    drives: &BTreeMap<char, PathBuf>,
) -> Result<PathBuf, UnmappedDrive> {
    to_host_path_as(PathStyle::host(), path, drives)
}

/// Map `path`, written in `style`, to a host path.
///
/// - [`PathStyle::Wine`]: `drives` maps lower-case drive letters other than `z` to host directories. A path without
///   a drive (host absolute or relative) is returned with its slashes normalised to `/`.
/// - [`PathStyle::Windows`]: the path is returned as it is, with its slashes normalised to `\`; `drives` is not
///   used and no drive is unmapped.
///
/// # Errors
/// Under Wine, the path names a drive that is neither `Z:` nor in `drives`.
pub fn to_host_path_as(
    style: PathStyle,
    path: &str,
    drives: &BTreeMap<char, PathBuf>,
) -> Result<PathBuf, UnmappedDrive> {
    match style {
        PathStyle::Windows => Ok(PathBuf::from(path.replace('/', "\\"))),
        PathStyle::Wine => wine_to_host(path, drives),
    }
}

fn wine_to_host(path: &str, drives: &BTreeMap<char, PathBuf>) -> Result<PathBuf, UnmappedDrive> {
    let slashed = path.replace('\\', "/");
    let mut chars = slashed.chars();
    match (chars.next(), chars.next()) {
        (Some(letter), Some(':')) if letter.is_ascii_alphabetic() => {
            let rest = &slashed[2..];
            let letter = letter.to_ascii_lowercase();
            if letter == 'z' {
                Ok(PathBuf::from(if rest.starts_with('/') {
                    rest.to_owned()
                } else {
                    format!("/{rest}")
                }))
            } else {
                let base = drives.get(&letter).ok_or(UnmappedDrive(letter))?;
                Ok(base.join(rest.trim_start_matches('/')))
            }
        }
        _ => Ok(PathBuf::from(slashed)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{covers, ids};

    fn wine(path: &str, drives: &BTreeMap<char, PathBuf>) -> Result<PathBuf, UnmappedDrive> {
        to_host_path_as(PathStyle::Wine, path, drives)
    }

    fn windows(path: &str) -> PathBuf {
        to_host_path_as(PathStyle::Windows, path, &BTreeMap::new()).unwrap()
    }

    #[test]
    fn z_maps_to_the_root_in_either_slash_style() {
        covers(&[ids::formats::files::WINE_PATHS]);
        let none = BTreeMap::new();
        assert_eq!(
            wine("Z:\\home\\u\\Game", &none).unwrap(),
            PathBuf::from("/home/u/Game")
        );
        assert_eq!(
            wine("z:/home/u/Game", &none).unwrap(),
            PathBuf::from("/home/u/Game")
        );
        assert_eq!(wine("Z:", &none).unwrap(), PathBuf::from("/"));
    }

    #[test]
    fn other_drives_need_a_mapping() {
        covers(&[ids::formats::files::WINE_PATHS]);
        let mut drives = BTreeMap::new();
        assert_eq!(wine("C:\\x", &drives), Err(UnmappedDrive('c')));
        drives.insert('c', PathBuf::from("/prefix/drive_c"));
        assert_eq!(
            wine("C:\\Users\\me", &drives).unwrap(),
            PathBuf::from("/prefix/drive_c/Users/me")
        );
        assert!(UnmappedDrive('c').to_string().contains("C:"));
    }

    #[test]
    fn host_and_relative_paths_pass_through() {
        let none = BTreeMap::new();
        assert_eq!(wine("/home/u", &none).unwrap(), PathBuf::from("/home/u"));
        assert_eq!(wine("mods\\x", &none).unwrap(), PathBuf::from("mods/x"));
        assert_eq!(wine("", &none).unwrap(), PathBuf::from(""));
    }

    #[test]
    fn a_windows_path_is_a_host_path_on_every_drive() {
        // The bytes are compared, so the separator is checked too.
        let text = |p: PathBuf| p.into_os_string().into_string().unwrap();
        assert_eq!(text(windows("C:\\Games\\FNV")), "C:\\Games\\FNV");
        assert_eq!(text(windows("C:/Games/FNV")), "C:\\Games\\FNV");
        assert_eq!(
            text(windows("Z:\\home\\u")),
            "Z:\\home\\u",
            "Z: is a drive like any other"
        );
        assert_eq!(text(windows("D:/x")), "D:\\x");
        assert_eq!(
            text(windows("\\\\server\\share\\x")),
            "\\\\server\\share\\x"
        );
        assert_eq!(text(windows("mods/x")), "mods\\x");
        assert_eq!(text(windows("")), "");
    }

    #[test]
    fn this_machine_reads_native_paths_on_windows_and_wine_paths_elsewhere() {
        let none = BTreeMap::new();
        if cfg!(windows) {
            assert_eq!(PathStyle::host(), PathStyle::Windows);
            assert_eq!(
                to_host_path("C:\\Games\\FNV", &none).unwrap(),
                windows("C:\\Games\\FNV")
            );
        } else {
            assert_eq!(PathStyle::host(), PathStyle::Wine);
            assert_eq!(to_host_path("C:\\x", &none), Err(UnmappedDrive('c')));
            assert_eq!(
                to_host_path("Z:\\home\\u", &none).unwrap(),
                PathBuf::from("/home/u")
            );
        }
    }
}
