// SPDX-License-Identifier: GPL-3.0-only
//! The one door through which `nvmod profile apply` writes. Every write, removal and link goes through a
//! [`Txn`], which (1) refuses a path outside the Instance and the named prefix, (2) keeps a before image in
//! `.nvmod/apply-journal/<id>/` so a failed or interrupted run is put back (and a crashed one on the next run),
//! and (3) in a dry run keeps the changes in memory, so later steps see them and nothing is written.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// One change a run made or would make.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Action {
    /// `write`, `remove`, `link`, `unlink`, `mkdir` or `rmdir`.
    pub action: &'static str,
    /// The path, as given.
    pub path: String,
    /// Why, in a few words.
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "kebab-case")]
enum Before {
    Absent,
    /// The name of the copy of the old bytes in the journal folder.
    File(String),
    /// The old target of a symbolic link.
    Link(String),
    /// A folder that was there (empty) and was removed.
    EmptyDir,
    /// A folder that did not exist and was made.
    MadeDir,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Entry {
    path: String,
    before: Before,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Journal {
    schema: u32,
    entries: Vec<Entry>,
}

const JOURNALS: &str = "apply-journal";

/// A set of changes that happen together or not at all.
#[derive(Debug)]
pub struct Txn {
    dry: bool,
    roots: Vec<PathBuf>,
    dir: Option<PathBuf>,
    journal: Journal,
    overlay: BTreeMap<PathBuf, Option<Vec<u8>>>,
    /// What was done (or would be).
    pub actions: Vec<Action>,
    /// Paths written or removed (for the read-only check of a dry run).
    pub touched: Vec<PathBuf>,
}

fn io(path: &Path, e: &std::io::Error) -> String {
    format!("{}: {e}", path.display())
}

fn lexical(path: &Path) -> bool {
    !path.components().any(|c| matches!(c, Component::ParentDir))
}

#[cfg(unix)]
fn symlink_dir(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn symlink_dir(target: &Path, link: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(target, link)
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| io(parent, &e))?;
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".nvmod-tmp");
    let tmp = PathBuf::from(tmp);
    fs::write(&tmp, bytes).map_err(|e| io(&tmp, &e))?;
    // A link in the way is replaced, never written through.
    if fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink()) {
        fs::remove_file(path).map_err(|e| io(path, &e))?;
    }
    fs::rename(&tmp, path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        io(path, &e)
    })
}

fn id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    format!("{now:016}-{}", std::process::id())
}

fn undo_entries(entries: &[Entry], dir: &Path) -> Result<(), String> {
    for entry in entries.iter().rev() {
        let path = PathBuf::from(&entry.path);
        let is_link = fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink());
        match &entry.before {
            Before::Absent => {
                if is_link || path.is_file() {
                    fs::remove_file(&path).map_err(|e| io(&path, &e))?;
                }
            }
            Before::File(name) => {
                let bytes = fs::read(dir.join(name)).map_err(|e| io(&dir.join(name), &e))?;
                atomic_write(&path, &bytes)?;
            }
            Before::Link(target) => {
                if is_link || path.is_file() {
                    fs::remove_file(&path).map_err(|e| io(&path, &e))?;
                }
                symlink_dir(Path::new(target), &path).map_err(|e| io(&path, &e))?;
            }
            Before::EmptyDir => {
                fs::create_dir_all(&path).map_err(|e| io(&path, &e))?;
            }
            Before::MadeDir => {
                let _ = fs::remove_dir(&path);
            }
        }
    }
    Ok(())
}

/// Put back what a run that never finished had changed (a crash, a power cut): every journal under
/// `.nvmod/apply-journal/` is undone and removed.
///
/// # Errors
/// A reason, when a before image cannot be restored (the journal is kept).
pub fn recover(instance_root: &Path) -> Result<usize, String> {
    let root = instance_root.join(".nvmod").join(JOURNALS);
    let mut done = 0;
    let mut dirs: Vec<PathBuf> = fs::read_dir(&root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .collect();
    dirs.sort();
    for dir in dirs.into_iter().rev() {
        if let Ok(text) = fs::read_to_string(dir.join("journal.json"))
            && let Ok(journal) = serde_json::from_str::<Journal>(&text)
        {
            undo_entries(&journal.entries, &dir)?;
            done += 1;
        }
        let _ = fs::remove_dir_all(&dir);
    }
    let _ = fs::remove_dir(&root);
    Ok(done)
}

impl Txn {
    /// Start a set of changes that may write under `roots` only. A dry run writes nothing at all.
    ///
    /// # Errors
    /// The journal folder cannot be made.
    pub fn begin(instance_root: &Path, roots: Vec<PathBuf>, dry: bool) -> Result<Txn, String> {
        let dir = if dry {
            None
        } else {
            let dir = instance_root.join(".nvmod").join(JOURNALS).join(id());
            fs::create_dir_all(&dir).map_err(|e| io(&dir, &e))?;
            Some(dir)
        };
        let txn = Txn {
            dry,
            roots,
            dir,
            journal: Journal {
                schema: 1,
                entries: Vec::new(),
            },
            overlay: BTreeMap::new(),
            actions: Vec::new(),
            touched: Vec::new(),
        };
        txn.save()?;
        Ok(txn)
    }

    fn save(&self) -> Result<(), String> {
        let Some(dir) = &self.dir else {
            return Ok(());
        };
        let text = serde_json::to_string(&self.journal).map_err(|e| e.to_string())?;
        atomic_write(&dir.join("journal.json"), text.as_bytes())
    }

    fn check(&self, path: &Path) -> Result<(), String> {
        if lexical(path) && self.roots.iter().any(|r| path.starts_with(r)) {
            Ok(())
        } else {
            Err(format!(
                "refusing to change {}: it is outside the Instance and the named prefix",
                path.display()
            ))
        }
    }

    fn note(&mut self, action: &'static str, path: &Path, note: &str) {
        self.actions.push(Action {
            action,
            path: path.display().to_string(),
            note: note.to_owned(),
        });
    }

    fn remember(&mut self, path: &Path, before: Before) -> Result<(), String> {
        if self.dir.is_none() {
            return Ok(());
        }
        self.journal.entries.push(Entry {
            path: path.display().to_string(),
            before,
        });
        self.save()
    }

    /// The before image of `path` (a file, a link or nothing), saved to the journal.
    fn image(&mut self, path: &Path) -> Result<(), String> {
        let Some(dir) = self.dir.clone() else {
            return Ok(());
        };
        let before = match fs::symlink_metadata(path) {
            Err(_) => Before::Absent,
            Ok(m) if m.file_type().is_symlink() => Before::Link(
                fs::read_link(path)
                    .map_err(|e| io(path, &e))?
                    .display()
                    .to_string(),
            ),
            Ok(m) if m.is_file() => {
                let name = format!("{:04}.bak", self.journal.entries.len());
                fs::copy(path, dir.join(&name)).map_err(|e| io(path, &e))?;
                Before::File(name)
            }
            Ok(_) => return Err(format!("{} is not a file", path.display())),
        };
        self.remember(path, before)
    }

    /// The place between two sub-steps (the cancellation and failure-injection hook); a dry run has none.
    ///
    /// # Errors
    /// The reason the run must stop.
    pub fn step(&self, name: &str) -> Result<(), String> {
        if self.dry {
            return Ok(());
        }
        nvmod_core::cancel::step_point(name)
    }

    /// The bytes of the file `path`, as this run has left it; `None` when there is no such file. A link is read
    /// through.
    ///
    /// # Errors
    /// The file exists and cannot be read, or is too large.
    pub fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, String> {
        if let Some(v) = self.overlay.get(path) {
            return Ok(v.clone());
        }
        if !path.is_file() {
            return Ok(None);
        }
        nvmod_formats::text::read_limited(path)
            .map(Some)
            .map_err(|e| io(path, &e))
    }

    /// Make the folder `path` and the folders above it that are missing.
    ///
    /// # Errors
    /// The path is outside the roots, or a folder cannot be made.
    pub fn make_dir(&mut self, path: &Path) -> Result<(), String> {
        self.check(path)?;
        let mut missing = Vec::new();
        let mut at = Some(path);
        while let Some(p) = at {
            if p.exists()
                || self.overlay.contains_key(p)
                || !self.roots.iter().any(|r| p.starts_with(r))
            {
                break;
            }
            missing.push(p.to_path_buf());
            at = p.parent();
        }
        for p in missing.into_iter().rev() {
            self.note("mkdir", &p, "folder made");
            if self.dry {
                self.overlay.insert(p, None);
                continue;
            }
            fs::create_dir(&p).map_err(|e| io(&p, &e))?;
            self.remember(&p, Before::MadeDir)?;
        }
        Ok(())
    }

    /// Write `bytes` to `path` (a temporary file, then a rename).
    ///
    /// # Errors
    /// The path is outside the roots or the write fails.
    pub fn write(&mut self, path: &Path, bytes: &[u8], why: &str) -> Result<(), String> {
        self.check(path)?;
        self.note("write", path, why);
        self.touched.push(path.to_path_buf());
        if self.dry {
            self.overlay
                .insert(path.to_path_buf(), Some(bytes.to_vec()));
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            self.make_dir(parent)?;
        }
        self.image(path)?;
        atomic_write(path, bytes)
    }

    /// Remove the file or link `path` (nothing when it is not there).
    ///
    /// # Errors
    /// The path is outside the roots or the removal fails.
    pub fn remove(&mut self, path: &Path, why: &str) -> Result<(), String> {
        self.check(path)?;
        let there = self.read(path)?.is_some()
            || fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink());
        if !there {
            return Ok(());
        }
        self.note("remove", path, why);
        self.touched.push(path.to_path_buf());
        if self.dry {
            self.overlay.insert(path.to_path_buf(), None);
            return Ok(());
        }
        self.image(path)?;
        fs::remove_file(path).map_err(|e| io(path, &e))
    }

    /// Remove the empty folder `path`.
    ///
    /// # Errors
    /// The path is outside the roots or the removal fails.
    pub fn remove_empty_dir(&mut self, path: &Path, why: &str) -> Result<(), String> {
        self.check(path)?;
        self.note("rmdir", path, why);
        if self.dry {
            return Ok(());
        }
        fs::remove_dir(path).map_err(|e| io(path, &e))?;
        self.remember(path, Before::EmptyDir)
    }

    /// Make `link` a symbolic link to the folder `target`, replacing a link that is there.
    ///
    /// # Errors
    /// The path is outside the roots, a file is in the way, or the link cannot be made.
    pub fn link(&mut self, link: &Path, target: &Path, why: &str) -> Result<(), String> {
        self.check(link)?;
        self.note("link", link, &format!("{why}: -> {}", target.display()));
        if self.dry {
            return Ok(());
        }
        if let Some(parent) = link.parent() {
            self.make_dir(parent)?;
        }
        self.image(link)?;
        if fs::symlink_metadata(link).is_ok() {
            fs::remove_file(link).map_err(|e| io(link, &e))?;
        }
        symlink_dir(target, link).map_err(|e| io(link, &e))
    }

    /// Remove the symbolic link `link` (never a folder or a file).
    ///
    /// # Errors
    /// The path is outside the roots or the removal fails.
    pub fn unlink(&mut self, link: &Path, why: &str) -> Result<(), String> {
        self.check(link)?;
        if !fs::symlink_metadata(link).is_ok_and(|m| m.file_type().is_symlink()) {
            return Ok(());
        }
        self.note("unlink", link, why);
        if self.dry {
            return Ok(());
        }
        self.image(link)?;
        fs::remove_file(link).map_err(|e| io(link, &e))
    }

    /// The run succeeded: drop the journal.
    pub fn commit(self) {
        if let Some(dir) = self.dir {
            let _ = fs::remove_dir_all(&dir);
            if let Some(parent) = dir.parent() {
                let _ = fs::remove_dir(parent);
            }
        }
    }

    /// The run failed: put every change back and drop the journal.
    ///
    /// # Errors
    /// A before image cannot be restored (the journal is kept, so the next run retries).
    pub fn rollback(self) -> Result<(), String> {
        let Some(dir) = self.dir else {
            return Ok(());
        };
        undo_entries(&self.journal.entries, &dir)?;
        let _ = fs::remove_dir_all(&dir);
        if let Some(parent) = dir.parent() {
            let _ = fs::remove_dir(parent);
        }
        Ok(())
    }
}
