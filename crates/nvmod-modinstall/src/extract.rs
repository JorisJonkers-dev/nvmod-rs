// SPDX-License-Identifier: GPL-3.0-only
//! Extraction of an archive into a scratch folder. Zip, tar, compressed tar (gzip, xz, bzip2, zstd) and 7z are read
//! in process; only formats without a reader (rar and the rest) go through an extraction tool found in `PATH`
//! (`7zz`, `7z`, `7za`, `bsdtar`) or named by the caller.
//!
//! Nothing leaves the destination: a path that is absolute or climbs out with `..`, a symbolic link, a device or
//! any other special entry, more entries than [`Limits::max_entries`] or more bytes than [`Limits::max_bytes`]
//! refuses the whole archive. Everything is checked while reading, before it is written.

use nvmod_runner::{Call, Runner};
use std::cell::Cell;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

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
/// How long an extraction tool may run.
const TOOL_TIMEOUT: Duration = Duration::from_hours(1);
/// How much of a tool's chatter is kept (it writes files, not output).
const TOOL_OUTPUT_CAP: usize = 16 * 1024 * 1024;

const TOOLS: [&str; 4] = ["7zz", "7z", "7za", "bsdtar"];

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
    match detect(magic) {
        Format::Zip => extract_zip(archive, dest, limits),
        Format::SevenZ => extract_7z(archive, dest, limits),
        Format::CompressedTar(codec) => extract_compressed_tar(archive, dest, limits, codec),
        Format::Tar => extract_tar(archive, dest, limits),
        Format::External => extract_external(archive, dest, limits),
    }
}

/// The only path that starts a process: a format without an in-process reader (rar and the rest).
fn extract_external(archive: &Path, dest: &Path, limits: Limits) -> Result<(), String> {
    let tool = find_tool().ok_or_else(|| {
        format!(
            "{} is not a zip or tar archive and none of {} is in PATH (or name a tool with --extract-tool)",
            archive.display(),
            TOOLS.join(", ")
        )
    })?;
    run_tool(&tool, archive, dest, limits)
}

/// How an archive is read, decided from its first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Format {
    Zip,
    SevenZ,
    Tar,
    CompressedTar(Codec),
    /// No in-process reader: handed to an extraction tool.
    External,
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
        Format::External
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
    /// Shared copy of `bytes`, read by [`GuardedReader`]; `None` when no tar stream is being walked.
    mirror: Option<Rc<Cell<u64>>>,
    /// Shared copy of `entries`, read by [`GuardedReader`].
    entries_mirror: Option<Rc<Cell<usize>>>,
    /// Set while a file body is being copied, so its bytes are not counted as metadata.
    in_body: Option<Rc<Cell<bool>>>,
}

impl Budget {
    fn new(limits: Limits) -> Budget {
        Budget {
            limits,
            bytes: 0,
            entries: 0,
            mirror: None,
            entries_mirror: None,
            in_body: None,
        }
    }

    fn entry(&mut self) -> Result<(), String> {
        self.entries += 1;
        if let Some(mirror) = &self.entries_mirror {
            mirror.set(self.entries);
        }
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
        if let Some(flag) = &self.in_body {
            flag.set(true);
        }
        let copied = io::copy(&mut from.take(allowed.saturating_add(1)), &mut out);
        if let Some(flag) = &self.in_body {
            flag.set(false);
        }
        let copied = copied.map_err(io)?;
        out.flush().map_err(io)?;
        if copied > allowed {
            return Err(too_big(self.limits.max_bytes));
        }
        self.bytes += copied;
        if let Some(mirror) = &self.mirror {
            mirror.set(self.bytes);
        }
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
    let mut budget = Budget::new(limits);
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
    extract_tar_reader(io::BufReader::new(file), archive, dest, limits)
}

/// Extract a compressed tar: the file is decoded on the fly and fed to the same walker as a plain tar.
fn extract_compressed_tar(
    archive: &Path,
    dest: &Path,
    limits: Limits,
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
    extract_tar_reader(reader, archive, dest, limits)
}

/// The stream a tar walk reads through. It counts every byte the tar crate pulls from the decoder: once the bytes
/// that are not file bodies (headers, padding, GNU long names, pax records, skipped bodies) pass a bound that grows
/// only with the entries seen, the archive is refused. The whole stream is also capped at the byte limit plus that bound.
struct GuardedReader<R: Read> {
    inner: R,
    read: u64,
    max_bytes: u64,
    written: Rc<Cell<u64>>,
    entries: Rc<Cell<usize>>,
    in_body: Rc<Cell<bool>>,
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
        let allowance = metadata_allowance(self.entries.get());
        if self.read > self.max_bytes.saturating_add(allowance) {
            return Err(io::Error::other(
                "the archive stream is larger than expected",
            ));
        }
        if !self.in_body.get() && self.read.saturating_sub(self.written.get()) > allowance {
            return Err(io::Error::other(
                "the archive metadata is larger than expected",
            ));
        }
        Ok(n)
    }
}

/// Walk the entries of a tar stream into `dest`. `archive` names the source in error messages only.
fn extract_tar_reader<R: Read>(
    reader: R,
    archive: &Path,
    dest: &Path,
    limits: Limits,
) -> Result<(), String> {
    let unreadable = |e: io::Error| format!("{}: not a readable archive: {e}", archive.display());
    // The tar crate reads long names, pax records and skipped bodies through the stream, so the guard sees all of
    // it: whatever is not a file body (counted by `Budget`) is metadata, and is capped.
    let written = Rc::new(Cell::new(0_u64));
    let entries = Rc::new(Cell::new(0_usize));
    let in_body = Rc::new(Cell::new(false));
    let guarded = GuardedReader {
        inner: reader,
        read: 0,
        max_bytes: limits.max_bytes,
        written: Rc::clone(&written),
        entries: Rc::clone(&entries),
        in_body: Rc::clone(&in_body),
    };
    let mut tar = tar::Archive::new(guarded);
    let mut budget = Budget {
        mirror: Some(written),
        entries_mirror: Some(entries),
        in_body: Some(in_body),
        ..Budget::new(limits)
    };
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

fn extract_7z(archive: &Path, dest: &Path, limits: Limits) -> Result<(), String> {
    let file = fs::File::open(archive).map_err(|e| format!("{}: {e}", archive.display()))?;
    let mut reader = sevenz_rust2::ArchiveReader::new_with_memory_limit(
        file,
        sevenz_rust2::Password::empty(),
        SEVENZ_MEMORY_LIMIT_KB,
    )
    .map_err(|e| sevenz_error(archive, &e))?;
    let files = &reader.archive().files;
    if files.len() > limits.max_entries {
        return Err(format!(
            "the archive has more than {} entries",
            limits.max_entries
        ));
    }
    let declared = files
        .iter()
        .fold(0_u64, |sum, entry| sum.saturating_add(entry.size));
    if declared > limits.max_bytes {
        return Err(too_big(limits.max_bytes));
    }
    let mut budget = Budget::new(limits);
    // The callback stops the walk with a placeholder error; the refusal it found is returned instead.
    let mut refused: Option<String> = None;
    let walked = reader.for_each_entries(|entry, stream| {
        match extract_7z_entry(entry, stream, dest, &mut budget) {
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
    let rel = safe_relative(name)?;
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
        fs::create_dir_all(&target).map_err(|e| format!("{}: {e}", target.display()))
    } else {
        // An entry without a stream is visited with an empty reader, so this creates it empty.
        budget.write(stream, &target)
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

fn run_tool(tool: &Path, archive: &Path, dest: &Path, limits: Limits) -> Result<(), String> {
    // Archive member names are not ASCII: the tool must read and write them as UTF-8.
    let runner = Runner {
        timeout: TOOL_TIMEOUT,
        max_output: TOOL_OUTPUT_CAP,
        max_stderr: TOOL_OUTPUT_CAP,
        ..Runner::default()
    }
    .with_env("LC_ALL", "C.UTF-8");
    run_tool_with(&runner, tool, archive, dest, limits)
}

fn run_tool_with(
    runner: &Runner,
    tool: &Path,
    archive: &Path,
    dest: &Path,
    limits: Limits,
) -> Result<(), String> {
    let name = tool
        .file_name()
        .map_or_default(|n| n.to_string_lossy().to_ascii_lowercase());
    let args: Vec<String> = if name.contains("tar") {
        vec![
            "-xf".to_owned(),
            archive.display().to_string(),
            "-C".to_owned(),
            dest.display().to_string(),
        ]
    } else {
        vec![
            "x".to_owned(),
            "-y".to_owned(),
            format!("-o{}", dest.display()),
            archive.display().to_string(),
        ]
    };
    let output = runner
        .run_any(&Call::new("extractor", tool, &args))
        .map_err(|e| format!("cannot run {}: {e}", tool.display()))?;
    if output.exit_code != 0 {
        return Err(format!(
            "{} failed: {}",
            tool.display(),
            output.stderr.trim()
        ));
    }
    let mut budget = Budget::new(limits);
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

#[cfg(test)]
mod detect_tests {
    use super::*;

    #[test]
    fn every_supported_format_is_read_in_process_and_rar_is_not() {
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
        assert_eq!(detect(b"Rar!\x1a\x07\x01\x00"), Format::External);
        assert_eq!(detect(b"not an archive"), Format::External);
        assert_eq!(detect(b""), Format::External);
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use nvmod_testkit::{TempDir, write_script};

    /// Run a script standing in for `7zz` that is given `body`, through the same path the real tool takes.
    fn run_script(body: &str, runner: &Runner) -> Result<(), String> {
        let dir = TempDir::new("extract-runner");
        let tool = write_script(dir.path(), "7zz", body);
        let dest = dir.path().join("out");
        fs::create_dir(&dest).unwrap();
        run_tool_with(
            runner,
            &tool,
            &dir.path().join("a.7z"),
            &dest,
            Limits::default(),
        )
    }

    #[test]
    fn the_extractor_is_stopped_when_it_runs_too_long() {
        let runner = Runner {
            timeout: Duration::from_millis(300),
            ..Runner::default()
        };
        let why = run_script("exec sleep 30", &runner).unwrap_err();
        assert!(why.contains("longer than"), "{why}");
    }

    #[test]
    fn the_extractor_is_stopped_when_it_writes_past_the_cap() {
        let runner = Runner {
            max_output: 1000,
            ..Runner::default()
        };
        let why = run_script("exec yes", &runner).unwrap_err();
        assert!(why.contains("cap of 1000"), "{why}");
    }

    #[test]
    fn the_extractor_sees_only_the_scrubbed_environment() {
        let dir = TempDir::new("extract-env");
        let seen = dir.path().join("seen");
        let tool = write_script(dir.path(), "7zz", &format!("env > {}", seen.display()));
        let dest = dir.path().join("out");
        fs::create_dir(&dest).unwrap();
        run_tool(&tool, &dir.path().join("a.7z"), &dest, Limits::default()).unwrap();
        let text = fs::read_to_string(seen).unwrap();
        let names: Vec<&str> = text.lines().filter_map(|l| l.split('=').next()).collect();
        assert!(
            names.contains(&"PATH") && names.contains(&"LC_ALL"),
            "{text}"
        );
        assert!(
            !names.contains(&"HOME") && !names.contains(&"USER"),
            "{text}"
        );
    }

    #[test]
    fn a_failing_extractor_reports_its_complaint() {
        let why = run_script("echo corrupt >&2; exit 2", &Runner::default()).unwrap_err();
        assert!(why.contains("failed: corrupt"), "{why}");
    }
}
