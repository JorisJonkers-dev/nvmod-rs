// SPDX-License-Identifier: GPL-3.0-only
//! Extraction of an archive into a scratch folder. Zip and tar are read in process; everything else (7z, rar, ...)
//! goes through an extraction tool found in `PATH` (`7zz`, `7z`, `7za`, `bsdtar`) or named by the caller.
//!
//! Nothing leaves the destination: a path that is absolute or climbs out with `..`, a symbolic link, a device or
//! any other special entry, more entries than [`Limits::max_entries`] or more bytes than [`Limits::max_bytes`]
//! refuses the whole archive. Everything is checked while reading, before it is written.

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// How much an archive may hold.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Total uncompressed bytes.
    pub max_bytes: u64,
    /// Number of entries.
    pub max_entries: usize,
}

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_bytes: 32 * 1024 * 1024 * 1024,
            max_entries: 500_000,
        }
    }
}

/// Tools tried in `PATH`, in order, for archives without a built-in reader.
const TOOLS: [&str; 4] = ["7zz", "7z", "7za", "bsdtar"];

/// A relative path made of normal components only, with `\` read as a separator (Windows archivers write it).
///
/// # Errors
/// The path is empty, absolute, names a drive, climbs with `..` or holds a control character.
pub fn safe_relative(name: &str) -> Result<PathBuf, String> {
    if name.chars().any(char::is_control) {
        return Err(format!(
            "path `{}` holds a control character",
            name.escape_debug()
        ));
    }
    let unified = name.replace('\\', "/");
    if unified.starts_with('/') || unified.as_bytes().get(1) == Some(&b':') {
        return Err(format!("path `{name}` is absolute"));
    }
    let mut out = PathBuf::new();
    for part in unified.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(format!("path `{name}` escapes the Mod folder")),
            other => out.push(other),
        }
    }
    if out.as_os_str().is_empty() {
        return Err("an entry has an empty path".to_owned());
    }
    debug_assert!(out.components().all(|c| matches!(c, Component::Normal(_))));
    Ok(out)
}

/// Extract `archive` into the existing folder `dest`.
///
/// # Errors
/// A reason, when the archive is refused, cannot be read, or the tool fails.
pub fn extract(
    archive: &Path,
    dest: &Path,
    tool: Option<&Path>,
    limits: Limits,
) -> Result<(), String> {
    if let Some(tool) = tool {
        return run_tool(tool, archive, dest, limits);
    }
    let mut magic = [0_u8; 512];
    let n = fs::File::open(archive)
        .and_then(|mut f| f.read(&mut magic))
        .map_err(|e| format!("{}: {e}", archive.display()))?;
    let magic = &magic[..n];
    if magic.starts_with(b"PK\x03\x04") || magic.starts_with(b"PK\x05\x06") {
        extract_zip(archive, dest, limits)
    } else if n == 512 && &magic[257..262] == b"ustar" {
        extract_tar(archive, dest, limits)
    } else {
        let tool = find_tool().ok_or_else(|| {
            format!(
                "{} is not a zip or tar archive and none of {} is in PATH (or name a tool with --extract-tool)",
                archive.display(),
                TOOLS.join(", ")
            )
        })?;
        run_tool(&tool, archive, dest, limits)
    }
}

fn find_tool() -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    TOOLS.iter().find_map(|name| {
        std::env::split_paths(&path)
            .map(|dir| dir.join(name))
            .find(|candidate| candidate.is_file())
    })
}

struct Budget {
    limits: Limits,
    bytes: u64,
    entries: usize,
}

impl Budget {
    fn entry(&mut self) -> Result<(), String> {
        self.entries += 1;
        if self.entries > self.limits.max_entries {
            return Err(format!(
                "the archive has more than {} entries",
                self.limits.max_entries
            ));
        }
        Ok(())
    }

    fn remaining(&self) -> u64 {
        self.limits.max_bytes.saturating_sub(self.bytes)
    }

    /// Copy at most the remaining budget from `from` into a new file at `to`.
    fn write(&mut self, from: &mut dyn Read, to: &Path) -> Result<(), String> {
        let io = |e: io::Error| format!("{}: {e}", to.display());
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(io)?;
        }
        let mut out = fs::File::create(to).map_err(io)?;
        let allowed = self.remaining();
        let copied = io::copy(&mut from.take(allowed.saturating_add(1)), &mut out).map_err(io)?;
        out.flush().map_err(io)?;
        if copied > allowed {
            return Err(too_big(self.limits.max_bytes));
        }
        self.bytes += copied;
        Ok(())
    }
}

fn too_big(max: u64) -> String {
    format!("the archive holds more than {max} bytes once extracted")
}

fn extract_zip(archive: &Path, dest: &Path, limits: Limits) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(io::BufReader::new(file))
        .map_err(|e| format!("{}: not a readable zip: {e}", archive.display()))?;
    if zip.len() > limits.max_entries {
        return Err(format!(
            "the archive has more than {} entries",
            limits.max_entries
        ));
    }
    let mut declared = 0_u64;
    for i in 0..zip.len() {
        let entry = zip.by_index_raw(i).map_err(|e| e.to_string())?;
        declared = declared.saturating_add(entry.size());
    }
    if declared > limits.max_bytes {
        return Err(too_big(limits.max_bytes));
    }
    let mut budget = Budget {
        limits,
        bytes: 0,
        entries: 0,
    };
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("entry {i}: {e}"))?;
        budget.entry()?;
        let name = entry
            .name()
            .map_err(|e| format!("entry {i}: {e}"))?
            .into_owned();
        if let Some(mode) = entry.unix_mode()
            && mode & 0o170_000 != 0o100_000
            && mode & 0o170_000 != 0o040_000
            && mode & 0o170_000 != 0
        {
            return Err(format!(
                "`{name}` is a link or special file, which is refused"
            ));
        }
        let rel = safe_relative(&name)?;
        let target = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        } else {
            budget.write(&mut entry, &target)?;
        }
    }
    Ok(())
}

fn extract_tar(archive: &Path, dest: &Path, limits: Limits) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let mut tar = tar::Archive::new(io::BufReader::new(file));
    let mut budget = Budget {
        limits,
        bytes: 0,
        entries: 0,
    };
    for entry in tar.entries().map_err(|e| e.to_string())? {
        let mut entry = entry.map_err(|e| e.to_string())?;
        budget.entry()?;
        let kind = entry.header().entry_type();
        if kind.is_pax_global_extensions() || kind.is_pax_local_extensions() {
            continue;
        }
        let name = entry
            .path()
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned();
        let rel = safe_relative(&name)?;
        let target = dest.join(rel);
        if kind.is_dir() {
            fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))?;
        } else if kind.is_file() {
            if entry.size() > budget.remaining() {
                return Err(too_big(limits.max_bytes));
            }
            budget.write(&mut entry, &target)?;
        } else {
            return Err(format!(
                "`{name}` is a link or special file, which is refused"
            ));
        }
    }
    Ok(())
}

fn run_tool(tool: &Path, archive: &Path, dest: &Path, limits: Limits) -> Result<(), String> {
    let name = tool
        .file_name()
        .map(|n| n.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let mut command = Command::new(tool);
    if name.contains("tar") {
        command.arg("-xf").arg(archive).arg("-C").arg(dest);
    } else {
        let mut out = std::ffi::OsString::from("-o");
        out.push(dest);
        command.arg("x").arg("-y").arg(out).arg(archive);
    }
    let output = command
        .output()
        .map_err(|e| format!("cannot run {}: {e}", tool.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} failed: {}",
            tool.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let mut budget = Budget {
        limits,
        bytes: 0,
        entries: 0,
    };
    check_tree(dest, &mut budget)
}

/// Refuse a tree an external tool made if it holds a link or special file, or is too large.
fn check_tree(dir: &Path, budget: &mut Budget) -> Result<(), String> {
    for entry in fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
    {
        budget.entry()?;
        let meta = fs::symlink_metadata(entry.path()).map_err(|e| e.to_string())?;
        if meta.is_dir() {
            check_tree(&entry.path(), budget)?;
        } else if meta.is_file() {
            budget.bytes = budget.bytes.saturating_add(meta.len());
            if budget.bytes > budget.limits.max_bytes {
                return Err(too_big(budget.limits.max_bytes));
            }
        } else {
            return Err(format!(
                "`{}` is a link or special file, which is refused",
                entry.path().display()
            ));
        }
    }
    Ok(())
}
