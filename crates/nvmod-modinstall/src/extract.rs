// SPDX-License-Identifier: GPL-3.0-only
//! Extraction of an archive into a scratch folder. Zip, tar, compressed tar (gzip, xz, bzip2, zstd) and 7z are read
//! in process; any other format (rar, for one) is refused with a message that says to unpack it and install the
//! folder. Nothing is run as a process.
//!
//! Nothing leaves the destination, and the work is bounded. A path that is absolute or climbs out with `..`, a
//! symbolic link, a hard link, a device or any other special entry, two entries that differ only in letter case,
//! a path nested deeper than [`Limits::max_path_depth`], more entries than [`Limits::max_entries`], a file larger
//! than [`Limits::max_entry_bytes`], more bytes than [`Limits::max_bytes`], an expansion past
//! [`Limits::max_ratio`] times the archive's size, or more work than [`Limits::max_operations`] or
//! [`Limits::max_duration`] refuses the whole archive with a message that names the limit. Everything is checked
//! while reading, before it is written, and every write is checked to land inside the destination.
//!
//! An archive found inside an archive is never opened: it is written as an ordinary file (counted against the
//! byte, ratio and time limits), so nesting depth is zero by construction and a nested bomb stays packed.

use std::cell::Cell;
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// How much an archive may hold, and how much work reading it may take. The defaults are documented in
/// `docs/DECISIONS.md`; each is overridable per command through [`LimitOverrides`].
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Total uncompressed bytes.
    pub max_bytes: u64,
    /// Number of entries.
    pub max_entries: usize,
    /// Uncompressed bytes of one file.
    pub max_entry_bytes: u64,
    /// How many times the archive's own size the extracted bytes may reach, checked once more than
    /// [`Limits::ratio_floor_bytes`] has been extracted.
    pub max_ratio: u64,
    /// Below this many extracted bytes the ratio is not checked (a small, highly repetitive file is harmless).
    pub ratio_floor_bytes: u64,
    /// Folders a path may be nested in (components of the relative path, the file name included).
    pub max_path_depth: usize,
    /// Steps of work: one per entry, one per chunk of file data copied and one per read of tar metadata.
    pub max_operations: u64,
    /// Wall-clock time the extraction may take.
    pub max_duration: Duration,
}

/// The default of [`Limits::max_bytes`]: 32 GiB.
pub const DEFAULT_MAX_BYTES: u64 = 32 * 1024 * 1024 * 1024;
/// The default of [`Limits::max_entries`].
pub const DEFAULT_MAX_ENTRIES: usize = 500_000;
/// The default of [`Limits::max_entry_bytes`]: 8 GiB.
pub const DEFAULT_MAX_ENTRY_BYTES: u64 = 8 * 1024 * 1024 * 1024;
/// The default of [`Limits::max_ratio`].
pub const DEFAULT_MAX_RATIO: u64 = 500;
/// The default of [`Limits::ratio_floor_bytes`]: 64 MiB.
pub const DEFAULT_RATIO_FLOOR_BYTES: u64 = 64 * 1024 * 1024;
/// The default of [`Limits::max_path_depth`].
pub const DEFAULT_MAX_PATH_DEPTH: usize = 64;
/// The default of [`Limits::max_operations`].
pub const DEFAULT_MAX_OPERATIONS: u64 = 4_000_000;
/// The default of [`Limits::max_duration`], in seconds: 30 minutes.
pub const DEFAULT_MAX_SECONDS: u64 = 30 * 60;

impl Default for Limits {
    fn default() -> Limits {
        Limits {
            max_bytes: DEFAULT_MAX_BYTES,
            max_entries: DEFAULT_MAX_ENTRIES,
            max_entry_bytes: DEFAULT_MAX_ENTRY_BYTES,
            max_ratio: DEFAULT_MAX_RATIO,
            ratio_floor_bytes: DEFAULT_RATIO_FLOOR_BYTES,
            max_path_depth: DEFAULT_MAX_PATH_DEPTH,
            max_operations: DEFAULT_MAX_OPERATIONS,
            max_duration: Duration::from_secs(DEFAULT_MAX_SECONDS),
        }
    }
}

/// Limits a caller (a command line flag, a library request) sets; `None` keeps the default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LimitOverrides {
    /// [`Limits::max_bytes`] (`--max-extract-bytes`).
    #[serde(default)]
    pub max_bytes: Option<u64>,
    /// [`Limits::max_entries`] (`--max-extract-entries`).
    #[serde(default)]
    pub max_entries: Option<usize>,
    /// [`Limits::max_entry_bytes`] (`--max-extract-entry-bytes`).
    #[serde(default)]
    pub max_entry_bytes: Option<u64>,
    /// [`Limits::max_ratio`] (`--max-extract-ratio`).
    #[serde(default)]
    pub max_ratio: Option<u64>,
    /// [`Limits::max_path_depth`] (`--max-extract-depth`).
    #[serde(default)]
    pub max_path_depth: Option<usize>,
    /// [`Limits::max_operations`] (`--max-extract-operations`).
    #[serde(default)]
    pub max_operations: Option<u64>,
    /// [`Limits::max_duration`], in seconds (`--max-extract-seconds`).
    #[serde(default)]
    pub max_seconds: Option<u64>,
}

impl Limits {
    /// The defaults with every value `overrides` sets replaced.
    #[must_use]
    pub fn with_overrides(overrides: &LimitOverrides) -> Limits {
        let base = Limits::default();
        Limits {
            max_bytes: overrides.max_bytes.unwrap_or(base.max_bytes),
            max_entries: overrides.max_entries.unwrap_or(base.max_entries),
            max_entry_bytes: overrides.max_entry_bytes.unwrap_or(base.max_entry_bytes),
            max_ratio: overrides.max_ratio.unwrap_or(base.max_ratio),
            max_path_depth: overrides.max_path_depth.unwrap_or(base.max_path_depth),
            max_operations: overrides.max_operations.unwrap_or(base.max_operations),
            max_duration: overrides
                .max_seconds
                .map_or(base.max_duration, Duration::from_secs),
            ..base
        }
    }
}

/// Bytes copied from an entry per step of work.
const CHUNK: usize = 64 * 1024;

/// Decoder dictionary memory allowed for a 7z archive, in KiB (512 MiB).
const SEVENZ_MEMORY_LIMIT_KB: usize = 512 * 1024;
/// Decoder dictionary memory allowed for an xz stream, in KiB (512 MiB).
const XZ_MEMORY_LIMIT_KB: u32 = 512 * 1024;

/// Tar bytes that are not file bodies (headers, padding, long names, pax records) may exceed the budget by this much.
const METADATA_SLACK: u64 = 1024 * 1024;
/// Per entry, the tar bytes beyond its body: a 512-byte header and at most 511 bytes of padding.
const METADATA_PER_ENTRY: u64 = 1536;

/// Windows device names that cannot be a file name, whatever their extension.
const RESERVED_WINDOWS_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// A compression wrapped around a tar stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Codec {
    Gzip,
    Xz,
    Bzip2,
    Zstd,
}

/// A relative path made of normal components only, with `\` read as a separator (Windows archivers write it).
///
/// # Errors
/// The path is empty, absolute, names a drive, holds a `:` (an alternate data stream on Windows), uses a Windows
/// device name such as `CON` or `COM1` as a component stem, climbs with `..` or holds a control character.
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
    if unified.contains(':') {
        return Err(format!("path `{name}` holds a `:`"));
    }
    let mut out = PathBuf::new();
    for part in unified.split('/') {
        match part {
            "" | "." => {}
            ".." => return Err(format!("path `{name}` escapes the Mod folder")),
            other => {
                let stem = other.split('.').next().unwrap_or(other);
                if RESERVED_WINDOWS_NAMES
                    .iter()
                    .any(|reserved| stem.eq_ignore_ascii_case(reserved))
                {
                    return Err(format!("path `{name}` uses a reserved Windows name"));
                }
                out.push(other);
            }
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
/// A reason, when the archive is refused, cannot be read, or is not a format nvmod reads.
pub fn extract(archive: &Path, dest: &Path, limits: Limits) -> Result<(), String> {
    let mut magic = [0_u8; 512];
    let n = fs::File::open(archive)
        .and_then(|mut f| f.read(&mut magic))
        .map_err(|e| format!("{}: {e}", archive.display()))?;
    let magic = &magic[..n];
    let format = detect(magic);
    if format == Format::Unsupported {
        return refuse_unsupported(archive);
    }
    let archive_len = fs::metadata(archive)
        .map_err(|e| format!("{}: {e}", archive.display()))?
        .len();
    let mut budget = Budget::new(limits, archive_len, dest)?;
    match format {
        Format::Zip => extract_zip(archive, dest, &mut budget),
        Format::SevenZ => extract_7z(archive, dest, &mut budget),
        Format::CompressedTar(codec) => extract_compressed_tar(archive, dest, &mut budget, codec),
        Format::Tar => extract_tar(archive, dest, &mut budget),
        Format::Unsupported => refuse_unsupported(archive),
    }
}

/// A format without an in-process reader (rar and the rest) is refused; nothing is written.
fn refuse_unsupported(archive: &Path) -> Result<(), String> {
    Err(format!(
        "{} is not an archive nvmod can read. It reads zip, 7z, tar, tar.gz, tar.xz, tar.bz2 and tar.zst; rar and other formats are not supported. Unpack it yourself and install the folder (`nvmod install <folder>`), or use a zip, 7z or tar release of the mod.",
        archive.display()
    ))
}

/// How an archive is read, decided from its first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Zip,
    SevenZ,
    Tar,
    CompressedTar(Codec),
    /// No in-process reader; refused.
    Unsupported,
}

fn detect(magic: &[u8]) -> Format {
    if magic.starts_with(b"PK\x03\x04") || magic.starts_with(b"PK\x05\x06") {
        Format::Zip
    } else if magic.starts_with(b"7z\xbc\xaf\x27\x1c") {
        Format::SevenZ
    } else if magic.starts_with(b"\x1f\x8b") {
        Format::CompressedTar(Codec::Gzip)
    } else if magic.starts_with(b"\xfd7zXZ\x00") {
        Format::CompressedTar(Codec::Xz)
    } else if magic.starts_with(b"BZh") {
        Format::CompressedTar(Codec::Bzip2)
    } else if magic.starts_with(b"\x28\xb5\x2f\xfd") {
        Format::CompressedTar(Codec::Zstd)
    } else if magic.len() == 512 && &magic[257..262] == b"ustar" {
        Format::Tar
    } else {
        Format::Unsupported
    }
}

/// Counters shared between the walker and the tar stream guard.
struct Meter {
    limits: Limits,
    started: Instant,
    bytes: Cell<u64>,
    entries: Cell<usize>,
    operations: Cell<u64>,
    /// Set while a file body is being copied, so its bytes are not counted as metadata.
    in_body: Cell<bool>,
}

impl Meter {
    /// Count `n` steps of work and refuse when the step or time budget is spent.
    fn step(&self, n: u64) -> Result<(), String> {
        let done = self.operations.get().saturating_add(n);
        self.operations.set(done);
        if done > self.limits.max_operations {
            return Err(format!(
                "the extraction needs more than {} steps of work (limit --max-extract-operations)",
                self.limits.max_operations
            ));
        }
        if self.started.elapsed() >= self.limits.max_duration {
            return Err(format!(
                "the extraction took longer than {} seconds (limit --max-extract-seconds)",
                self.limits.max_duration.as_secs()
            ));
        }
        Ok(())
    }
}

/// Windows drops trailing dots and spaces from a name and compares names without regard to case, so two file paths
/// that differ only that way are the same file there. Each folder and file seen is kept under its folded name;
/// folders that differ only in case are allowed, files (and a file against a folder) are not.
#[derive(Default)]
struct Names {
    seen: HashMap<String, (String, bool)>,
}

impl Names {
    /// Admit the relative path `rel` (a folder when `dir`), or say why it collides with an earlier entry.
    fn admit(&mut self, name: &str, rel: &Path, dir: bool) -> Result<(), String> {
        let parts: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        if parts.iter().any(|p| p.ends_with('.') || p.ends_with(' ')) {
            return Err(format!(
                "path `{name}` has a name that ends with a dot or a space, which Windows drops"
            ));
        }
        let mut prefix = String::new();
        for (i, part) in parts.iter().enumerate() {
            if i > 0 {
                prefix.push('/');
            }
            prefix.push_str(part);
            let is_dir = dir || i + 1 < parts.len();
            let folded = prefix.to_lowercase();
            if let Some((spelling, was_dir)) = self.seen.get(&folded) {
                if *was_dir && is_dir {
                    // Folders that differ only in case are fine: real archives hold `Textures` and `textures`,
                    // and what matters is that no two files collide.
                    continue;
                }
                if *was_dir != is_dir {
                    return Err(format!("path `{name}` is both a file and a folder"));
                }
                if *spelling != prefix {
                    return Err(format!(
                        "path `{name}` differs only in letter case from the file `{spelling}`, which would overwrite it on a case-insensitive file system"
                    ));
                }
                return Err(format!("path `{name}` appears twice in the archive"));
            }
            self.seen.insert(folded, (prefix.clone(), is_dir));
        }
        Ok(())
    }
}

struct Budget {
    meter: Rc<Meter>,
    /// Size of the archive file, the base of the expansion ratio.
    archive_len: u64,
    /// The canonical destination: every write must resolve inside it.
    root: PathBuf,
    /// The folder last verified to resolve inside `root`.
    verified: Option<PathBuf>,
    names: Names,
    buf: Vec<u8>,
}

impl Budget {
    fn new(limits: Limits, archive_len: u64, dest: &Path) -> Result<Budget, String> {
        let root = dest
            .canonicalize()
            .map_err(|e| format!("{}: {e}", dest.display()))?;
        Ok(Budget {
            meter: Rc::new(Meter {
                limits,
                started: Instant::now(),
                bytes: Cell::new(0),
                entries: Cell::new(0),
                operations: Cell::new(0),
                in_body: Cell::new(false),
            }),
            archive_len,
            root,
            verified: None,
            names: Names::default(),
            buf: Vec::new(),
        })
    }

    fn limits(&self) -> &Limits {
        &self.meter.limits
    }

    /// Count one more entry.
    fn entry(&mut self) -> Result<(), String> {
        let entries = self.meter.entries.get() + 1;
        self.meter.entries.set(entries);
        if entries > self.limits().max_entries {
            return Err(too_many(self.limits().max_entries));
        }
        self.meter.step(1)
    }

    /// The relative path of an entry named `name`: safe, not too deep and not colliding with an earlier entry.
    fn path(&mut self, name: &str, dir: bool) -> Result<PathBuf, String> {
        let rel = safe_relative(name)?;
        let depth = rel.components().count();
        if depth > self.limits().max_path_depth {
            return Err(format!(
                "path `{name}` is nested {depth} folders deep, more than {} (limit --max-extract-depth)",
                self.limits().max_path_depth
            ));
        }
        self.names.admit(name, &rel, dir)?;
        Ok(rel)
    }

    fn remaining(&self) -> u64 {
        self.limits()
            .max_bytes
            .saturating_sub(self.meter.bytes.get())
    }

    /// Refuse a file whose declared size already breaks a limit.
    fn check_declared(&self, name: &str, size: u64) -> Result<(), String> {
        if size > self.limits().max_entry_bytes {
            return Err(too_big_entry(name, self.limits().max_entry_bytes));
        }
        if size > self.remaining() {
            return Err(too_big(self.limits().max_bytes));
        }
        Ok(())
    }

    /// Refuse sizes that, taken together, are already past the byte and ratio limits.
    fn check_total(&self, declared: u64) -> Result<(), String> {
        if declared > self.limits().max_bytes {
            return Err(too_big(self.limits().max_bytes));
        }
        self.check_ratio(declared)
    }

    fn check_ratio(&self, total: u64) -> Result<(), String> {
        let limits = self.limits();
        let allowed = self.archive_len.max(1).saturating_mul(limits.max_ratio);
        if total > limits.ratio_floor_bytes && total > allowed {
            return Err(format!(
                "the archive expands to more than {} times its size (limit --max-extract-ratio)",
                limits.max_ratio
            ));
        }
        Ok(())
    }

    /// Check that the folder `dir` resolves inside the destination (no link on the way leads out).
    fn verify_inside(&mut self, dir: &Path) -> Result<(), String> {
        if self.verified.as_deref() == Some(dir) {
            return Ok(());
        }
        let resolved = dir
            .canonicalize()
            .map_err(|e| format!("{}: {e}", dir.display()))?;
        if !resolved.starts_with(&self.root) {
            return Err(format!(
                "`{}` would be written outside the Mod folder",
                dir.display()
            ));
        }
        self.verified = Some(dir.to_path_buf());
        Ok(())
    }

    /// Create the folder `target`, checked to lie inside the destination.
    fn make_dir(&mut self, target: &Path) -> Result<(), String> {
        fs::create_dir_all(target).map_err(|e| format!("{}: {e}", target.display()))?;
        self.verify_inside(target)
    }

    /// Copy the body of the file `name` (declared `declared` bytes) from `from` into a new file at `to`.
    fn write(
        &mut self,
        from: &mut dyn Read,
        to: &Path,
        name: &str,
        declared: u64,
    ) -> Result<(), String> {
        self.check_declared(name, declared)?;
        let io = |e: io::Error| format!("{}: {e}", to.display());
        if let Some(parent) = to.parent() {
            fs::create_dir_all(parent).map_err(io)?;
            self.verify_inside(parent)?;
        }
        // `create_new` neither follows a link at `to` nor replaces an existing file.
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(to)
            .map_err(io)?;
        self.meter.in_body.set(true);
        let copied = self.copy(from, &mut out, name);
        self.meter.in_body.set(false);
        let copied =
            copied.and_then(|()| out.flush().map_err(|e| format!("{}: {e}", to.display())));
        drop(out);
        if copied.is_err() {
            let _ = fs::remove_file(to);
        }
        copied
    }

    /// Copy `from` into `out` a chunk at a time, checking every limit before each chunk is written.
    fn copy(&mut self, from: &mut dyn Read, out: &mut fs::File, name: &str) -> Result<(), String> {
        let mut buf = std::mem::take(&mut self.buf);
        buf.resize(CHUNK, 0);
        let result = self.copy_chunks(from, out, name, &mut buf);
        self.buf = buf;
        result
    }

    fn copy_chunks(
        &self,
        from: &mut dyn Read,
        out: &mut fs::File,
        name: &str,
        buf: &mut [u8],
    ) -> Result<(), String> {
        let limits = self.limits();
        let mut entry_bytes = 0_u64;
        loop {
            let n = match from.read(buf) {
                Ok(0) => return Ok(()),
                Ok(n) => n,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(format!("`{name}`: {e}")),
            };
            let n64 = u64::try_from(n).unwrap_or(u64::MAX);
            entry_bytes = entry_bytes.saturating_add(n64);
            let total = self.meter.bytes.get().saturating_add(n64);
            if entry_bytes > limits.max_entry_bytes {
                return Err(too_big_entry(name, limits.max_entry_bytes));
            }
            if total > limits.max_bytes {
                return Err(too_big(limits.max_bytes));
            }
            self.check_ratio(total)?;
            self.meter.step(1)?;
            out.write_all(&buf[..n])
                .map_err(|e| format!("`{name}`: {e}"))?;
            self.meter.bytes.set(total);
        }
    }
}

fn too_big(max: u64) -> String {
    format!("the archive holds more than {max} bytes once extracted (limit --max-extract-bytes)")
}

fn too_big_entry(name: &str, max: u64) -> String {
    format!("`{name}` is larger than {max} bytes (limit --max-extract-entry-bytes)")
}

fn too_many(max: usize) -> String {
    format!("the archive has more than {max} entries (limit --max-extract-entries)")
}

fn extract_zip(archive: &Path, dest: &Path, budget: &mut Budget) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let mut zip = zip::ZipArchive::new(io::BufReader::new(file))
        .map_err(|e| format!("{}: not a readable zip: {e}", archive.display()))?;
    if zip.len() > budget.limits().max_entries {
        return Err(too_many(budget.limits().max_entries));
    }
    let mut declared = 0_u64;
    for i in 0..zip.len() {
        let entry = zip.by_index_raw(i).map_err(|e| e.to_string())?;
        if entry.size() > budget.limits().max_entry_bytes {
            return Err(too_big_entry(
                &entry.name_raw().escape_ascii().to_string(),
                budget.limits().max_entry_bytes,
            ));
        }
        declared = declared.saturating_add(entry.size());
    }
    budget.check_total(declared)?;
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
        let rel = budget.path(&name, entry.is_dir())?;
        let target = dest.join(rel);
        if entry.is_dir() {
            budget.make_dir(&target)?;
        } else {
            let size = entry.size();
            budget.write(&mut entry, &target, &name, size)?;
        }
    }
    Ok(())
}

fn extract_tar(archive: &Path, dest: &Path, budget: &mut Budget) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    extract_tar_reader(io::BufReader::new(file), archive, dest, budget)
}

/// Extract a compressed tar: the file is decoded on the fly and fed to the same walker as a plain tar.
fn extract_compressed_tar(
    archive: &Path,
    dest: &Path,
    budget: &mut Budget,
    codec: Codec,
) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let input = io::BufReader::new(file);
    let unreadable =
        |e: &dyn std::fmt::Display| format!("{}: not a readable archive: {e}", archive.display());
    let reader: Box<dyn Read> = match codec {
        Codec::Gzip => Box::new(flate2::read::MultiGzDecoder::new(input)),
        Codec::Xz => Box::new(lzma_rust2::XzReader::new_mem_limit(
            input,
            true,
            XZ_MEMORY_LIMIT_KB,
        )),
        Codec::Bzip2 => Box::new(bzip2::read::MultiBzDecoder::new(input)),
        Codec::Zstd => {
            Box::new(ruzstd::decoding::StreamingDecoder::new(input).map_err(|e| unreadable(&e))?)
        }
    };
    extract_tar_reader(reader, archive, dest, budget)
}

/// The stream a tar walk reads through. It counts every byte the tar crate pulls from the decoder: once the bytes
/// that are not file bodies (headers, padding, GNU long names, pax records, skipped bodies) pass a bound that grows
/// only with the entries seen, the archive is refused. The whole stream is also capped at the byte limit plus that
/// bound, and each read that is not a file body is a step of work.
struct GuardedReader<R: Read> {
    inner: R,
    read: u64,
    meter: Rc<Meter>,
}

/// How many non-body bytes an archive with `entries` entries may hold.
fn metadata_allowance(entries: usize) -> u64 {
    let per_entry = u64::try_from(entries).unwrap_or(u64::MAX);
    METADATA_SLACK.saturating_add(METADATA_PER_ENTRY.saturating_mul(per_entry))
}

impl<R: Read> Read for GuardedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        self.read = self
            .read
            .saturating_add(u64::try_from(n).unwrap_or(u64::MAX));
        let allowance = metadata_allowance(self.meter.entries.get());
        if self.read > self.meter.limits.max_bytes.saturating_add(allowance) {
            return Err(io::Error::other(
                "the archive stream is larger than expected",
            ));
        }
        if !self.meter.in_body.get() {
            if self.read.saturating_sub(self.meter.bytes.get()) > allowance {
                return Err(io::Error::other(
                    "the archive metadata is larger than expected",
                ));
            }
            self.meter.step(1).map_err(io::Error::other)?;
        }
        Ok(n)
    }
}

/// Walk the entries of a tar stream into `dest`. `archive` names the source in error messages only.
fn extract_tar_reader<R: Read>(
    reader: R,
    archive: &Path,
    dest: &Path,
    budget: &mut Budget,
) -> Result<(), String> {
    let unreadable = |e: io::Error| format!("{}: not a readable archive: {e}", archive.display());
    // The tar crate reads long names, pax records and skipped bodies through the stream, so the guard sees all of
    // it: whatever is not a file body (counted by `Budget`) is metadata, and is capped.
    let guarded = GuardedReader {
        inner: reader,
        read: 0,
        meter: Rc::clone(&budget.meter),
    };
    let mut tar = tar::Archive::new(guarded);
    for entry in tar.entries().map_err(unreadable)? {
        let mut entry = entry.map_err(unreadable)?;
        budget.entry()?;
        let kind = entry.header().entry_type();
        if kind.is_pax_global_extensions() || kind.is_pax_local_extensions() {
            continue;
        }
        let name = entry
            .path()
            .map_err(unreadable)?
            .to_string_lossy()
            .into_owned();
        let rel = budget.path(&name, kind.is_dir())?;
        let target = dest.join(rel);
        if kind.is_dir() {
            budget.make_dir(&target)?;
        } else if kind.is_file() {
            let size = entry.size();
            budget.write(&mut entry, &target, &name, size)?;
        } else {
            return Err(format!(
                "`{name}` is a link or special file, which is refused"
            ));
        }
    }
    Ok(())
}

fn extract_7z(archive: &Path, dest: &Path, budget: &mut Budget) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let mut reader = sevenz_rust2::ArchiveReader::new_with_memory_limit(
        file,
        sevenz_rust2::Password::empty(),
        SEVENZ_MEMORY_LIMIT_KB,
    )
    .map_err(|e| sevenz_error(archive, &e))?;
    let files = &reader.archive().files;
    if files.len() > budget.limits().max_entries {
        return Err(too_many(budget.limits().max_entries));
    }
    let mut declared = 0_u64;
    for entry in files {
        if entry.size > budget.limits().max_entry_bytes {
            return Err(too_big_entry(&entry.name, budget.limits().max_entry_bytes));
        }
        declared = declared.saturating_add(entry.size);
    }
    budget.check_total(declared)?;
    // The callback stops the walk with a placeholder error; the refusal it found is returned instead.
    let mut refused: Option<String> = None;
    let walked = reader.for_each_entries(|entry, stream| {
        match extract_7z_entry(entry, stream, dest, budget) {
            Ok(()) => Ok(true),
            Err(why) => {
                refused = Some(why);
                Err(sevenz_rust2::Error::Other("extraction refused".into()))
            }
        }
    });
    if let Some(why) = refused {
        return Err(why);
    }
    walked.map_err(|e| sevenz_error(archive, &e))
}

fn extract_7z_entry(
    entry: &sevenz_rust2::ArchiveEntry,
    stream: &mut dyn Read,
    dest: &Path,
    budget: &mut Budget,
) -> Result<(), String> {
    budget.entry()?;
    let name = entry.name.as_str();
    let rel = budget.path(name, entry.is_directory)?;
    if entry.is_anti_item {
        return Err(format!("`{name}` is a deletion marker, which is refused"));
    }
    if is_7z_link_or_special(entry) {
        return Err(format!(
            "`{name}` is a link or special file, which is refused"
        ));
    }
    let target = dest.join(rel);
    if entry.is_directory {
        budget.make_dir(&target)
    } else {
        // An entry without a stream is visited with an empty reader, so this creates it empty.
        budget.write(stream, &target, name, entry.size)
    }
}

/// A reparse point, or a Unix mode (high 16 bits, flagged by 0x8000) that is not a regular file or a folder.
fn is_7z_link_or_special(entry: &sevenz_rust2::ArchiveEntry) -> bool {
    if !entry.has_windows_attributes {
        return false;
    }
    let attributes = entry.windows_attributes;
    if attributes & 0x400 != 0 {
        return true;
    }
    if attributes & 0x8000 != 0 {
        let kind = (attributes >> 16) & 0o170_000;
        return kind != 0o100_000 && kind != 0o040_000 && kind != 0;
    }
    false
}

fn sevenz_error(archive: &Path, e: &sevenz_rust2::Error) -> String {
    match e {
        sevenz_rust2::Error::PasswordRequired | sevenz_rust2::Error::MaybeBadPassword(_) => {
            format!(
                "{}: the archive is encrypted; password-protected archives are not supported",
                archive.display()
            )
        }
        other => format!("{}: not a readable 7z archive: {other}", archive.display()),
    }
}

#[cfg(test)]
mod detect_tests {
    use super::*;
    use nvmod_testkit::{covers, ids};

    #[test]
    fn every_supported_format_is_read_in_process_and_rar_is_not() {
        covers(&[ids::mods::install::ARCHIVE_FORMATS]);
        assert_eq!(detect(b"PK\x03\x04rest"), Format::Zip);
        assert_eq!(detect(b"7z\xbc\xaf\x27\x1c\x00\x04"), Format::SevenZ);
        assert_eq!(detect(b"\x1f\x8b\x08"), Format::CompressedTar(Codec::Gzip));
        assert_eq!(
            detect(b"\xfd7zXZ\x00\x00"),
            Format::CompressedTar(Codec::Xz)
        );
        assert_eq!(detect(b"BZh91AY"), Format::CompressedTar(Codec::Bzip2));
        assert_eq!(
            detect(b"\x28\xb5\x2f\xfd\x04"),
            Format::CompressedTar(Codec::Zstd)
        );
        let mut tar = [0_u8; 512];
        tar[257..262].copy_from_slice(b"ustar");
        assert_eq!(detect(&tar), Format::Tar);
        assert_eq!(detect(b"Rar!\x1a\x07\x01\x00"), Format::Unsupported);
        assert_eq!(detect(b"not an archive"), Format::Unsupported);
        assert_eq!(detect(b""), Format::Unsupported);
    }
}
