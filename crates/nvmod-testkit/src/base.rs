// SPDX-License-Identifier: GPL-3.0-only
#![allow(clippy::missing_panics_doc, clippy::must_use_candidate)]
//! Builds a fake base copy of the game (a Steam, GOG or Epic folder) and snapshots a folder tree, in code. The
//! storefront markers are the ones `docs/research/nv-rs-build-and-install.md` of NV-Linux-Modding describes for
//! the three stores; the executable is a synthetic PE file, never a real one.

use crate::{PeBuilder, write_file};
use std::collections::BTreeMap;
use std::path::Path;

/// Which storefront the fake base copy looks like.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Store {
    /// `steam_api.dll` next to the executable.
    Steam,
    /// `goggame-1454587428.info` and `goglog.ini`.
    Gog,
    /// A `.egstore` folder.
    Epic,
    /// No marker of any store.
    Unknown,
}

/// A fake base copy to write into a folder.
#[derive(Debug, Clone)]
pub struct BaseBuilder {
    store: Store,
    version: [u16; 4],
    patched_4gb: bool,
    executable: bool,
    data_dir: bool,
}

impl BaseBuilder {
    fn new(store: Store) -> BaseBuilder {
        BaseBuilder {
            store,
            version: [1, 4, 0, 525],
            patched_4gb: false,
            executable: true,
            data_dir: true,
        }
    }

    /// A Steam copy with the supported executable version.
    pub fn steam() -> BaseBuilder {
        Self::new(Store::Steam)
    }

    /// A GOG copy with the supported executable version.
    pub fn gog() -> BaseBuilder {
        Self::new(Store::Gog)
    }

    /// An Epic copy.
    pub fn epic() -> BaseBuilder {
        Self::new(Store::Epic)
    }

    /// A copy that shows no storefront marker.
    pub fn unknown() -> BaseBuilder {
        Self::new(Store::Unknown)
    }

    /// Set the executable's file version.
    #[must_use]
    pub fn exe_version(mut self, version: [u16; 4]) -> BaseBuilder {
        self.version = version;
        self
    }

    /// Make the executable large address aware, as a 4GB-patched one is.
    #[must_use]
    pub fn patched_4gb(mut self) -> BaseBuilder {
        self.patched_4gb = true;
        self
    }

    /// Leave the executable out.
    #[must_use]
    pub fn without_executable(mut self) -> BaseBuilder {
        self.executable = false;
        self
    }

    /// Leave the `Data` folder out.
    #[must_use]
    pub fn without_data(mut self) -> BaseBuilder {
        self.data_dir = false;
        self
    }

    /// Write the copy into `root`.
    pub fn write(&self, root: &Path) {
        if self.executable {
            let exe = PeBuilder::exe()
                .versions(self.version, self.version)
                .large_address_aware(self.patched_4gb)
                .build();
            write_file(root, "FalloutNV.exe", &exe);
        }
        if self.data_dir {
            write_file(root, "Data/FalloutNV.esm", b"synthetic master");
        }
        match self.store {
            Store::Steam => write_file(root, "steam_api.dll", b"synthetic steam api"),
            Store::Gog => {
                write_file(root, "goggame-1454587428.info", b"{}");
                write_file(root, "goglog.ini", b"[GOG]");
            }
            Store::Epic => write_file(root, ".egstore/manifest.item", b"{}"),
            Store::Unknown => {}
        }
    }
}

/// Every file under `root` (relative path with `/`) with its bytes, and every folder as an empty entry ending in
/// `/`. Two snapshots are equal when the trees are byte for byte the same.
pub fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(dir: &Path, prefix: &str, out: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).expect("read dir").flatten() {
            let name = format!("{prefix}{}", entry.file_name().to_string_lossy());
            let path = entry.path();
            if path.is_dir() {
                out.insert(format!("{name}/"), Vec::new());
                walk(&path, &format!("{name}/"), out);
            } else {
                out.insert(name, std::fs::read(&path).expect("read file"));
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, "", &mut out);
    out
}
