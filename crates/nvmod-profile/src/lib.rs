// SPDX-License-Identifier: GPL-3.0-only
//! Profile materialisation: what Mod Organizer 2 does when it starts a profile, done before a launch.
//!
//! [`apply`] places a profile in a Wine prefix: its game INIs (with `LocalSettings`), a regenerated
//! `initweaks.ini`, the local-saves redirect, `plugins.txt` and `loadorder.txt`, and archive invalidation.
//! The game and the tools write those files at the prefix paths, so placement is a copy with a record of what
//! was placed; [`finish`] (and the start of the next [`apply`]) copies back what changed, so nothing the game
//! wrote is lost. Whatever apply displaces is backed up, and the user's own files come back when the profile
//! stops placing them. Every write goes through [`txn::Txn`]: nothing outside the Instance and the named prefix
//! is written, and a failed or interrupted run is rolled back.

pub mod archive;
pub mod settings;
pub mod target;
pub mod tweaks;
pub mod txn;

use nvmod_formats::metaini::MetaIni;
use nvmod_instance::{NvmodInstance, find_ci};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};

pub use settings::Settings;
pub use target::Target;
pub use txn::Action;

/// The five game INIs of Fallout: New Vegas and TTW, in MO2's order.
pub const INIS: [&str; 5] = [
    "fallout.ini",
    "falloutprefs.ini",
    "falloutcustom.ini",
    "GECKCustom.ini",
    "GECKPrefs.ini",
];
/// The plugin lists placed in the prefix.
pub const LISTS: [&str; 2] = ["plugins.txt", "loadorder.txt"];
/// The save folder name MO2 redirects to the profile's `saves/`.
pub const SAVE_FOLDER: &str = "__MO_Saves";
/// The value `sLocalSavePath` takes while local saves are on.
pub const SAVE_PATH_VALUE: &str = "__MO_Saves\\";

/// Why nothing was applied.
#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    /// Refused before anything was written (exit code 2).
    Refused(String),
    /// A step failed and what was started has been put back (exit code 4).
    Failed(String),
}

/// What a run did (or, for a dry run, would do).
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// `applied`, `unchanged`, `dry-run` or `collected`.
    pub status: &'static str,
    /// The profile now placed (`None` after a `finish` with nothing placed).
    pub profile: Option<String>,
    /// The profile that was placed before.
    pub previous: Option<String>,
    /// The prefix.
    pub prefix: String,
    /// The switches read from `settings.ini`.
    pub settings: Settings,
    /// Every change, in order.
    pub actions: Vec<Action>,
    /// Things left alone on purpose.
    pub notes: Vec<String>,
    /// Files in the Staging tree after a restage (only when the invalidation archive came or went).
    pub staged_files: Option<usize>,
}

/// What is recorded about a prefix between runs: `.nvmod/prefix/<key>/state.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct State {
    schema: u32,
    prefix: String,
    /// The profile whose files are in the prefix.
    profile: Option<String>,
    /// File name -> SHA-256 of the bytes placed (the prefix copy differs from it when the game wrote).
    slots: BTreeMap<String, String>,
    /// File name -> `file` (the user's own file is saved under `originals/`) or `none` (there was none).
    originals: BTreeMap<String, String>,
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut s, b| {
            let _ = write!(s, "{b:02x}");
            s
        })
}

fn state_dir(instance: &NvmodInstance, target: &Target) -> PathBuf {
    let key = hex(target.root.display().to_string().as_bytes());
    instance.root.join(".nvmod").join("prefix").join(&key[..16])
}

fn at(dir: &Path, name: &str) -> PathBuf {
    find_ci(dir, name).unwrap_or_else(|| dir.join(name))
}

struct Run<'a> {
    instance: &'a NvmodInstance,
    target: &'a Target,
    txn: txn::Txn,
    state: State,
    dir: PathBuf,
    notes: Vec<String>,
}

impl Run<'_> {
    fn slot_dir(&self, name: &str) -> &Path {
        if LISTS.contains(&name) {
            &self.target.app_data
        } else {
            &self.target.my_games
        }
    }

    fn state_path(&self) -> PathBuf {
        self.dir.join("state.json")
    }

    fn save_state(&mut self) -> Result<(), String> {
        let text = serde_json::to_string_pretty(&self.state).map_err(|e| e.to_string())? + "\n";
        let path = self.state_path();
        if self.txn.read(&path)?.as_deref() == Some(text.as_bytes()) {
            return Ok(());
        }
        self.txn
            .write(&path, text.as_bytes(), "record of the placed files")
    }

    /// Copy what the game changed in the prefix back into the profile that was placed.
    fn reclaim(&mut self) -> Result<(), String> {
        let Some(prev) = self.state.profile.clone() else {
            return Ok(());
        };
        let pdir = self.instance.profile_dir(&prev);
        let names: Vec<String> = self.state.slots.keys().cloned().collect();
        for name in names {
            let live = at(self.slot_dir(&name), &name);
            let Some(bytes) = self.txn.read(&live)? else {
                continue;
            };
            let sha = hex(&bytes);
            if self.state.slots.get(&name) == Some(&sha) {
                continue;
            }
            if pdir.is_dir() {
                let own = at(&pdir, &name);
                self.txn.write(
                    &own,
                    &bytes,
                    &format!("collected from the prefix (profile `{prev}`)"),
                )?;
            } else {
                self.notes.push(format!(
                    "profile `{prev}` is gone; the changed {name} in the prefix was kept there"
                ));
            }
            self.state.slots.insert(name, sha);
        }
        Ok(())
    }

    fn original_path(&self, name: &str) -> PathBuf {
        self.dir.join("originals").join(name)
    }

    /// Put the user's own file back (or remove the placed one) when this profile does not place `name`.
    fn unplace(&mut self, name: &str) -> Result<(), String> {
        let owned = self.state.slots.remove(name).is_some();
        let Some(kind) = self.state.originals.remove(name) else {
            return Ok(());
        };
        if !owned {
            return Ok(());
        }
        let live = at(self.slot_dir(name), name);
        let saved = self.original_path(name);
        if kind == "file" {
            let bytes = self
                .txn
                .read(&saved)?
                .ok_or_else(|| format!("the saved original {} is missing", saved.display()))?;
            self.txn.write(&live, &bytes, "your own file put back")?;
            self.txn.remove(&saved, "original no longer needed")?;
        } else {
            self.txn
                .remove(&live, "no file of the profile or your own to place")?;
        }
        Ok(())
    }

    /// Place `bytes` as `name` in the prefix, saving the user's own file first.
    fn place(&mut self, name: &str, bytes: &[u8]) -> Result<(), String> {
        let live = at(self.slot_dir(name), name);
        let current = self.txn.read(&live)?;
        let owned = self.state.slots.contains_key(name);
        if !owned && !self.state.originals.contains_key(name) {
            if let Some(own) = &current {
                let saved = self.original_path(name);
                self.txn
                    .write(&saved, own, "your own file, saved before it is replaced")?;
                self.state
                    .originals
                    .insert(name.to_owned(), "file".to_owned());
            } else {
                self.state
                    .originals
                    .insert(name.to_owned(), "none".to_owned());
            }
        }
        if current.as_deref() != Some(bytes) {
            self.txn.write(&live, bytes, "placed from the profile")?;
        }
        self.state.slots.insert(name.to_owned(), hex(bytes));
        Ok(())
    }

    /// The text of `fallout_default.ini` of the base copy, the seed of a profile that has no INI of its own.
    fn default_ini(&self) -> Option<Vec<u8>> {
        let file = find_ci(&self.instance.config.base, "fallout_default.ini")?;
        nvmod_formats::text::read_limited(&file).ok()
    }

    /// A profile INI that is missing is seeded from the user's own file, else from the base copy's default.
    fn seed(&mut self, pdir: &Path, name: &str) -> Result<(), String> {
        let own = at(pdir, name);
        if self.txn.read(&own)?.is_some() {
            return Ok(());
        }
        let live = at(self.slot_dir(name), name);
        let from_user = if self.state.slots.contains_key(name) {
            match self.state.originals.get(name).map(String::as_str) {
                Some("file") => self.txn.read(&self.original_path(name))?,
                _ => None,
            }
        } else {
            self.txn.read(&live)?
        };
        let seed = from_user.or_else(|| {
            name.eq_ignore_ascii_case("fallout.ini")
                .then(|| self.default_ini())
                .flatten()
        });
        if let Some(bytes) = seed {
            self.txn.write(
                &own,
                &bytes,
                "profile INI started from your own file or the default",
            )?;
        }
        Ok(())
    }

    /// The saves switch on the active INI, with `savepath.ini` holding the old values.
    fn saves_ini(
        &mut self,
        pdir: &Path,
        ini: &mut MetaIni,
        local_saves: bool,
    ) -> Result<(), String> {
        let saved = at(pdir, "savepath.ini");
        let old = self.txn.read(&saved)?;
        if local_saves {
            if old.is_none() {
                let mut body = String::from("[General]\r\n");
                let path = ini.get("General", "sLocalSavePath");
                if let Some(p) = path.filter(|p| p != SAVE_PATH_VALUE) {
                    let _ = write!(body, "sLocalSavePath={p}\r\n");
                }
                if let Some(u) = ini.get("General", "bUseMyGamesDirectory") {
                    let _ = write!(body, "bUseMyGamesDirectory={u}\r\n");
                }
                self.txn.write(
                    &saved,
                    body.as_bytes(),
                    "the old save settings, kept for the way back",
                )?;
            }
            archive::set(ini, "General", "sLocalSavePath", SAVE_PATH_VALUE);
            archive::set(ini, "General", "bUseMyGamesDirectory", "1");
        } else if let Some(bytes) = old {
            let kept = MetaIni::parse(&bytes);
            for key in ["sLocalSavePath", "bUseMyGamesDirectory"] {
                match kept.get("General", key) {
                    Some(v) => archive::set(ini, "General", key, &v),
                    None => ini.remove("General", key),
                }
            }
            self.txn.remove(&saved, "local saves are off")?;
        } else if ini.get("General", "sLocalSavePath").as_deref() == Some(SAVE_PATH_VALUE) {
            ini.remove("General", "sLocalSavePath");
            self.notes.push(
                "the INI still redirected saves to __MO_Saves with no record of the old value; the redirect was removed"
                    .to_owned(),
            );
        }
        Ok(())
    }

    fn saves_link(&mut self, pdir: &Path, local_saves: bool) -> Result<(), Error> {
        let fail = Error::Failed;
        let link = self.target.my_games.join(SAVE_FOLDER);
        if !local_saves {
            return self.txn.unlink(&link, "local saves are off").map_err(fail);
        }
        let saves = pdir.join("saves");
        self.txn.make_dir(&saves).map_err(fail)?;
        let meta = std::fs::symlink_metadata(&link);
        match meta {
            Ok(m) if m.file_type().is_symlink() => {
                if std::fs::read_link(&link).is_ok_and(|t| t == saves) {
                    return Ok(());
                }
            }
            Ok(m) if m.is_dir() => {
                let empty = std::fs::read_dir(&link).is_ok_and(|mut d| d.next().is_none());
                if !empty {
                    return Err(Error::Refused(format!(
                        "{} holds saves of its own; move them away or turn LocalSaves off",
                        link.display()
                    )));
                }
                self.txn
                    .remove_empty_dir(&link, "an empty folder in the way of the save link")
                    .map_err(fail)?;
            }
            Ok(_) => {
                return Err(Error::Refused(format!(
                    "{} is a file; it is in the way of the save folder",
                    link.display()
                )));
            }
            Err(_) => {}
        }
        self.txn
            .link(&link, &saves, "the profile's saves are the save folder")
            .map_err(fail)
    }

    /// The invalidation archive lives in `overwrite/` (the write target), so the Staging tree carries it.
    fn bsa(&mut self, on: bool) -> Result<bool, String> {
        let dir = self.instance.root.join("overwrite");
        let file = at(&dir, archive::BSA_NAME);
        let there = self.txn.read(&file)?;
        if on && there.is_none() {
            self.txn.write(
                &dir.join(archive::BSA_NAME),
                &archive::EMPTY_BSA,
                "the empty invalidation archive",
            )?;
            Ok(true)
        } else if !on && there.as_deref() == Some(&archive::EMPTY_BSA[..]) {
            self.txn.remove(&file, "archive invalidation is off")?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn materialise(&mut self, profile: &str) -> Result<Option<bool>, Error> {
        let fail = Error::Failed;
        let pdir = self.instance.profile_dir(profile);
        let settings = Settings::read(&pdir);
        self.reclaim().map_err(fail)?;
        let local = settings.local_settings;

        // The active fallout.ini: the profile's own, or the user's in the prefix.
        let active = if local {
            for name in INIS {
                self.seed(&pdir, name).map_err(Error::Failed)?;
            }
            at(&pdir, "fallout.ini")
        } else {
            for name in INIS {
                self.unplace(name).map_err(Error::Failed)?;
            }
            at(&self.target.my_games, "fallout.ini")
        };
        self.txn.step("profile-ini").map_err(Error::Failed)?;
        let bytes = self.txn.read(&active).map_err(Error::Failed)?;
        if bytes.is_none() && !local && settings.local_saves {
            return Err(Error::Refused(format!(
                "LocalSaves is on but there is no fallout.ini at {} to redirect the saves in; run the game once or turn LocalSettings on",
                active.display()
            )));
        }
        if let Some(bytes) = bytes.or_else(|| local.then(Vec::new)) {
            if !local {
                let kept = self.dir.join("edited-originals").join("fallout.ini");
                if self.txn.read(&kept).map_err(Error::Failed)?.is_none() {
                    self.txn
                        .write(
                            &kept,
                            &bytes,
                            "your fallout.ini as it was before the first edit",
                        )
                        .map_err(Error::Failed)?;
                }
            }
            let mut ini = MetaIni::parse(&bytes);
            self.saves_ini(&pdir, &mut ini, settings.local_saves)
                .map_err(Error::Failed)?;
            if let Some(note) = archive::edit(&mut ini, settings.archive_invalidation) {
                self.notes.push(note);
            }
            let edited = ini.to_bytes();
            if edited != bytes || (local && !active.exists()) {
                self.txn
                    .write(&active, &edited, "saves and archive invalidation")
                    .map_err(Error::Failed)?;
            }
        } else {
            self.notes.push(
                "no fallout.ini in the prefix, so archive invalidation was not applied (run the game once)".to_owned(),
            );
        }
        self.txn.step("initweaks").map_err(Error::Failed)?;
        let tweaks = tweaks::build(self.instance, profile);
        if self
            .txn
            .read(&pdir.join("initweaks.ini"))
            .map_err(Error::Failed)?
            .as_deref()
            != Some(&tweaks[..])
        {
            self.txn
                .write(&pdir.join("initweaks.ini"), &tweaks, "merged INI tweaks")
                .map_err(Error::Failed)?;
        }

        self.txn.step("place").map_err(Error::Failed)?;
        if local {
            for name in INIS {
                match self.txn.read(&at(&pdir, name)).map_err(Error::Failed)? {
                    Some(bytes) => self.place(name, &bytes).map_err(Error::Failed)?,
                    None => self.unplace(name).map_err(Error::Failed)?,
                }
            }
        }
        for name in LISTS {
            match self.txn.read(&at(&pdir, name)).map_err(Error::Failed)? {
                Some(bytes) => self.place(name, &bytes).map_err(Error::Failed)?,
                None => self.unplace(name).map_err(Error::Failed)?,
            }
        }
        self.txn.step("saves").map_err(Error::Failed)?;
        self.saves_link(&pdir, settings.local_saves)?;
        let bsa = self
            .bsa(settings.archive_invalidation)
            .map_err(Error::Failed)?;
        self.state.profile = Some(profile.to_owned());
        self.state.schema = 1;
        self.state.prefix = self.target.root.display().to_string();
        self.save_state().map_err(Error::Failed)?;
        Ok(Some(bsa))
    }
}

fn load_state(txn: &txn::Txn, dir: &Path) -> State {
    txn.read(&dir.join("state.json"))
        .ok()
        .flatten()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

fn execute(
    instance: &NvmodInstance,
    target: &Target,
    profile: Option<&str>,
    dry_run: bool,
) -> Result<(Report, Vec<PathBuf>), Error> {
    let roots = vec![instance.root.clone(), target.root.clone()];
    let txn = txn::Txn::begin(&instance.root, roots, dry_run).map_err(Error::Failed)?;
    let dir = state_dir(instance, target);
    let state = load_state(&txn, &dir);
    let previous = state.profile.clone();
    let mut run = Run {
        instance,
        target,
        txn,
        state,
        dir,
        notes: Vec::new(),
    };
    let result = match profile {
        Some(profile) => run.materialise(profile),
        None if previous.is_none() => Ok(None),
        None => run
            .reclaim()
            .and_then(|()| run.save_state())
            .map(|()| None)
            .map_err(Error::Failed),
    };
    let settings = profile.map_or_else(Settings::default, |p| {
        Settings::read(&instance.profile_dir(p))
    });
    match result {
        Ok(bsa_changed) => {
            let Run { txn, notes, .. } = run;
            let actions = txn.actions.clone();
            let touched = txn.touched.clone();
            txn.commit();
            let status = if dry_run {
                "dry-run"
            } else if actions.is_empty() {
                "unchanged"
            } else if profile.is_none() {
                "collected"
            } else {
                "applied"
            };
            let mut report = Report {
                status,
                profile: profile.map(str::to_owned).or(previous.clone()),
                previous,
                prefix: target.root.display().to_string(),
                settings,
                actions,
                notes,
                staged_files: None,
            };
            if bsa_changed == Some(true)
                && !dry_run
                && let Some(profile) = profile
                && instance.staging_dir(profile).exists()
            {
                let staged = nvmod_stage::stage(instance, profile, nvmod_stage::Options::default())
                    .map_err(|e| match e {
                        nvmod_stage::StageError::Refused(m)
                        | nvmod_stage::StageError::Failed(m) => Error::Failed(format!(
                            "the profile was placed but staging again failed: {m}"
                        )),
                    })?;
                report.staged_files = Some(staged.manifest.files);
            }
            Ok((report, touched))
        }
        Err(e) => {
            let undo = run.txn.rollback();
            match (e, undo) {
                (e, Ok(())) => Err(e),
                (_, Err(why)) => Err(Error::Failed(format!(
                    "{why}; the journal is kept and the next run puts it back"
                ))),
            }
        }
    }
}

fn refuse_read_only(paths: &[PathBuf]) -> Result<(), Error> {
    for path in paths {
        if std::fs::metadata(path).is_ok_and(|m| m.permissions().readonly()) {
            return Err(Error::Refused(format!(
                "{} is read-only; nothing was changed (clear the read-only flag and apply again)",
                path.display()
            )));
        }
    }
    Ok(())
}

fn guard(instance: &NvmodInstance, dry_run: bool) -> Result<(), Error> {
    if let Some(name) = nvmod_backup::game_running(&instance.root) {
        return Err(Error::Refused(format!(
            "the game is running (`{name}`); close it first"
        )));
    }
    if !dry_run {
        txn::recover(&instance.root).map_err(Error::Failed)?;
    }
    Ok(())
}

/// Place `profile` in the prefix of `target` (or, with `dry_run`, work out what would change and write nothing).
///
/// The order: collect what the game changed since the last apply into the profile that was placed; place or
/// release the five INIs (`LocalSettings`); edit the active `fallout.ini` for local saves and archive
/// invalidation; regenerate `initweaks.ini`; place `plugins.txt` and `loadorder.txt`; link the save folder;
/// write the record. A read-only file in the way is reported (`Refused`) before anything is written.
///
/// # Errors
/// `Refused` for a missing profile, a running game, a read-only file or a save folder that holds saves of its
/// own (nothing was changed); `Failed` when a step fails or the run is interrupted (everything is put back).
pub fn apply(
    instance: &NvmodInstance,
    profile: &str,
    target: &Target,
    dry_run: bool,
) -> Result<Report, Error> {
    if !instance.profile_dir(profile).is_dir() {
        return Err(Error::Refused(format!(
            "the Instance has no profile `{profile}`"
        )));
    }
    guard(instance, dry_run)?;
    // A dry pass first: it finds every refusal and every file to be written before the first write.
    let (plan, touched) = execute(instance, target, Some(profile), true)?;
    let mut check = touched;
    check.push(at(&instance.profile_dir(profile), "fallout.ini"));
    check.push(at(&target.my_games, "fallout.ini"));
    refuse_read_only(&check)?;
    if dry_run {
        return Ok(plan);
    }
    execute(instance, target, Some(profile), false).map(|(r, _)| r)
}

/// Copy back into the placed profile whatever the game or a tool changed in the prefix files since the last
/// apply. Run it after a launch; it changes only profile files and the record.
///
/// # Errors
/// As [`apply`].
pub fn finish(instance: &NvmodInstance, target: &Target, dry_run: bool) -> Result<Report, Error> {
    guard(instance, dry_run)?;
    execute(instance, target, None, dry_run).map(|(r, _)| r)
}
