// SPDX-License-Identifier: GPL-3.0-only
//! The changes the library makes: the state keys of a `.meta`, and moving an archive to the trash.

use crate::{Error, META_SUFFIX};
use nvmod_formats::metaini::MetaIni;
use std::path::{Path, PathBuf};

/// A change to the state keys of an archive's `.meta`. Each edits only its own lines (a key that is missing is
/// added at the end of `[General]`), so the rest of the file stays as MO2 wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaChange {
    /// `installed=true`, and `uninstalled=false` when it was true.
    Installed,
    /// `installed=false` when it was true.
    NotInstalled,
    /// `removed=true`: MO2's hide.
    Hide,
    /// `removed=false` when it was true.
    Unhide,
}

/// Whether `name` is a plain file name an archive can have: no directory part of either kind, no control
/// character, not `.` or `..`, and not a meta file.
///
/// # Errors
/// [`Error::Refused`] saying why not.
pub fn check_file_name(name: &str) -> Result<&str, Error> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\', ':'])
        || name.chars().any(char::is_control)
        || name.ends_with(META_SUFFIX)
    {
        return Err(Error::Refused(format!(
            "`{name}` is not the file name of a download (a name only, not a path, and not a .meta file)"
        )));
    }
    Ok(name)
}

fn is_true(meta: &MetaIni, key: &str) -> bool {
    meta.get_string("General", key)
        .is_some_and(|v| matches!(v.trim().to_ascii_lowercase().as_str(), "true" | "1"))
}

fn require_archive(dir: &Path, file: &str) -> Result<(), Error> {
    check_file_name(file)?;
    if dir.join(file).is_file() {
        Ok(())
    } else {
        Err(Error::Refused(format!(
            "{file} is not in {}",
            dir.display()
        )))
    }
}

/// Apply `change` to `<dir>/<file>.meta`, creating it when the archive has none and the change needs it. Returns
/// whether anything changed; nothing is written when the state is already as asked.
///
/// # Errors
/// [`Error::Refused`] for a bad name or an archive that is not there; [`Error::Failed`] when the file cannot be
/// read or written.
pub fn change_meta(dir: &Path, file: &str, change: MetaChange) -> Result<bool, Error> {
    require_archive(dir, file)?;
    let path = dir.join(format!("{file}{META_SUFFIX}"));
    let io = |e: std::io::Error| Error::Failed(format!("{}: {e}", path.display()));
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(io(e)),
    };
    let mut meta = MetaIni::parse(&bytes);
    let changed = match change {
        MetaChange::Installed => {
            let mut changed = false;
            if !is_true(&meta, "installed") {
                meta.set("General", "installed", "true");
                changed = true;
            }
            if is_true(&meta, "uninstalled") {
                meta.set("General", "uninstalled", "false");
                changed = true;
            }
            changed
        }
        MetaChange::NotInstalled => {
            let was = is_true(&meta, "installed");
            if was {
                meta.set("General", "installed", "false");
            }
            was
        }
        MetaChange::Hide => {
            let was = is_true(&meta, "removed");
            if !was {
                meta.set("General", "removed", "true");
            }
            !was
        }
        MetaChange::Unhide => {
            let was = is_true(&meta, "removed");
            if was {
                meta.set("General", "removed", "false");
            }
            was
        }
    };
    if changed {
        let tmp = dir.join(format!("{file}{META_SUFFIX}.nvmod-tmp"));
        std::fs::write(&tmp, meta.to_bytes()).map_err(io)?;
        std::fs::rename(&tmp, &path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            io(e)
        })?;
    }
    Ok(changed)
}

fn move_file(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    // Another volume: copy, then remove, so nothing is lost when the copy fails.
    std::fs::copy(from, to)?;
    std::fs::remove_file(from).inspect_err(|_| {
        let _ = std::fs::remove_file(to);
    })
}

/// Move `<dir>/<file>` and its `.meta` into a new folder under `trash_root`; nothing is deleted. Returns the new
/// folder (`<trash_root>/<millis>-<pid>`).
///
/// # Errors
/// [`Error::Refused`] for a bad name or an archive that is not there; [`Error::Failed`] when a move failed (what
/// was moved is put back).
pub fn trash(dir: &Path, file: &str, trash_root: &Path) -> Result<PathBuf, Error> {
    require_archive(dir, file)?;
    let from = dir.join(file);
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let to_dir = trash_root.join(format!("{millis:016}-{}", std::process::id()));
    std::fs::create_dir_all(&to_dir)
        .map_err(|e| Error::Failed(format!("{}: {e}", to_dir.display())))?;
    let fail = |at: &Path, e: std::io::Error| Error::Failed(format!("{}: {e}", at.display()));
    let to = to_dir.join(file);
    move_file(&from, &to).map_err(|e| {
        let _ = std::fs::remove_dir(&to_dir);
        fail(&from, e)
    })?;
    let meta_name = format!("{file}{META_SUFFIX}");
    let meta_from = dir.join(&meta_name);
    if meta_from.is_file()
        && let Err(e) = move_file(&meta_from, &to_dir.join(&meta_name))
    {
        let _ = move_file(&to, &from);
        let _ = std::fs::remove_dir(&to_dir);
        return Err(fail(&meta_from, e));
    }
    Ok(to_dir)
}
