// SPDX-License-Identifier: GPL-3.0-only
//! Windows paths as Wine writes them into `ModOrganizer.ini` (`Z:\home\user\...`), mapped to host paths.
//! `Z:` is Wine's mapping of `/`; other drive letters need the prefix's `dosdevices` links, which the caller
//! supplies (research, section 2.12 item 6).

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

/// Map `path` to a host path. `drives` maps lower-case drive letters other than `z` to host directories. A path
/// without a drive (host absolute or relative) is returned with its slashes normalised.
///
/// # Errors
/// The path names a drive that is neither `Z:` nor in `drives`.
pub fn to_host_path(
    path: &str,
    drives: &BTreeMap<char, PathBuf>,
) -> Result<PathBuf, UnmappedDrive> {
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

    #[test]
    fn z_maps_to_the_root_in_either_slash_style() {
        let none = BTreeMap::new();
        assert_eq!(
            to_host_path("Z:\\home\\u\\Game", &none).unwrap(),
            PathBuf::from("/home/u/Game")
        );
        assert_eq!(
            to_host_path("z:/home/u/Game", &none).unwrap(),
            PathBuf::from("/home/u/Game")
        );
        assert_eq!(to_host_path("Z:", &none).unwrap(), PathBuf::from("/"));
    }

    #[test]
    fn other_drives_need_a_mapping() {
        let mut drives = BTreeMap::new();
        assert_eq!(to_host_path("C:\\x", &drives), Err(UnmappedDrive('c')));
        drives.insert('c', PathBuf::from("/prefix/drive_c"));
        assert_eq!(
            to_host_path("C:\\Users\\me", &drives).unwrap(),
            PathBuf::from("/prefix/drive_c/Users/me")
        );
        assert!(UnmappedDrive('c').to_string().contains("C:"));
    }

    #[test]
    fn host_and_relative_paths_pass_through() {
        let none = BTreeMap::new();
        assert_eq!(
            to_host_path("/home/u", &none).unwrap(),
            PathBuf::from("/home/u")
        );
        assert_eq!(
            to_host_path("mods\\x", &none).unwrap(),
            PathBuf::from("mods/x")
        );
        assert_eq!(to_host_path("", &none).unwrap(), PathBuf::from(""));
    }
}
