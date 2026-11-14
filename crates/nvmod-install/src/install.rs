// SPDX-License-Identifier: GPL-3.0-only
//! The install itself: stage, move into place, run the play-patch step, record; undo everything on failure.

use crate::build::{build_from_source, resolve_cargo};
use crate::fetch::Curl;
use crate::release::{self, Source};
use nvmod_instance::NvmodInstance;
use nvmod_tools::sha256_hex;
use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// The fork's release, used when none is given. Unverified: the fork has published no release yet.
pub const DEFAULT_RELEASE_URL: &str =
    "https://github.com/JorisJonkers-dev/nv-rs/releases/latest/download";
/// The release asset fetched by default (the name of the fork's Linux build workflow output).
pub const DEFAULT_ASSET: &str = "nv-rs-linux-x86_64-dev.tar.gz";
/// The repository whose attestation is required.
pub const DEFAULT_REPO: &str = "JorisJonkers-dev/nv-rs";

/// Why the install did not happen. Each maps to an exit code of the command line.
#[derive(Debug, PartialEq, Eq)]
pub enum InstallError {
    /// Refused before anything was written (exit 2).
    Refused(String),
    /// A checksum, an attestation or a response did not check out; nothing was installed (exit 3).
    Verification(String),
    /// A step failed and what it had written was removed (exit 4).
    Failed(String),
}

impl fmt::Display for InstallError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InstallError::Refused(s) | InstallError::Verification(s) | InstallError::Failed(s) => {
                f.write_str(s)
            }
        }
    }
}

impl std::error::Error for InstallError {}

/// How nv-rs was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Built from a source workspace.
    Source,
    /// The fork's release, verified.
    Release,
}

/// What `install` was asked to do.
#[derive(Debug, Clone)]
pub struct Options {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The nv-rs workspace to build.
    pub source: Option<PathBuf>,
    /// Fetch the release even when a toolchain exists.
    pub release: bool,
    /// `--cargo` or `NVMOD_CARGO`.
    pub cargo: Option<PathBuf>,
    /// Where the release is fetched from.
    pub release_url: String,
    /// The release asset.
    pub asset: String,
    /// The repository the attestation must name.
    pub repo: String,
    /// The `gh` program.
    pub gh: PathBuf,
    /// The `curl` program.
    pub curl: PathBuf,
    /// nv-rs's play-patch step; when `None`, a `play-patch` program shipped in the release is used.
    pub play_patch: Option<PathBuf>,
    /// Wait between fetch retries.
    pub retry_delay: Duration,
}

/// What was installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installed {
    /// How nv-rs was obtained.
    pub method: Method,
    /// The installed program.
    pub binary: PathBuf,
    /// The play-patch program that ran, if any.
    pub play_patch: Option<PathBuf>,
}

const STAGING: &str = ".nv-rs-staging";
const FINAL: &str = "nv-rs";

fn io(path: &Path, e: &std::io::Error) -> InstallError {
    InstallError::Failed(format!("{}: {e}", path.display()))
}

fn exe_name(stem: &str) -> String {
    format!("{stem}{}", std::env::consts::EXE_SUFFIX)
}

fn install_file(from: &Path, bin: &Path) -> Result<PathBuf, InstallError> {
    let to = bin.join(from.file_name().unwrap_or_default());
    std::fs::copy(from, &to).map_err(|e| io(&to, &e))?;
    Ok(to)
}

/// Install nv-rs into the Instance. Nothing outside the Instance is written except the build folder of a source
/// build, and on failure the Instance is as it was.
///
/// # Errors
/// See [`InstallError`].
pub fn install(opts: &Options) -> Result<Installed, InstallError> {
    let instance = NvmodInstance::open(&opts.instance)
        .map_err(|e| InstallError::Refused(format!("not an nvmod Instance: {e}")))?;
    let root = std::fs::canonicalize(&instance.root).map_err(|e| io(&instance.root, &e))?;
    if root.join(FINAL).exists() {
        return Err(InstallError::Refused(format!(
            "nv-rs is already installed in {}; nvmod does not overwrite it",
            root.display()
        )));
    }
    let cargo = resolve_cargo(opts.cargo.as_deref());
    let method = if opts.release || cargo.is_none() {
        Method::Release
    } else {
        Method::Source
    };
    if method == Method::Source && opts.source.is_none() {
        return Err(InstallError::Refused(
            "a Rust toolchain was found but there is no nv-rs source: give --source <dir>, or --release to fetch the release".to_owned(),
        ));
    }
    if let Some(source) = opts.source.as_ref().filter(|_| method == Method::Source)
        && !source.join("Cargo.toml").is_file()
    {
        return Err(InstallError::Refused(format!(
            "{} is not a cargo workspace",
            source.display()
        )));
    }

    let staging = root.join(STAGING);
    let _ = std::fs::remove_dir_all(&staging);
    let bin = staging.join("bin");
    let result = stage(opts, method, cargo.as_deref(), &staging, &bin)
        .and_then(|()| commit(opts, method, &root, &instance, &staging));
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result
}

fn stage(
    opts: &Options,
    method: Method,
    cargo: Option<&Path>,
    staging: &Path,
    bin: &Path,
) -> Result<(), InstallError> {
    std::fs::create_dir_all(bin).map_err(|e| io(bin, &e))?;
    match method {
        Method::Source => {
            let source = opts.source.as_deref().unwrap_or(Path::new("."));
            let built = build_from_source(cargo.unwrap_or(Path::new("cargo")), source)
                .map_err(InstallError::Failed)?;
            install_file(&built, bin)?;
        }
        Method::Release => {
            let curl = Curl {
                program: opts.curl.clone(),
            };
            let src = Source {
                fetcher: &curl,
                base_url: &opts.release_url,
                asset: &opts.asset,
                repo: &opts.repo,
                gh: &opts.gh,
                delay: opts.retry_delay,
            };
            let asset = release::fetch_verified(&src, staging)?;
            let unpacked = staging.join("unpacked");
            release::extract(&asset, &unpacked)?;
            let viewer = exe_name("nv-viewer");
            let found = release::find_file(&unpacked, &viewer).ok_or_else(|| {
                InstallError::Failed(format!("the release archive holds no {viewer}"))
            })?;
            install_file(&found, bin)?;
            if let Some(patch) = release::find_file(&unpacked, &exe_name("play-patch")) {
                install_file(&patch, bin)?;
            }
            std::fs::remove_dir_all(&unpacked).map_err(|e| io(&unpacked, &e))?;
            std::fs::remove_file(&asset).map_err(|e| io(&asset, &e))?;
        }
    }
    Ok(())
}

fn commit(
    opts: &Options,
    method: Method,
    root: &Path,
    instance: &NvmodInstance,
    staging: &Path,
) -> Result<Installed, InstallError> {
    let target = root.join(FINAL);
    std::fs::rename(staging, &target).map_err(|e| io(&target, &e))?;
    let finish = || -> Result<Installed, InstallError> {
        let binary = target.join("bin").join(exe_name("nv-viewer"));
        let shipped = target.join("bin").join(exe_name("play-patch"));
        let patch = opts
            .play_patch
            .clone()
            .or_else(|| shipped.is_file().then_some(shipped));
        if let Some(patch) = &patch {
            run_patch_step(patch, &binary, &instance.config.base, root)?;
        }
        write_record(&target, method, &binary, patch.as_deref(), opts)?;
        Ok(Installed {
            method,
            binary,
            play_patch: patch,
        })
    };
    finish().inspect_err(|_| {
        let _ = std::fs::remove_dir_all(&target);
    })
}

/// Run nv-rs's play-patch step. nv-rs ships no standalone program for it yet (docs/DECISIONS.md), so the
/// invocation is nvmod's: the installed program, the base copy (read only) and the Instance.
fn run_patch_step(
    patch: &Path,
    binary: &Path,
    base: &Path,
    instance: &Path,
) -> Result<(), InstallError> {
    let out = Command::new(patch)
        .arg("--nv-rs")
        .arg(binary)
        .arg("--base")
        .arg(base)
        .arg("--instance")
        .arg(instance)
        .output()
        .map_err(|e| {
            InstallError::Failed(format!(
                "cannot run the play-patch step {}: {e}",
                patch.display()
            ))
        })?;
    if out.status.success() {
        Ok(())
    } else {
        Err(InstallError::Failed(format!(
            "the play-patch step {} failed ({}): {}{}",
            patch.display(),
            out.status,
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        )))
    }
}

fn write_record(
    target: &Path,
    method: Method,
    binary: &Path,
    patch: Option<&Path>,
    opts: &Options,
) -> Result<(), InstallError> {
    let bytes = std::fs::read(binary).map_err(|e| io(binary, &e))?;
    let origin = match method {
        Method::Source => serde_json::json!({
            "method": "source",
            "source": opts.source.as_ref().map(|p| p.display().to_string()),
        }),
        Method::Release => serde_json::json!({
            "method": "release",
            "release_url": opts.release_url,
            "asset": opts.asset,
            "repo": opts.repo,
        }),
    };
    let mut record = origin;
    record["sha256"] = sha256_hex(&bytes).into();
    record["play_patch"] = serde_json::json!({
        "ran": patch.is_some(),
        "command": patch.map(|p| p.display().to_string()),
    });
    let text =
        serde_json::to_string_pretty(&record).map_err(|e| InstallError::Failed(e.to_string()))?;
    let file = target.join("install.json");
    std::fs::write(&file, text + "\n").map_err(|e| io(&file, &e))
}
