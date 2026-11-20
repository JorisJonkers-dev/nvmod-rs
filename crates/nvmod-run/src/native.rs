// SPDX-License-Identifier: GPL-3.0-only
//! The native run of nv-rs: the run folder it starts from, the viewer it finds and the log it writes.
//!
//! nv-rs reads `FalloutNV.exe` from the folder that holds `Data` and keeps the player's settings and `plugins.txt`
//! under a `userdata` folder it takes from `linux_profile.txt` (`NV_LINUX_PROFILE`). The run folder of a profile,
//! `<instance>/run/<profile>/`, gives it all three without copying a game file or touching the Steam folder:
//!
//! ```text
//! run/<profile>/Data               -> staging/<profile>/data   (the merged tree, hard links)
//! run/<profile>/FalloutNV.exe      -> <base copy>/FalloutNV.exe (a link, never a copy)
//! run/<profile>/Fallout_default.ini -> <base copy>/Fallout_default.ini
//! run/<profile>/linux_profile.txt   `userdata = <run>/userdata`
//! run/<profile>/userdata/...        My Games/FalloutNV INIs and AppData/Local/FalloutNV plugin lists
//! ```

use nvmod_instance::{GAME_EXE, NvmodInstance, find_ci};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

/// The base copy's own default settings, which nv-rs seeds `Fallout.ini` from.
const DEFAULT_INI: &str = "Fallout_default.ini";

/// The native run of one profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Native {
    /// The folder nv-viewer is started with (it holds `Data` and `FalloutNV.exe`).
    pub run_dir: PathBuf,
    /// The player's user data of this profile (`Documents`, `AppData/Local`).
    pub userdata: PathBuf,
    /// The log file the run's output goes to.
    pub log: PathBuf,
    /// Where the viewer was found.
    pub viewer: PathBuf,
}

/// The run folder of `profile`.
#[must_use]
pub fn run_dir(instance: &NvmodInstance, profile: &str) -> PathBuf {
    instance.root.join("run").join(profile)
}

/// `Documents/My Games/FalloutNV` of the profile's user data.
#[must_use]
pub fn my_games(userdata: &Path) -> PathBuf {
    userdata
        .join("Documents")
        .join("My Games")
        .join("FalloutNV")
}

/// `AppData/Local/FalloutNV` of the profile's user data.
#[must_use]
pub fn app_data(userdata: &Path) -> PathBuf {
    userdata.join("AppData").join("Local").join("FalloutNV")
}

/// The program to start: the one named with `--viewer` (or `NV_VIEWER`). raul links the engine and names its own
/// program; nothing is searched for.
///
/// # Errors
/// No program is named, or the one named is not a file.
pub fn find_viewer(named: Option<&Path>) -> Result<PathBuf, String> {
    match named {
        Some(p) if p.is_file() => Ok(p.to_path_buf()),
        Some(p) => Err(format!(
            "the program to start, {}, is not a file",
            p.display()
        )),
        None => Err(
            "no program to start is named: nv-rs starts the program given with --viewer".to_owned(),
        ),
    }
}

/// The state folder logs go to: `$XDG_STATE_HOME/nv-rs/logs`, else `~/.local/state/nv-rs/logs`.
#[must_use]
pub fn log_dir() -> Option<PathBuf> {
    let state = std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| {
            std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state"))
        })?;
    Some(state.join("nv-rs").join("logs"))
}

/// `20261115T093000` (UTC) for a Unix time.
#[must_use]
pub fn stamp(secs: u64) -> String {
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = i64::try_from(days).unwrap_or(0) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}{month:02}{day:02}T{:02}{:02}{:02}",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// The log file of a run started now.
#[must_use]
pub fn log_path() -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    log_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(format!("run-{}.log", stamp(now)))
}

#[cfg(unix)]
fn link(target: &Path, at: &Path) -> Result<(), String> {
    if std::fs::read_link(at).is_ok_and(|t| t == target) {
        return Ok(());
    }
    if std::fs::symlink_metadata(at).is_ok() {
        std::fs::remove_file(at).map_err(|e| format!("{}: {e}", at.display()))?;
    }
    std::os::unix::fs::symlink(target, at).map_err(|e| format!("{}: {e}", at.display()))
}

#[cfg(not(unix))]
fn link(_: &Path, _: &Path) -> Result<(), String> {
    Err(
        "the native run folder needs symbolic links, which this platform build does not make"
            .to_owned(),
    )
}

/// Make (or refresh) the run folder of `profile`: the links, the user data folders and `linux_profile.txt`.
/// Nothing is copied and nothing outside the Instance is written.
///
/// # Errors
/// The base copy has no game executable, or a link or folder cannot be made.
pub fn prepare_run_dir(instance: &NvmodInstance, profile: &str) -> Result<Native, String> {
    let run = run_dir(instance, profile);
    let userdata = run.join("userdata");
    let exe = find_ci(&instance.config.base, GAME_EXE).ok_or_else(|| {
        format!(
            "the base copy {} has no {GAME_EXE}",
            instance.config.base.display()
        )
    })?;
    for dir in [&run, &my_games(&userdata), &app_data(&userdata)] {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    link(&crate::data_dir(instance, profile), &run.join("Data"))?;
    link(&exe, &run.join(GAME_EXE))?;
    if let Some(ini) = find_ci(&instance.config.base, DEFAULT_INI) {
        link(&ini, &run.join(DEFAULT_INI))?;
    }
    let file = run.join("linux_profile.txt");
    let text = format!("userdata = {}\n", userdata.display());
    if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
        std::fs::write(&file, text).map_err(|e| format!("{}: {e}", file.display()))?;
    }
    Ok(Native {
        run_dir: run,
        userdata,
        log: log_path(),
        viewer: PathBuf::new(),
    })
}

/// The first lines of a run's log, as `nv-rs-run` writes them: what is run, with what, where.
#[must_use]
pub fn log_header(native: &Native, profile: &str, env_note: &str) -> String {
    format!(
        "# nvmod run {}\n# build: {}\n# profile: {profile}\n# run folder: {}\n# host: {} {}; session: {}\n# {env_note}\n",
        native
            .log
            .file_stem()
            .map_or_else(String::new, |s| s.to_string_lossy().into_owned()),
        native.viewer.display(),
        native.run_dir.display(),
        std::env::consts::OS,
        std::env::consts::ARCH,
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "unknown".to_owned()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_named_program_is_found_and_the_refusals_name_no_command() {
        let file = std::env::temp_dir().join(format!("nvmod-viewer-{}", std::process::id()));
        std::fs::write(&file, b"x").unwrap();
        assert_eq!(find_viewer(Some(&file)).unwrap(), file);
        std::fs::remove_file(&file).unwrap();
        let gone = find_viewer(Some(&file)).unwrap_err();
        assert!(
            gone.contains("is not a file") && !gone.contains("nvmod "),
            "{gone}"
        );
        let none = find_viewer(None).unwrap_err();
        assert!(
            none.contains("--viewer") && !none.contains("install"),
            "{none}"
        );
    }

    #[test]
    fn stamps_are_utc_dates() {
        assert_eq!(stamp(0), "19700101T000000");
        assert_eq!(stamp(1_794_000_000), "20261106T212000");
        assert_eq!(stamp(951_782_400 + 86_399), "20000229T235959");
    }
}
