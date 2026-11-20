// SPDX-License-Identifier: GPL-3.0-only
//! The answers an install of a FOMOD was given, kept next to the Mod as `.nvmod/fomod-answers.json`, and the two
//! things they are for: [`reinstall`] replays them against the archive, [`reconfigure`] replays them with some
//! of them changed (and [`open_saved`] hands the previous choices to a [`Session`] so a prompt can show them
//! ticked).

use crate::fomod::Answers;
use crate::session::Session;
use crate::{InstallError, Limits, OnConflict, Report, Request, install, journal};
use nvmod_instance::{NvmodInstance, find_ci};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// Where the answers are kept, relative to the Mod folder.
pub const ANSWERS_PATH: &str = ".nvmod/fomod-answers.json";

/// The version of the answers file this nvmod writes and reads.
pub const ANSWERS_SCHEMA_VERSION: u32 = 1;

/// The content of `.nvmod/fomod-answers.json`. `nvmod install --answers` reads the same file (it only looks at
/// `answers`), so a saved file can be edited and given back.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AnswersFile {
    /// The version of this file's layout, `1`.
    pub schema_version: u32,
    /// The FOMOD's `moduleName` (empty when it has none).
    pub module: String,
    /// The archive (or folder) the Mod was installed from, as an absolute path. `nvmod mods reinstall` reads
    /// it again from there unless it is given another.
    pub archive: String,
    /// Question id (`step name/group name`) to the names of the options chosen. Only the groups that were
    /// answered are here; a group the installer decided is decided again the same way.
    pub answers: Answers,
}

/// Write the file into `mod_dir`.
pub(crate) fn write(mod_dir: &Path, file: &AnswersFile) -> Result<(), String> {
    let mut text = serde_json::to_string_pretty(file).map_err(|e| e.to_string())?;
    text.push('\n');
    journal::write_atomic(&mod_dir.join(ANSWERS_PATH), text.as_bytes())
}

/// The absolute form of `path`, for the record of where a Mod came from.
pub(crate) fn absolute(path: &Path) -> String {
    std::path::absolute(path)
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned()
}

/// The answers saved with the Mod `name`.
///
/// # Errors
/// The Mod has no folder, no saved answers (it was not installed from a FOMOD, or by an older nvmod), or the file
/// is not one this nvmod reads.
pub fn read(instance: &NvmodInstance, name: &str) -> Result<(String, AnswersFile), String> {
    let folder = find_ci(&instance.root.join("mods"), name)
        .filter(|p| p.is_dir())
        .ok_or_else(|| format!("there is no Mod folder `{name}` in the Instance"))?;
    let folder_name = folder
        .file_name()
        .map_or_default(|n| n.to_string_lossy().into_owned());
    let path = folder.join(ANSWERS_PATH);
    let text = fs::read_to_string(&path).map_err(|_| {
        format!("`{folder_name}` has no saved FOMOD answers ({ANSWERS_PATH}); install it again from its archive")
    })?;
    let file: AnswersFile = serde_json::from_str(&text)
        .map_err(|e| format!("{}: not a FOMOD answers file: {e}", path.display()))?;
    if file.schema_version != ANSWERS_SCHEMA_VERSION {
        return Err(format!(
            "{}: schema_version {} is not known to this nvmod",
            path.display(),
            file.schema_version
        ));
    }
    Ok((folder_name, file))
}

/// What to reinstall or reconfigure.
#[derive(Debug, Clone)]
pub struct ReinstallRequest<'a> {
    /// The Instance.
    pub instance: &'a NvmodInstance,
    /// The profile.
    pub profile: &'a str,
    /// The Mod to install again.
    pub r#mod: &'a str,
    /// The archive, when it is not where the saved answers say it was.
    pub archive: Option<&'a Path>,
    /// [`reconfigure`] only: answers that replace the saved ones for the same group.
    pub answers: Option<&'a Answers>,
    /// Plan only; write nothing.
    pub dry_run: bool,
    /// Size limits of the extraction.
    pub limits: Limits,
}

fn archive_of(request: &ReinstallRequest, saved: &AnswersFile) -> Result<PathBuf, InstallError> {
    let path = request
        .archive
        .map_or_else(|| PathBuf::from(&saved.archive), Path::to_path_buf);
    if path.exists() {
        Ok(path)
    } else {
        Err(InstallError::Refused(format!(
            "the archive `{}` is not there; give the archive with --archive",
            path.display()
        )))
    }
}

fn replay(request: &ReinstallRequest, changes: Option<&Answers>) -> Result<Report, InstallError> {
    let (folder, saved) = read(request.instance, request.r#mod).map_err(InstallError::Refused)?;
    let archive = archive_of(request, &saved)?;
    let mut answers = saved.answers.clone();
    for (id, picked) in changes.into_iter().flatten() {
        answers.insert(id.clone(), picked.clone());
    }
    install(&Request {
        instance: request.instance,
        profile: request.profile,
        archive: &archive,
        name: Some(&folder),
        dry_run: request.dry_run,
        answers: None,
        answers_inline: Some(&answers),
        limits: request.limits,
        on_conflict: OnConflict::Replace,
    })
}

/// Install the Mod again from its archive with the answers it was installed with, replacing the Mod folder and
/// keeping its priority, enabled state, Plugin states, notes and categories.
///
/// # Errors
/// See [`InstallError`]. A refusal writes nothing; the Mod is as it was.
pub fn reinstall(request: &ReinstallRequest) -> Result<Report, InstallError> {
    replay(request, None)
}

/// Like [`reinstall`], with `request.answers` replacing the saved answer of each group they name. A group the
/// changes open up (a step a new flag shows) comes back as [`InstallError::Questions`], and nothing is written;
/// [`open_saved`] and [`Session::next`] ask those step by step with the old choices ticked.
///
/// # Errors
/// See [`InstallError`].
pub fn reconfigure(request: &ReinstallRequest) -> Result<Report, InstallError> {
    replay(request, request.answers)
}

/// Open the archive a Mod was installed from, with the answers it was installed with: for a prompt that
/// reconfigures it. Pass the saved answers as `previous` to [`Session::next`] and the answers collected from
/// the owner as `answers`, then install with [`install`] using `name` = the Mod, [`OnConflict::Replace`] and
/// the final answers as `answers_inline`.
///
/// # Errors
/// The Mod has no saved answers, or its archive is gone or refused.
pub fn open_saved(request: &ReinstallRequest) -> Result<(Session, AnswersFile), InstallError> {
    let (_, saved) = read(request.instance, request.r#mod).map_err(InstallError::Refused)?;
    let archive = archive_of(request, &saved)?;
    let session = Session::open(request.instance, request.profile, &archive, request.limits)
        .map_err(InstallError::Refused)?;
    Ok((session, saved))
}
