// SPDX-License-Identifier: GPL-3.0-only
//! The downloads library: the archives of the Instance's downloads folder with their MO2 `.meta` files, matched
//! to the installed Mods; installing a Mod from one; hiding, unhiding and forgetting one.
//!
//! The downloads folder is `<Instance>/downloads` unless a request names another (a relative path is read from
//! the Instance folder). Nothing here uses the network or starts a process.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
pub use nvmod_downloads::{
    Archive, IdSource, ListReport as ListOutput, MatchedBy, MatchedMod, ModWithoutArchive,
    ShowReport as ShowOutput, State, list_schema, show_schema,
};
use nvmod_downloads::{Error, ListOptions, MetaChange};
use nvmod_instance::NvmodInstance;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const LIST: &str = "downloads.list";
const SHOW: &str = "downloads.show";
const INSTALL: &str = "downloads.install";
const CHANGE: &str = "downloads.change";

/// Where the hash cache lives, below the Instance folder.
const HASH_CACHE: &str = ".nvmod/downloads-hashes.json";
/// Where a forgotten archive goes, below the Instance folder.
const TRASH: &str = ".nvmod/trash/downloads";

/// What [`list`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The downloads folder, `None` for `<Instance>/downloads`.
    #[serde(default)]
    pub downloads: Option<PathBuf>,
    /// Only the archives that match no Mod, and the Mods that have no archive.
    #[serde(default)]
    pub unmatched: bool,
    /// Add MD5 and SHA-256 to each archive (cached by size and modification time).
    #[serde(default)]
    pub hash: bool,
}

/// What [`show`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShowRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The downloads folder, `None` for `<Instance>/downloads`.
    #[serde(default)]
    pub downloads: Option<PathBuf>,
    /// The archive's file name (a name only, never a path).
    pub archive: String,
    /// Add MD5 and SHA-256.
    #[serde(default)]
    pub hash: bool,
}

/// What [`install`] needs: an archive of the library, and what `install::install` takes besides the path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The downloads folder, `None` for `<Instance>/downloads`.
    #[serde(default)]
    pub downloads: Option<PathBuf>,
    /// The archive's file name (a name only, never a path).
    pub archive: String,
    /// The profile, `None` for the only one there is.
    #[serde(default)]
    pub profile: Option<String>,
    /// The Mod's folder name; the archive's file name without its extension when `None`.
    #[serde(default)]
    pub name: Option<String>,
    /// Plan only: nothing is written, the `.meta` included.
    #[serde(default)]
    pub dry_run: bool,
    /// A JSON file with answers to the FOMOD's questions.
    #[serde(default)]
    pub answers: Option<PathBuf>,
    /// The most bytes the extraction may write; `None` for the default limit.
    #[serde(default)]
    pub max_extract_bytes: Option<u64>,
    /// The other limits of the extraction (entries, bytes per entry, expansion ratio, folder depth, steps of work,
    /// seconds); a value left `None` keeps its default. `max_extract_bytes`, when set, wins over `max_bytes`.
    #[serde(default)]
    pub extract_limits: crate::install::LimitOverrides,
}

/// What [`install`] did to the archive's `.meta`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MetaUpdate {
    /// The `.meta` now says the archive is installed (it was written, or already said so).
    pub installed: bool,
    /// Whether the file was written.
    pub written: bool,
    /// Why the `.meta` could not be updated. The Mod is installed all the same.
    pub warning: Option<String>,
}

/// The result of [`install`].
#[derive(Debug, Clone, Serialize)]
pub struct InstallOutput {
    /// What the install job gave: the Mod installed (or planned), or the FOMOD's questions.
    pub install: crate::install::InstallOutput,
    /// The `.meta` update; `None` for a dry run and for a FOMOD that needs answers.
    pub meta: Option<MetaUpdate>,
}

/// What [`change`] does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Action {
    /// Hide the archive (`removed=true` in its `.meta`).
    Hide,
    /// Show it again.
    Unhide,
    /// Move the archive and its `.meta` to `.nvmod/trash/downloads/`; nothing is deleted.
    Forget,
}

/// What [`change`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The downloads folder, `None` for `<Instance>/downloads`.
    #[serde(default)]
    pub downloads: Option<PathBuf>,
    /// The archive's file name (a name only, never a path).
    pub archive: String,
    /// What to do.
    pub action: Action,
    /// Plan only: nothing is written.
    #[serde(default)]
    pub dry_run: bool,
}

/// The result of [`change`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChangeOutput {
    /// What was asked.
    pub action: Action,
    /// The archive's file name.
    pub archive: String,
    /// Whether anything changed (false when the archive was already hidden, or not hidden).
    pub changed: bool,
    /// True when nothing was written because of `dry_run`.
    pub dry_run: bool,
    /// For [`Action::Forget`]: the folder below the Instance (written with `/`) that now holds the files.
    pub trashed_to: Option<String>,
}

/// The downloads folder of a request and whether the request named it.
#[must_use]
pub fn downloads_dir(instance: &Path, over: Option<&Path>) -> (PathBuf, bool) {
    match over {
        Some(path) if path.is_absolute() => (path.to_path_buf(), true),
        Some(path) => (instance.join(path), true),
        None => (instance.join("downloads"), false),
    }
}

/// `e` as the job's own result: [`Outcome::Cancelled`] or the error.
fn stop<T>(e: Error, job: &'static str) -> Result<Outcome<T>, JobError> {
    match e {
        Error::Cancelled => Ok(Outcome::Cancelled),
        Error::Refused(why) => Err(JobError::new(ErrorKind::Refused, job, why)),
        Error::Failed(why) => Err(JobError::new(ErrorKind::Failed, job, why)),
    }
}

fn forward(progress: &mut dyn Progress, step: &'static str, done: u64, total: u64) {
    let label = if step == nvmod_downloads::STEP_HASH {
        "downloads.hash"
    } else {
        "downloads.list.reading"
    };
    progress.report(&Event {
        step,
        label,
        done: Some(done),
        total: Some(total),
    });
}

struct Context {
    instance: NvmodInstance,
    dir: PathBuf,
    mods: Vec<nvmod_downloads::InstalledMod>,
}

fn context(
    instance: &Path,
    downloads: Option<&Path>,
    write: bool,
    job: &'static str,
) -> Result<Context, JobError> {
    let inst = crate::open(instance, write, job)?;
    let (dir, named) = downloads_dir(&inst.root, downloads);
    let mods = nvmod_downloads::installed_mods(&inst.root.join("mods"));
    if named && !dir.is_dir() {
        return Err(JobError::new(
            ErrorKind::Refused,
            job,
            format!("the downloads folder {} does not exist", dir.display()),
        ));
    }
    Ok(Context {
        instance: inst,
        dir,
        mods,
    })
}

/// The archives of the downloads folder with their state, Nexus ids and matched Mod; with `unmatched` only those
/// that match no Mod, and the Mods that have no archive. A downloads folder that does not exist (the default one)
/// is an empty library. Reads only; hashing keeps its cache in the Instance's `.nvmod` folder.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance or a named downloads folder is missing;
/// [`ErrorKind::Failed`] when the folder cannot be read.
pub fn list(
    request: &ListRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<ListOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let ctx = context(&request.instance, request.downloads.as_deref(), false, LIST)?;
    if !ctx.dir.is_dir() {
        return Ok(Outcome::Done(ListOutput {
            schema_version: nvmod_downloads::SCHEMA_VERSION,
            downloads_dir: ctx.dir.to_string_lossy().into_owned(),
            unmatched_only: request.unmatched,
            archives: Vec::new(),
            mods_without_archive: if request.unmatched {
                ctx.mods
                    .iter()
                    .map(|m| ModWithoutArchive {
                        name: m.name.clone(),
                        installation_file: m.installation_file.clone(),
                        nexus_mod_id: m.modid,
                    })
                    .collect()
            } else {
                Vec::new()
            },
        }));
    }
    let cache = ctx.instance.root.join(HASH_CACHE);
    let options = ListOptions {
        unmatched_only: request.unmatched,
        hash: request.hash,
    };
    let result = nvmod_downloads::list(
        &ctx.dir,
        &ctx.mods,
        options,
        Some(&cache),
        &|| cancel.is_cancelled(),
        &mut |step, done, total| forward(progress, step, done, total),
    );
    match result {
        Ok(report) => Ok(Outcome::Done(report)),
        Err(e) => stop(e, LIST),
    }
}

/// One archive of the downloads folder, like a row of [`list`].
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance, the name is not a file name or the archive is not
/// in the folder; [`ErrorKind::Failed`] when a read failed.
pub fn show(
    request: &ShowRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<ShowOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let ctx = context(&request.instance, request.downloads.as_deref(), false, SHOW)?;
    let cache = ctx.instance.root.join(HASH_CACHE);
    let result = nvmod_downloads::show(
        &ctx.dir,
        &request.archive,
        &ctx.mods,
        request.hash,
        Some(&cache),
        &|| cancel.is_cancelled(),
        &mut |step, done, total| forward(progress, step, done, total),
    );
    match result {
        Ok(report) => Ok(Outcome::Done(report)),
        Err(e) => stop(e, SHOW),
    }
}

/// Install the Mod of an archive of the library: [`crate::install::install`] on the archive's path, then, when the
/// Mod was installed, the archive's `.meta` marked installed (`installed=true`, and `uninstalled=false` when it
/// was true). A failed `.meta` update is reported in [`MetaUpdate::warning`]; the Mod stays installed. A paused
/// (unfinished) download is refused.
///
/// # Errors
/// As [`crate::install::install`], and [`ErrorKind::Refused`] when the name is not a file name, the archive is
/// not in the folder or is an unfinished download.
pub fn install(
    request: &InstallRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<InstallOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let ctx = context(
        &request.instance,
        request.downloads.as_deref(),
        !request.dry_run,
        INSTALL,
    )?;
    let shown = nvmod_downloads::show(
        &ctx.dir,
        &request.archive,
        &ctx.mods,
        false,
        None,
        &|| cancel.is_cancelled(),
        &mut |_, _, _| {},
    );
    let shown = match shown {
        Ok(shown) => shown,
        Err(e) => return stop(e, INSTALL),
    };
    if shown.archive.state == State::Paused {
        return Err(JobError::new(
            ErrorKind::Refused,
            INSTALL,
            format!("{} is an unfinished download", request.archive),
        ));
    }
    let inner = crate::install::InstallRequest {
        instance: request.instance.clone(),
        profile: request.profile.clone(),
        archive: ctx.dir.join(&request.archive),
        name: request.name.clone(),
        dry_run: request.dry_run,
        answers: request.answers.clone(),
        fomod_answers: None,
        on_conflict: crate::install::OnConflict::Refuse,
        max_extract_bytes: request.max_extract_bytes,
        extract_limits: request.extract_limits,
    };
    let output = match crate::install::install(&inner, progress, cancel)? {
        Outcome::Cancelled => return Ok(Outcome::Cancelled),
        Outcome::Done(output) => output,
    };
    let meta = match &output {
        crate::install::InstallOutput::Installed(report) if !report.dry_run => {
            progress.report(&Event::step(
                "update-meta",
                "downloads.install.updating-meta",
            ));
            Some(
                match nvmod_downloads::change_meta(
                    &ctx.dir,
                    &request.archive,
                    MetaChange::Installed,
                ) {
                    Ok(written) => MetaUpdate {
                        installed: true,
                        written,
                        warning: None,
                    },
                    Err(e) => MetaUpdate {
                        installed: false,
                        written: false,
                        warning: Some(e.to_string()),
                    },
                },
            )
        }
        _ => None,
    };
    Ok(Outcome::Done(InstallOutput {
        install: output,
        meta,
    }))
}

/// Hide, unhide or forget an archive of the library. Hiding and showing edit only the `removed` line of the
/// `.meta` (a missing `.meta` is made for a hide). Forgetting moves the archive and its `.meta` to
/// `.nvmod/trash/downloads/<id>/`; nothing is deleted.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance, the name is not a file name or the archive is not
/// in the folder; [`ErrorKind::Failed`] when a write or move failed (what was moved is put back).
pub fn change(
    request: &ChangeRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<ChangeOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let ctx = context(
        &request.instance,
        request.downloads.as_deref(),
        !request.dry_run,
        CHANGE,
    )?;
    let name = nvmod_downloads::check_file_name(&request.archive)
        .map_err(|e| JobError::new(ErrorKind::Refused, CHANGE, e.to_string()))?;
    if !ctx.dir.join(name).is_file() {
        return Err(JobError::new(
            ErrorKind::Refused,
            CHANGE,
            format!("{name} is not in {}", ctx.dir.display()),
        ));
    }
    progress.report(&Event::step("change", "downloads.change.changing"));
    let out = |changed, trashed_to| ChangeOutput {
        action: request.action,
        archive: name.to_owned(),
        changed,
        dry_run: request.dry_run,
        trashed_to,
    };
    if request.dry_run {
        return Ok(Outcome::Done(out(request.action == Action::Forget, None)));
    }
    let result = match request.action {
        Action::Hide => nvmod_downloads::change_meta(&ctx.dir, name, MetaChange::Hide)
            .map(|changed| out(changed, None)),
        Action::Unhide => nvmod_downloads::change_meta(&ctx.dir, name, MetaChange::Unhide)
            .map(|changed| out(changed, None)),
        Action::Forget => {
            let root = ctx.instance.root.join(TRASH);
            nvmod_downloads::trash(&ctx.dir, name, &root).map(|folder| {
                let id = folder
                    .file_name()
                    .map_or_default(|n| n.to_string_lossy().into_owned());
                out(true, Some(format!("{TRASH}/{id}")))
            })
        }
    };
    match result {
        Ok(output) => Ok(Outcome::Done(output)),
        Err(e) => stop(e, CHANGE),
    }
}
