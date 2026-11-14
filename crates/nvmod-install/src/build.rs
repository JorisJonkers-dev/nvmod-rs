// SPDX-License-Identifier: GPL-3.0-only
//! The Rust toolchain lookup and the source build of nv-rs.

use nvmod_runner::{Call, NETWORK_ENV, Runner};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// How long a source build may take: a cold release build of the workspace is slow, but it ends.
const BUILD_TIMEOUT: Duration = Duration::from_hours(2);
/// How much of a build log is kept, per stream.
const BUILD_LOG_CAP: usize = 32 * 1024 * 1024;
/// What cargo and rustup need from nvmod's environment to find the toolchain, the registry cache and the network.
const BUILD_ENV: &[&str] = &[
    "HOME",
    "CARGO_HOME",
    "RUSTUP_HOME",
    "RUSTUP_TOOLCHAIN",
    "TMPDIR",
    "CARGO_NET_OFFLINE",
    "CARGO_HTTP_CAINFO",
    "CARGO_REGISTRIES_CRATES_IO_PROTOCOL",
];

/// Find `cargo`. `configured` (the `--cargo` flag, else `NVMOD_CARGO`) names the program to use; when it does not
/// exist the toolchain counts as absent, which is how tests simulate a machine without Rust. Without it, `cargo`
/// is looked up on `PATH`.
#[must_use]
pub fn resolve_cargo(configured: Option<&Path>) -> Option<PathBuf> {
    if let Some(path) = configured {
        return path.is_file().then(|| path.to_path_buf());
    }
    let name = format!("cargo{}", std::env::consts::EXE_SUFFIX);
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|dir| dir.join(&name))
        .find(|candidate| candidate.is_file())
}

/// Build the `nv-viewer` program of the nv-rs workspace at `source` and return the path of the built program.
///
/// # Errors
/// The build log (cargo's output) when cargo cannot run or the build fails.
pub(crate) fn build_from_source(cargo: &Path, source: &Path) -> Result<PathBuf, String> {
    let target = source.join("target");
    let args: Vec<String> = [
        "build",
        "--release",
        "--locked",
        "-p",
        "nv-viewer",
        "--target-dir",
    ]
    .iter()
    .map(|a| (*a).to_owned())
    .chain([target.display().to_string()])
    .collect();
    let runner = Runner {
        timeout: BUILD_TIMEOUT,
        max_output: BUILD_LOG_CAP,
        max_stderr: BUILD_LOG_CAP,
        ..Runner::default()
    }
    .inherit(NETWORK_ENV)
    .inherit(BUILD_ENV);
    let mut call = Call::new("cargo", cargo, &args);
    call.cwd = Some(source);
    let out = runner
        .run_any(&call)
        .map_err(|e| format!("cannot run {}: {e}", cargo.display()))?;
    if out.exit_code != 0 {
        return Err(format!(
            "{} build failed (exit status: {})\n{}{}",
            cargo.display(),
            out.exit_code,
            String::from_utf8_lossy(&out.stdout),
            out.stderr
        ));
    }
    let built = target
        .join("release")
        .join(format!("nv-viewer{}", std::env::consts::EXE_SUFFIX));
    if built.is_file() {
        Ok(built)
    } else {
        Err(format!("the build did not produce {}", built.display()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_configured_path_that_does_not_exist_means_no_toolchain() {
        assert_eq!(resolve_cargo(Some(Path::new("/nonexistent/cargo"))), None);
    }

    #[test]
    fn a_configured_path_that_exists_is_used() {
        let here = Path::new(env!("CARGO"));
        assert_eq!(resolve_cargo(Some(here)), Some(here.to_path_buf()));
    }
}
