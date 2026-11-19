// SPDX-License-Identifier: GPL-3.0-only
//! The synthetic inputs of one `nvmod setup` run, written to a folder: a base copy, a masterlist, a fake nv-rs
//! release served from a folder, a fake `gh` and one fixture archive. Everything is made by `nvmod-testkit`, in code; no game data, no network.
//!
//! The cross-repo end-to-end test in nv-linux runs the pinned `nvmod` against these. The same fixtures back
//! `crates/nvmod-cli/tests/setup.rs`, so a change in what `setup` needs shows up there first.
#![cfg(unix)]

use nvmod_testkit::{
    BaseBuilder, FakeRelease, PeBuilder, PluginBuilder, ZipBuilder, write_file, write_script,
};
use nvmod_tools::sha256_hex;
use std::path::{Path, PathBuf};

/// Which run the fixtures are made for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Variant {
    /// Every step succeeds: `setup` exits 0 and `verify` exits 0.
    Ok,
    /// The fake nv-rs release lists a wrong SHA-256: `setup` exits 3 and leaves no Instance.
    WrongChecksum,
}

impl Variant {
    /// Parse the command-line name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Variant> {
        match name {
            "ok" => Some(Variant::Ok),
            "wrong-checksum" => Some(Variant::WrongChecksum),
            _ => None,
        }
    }

    /// The exit code `nvmod setup` is expected to return for this variant.
    #[must_use]
    pub fn expected_setup_exit(self) -> i32 {
        match self {
            Variant::Ok => 0,
            Variant::WrongChecksum => 3,
        }
    }
}

/// What was written: the arguments to run and the paths a test inspects.
#[derive(Debug)]
pub struct Rig {
    /// The base copy; it must be byte-identical after any run.
    pub base: PathBuf,
    /// Where the Instance is made. It does not exist yet.
    pub instance: PathBuf,
    /// The masterlist `doctor`, `verify` and `setup` read.
    pub masterlist: PathBuf,
    /// The arguments of `nvmod setup`, without `--json`.
    pub setup_args: Vec<String>,
    /// The arguments of `nvmod verify` on the finished Instance.
    pub verify_args: Vec<String>,
    /// The exit code `setup` should return.
    pub expected_setup_exit: i32,
}

fn s(path: &Path) -> String {
    path.to_str().expect("fixture paths are UTF-8").to_owned()
}

/// Write the fixtures for `variant` under `dir` (which must exist and be empty).
///
/// # Panics
/// On an I/O error: the folder is scratch space made for this run.
#[must_use]
pub fn write_rig(dir: &Path, variant: Variant) -> Rig {
    let base = dir.join("base");
    let work = dir.join("work");
    let release = work.join("release");
    std::fs::create_dir_all(&release).expect("create work folder");
    BaseBuilder::steam().write(&base);
    // The vanilla target needs xNVSE's DLL in the base copy.
    write_file(&base, "nvse_1_4.dll", &PeBuilder::dll().build());
    write_file(
        &base,
        "Data/FalloutNV.esm",
        &PluginBuilder::master().build(),
    );
    let masterlist = work.join("masterlist.yaml");
    std::fs::write(
        &masterlist,
        "globals:\n  - {type: say, content: 'Masterlist for tests'}\nplugins: []\n",
    )
    .expect("write masterlist");
    let fake_release = if variant == Variant::WrongChecksum {
        FakeRelease::new().wrong_checksum()
    } else {
        FakeRelease::new()
    };
    fake_release.write(&release, sha256_hex);

    let gh = s(&write_script(&work, "gh", "exit 0"));

    let core = work.join("Core.zip");
    ZipBuilder::new()
        .file("Core.esp", &PluginBuilder::new().build())
        .file("Textures/dirt.dds", b"dirt")
        .write(&core);

    let instance = dir.join("out").join("fnv");
    let setup_args = [
        "setup",
        "--base",
        &s(&base),
        "--instance",
        &s(&instance),
        "--masterlist",
        &s(&masterlist),
        "--release",
        "--release-url",
        &format!("file://{}", release.display()),
        "--gh",
        &gh,
        "--retry-delay-ms",
        "0",
        "--mod",
        &s(&core),
    ]
    .map(str::to_owned)
    .to_vec();
    let verify_args = [
        "verify",
        "--instance",
        &s(&instance),
        "--masterlist",
        &s(&masterlist),
    ]
    .map(str::to_owned)
    .to_vec();
    Rig {
        base,
        instance,
        masterlist,
        setup_args,
        verify_args,
        expected_setup_exit: variant.expected_setup_exit(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::TempDir;

    #[test]
    fn the_rig_has_every_input_the_setup_arguments_name() {
        let dir = TempDir::new("fixtures-rig");
        let rig = write_rig(dir.path(), Variant::Ok);
        assert!(rig.base.join("Data/FalloutNV.esm").is_file());
        assert!(rig.masterlist.is_file());
        assert!(!rig.instance.exists());
        for arg in &rig.setup_args {
            if arg.starts_with('/') && !arg.contains("/out/") && !arg.starts_with("file://") {
                assert!(Path::new(arg).exists(), "{arg}");
            }
        }
        assert_eq!(rig.expected_setup_exit, 0);
    }

    #[test]
    fn variants_parse_and_name_their_exit_codes() {
        assert_eq!(
            Variant::parse("wrong-checksum")
                .unwrap()
                .expected_setup_exit(),
            3
        );
        assert!(Variant::parse("nope").is_none());
    }
}
