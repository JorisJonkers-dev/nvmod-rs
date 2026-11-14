// SPDX-License-Identifier: GPL-3.0-only
//! The NVSE view of a plugin's exports.

use crate::model::{Export, NvseInfo, VersionStruct};

/// The older entry point: asks the plugin who it is.
pub const QUERY: &str = "NVSEPlugin_Query";
/// The entry point called when the plugin is loaded.
pub const LOAD: &str = "NVSEPlugin_Load";
/// The newer, data-only declaration of the plugin's version and compatibility.
pub const VERSION: &str = "NVSEPlugin_Version";

/// Classify `exports`. `version_struct` is what was read at the `NVSEPlugin_Version` address, if anything.
#[must_use]
pub fn classify(exports: &[Export], version_struct: Option<VersionStruct>) -> NvseInfo {
    let has = |name: &str| exports.iter().any(|e| e.name == name);
    let (has_query, has_load, has_version_export) = (has(QUERY), has(LOAD), has(VERSION));
    NvseInfo {
        is_plugin: has_query || has_load || has_version_export,
        has_query,
        has_load,
        has_version_export,
        version_struct: if has_version_export {
            version_struct
        } else {
            None
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn export(name: &str) -> Export {
        Export {
            name: name.to_owned(),
            rva: 0x1000,
            forwarded: false,
        }
    }

    #[test]
    fn the_three_entry_points_are_recognised_separately() {
        let info = classify(&[export(QUERY), export(LOAD)], None);
        assert!(info.is_plugin && info.has_query && info.has_load && !info.has_version_export);
        let info = classify(
            &[export(VERSION), export(LOAD)],
            Some(VersionStruct {
                info_version: 1,
                version: 7,
            }),
        );
        assert!(info.has_version_export && !info.has_query);
        assert_eq!(
            info.version_struct,
            Some(VersionStruct {
                info_version: 1,
                version: 7
            })
        );
    }

    #[test]
    fn a_library_without_entry_points_is_not_a_plugin() {
        let info = classify(&[export("DllMain"), export("nvse_plugin_load")], None);
        assert!(!info.is_plugin, "names are case sensitive C symbols");
    }

    #[test]
    fn a_version_struct_without_the_export_is_dropped() {
        let info = classify(
            &[export(LOAD)],
            Some(VersionStruct {
                info_version: 1,
                version: 1,
            }),
        );
        assert_eq!(info.version_struct, None);
    }
}
