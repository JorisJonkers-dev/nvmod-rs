// SPDX-License-Identifier: GPL-3.0-only
//! A bounded, read-only inspector of PE files: NVSE plugin DLLs and the game executable.
//!
//! The files are untrusted (they come from mod archives), so the parser follows the rules that `loot-rs`
//! `src/exe_version.rs` set for the same job:
//!
//! - nothing is executed and nothing is mapped; bytes are read through [`ReadAt`], which refuses any read outside
//!   the file or larger than [`MAX_READ`];
//! - every count taken from a header is checked against a fixed limit (the constants below) before it sizes a
//!   buffer or a loop, so a hostile header cannot make the parser allocate, loop or recurse without bound;
//! - offsets are added with checked arithmetic, and a relative virtual address is only followed when one section
//!   holds all the bytes asked for;
//! - the headers decide whether the file is usable at all ([`PeError`]); a broken export, import or version table
//!   only adds a line to [`PeReport::issues`], and the rest of the report stays valid.
//!
//! `unsafe_code` is forbidden in this crate. The one input the parser reads from memory it does not own is the
//! file's bytes, copied out bounded.

mod model;
pub mod nvse;
mod source;

pub use model::{Export, NvseInfo, PeError, PeFormat, PeReport, Version4, VersionStruct};
pub use source::{FileSource, MAX_READ, ReadAt, SliceSource};

use std::path::Path;

/// Most sections accepted (the PE format allows 96).
pub const MAX_SECTIONS: u16 = 96;
/// Most data directories a header may declare (real images declare 16).
pub const MAX_DATA_DIRECTORIES: u32 = 1024;
/// Largest optional header accepted, in bytes (real ones are 224 or 240).
pub const MAX_OPTIONAL_HEADER: u16 = 4096;
/// Most named exports read.
pub const MAX_EXPORT_NAMES: u32 = 16_384;
/// Most export address table entries read.
pub const MAX_EXPORT_FUNCTIONS: u32 = 65_536;
/// Most import descriptors read (one per imported DLL).
pub const MAX_IMPORT_DLLS: u32 = 512;
/// Longest name read (a symbol or a DLL name), in bytes.
pub const MAX_NAME: usize = 256;
/// Most entries looked at in one resource directory table.
pub const MAX_TABLE_ENTRIES: u16 = 256;
/// Most resource directory tables read while looking for the version block.
pub const MAX_TABLES: usize = 64;
/// Largest version block, in bytes (`wLength` is a `u16`).
pub const MAX_VERSION_BLOCK: u32 = 0xFFFF;

const CHARACTERISTIC_DLL: u16 = 0x2000;
const CHARACTERISTIC_LARGE_ADDRESS_AWARE: u16 = 0x0020;
const FIXED_FILE_INFO_SIGNATURE: u32 = 0xFEEF_04BD;
const RT_VERSION: u32 = 16;
/// `VS_VERSION_INFO\0` in UTF-16, the key of a version block.
const VERSION_KEY: &[u8; 32] = b"V\0S\0_\0V\0E\0R\0S\0I\0O\0N\0_\0I\0N\0F\0O\0\0\0";

fn u16_at(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(at..at.checked_add(2)?)?.try_into().ok()?,
    ))
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(at..at.checked_add(4)?)?.try_into().ok()?,
    ))
}

/// One section header, reduced to what address translation needs.
struct Section {
    virtual_address: u32,
    /// Bytes of the section that exist in the file and count.
    size: u32,
    raw_offset: u32,
}

struct Image<'a, R: ReadAt> {
    source: &'a mut R,
    sections: Vec<Section>,
}

impl<R: ReadAt> Image<'_, R> {
    /// The file offset of `size` bytes at `rva`, only if one section holds all of them.
    fn offset_of(&self, rva: u32, size: u32) -> Result<u64, &'static str> {
        let end = u64::from(rva) + u64::from(size);
        for s in &self.sections {
            if rva >= s.virtual_address && end <= u64::from(s.virtual_address) + u64::from(s.size) {
                let offset = u64::from(s.raw_offset) + u64::from(rva - s.virtual_address);
                if offset + u64::from(size) > self.source.len() {
                    return Err("data is cut off by the end of the file");
                }
                return Ok(offset);
            }
        }
        Err("address is not inside a section")
    }

    fn read_rva(&mut self, rva: u32, len: usize) -> Result<Vec<u8>, &'static str> {
        let size = u32::try_from(len).map_err(|_| "read is too large")?;
        let offset = self.offset_of(rva, size)?;
        self.source
            .read_at(offset, len)
            .ok_or("read is outside the file")
    }

    /// A NUL-terminated name at `rva`, at most [`MAX_NAME`] bytes, never reading past its section or the file.
    fn read_name(&mut self, rva: u32) -> Result<String, &'static str> {
        let section = self
            .sections
            .iter()
            .find(|s| rva >= s.virtual_address && rva - s.virtual_address < s.size)
            .ok_or("name is not inside a section")?;
        let in_section = section.size - (rva - section.virtual_address);
        let offset = u64::from(section.raw_offset) + u64::from(rva - section.virtual_address);
        let in_file = self.source.len().saturating_sub(offset);
        let room = usize::try_from(in_section.min(u32::try_from(in_file).unwrap_or(u32::MAX)))
            .unwrap_or(MAX_NAME)
            .min(MAX_NAME);
        let bytes = self
            .source
            .read_at(offset, room)
            .ok_or("name is outside the file")?;
        let end = bytes
            .iter()
            .position(|&b| b == 0)
            .ok_or("name is not terminated")?;
        Ok(String::from_utf8_lossy(&bytes[..end]).into_owned())
    }
}

/// Inspect the PE file behind `source`.
///
/// # Errors
/// The headers are not those of a PE file the inspector accepts. Damage further in (exports, imports, version
/// resource) is reported in [`PeReport::issues`] instead.
pub fn inspect<R: ReadAt>(source: &mut R) -> Result<PeReport, PeError> {
    let dos = source
        .read_at(0, 64)
        .ok_or(PeError::Truncated("DOS header"))?;
    if &dos[..2] != b"MZ" {
        return Err(PeError::NotPe("no MZ signature"));
    }
    let pe_offset = u64::from(u32_at(&dos, 0x3C).ok_or(PeError::Truncated("DOS header"))?);
    if pe_offset < 64 {
        return Err(PeError::NotPe("PE header offset is inside the DOS header"));
    }
    let coff = source
        .read_at(pe_offset, 24)
        .ok_or(PeError::Truncated("COFF header"))?;
    if &coff[..4] != b"PE\0\0" {
        return Err(PeError::NotPe("no PE signature"));
    }
    let machine = u16_at(&coff, 4).ok_or(PeError::Truncated("COFF header"))?;
    let section_count = u16_at(&coff, 6).ok_or(PeError::Truncated("COFF header"))?;
    let timestamp = u32_at(&coff, 8).ok_or(PeError::Truncated("COFF header"))?;
    let optional_size = u16_at(&coff, 20).ok_or(PeError::Truncated("COFF header"))?;
    let characteristics = u16_at(&coff, 22).ok_or(PeError::Truncated("COFF header"))?;
    if section_count > MAX_SECTIONS {
        return Err(PeError::Unsupported(
            "more sections than the PE format allows",
        ));
    }
    if optional_size > MAX_OPTIONAL_HEADER {
        return Err(PeError::Unsupported("optional header is too large"));
    }
    let optional = source
        .read_at(pe_offset + 24, usize::from(optional_size))
        .ok_or(PeError::Truncated("optional header"))?;
    let format = match u16_at(&optional, 0) {
        Some(0x010b) => PeFormat::Pe32,
        Some(0x020b) => PeFormat::Pe32Plus,
        Some(_) => return Err(PeError::NotPe("unknown optional header magic")),
        None => return Err(PeError::Truncated("optional header")),
    };
    let (count_at, dirs_at) = match format {
        PeFormat::Pe32 => (92, 96),
        PeFormat::Pe32Plus => (108, 112),
    };
    let subsystem = u16_at(&optional, 68).ok_or(PeError::Truncated("optional header"))?;
    let declared = u32_at(&optional, count_at).ok_or(PeError::Truncated("optional header"))?;
    if declared > MAX_DATA_DIRECTORIES {
        return Err(PeError::Unsupported("too many data directories"));
    }
    let room = (optional.len() - dirs_at) / 8;
    let directories: Vec<(u32, u32)> =
        (0..usize::try_from(declared).unwrap_or(0).min(room).min(16))
            .filter_map(|n| {
                Some((
                    u32_at(&optional, dirs_at + 8 * n)?,
                    u32_at(&optional, dirs_at + 8 * n + 4)?,
                ))
            })
            .collect();

    let sections = read_sections(
        source,
        pe_offset + 24 + u64::from(optional_size),
        section_count,
    )?;
    let mut image = Image { source, sections };
    let mut issues = Vec::new();
    let Tables {
        exports,
        imports,
        versions,
        version_struct,
    } = read_tables(&mut image, &directories, &mut issues);

    Ok(PeReport {
        machine,
        machine_name: machine_name(machine),
        format,
        is_dll: characteristics & CHARACTERISTIC_DLL != 0,
        large_address_aware: characteristics & CHARACTERISTIC_LARGE_ADDRESS_AWARE != 0,
        subsystem,
        timestamp,
        sections: section_count,
        file_version: versions.map(|v| v.0),
        product_version: versions.map(|v| v.1),
        nvse: nvse::classify(&exports, version_struct),
        exports,
        imports,
        issues,
    })
}

fn read_sections<R: ReadAt>(source: &mut R, at: u64, count: u16) -> Result<Vec<Section>, PeError> {
    let table = source
        .read_at(at, 40 * usize::from(count))
        .ok_or(PeError::Truncated("section table"))?;
    Ok((0..usize::from(count))
        .filter_map(|n| {
            let at = 40 * n;
            let virtual_size = u32_at(&table, at + 8)?;
            let raw_size = u32_at(&table, at + 16)?;
            Some(Section {
                virtual_address: u32_at(&table, at + 12)?,
                size: if virtual_size == 0 {
                    raw_size
                } else {
                    virtual_size.min(raw_size)
                },
                raw_offset: u32_at(&table, at + 20)?,
            })
        })
        .collect())
}

/// What the data directories hold.
struct Tables {
    exports: Vec<Export>,
    imports: Vec<String>,
    versions: Option<FileAndProduct>,
    version_struct: Option<VersionStruct>,
}

/// Read the export, import and resource directories. A broken one adds a line to `issues` and reads as empty.
fn read_tables<R: ReadAt>(
    image: &mut Image<'_, R>,
    directories: &[(u32, u32)],
    issues: &mut Vec<String>,
) -> Tables {
    let directory = |n: usize| {
        directories
            .get(n)
            .copied()
            .filter(|&(rva, size)| rva != 0 && size != 0)
    };
    let exports = match directory(0) {
        Some(dir) => read_exports(image, dir).unwrap_or_else(|why| {
            issues.push(format!("exports: {why}"));
            Vec::new()
        }),
        None => Vec::new(),
    };
    let imports = match directory(1) {
        Some((rva, _)) => read_imports(image, rva).unwrap_or_else(|why| {
            issues.push(format!("imports: {why}"));
            Vec::new()
        }),
        None => Vec::new(),
    };
    let versions = match directory(2) {
        Some(dir) => read_versions(image, dir).unwrap_or_else(|why| {
            issues.push(format!("version resource: {why}"));
            None
        }),
        None => None,
    };
    let version_struct = exports
        .iter()
        .find(|e| e.name == nvse::VERSION && !e.forwarded)
        .and_then(|e| match image.read_rva(e.rva, 8) {
            Ok(bytes) => Some(VersionStruct {
                info_version: u32_at(&bytes, 0)?,
                version: u32_at(&bytes, 4)?,
            }),
            Err(why) => {
                issues.push(format!("{}: {why}", nvse::VERSION));
                None
            }
        });
    Tables {
        exports,
        imports,
        versions,
        version_struct,
    }
}

/// Inspect a file held in memory.
///
/// # Errors
/// See [`inspect`].
pub fn inspect_bytes(bytes: &[u8]) -> Result<PeReport, PeError> {
    inspect(&mut SliceSource(bytes))
}

/// Inspect the file at `path`.
///
/// # Errors
/// See [`inspect`]; also [`PeError::Io`] when the file cannot be opened.
pub fn inspect_file(path: &Path) -> Result<PeReport, PeError> {
    let mut source = FileSource::open(path).map_err(|e| PeError::Io(e.to_string()))?;
    inspect(&mut source)
}

fn machine_name(machine: u16) -> String {
    match machine {
        0x014c => "i386".to_owned(),
        0x8664 => "amd64".to_owned(),
        0x01c0 | 0x01c4 => "arm".to_owned(),
        0xaa64 => "arm64".to_owned(),
        other => format!("unknown(0x{other:04x})"),
    }
}

fn read_exports<R: ReadAt>(
    image: &mut Image<'_, R>,
    (rva, size): (u32, u32),
) -> Result<Vec<Export>, &'static str> {
    let head = image.read_rva(rva, 40)?;
    let field = |at| u32_at(&head, at).ok_or("export directory is cut off");
    let (function_count, name_count) = (field(20)?, field(24)?);
    let (functions_rva, names_rva, ordinals_rva) = (field(28)?, field(32)?, field(36)?);
    if name_count > MAX_EXPORT_NAMES || function_count > MAX_EXPORT_FUNCTIONS {
        return Err("too many exports");
    }
    if name_count == 0 {
        return Ok(Vec::new());
    }
    let (function_count, name_count) = (function_count as usize, name_count as usize);
    let functions = image.read_rva(functions_rva, 4 * function_count)?;
    let names = image.read_rva(names_rva, 4 * name_count)?;
    let ordinals = image.read_rva(ordinals_rva, 2 * name_count)?;
    let mut exports = Vec::with_capacity(name_count);
    for n in 0..name_count {
        let name_at = u32_at(&names, 4 * n).ok_or("name table is cut off")?;
        let ordinal = usize::from(u16_at(&ordinals, 2 * n).ok_or("ordinal table is cut off")?);
        let address = u32_at(&functions, 4 * ordinal).ok_or("export ordinal is out of range")?;
        exports.push(Export {
            name: image.read_name(name_at)?,
            rva: address,
            forwarded: address >= rva && u64::from(address) < u64::from(rva) + u64::from(size),
        });
    }
    Ok(exports)
}

fn read_imports<R: ReadAt>(
    image: &mut Image<'_, R>,
    rva: u32,
) -> Result<Vec<String>, &'static str> {
    let mut dlls = Vec::new();
    for n in 0..MAX_IMPORT_DLLS {
        let at = rva
            .checked_add(20 * n)
            .ok_or("import table address overflows")?;
        let descriptor = image.read_rva(at, 20)?;
        let field = |at| u32_at(&descriptor, at).ok_or("import descriptor is cut off");
        let (lookup, name, first_thunk) = (field(0)?, field(12)?, field(16)?);
        if lookup == 0 && name == 0 && first_thunk == 0 {
            return Ok(dlls);
        }
        dlls.push(image.read_name(name)?);
    }
    Err("too many imported DLLs")
}

type FileAndProduct = (Version4, Version4);

/// The `(rva, size)` of the first resource data entry of type `RT_VERSION`, found the way Windows tools find it:
/// the version type's first table, its first sub-table, that table's first leaf.
fn find_version_entry<R: ReadAt>(
    image: &mut Image<'_, R>,
    (dir_rva, dir_size): (u32, u32),
) -> Result<Option<(u32, u32)>, &'static str> {
    const BROKEN: &str = "malformed resource directory";
    let mut tables = 0usize;
    let mut table = |image: &mut Image<'_, R>, at: u32| -> Result<Vec<(u32, u32)>, &'static str> {
        tables += 1;
        if tables > MAX_TABLES {
            return Err("too many resource directory tables");
        }
        let end = u64::from(at) + 16;
        if end > u64::from(dir_size) {
            return Err(BROKEN);
        }
        let head = image.read_rva(dir_rva.checked_add(at).ok_or(BROKEN)?, 16)?;
        let entries = usize::from(u16_at(&head, 12).ok_or(BROKEN)?)
            + usize::from(u16_at(&head, 14).ok_or(BROKEN)?);
        if entries > usize::from(MAX_TABLE_ENTRIES) {
            return Err("resource directory table is too large");
        }
        if end + 8 * entries as u64 > u64::from(dir_size) {
            return Err(BROKEN);
        }
        let raw = image.read_rva(dir_rva + at + 16, 8 * entries)?;
        Ok((0..entries)
            .filter_map(|n| Some((u32_at(&raw, 8 * n)?, u32_at(&raw, 8 * n + 4)?)))
            .collect())
    };
    let is_table = |target: u32| target & 0x8000_0000 != 0;
    for (id, target) in table(image, 0)? {
        if id != RT_VERSION || !is_table(target) {
            continue;
        }
        for (_, names) in table(image, target & 0x7FFF_FFFF)? {
            if !is_table(names) {
                continue;
            }
            for (_, leaf) in table(image, names & 0x7FFF_FFFF)? {
                if is_table(leaf) {
                    continue;
                }
                if u64::from(leaf) + 16 > u64::from(dir_size) {
                    return Err(BROKEN);
                }
                let entry = image.read_rva(dir_rva + leaf, 16)?;
                return match (u32_at(&entry, 0), u32_at(&entry, 4)) {
                    (Some(rva), Some(len)) => Ok(Some((rva, len))),
                    _ => Err(BROKEN),
                };
            }
        }
    }
    Ok(None)
}

fn read_versions<R: ReadAt>(
    image: &mut Image<'_, R>,
    directory: (u32, u32),
) -> Result<Option<FileAndProduct>, &'static str> {
    let Some((rva, len)) = find_version_entry(image, directory)? else {
        return Ok(None);
    };
    if len > MAX_VERSION_BLOCK {
        return Err("version block is larger than 64 KiB");
    }
    let block = image.read_rva(rva, len as usize)?;
    // VS_VERSIONINFO: wLength, wValueLength, wType, the key "VS_VERSION_INFO" in UTF-16, padding to four bytes,
    // then VS_FIXEDFILEINFO (signature, struct version, file version, product version, ...).
    if block.get(6..38) != Some(VERSION_KEY.as_slice()) {
        return Err("version block has no VS_VERSION_INFO key");
    }
    if u32_at(&block, 40) != Some(FIXED_FILE_INFO_SIGNATURE) {
        return Err("version block has no fixed file info");
    }
    let parts = |at: usize| -> Option<Version4> {
        let high = u32_at(&block, at)?.to_be_bytes();
        let low = u32_at(&block, at + 4)?.to_be_bytes();
        Some(Version4([
            u16::from_be_bytes([high[0], high[1]]),
            u16::from_be_bytes([high[2], high[3]]),
            u16::from_be_bytes([low[0], low[1]]),
            u16::from_be_bytes([low[2], low[3]]),
        ]))
    };
    match (parts(48), parts(56)) {
        (Some(file), Some(product)) => Ok(Some((file, product))),
        _ => Err("fixed file info is cut off"),
    }
}
