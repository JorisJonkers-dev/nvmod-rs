// SPDX-License-Identifier: GPL-3.0-only
#![allow(clippy::missing_panics_doc, clippy::must_use_candidate)]
//! Builds a small Mod Organizer 2 instance and a game folder in a temporary directory, in code. The text formats
//! follow `docs/research/mo2-internals-and-import.md` of NV-Linux-Modding (CRLF, `@ByteArray` wrappers, Wine paths).

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

static COUNTER: AtomicU32 = AtomicU32::new(0);

/// A temporary directory that is removed when dropped.
#[derive(Debug)]
pub struct TempDir(PathBuf);

impl TempDir {
    /// A new empty directory whose name contains `label`.
    #[must_use]
    pub fn new(label: &str) -> TempDir {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("nvmod-{label}-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create temp dir");
        TempDir(dir)
    }

    /// The directory.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Write `bytes` to `root/relative`, creating folders.
pub fn write_file(root: &Path, relative: &str, bytes: &[u8]) {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().expect("has a parent")).expect("create folders");
    std::fs::write(path, bytes).expect("write file");
}

/// A mod folder to create.
#[derive(Debug, Clone, Default)]
pub struct ModSpec {
    name: String,
    files: Vec<(String, Vec<u8>)>,
    meta: Option<String>,
}

impl ModSpec {
    /// A mod called `name` (its folder name).
    #[must_use]
    pub fn new(name: &str) -> ModSpec {
        ModSpec {
            name: name.to_owned(),
            ..ModSpec::default()
        }
    }

    /// Add a file at `path` (relative to the mod folder).
    #[must_use]
    pub fn file(mut self, path: &str, bytes: &[u8]) -> ModSpec {
        self.files.push((path.to_owned(), bytes.to_vec()));
        self
    }

    /// Give the mod a `meta.ini` with this Nexus id and, optionally, a version.
    #[must_use]
    pub fn nexus(mut self, modid: u64, version: &str) -> ModSpec {
        self.meta = Some(format!(
            "[General]\r\ngameName=newvegas\r\nmodid={modid}\r\nversion={version}\r\ninstallationFile=C:/Users/someone/Downloads/{}-{modid}.7z\r\nrepository=Nexus\r\n",
            self.name
        ));
        self
    }
}

/// An instance to create.
#[derive(Debug, Clone)]
pub struct Mo2Builder {
    game_dir: PathBuf,
    mods: Vec<ModSpec>,
    profiles: Vec<(String, Vec<String>, Vec<String>)>,
    executables: Vec<(String, String)>,
    selected: String,
}

impl Mo2Builder {
    /// An instance whose game folder is `game_dir`.
    #[must_use]
    pub fn new(game_dir: &Path) -> Mo2Builder {
        Mo2Builder {
            game_dir: game_dir.to_path_buf(),
            mods: Vec::new(),
            profiles: Vec::new(),
            executables: Vec::new(),
            selected: "Default".to_owned(),
        }
    }

    /// Add a mod folder.
    #[must_use]
    pub fn mod_dir(mut self, spec: ModSpec) -> Mo2Builder {
        self.mods.push(spec);
        self
    }

    /// Add a profile with these `modlist.txt` lines (highest priority first, as MO2 writes them) and active plugins.
    #[must_use]
    pub fn profile(mut self, name: &str, modlist: &[&str], plugins: &[&str]) -> Mo2Builder {
        self.profiles.push((
            name.to_owned(),
            modlist.iter().map(|s| (*s).to_owned()).collect(),
            plugins.iter().map(|s| (*s).to_owned()).collect(),
        ));
        self
    }

    /// Add an executable (`binary` is a path on the host; it is written as a Wine path).
    #[must_use]
    pub fn executable(mut self, title: &str, binary: &Path) -> Mo2Builder {
        self.executables
            .push((title.to_owned(), binary.to_string_lossy().into_owned()));
        self
    }

    /// Choose the selected profile.
    #[must_use]
    pub fn selected(mut self, name: &str) -> Mo2Builder {
        name.clone_into(&mut self.selected);
        self
    }

    /// Create everything under `root`.
    pub fn write(&self, root: &Path) {
        let wine = |p: &str| format!("Z:{p}");
        let mut ini = format!(
            "[General]\r\ngameName=TTW\r\ngamePath=@ByteArray({})\r\nselected_profile=@ByteArray({})\r\nversion=2.5.2\r\n\r\n[customExecutables]\r\nsize={}\r\n",
            wine(&self.game_dir.to_string_lossy()).replace('/', "\\\\"),
            self.selected,
            self.executables.len()
        );
        for (n, (title, binary)) in self.executables.iter().enumerate() {
            let n = n + 1;
            let _ = write!(
                ini,
                "{n}\\title={title}\r\n{n}\\binary={}\r\n{n}\\arguments=\r\n",
                wine(binary)
            );
        }
        write_file(root, "ModOrganizer.ini", ini.as_bytes());
        std::fs::create_dir_all(root.join("mods")).expect("mods");
        std::fs::create_dir_all(root.join("profiles")).expect("profiles");
        for spec in &self.mods {
            std::fs::create_dir_all(root.join("mods").join(&spec.name)).expect("mod");
            for (path, bytes) in &spec.files {
                write_file(&root.join("mods").join(&spec.name), path, bytes);
            }
            if let Some(meta) = &spec.meta {
                write_file(
                    &root.join("mods").join(&spec.name),
                    "meta.ini",
                    meta.as_bytes(),
                );
            }
        }
        for (name, modlist, plugins) in &self.profiles {
            let mut text =
                String::from("# This file was automatically generated by Mod Organizer.\r\n");
            for line in modlist {
                text += line;
                text += "\r\n";
            }
            let base = format!("profiles/{name}");
            write_file(root, &format!("{base}/modlist.txt"), text.as_bytes());
            let plugin_text: String = plugins.iter().flat_map(|p| [p.as_str(), "\r\n"]).collect();
            write_file(root, &format!("{base}/plugins.txt"), plugin_text.as_bytes());
            write_file(
                root,
                &format!("{base}/loadorder.txt"),
                plugin_text.as_bytes(),
            );
            write_file(root, &format!("{base}/fallout.ini"), b"[General]\r\n");
        }
    }
}
