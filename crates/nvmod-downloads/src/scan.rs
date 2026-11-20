// SPDX-License-Identifier: GPL-3.0-only
//! Reading the downloads folder: each archive with its `.meta`, its state and the Mod it matches.

use crate::{
    Digests, Error, HashCache, META_SUFFIX, SCHEMA_VERSION, STEP_HASH, STEP_READ, UNFINISHED_SUFFIX,
};
use nvmod_formats::metaini::{MetaIni, file_name_only, parse_nexus_name};
use nvmod_formats::text::read_limited;
use schemars::JsonSchema;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

/// Where an archive stands, as MO2 shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum State {
    /// A Mod installed from this archive is in the Instance, or the `.meta` says it was installed.
    Installed,
    /// Downloaded and not installed (including one that was installed and then removed).
    Uninstalled,
    /// Hidden from the list (`removed=true` in the `.meta`, MO2's "hide").
    Hidden,
    /// An unfinished download (`paused=true`, or a file name ending in `.unfinished`).
    Paused,
}

/// How an installed Mod was matched to an archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum MatchedBy {
    /// The Mod's `installationFile` is this archive's file name (compared without regard to case).
    InstallationFile,
    /// No Mod names this archive, but a Mod has the same Nexus mod id: another file of the same Nexus mod.
    ModId,
}

/// The installed Mod an archive matches.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct MatchedMod {
    /// The Mod's folder name.
    pub name: String,
    /// How it matched.
    pub by: MatchedBy,
}

/// Where the Nexus mod id of an archive came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum IdSource {
    /// The `modID` key of the `.meta`.
    Meta,
    /// The Nexus file name pattern (`<name>-<modid>-<version>-<timestamp>.<ext>`).
    FileName,
}

/// One archive of the downloads folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct Archive {
    /// The archive's file name only.
    pub file: String,
    /// Its size in bytes.
    pub size: u64,
    /// Its last change, in seconds since 1970 (`None` when the file system does not say).
    pub modified: Option<u64>,
    /// Its state.
    pub state: State,
    /// Whether a `.meta` file sits beside it. Without one the provenance is unknown.
    pub has_meta: bool,
    /// `gameName` of the `.meta`.
    pub game_name: Option<String>,
    /// The Nexus mod id (`modID` of the `.meta`, else the file name pattern).
    pub nexus_mod_id: Option<u64>,
    /// The Nexus file id (`fileID` of the `.meta`; the file name carries none).
    pub nexus_file_id: Option<u64>,
    /// Where `nexus_mod_id` came from.
    pub id_source: Option<IdSource>,
    /// `name` of the `.meta`: the file's name on the Nexus.
    pub name: Option<String>,
    /// `modName` of the `.meta`.
    pub mod_name: Option<String>,
    /// `version` of the `.meta`, else the version of the file name pattern.
    pub version: Option<String>,
    /// `newestVersion` of the `.meta`.
    pub newest_version: Option<String>,
    /// `repository` of the `.meta`.
    pub repository: Option<String>,
    /// The installed Mod it matches.
    pub matched_mod: Option<MatchedMod>,
    /// MD5, when asked for.
    pub md5: Option<String>,
    /// SHA-256, when asked for.
    pub sha256: Option<String>,
}

/// An installed Mod that has no archive in the downloads folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ModWithoutArchive {
    /// The Mod's folder name.
    pub name: String,
    /// Its `installationFile`: a file name only, `None` when it names none.
    pub installation_file: Option<String>,
    /// Its Nexus mod id.
    pub nexus_mod_id: Option<u64>,
}

/// `nvmod downloads list --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ListReport {
    /// The version of this layout.
    pub schema_version: u32,
    /// The folder that was read.
    pub downloads_dir: String,
    /// Whether the list holds only archives that match no Mod (and `mods_without_archive` is filled).
    pub unmatched_only: bool,
    /// The archives, sorted by file name without regard to case.
    pub archives: Vec<Archive>,
    /// With `unmatched_only`: the Mods none of the archives belongs to; empty otherwise.
    pub mods_without_archive: Vec<ModWithoutArchive>,
}

/// `nvmod downloads show --json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct ShowReport {
    /// The version of this layout.
    pub schema_version: u32,
    /// The folder the archive is in.
    pub downloads_dir: String,
    /// The archive.
    pub archive: Archive,
}

/// What [`list`] does besides reading.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ListOptions {
    /// Only archives that match no Mod, and the Mods that have no archive.
    pub unmatched_only: bool,
    /// Add MD5 and SHA-256 of each archive listed (reads the files unless the cache has them).
    pub hash: bool,
}

/// An installed Mod, as far as the library needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstalledMod {
    /// The Mod's folder name.
    pub name: String,
    /// `installationFile` of its `meta.ini`, as a file name only.
    pub installation_file: Option<String>,
    /// `modid` of its `meta.ini`.
    pub modid: Option<u64>,
}

/// The Mods in `mods_dir` (separators left out), sorted by folder name. A folder without a readable `meta.ini` is
/// a Mod of unknown origin.
#[must_use]
pub fn installed_mods(mods_dir: &Path) -> Vec<InstalledMod> {
    let mut mods: Vec<InstalledMod> = std::fs::read_dir(mods_dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            if name.ends_with("_separator") {
                return None;
            }
            let meta = read_limited(&e.path().join("meta.ini"))
                .ok()
                .map(|b| MetaIni::parse(&b));
            let general = |key: &str| {
                meta.as_ref()
                    .and_then(|m| m.get_string("General", key))
                    .filter(|v| !v.is_empty())
            };
            Some(InstalledMod {
                installation_file: general("installationFile")
                    .map(|p| file_name_only(&p).to_owned())
                    .filter(|n| !n.is_empty()),
                modid: general("modid")
                    .and_then(|v| v.trim().parse().ok())
                    .filter(|&id| id != 0),
                name,
            })
        })
        .collect();
    mods.sort_by(|a, b| a.name.cmp(&b.name));
    mods
}

/// The Mods by the archive that installed them and by Nexus mod id; the first Mod (by folder name) wins a tie.
struct ModIndex<'a> {
    by_file: HashMap<String, &'a str>,
    by_id: HashMap<u64, &'a str>,
}

impl<'a> ModIndex<'a> {
    fn new(mods: &'a [InstalledMod]) -> Self {
        let mut by_file = HashMap::new();
        let mut by_id = HashMap::new();
        for m in mods {
            if let Some(file) = &m.installation_file {
                by_file
                    .entry(file.to_lowercase())
                    .or_insert(m.name.as_str());
            }
            if let Some(id) = m.modid {
                by_id.entry(id).or_insert(m.name.as_str());
            }
        }
        ModIndex { by_file, by_id }
    }
}

fn flag(meta: Option<&MetaIni>, key: &str) -> bool {
    meta.and_then(|m| m.get_string("General", key))
        .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1"))
}

fn number(meta: Option<&MetaIni>, key: &str) -> Option<u64> {
    meta.and_then(|m| m.get_string("General", key))
        .and_then(|v| v.trim().parse().ok())
        .filter(|&n| n != 0)
}

fn text(meta: Option<&MetaIni>, key: &str) -> Option<String> {
    meta.and_then(|m| m.get_string("General", key))
        .filter(|v| !v.trim().is_empty())
}

fn seconds(time: Option<SystemTime>) -> Option<u64> {
    time.and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

fn archive(
    file: &str,
    size: u64,
    modified: Option<SystemTime>,
    meta: Option<&MetaIni>,
    index: &ModIndex<'_>,
) -> Archive {
    let from_name = parse_nexus_name(file);
    let meta_id = number(meta, "modID");
    let nexus_mod_id = meta_id.or(from_name.as_ref().map(|n| n.modid));
    let matched_mod = index
        .by_file
        .get(&file.to_lowercase())
        .map(|name| (name, MatchedBy::InstallationFile))
        .or_else(|| {
            nexus_mod_id
                .and_then(|id| index.by_id.get(&id))
                .map(|name| (name, MatchedBy::ModId))
        })
        .map(|(name, by)| MatchedMod {
            name: (*name).to_owned(),
            by,
        });
    let by_file = matched_mod
        .as_ref()
        .is_some_and(|m| m.by == MatchedBy::InstallationFile);
    let state = if flag(meta, "removed") {
        State::Hidden
    } else if flag(meta, "paused") || file.ends_with(UNFINISHED_SUFFIX) {
        State::Paused
    } else if by_file || (flag(meta, "installed") && !flag(meta, "uninstalled")) {
        State::Installed
    } else {
        State::Uninstalled
    };
    Archive {
        file: file.to_owned(),
        size,
        modified: seconds(modified),
        state,
        has_meta: meta.is_some(),
        game_name: text(meta, "gameName"),
        nexus_mod_id,
        nexus_file_id: number(meta, "fileID"),
        id_source: match (meta_id, &from_name) {
            (Some(_), _) => Some(IdSource::Meta),
            (None, Some(_)) => Some(IdSource::FileName),
            (None, None) => None,
        },
        name: text(meta, "name"),
        mod_name: text(meta, "modName"),
        version: text(meta, "version").or_else(|| from_name.map(|n| n.version)),
        newest_version: text(meta, "newestVersion"),
        repository: text(meta, "repository"),
        matched_mod,
        md5: None,
        sha256: None,
    }
}

fn read_meta(dir: &Path, file: &str) -> Option<MetaIni> {
    read_limited(&dir.join(format!("{file}{META_SUFFIX}")))
        .ok()
        .map(|b| MetaIni::parse(&b))
}

/// A name in the folder that is an archive: not a meta file, and not a leftover of nvmod's own atomic writes.
fn is_archive_name(name: &str) -> bool {
    !name.ends_with(META_SUFFIX) && !name.ends_with(".nvmod-tmp") && !name.starts_with('.')
}

fn add_hashes(
    archive: &mut Archive,
    dir: &Path,
    modified: Option<SystemTime>,
    cache: &mut HashCache,
    cancelled: &dyn Fn() -> bool,
) -> Result<(), Error> {
    let Digests { md5, sha256 } = cache.digests(
        &archive.file,
        &dir.join(&archive.file),
        archive.size,
        modified,
        cancelled,
    )?;
    archive.md5 = Some(md5);
    archive.sha256 = Some(sha256);
    Ok(())
}

/// The archives of `dir`, with their state, ids and matched Mod. Subfolders and `.meta` files are not archives.
///
/// `cache_path` is where the hash cache lives; it is read and written only with `options.hash`. `progress` gets
/// `(step, done, total)` for the steps [`STEP_READ`] and [`STEP_HASH`].
///
/// # Errors
/// [`Error::Cancelled`], or [`Error::Failed`] when the folder cannot be read.
pub fn list(
    dir: &Path,
    mods: &[InstalledMod],
    options: ListOptions,
    cache_path: Option<&Path>,
    cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(&'static str, u64, u64),
) -> Result<ListReport, Error> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| Error::Failed(format!("{}: {e}", dir.display())))?;
    let mut files: Vec<(String, u64, Option<SystemTime>)> = Vec::new();
    let mut metas: HashSet<String> = HashSet::new();
    for entry in entries.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        let Ok(info) = entry.metadata() else { continue };
        // `metadata` follows nothing for a DirEntry, so a symbolic link is looked through here.
        let info = if entry.file_type().is_ok_and(|t| t.is_symlink()) {
            std::fs::metadata(entry.path()).unwrap_or(info)
        } else {
            info
        };
        if !info.is_file() {
            continue;
        }
        if name.ends_with(META_SUFFIX) {
            metas.insert(name);
        } else if is_archive_name(&name) {
            files.push((name, info.len(), info.modified().ok()));
        }
    }
    files.sort_by_cached_key(|(name, _, _)| (name.to_lowercase(), name.clone()));
    let index = ModIndex::new(mods);
    let total = files.len() as u64;
    let mut archives = Vec::with_capacity(files.len());
    for (i, (file, size, modified)) in files.iter().enumerate() {
        if i % 64 == 0 {
            if cancelled() {
                return Err(Error::Cancelled);
            }
            progress(STEP_READ, i as u64, total);
        }
        let meta = metas
            .contains(&format!("{file}{META_SUFFIX}"))
            .then(|| read_meta(dir, file))
            .flatten();
        let mut a = archive(file, *size, *modified, meta.as_ref(), &index);
        // A `.meta` that exists but could not be read still means the provenance is on record.
        a.has_meta = metas.contains(&format!("{file}{META_SUFFIX}"));
        archives.push(a);
    }
    progress(STEP_READ, total, total);

    let mut mods_without_archive = Vec::new();
    if options.unmatched_only {
        let present: HashSet<String> = files.iter().map(|(n, _, _)| n.to_lowercase()).collect();
        mods_without_archive = mods
            .iter()
            .filter(|m| {
                m.installation_file
                    .as_ref()
                    .is_none_or(|f| !present.contains(&f.to_lowercase()))
            })
            .map(|m| ModWithoutArchive {
                name: m.name.clone(),
                installation_file: m.installation_file.clone(),
                nexus_mod_id: m.modid,
            })
            .collect();
        archives.retain(|a| a.matched_mod.is_none());
    }
    if options.hash {
        let mut cache = HashCache::open(cache_path);
        let names: HashSet<&str> = files.iter().map(|(n, _, _)| n.as_str()).collect();
        cache.retain_only(&names);
        let modified: HashMap<&str, Option<SystemTime>> =
            files.iter().map(|(n, _, t)| (n.as_str(), *t)).collect();
        let total = archives.len() as u64;
        for (i, a) in archives.iter_mut().enumerate() {
            if cancelled() {
                cache.save();
                return Err(Error::Cancelled);
            }
            progress(STEP_HASH, i as u64, total);
            let when = modified.get(a.file.as_str()).copied().flatten();
            if let Err(e) = add_hashes(a, dir, when, &mut cache, cancelled) {
                cache.save();
                return Err(e);
            }
        }
        progress(STEP_HASH, total, total);
        cache.save();
    }
    Ok(ListReport {
        schema_version: SCHEMA_VERSION,
        downloads_dir: dir.to_string_lossy().into_owned(),
        unmatched_only: options.unmatched_only,
        archives,
        mods_without_archive,
    })
}

/// One archive of `dir`, named by its file name.
///
/// # Errors
/// [`Error::Refused`] for a name that is not a plain file name or a file that is not there; [`Error::Cancelled`];
/// [`Error::Failed`] when a read failed.
pub fn show(
    dir: &Path,
    file: &str,
    mods: &[InstalledMod],
    hash: bool,
    cache_path: Option<&Path>,
    cancelled: &dyn Fn() -> bool,
    progress: &mut dyn FnMut(&'static str, u64, u64),
) -> Result<ShowReport, Error> {
    crate::check_file_name(file)?;
    let path = dir.join(file);
    let info = std::fs::metadata(&path)
        .ok()
        .filter(std::fs::Metadata::is_file)
        .ok_or_else(|| Error::Refused(format!("{file} is not in {}", dir.display())))?;
    let meta_path = dir.join(format!("{file}{META_SUFFIX}"));
    let meta = read_meta(dir, file);
    let index = ModIndex::new(mods);
    let modified = info.modified().ok();
    let mut a = archive(file, info.len(), modified, meta.as_ref(), &index);
    a.has_meta = meta_path.is_file();
    if hash {
        let mut cache = HashCache::open(cache_path);
        progress(STEP_HASH, 0, 1);
        let done = add_hashes(&mut a, dir, modified, &mut cache, cancelled);
        cache.save();
        done?;
        progress(STEP_HASH, 1, 1);
    }
    Ok(ShowReport {
        schema_version: SCHEMA_VERSION,
        downloads_dir: dir.to_string_lossy().into_owned(),
        archive: a,
    })
}
