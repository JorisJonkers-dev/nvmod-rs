// SPDX-License-Identifier: GPL-3.0-only
//! A small PE writer: one section holding the export, import and resource tables, a version resource and the
//! data of data exports. It writes what the inspector reads and nothing more; it is not a linker. Test support:
//! it panics on sizes that cannot occur in a synthetic file, and `build` is one long function on purpose.
#![allow(clippy::missing_panics_doc, clippy::too_many_lines)]

const SECTION_RVA: u32 = 0x1000;
const SECTION_FILE_OFFSET: u32 = 0x400;
const FILE_ALIGNMENT: usize = 0x200;

/// One export of a synthetic DLL.
#[derive(Debug, Clone)]
pub struct ExportSpec {
    /// Exported name.
    pub name: String,
    /// `None` is a code export (one `ret` byte); `Some(bytes)` is a data export holding those bytes.
    pub data: Option<Vec<u8>>,
}

/// Builds a synthetic PE file. Start from [`PeBuilder::dll`] or [`PeBuilder::nvse_plugin`] and adjust.
#[derive(Debug, Clone)]
pub struct PeBuilder {
    machine: u16,
    pe32_plus: bool,
    dll: bool,
    large_address_aware: bool,
    file_version: Option<[u16; 4]>,
    product_version: Option<[u16; 4]>,
    exports: Vec<ExportSpec>,
    imports: Vec<String>,
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn rva_of(section: &[u8]) -> u32 {
    SECTION_RVA + u32::try_from(section.len()).expect("synthetic sections are small")
}

fn pad4(section: &mut Vec<u8>) {
    while !section.len().is_multiple_of(4) {
        section.push(0);
    }
}

impl PeBuilder {
    /// A 32-bit x86 DLL with no exports, imports or version resource.
    #[must_use]
    pub fn dll() -> Self {
        PeBuilder {
            machine: 0x014c,
            pe32_plus: false,
            dll: true,
            large_address_aware: false,
            file_version: None,
            product_version: None,
            exports: Vec::new(),
            imports: Vec::new(),
        }
    }

    /// A 32-bit x86 executable (not a DLL), as the game's own is.
    #[must_use]
    pub fn exe() -> Self {
        PeBuilder {
            dll: false,
            ..Self::dll()
        }
    }

    /// A 32-bit DLL as an older NVSE plugin ships: exports `NVSEPlugin_Query` and `NVSEPlugin_Load`, imports
    /// `KERNEL32.dll`.
    #[must_use]
    pub fn nvse_plugin() -> Self {
        Self::dll()
            .export_code("NVSEPlugin_Query")
            .export_code("NVSEPlugin_Load")
            .import("KERNEL32.dll")
    }

    /// Set the machine type (`0x14c` x86, `0x8664` x64).
    #[must_use]
    pub fn machine(mut self, machine: u16) -> Self {
        self.machine = machine;
        self
    }

    /// Use the 64-bit optional header layout.
    #[must_use]
    pub fn pe32_plus(mut self) -> Self {
        self.pe32_plus = true;
        self
    }

    /// Set or clear the large-address-aware flag.
    #[must_use]
    pub fn large_address_aware(mut self, on: bool) -> Self {
        self.large_address_aware = on;
        self
    }

    /// Add a version resource with these file and product versions.
    #[must_use]
    pub fn versions(mut self, file: [u16; 4], product: [u16; 4]) -> Self {
        self.file_version = Some(file);
        self.product_version = Some(product);
        self
    }

    /// Export a code symbol.
    #[must_use]
    pub fn export_code(mut self, name: &str) -> Self {
        self.exports.push(ExportSpec {
            name: name.to_owned(),
            data: None,
        });
        self
    }

    /// Export a data symbol holding `bytes`.
    #[must_use]
    pub fn export_data(mut self, name: &str, bytes: &[u8]) -> Self {
        self.exports.push(ExportSpec {
            name: name.to_owned(),
            data: Some(bytes.to_vec()),
        });
        self
    }

    /// Export `NVSEPlugin_Version` as a data symbol that starts with `info_version` and `version`, then 504 bytes
    /// of padding (the size of the struct is not known; the doctor only reads the first eight bytes).
    #[must_use]
    pub fn nvse_version(self, info_version: u32, version: u32) -> Self {
        let mut data = Vec::new();
        put_u32(&mut data, info_version);
        put_u32(&mut data, version);
        data.resize(512, 0);
        self.export_data("NVSEPlugin_Version", &data)
    }

    /// Import from the DLL `name` (no functions are listed; the inspector only reads the DLL names).
    #[must_use]
    pub fn import(mut self, name: &str) -> Self {
        self.imports.push(name.to_owned());
        self
    }

    fn version_block(file: [u16; 4], product: [u16; 4]) -> Vec<u8> {
        let mut block = Vec::new();
        put_u16(&mut block, 0); // wLength, patched below
        put_u16(&mut block, 52); // wValueLength
        put_u16(&mut block, 0); // wType
        for unit in "VS_VERSION_INFO\0".encode_utf16() {
            put_u16(&mut block, unit);
        }
        while !block.len().is_multiple_of(4) {
            block.push(0);
        }
        put_u32(&mut block, 0xFEEF_04BD);
        put_u32(&mut block, 0x0001_0000);
        for parts in [file, product] {
            put_u32(
                &mut block,
                (u32::from(parts[0]) << 16) | u32::from(parts[1]),
            );
            put_u32(
                &mut block,
                (u32::from(parts[2]) << 16) | u32::from(parts[3]),
            );
        }
        block.resize(40 + 52, 0);
        let length = u16::try_from(block.len()).expect("small");
        block[..2].copy_from_slice(&length.to_le_bytes());
        block
    }

    /// The finished file.
    #[must_use]
    pub fn build(&self) -> Vec<u8> {
        let mut section: Vec<u8> = Vec::new();
        let mut export_dir = (0u32, 0u32);
        let mut import_dir = (0u32, 0u32);
        let mut resource_dir = (0u32, 0u32);

        if !self.exports.is_empty() {
            // Names sorted, as a linker writes them; each export's body is placed first.
            let mut sorted = self.exports.clone();
            sorted.sort_by(|a, b| a.name.cmp(&b.name));
            let mut bodies = Vec::new();
            for export in &sorted {
                bodies.push(rva_of(&section));
                section.extend_from_slice(export.data.as_deref().unwrap_or(&[0xC3]));
                pad4(&mut section);
            }
            let count = u32::try_from(sorted.len()).expect("small");
            let dir_start = rva_of(&section);
            let dir_len = 40 + 4 * sorted.len() * 2 + 2 * sorted.len() + 16;
            let functions_rva = dir_start + 40;
            let names_rva = functions_rva + 4 * count;
            let ordinals_rva = names_rva + 4 * count;
            let strings_start = ordinals_rva + 2 * count;
            let mut name_rvas = Vec::new();
            let mut strings = Vec::new();
            let mut at = strings_start + 16;
            for export in &sorted {
                name_rvas.push(at);
                strings.extend_from_slice(export.name.as_bytes());
                strings.push(0);
                at += u32::try_from(export.name.len() + 1).expect("small");
            }
            put_u32(&mut section, 0); // characteristics
            put_u32(&mut section, 0); // time stamp
            put_u32(&mut section, 0); // version
            put_u32(&mut section, strings_start); // name of the DLL
            put_u32(&mut section, 1); // ordinal base
            put_u32(&mut section, count); // address table entries
            put_u32(&mut section, count); // name pointers
            put_u32(&mut section, functions_rva);
            put_u32(&mut section, names_rva);
            put_u32(&mut section, ordinals_rva);
            for body in &bodies {
                put_u32(&mut section, *body);
            }
            for rva in &name_rvas {
                put_u32(&mut section, *rva);
            }
            for n in 0..sorted.len() {
                put_u16(&mut section, u16::try_from(n).expect("small"));
            }
            section.extend_from_slice(b"synthetic.dll\0\0\0");
            section.extend_from_slice(&strings);
            pad4(&mut section);
            export_dir = (
                dir_start,
                u32::try_from(dir_len + strings.len()).expect("small"),
            );
        }

        if !self.imports.is_empty() {
            let start = rva_of(&section);
            let descriptors = 20 * (self.imports.len() + 1);
            let mut at = start + u32::try_from(descriptors).expect("small");
            let mut names = Vec::new();
            for dll in &self.imports {
                put_u32(&mut section, 0);
                put_u32(&mut section, 0);
                put_u32(&mut section, 0);
                put_u32(&mut section, at);
                put_u32(&mut section, 0);
                names.extend_from_slice(dll.as_bytes());
                names.push(0);
                at += u32::try_from(dll.len() + 1).expect("small");
            }
            section.extend_from_slice(&[0; 20]);
            section.extend_from_slice(&names);
            pad4(&mut section);
            import_dir = (
                start,
                u32::try_from(descriptors + names.len()).expect("small"),
            );
        }

        if let (Some(file), Some(product)) = (self.file_version, self.product_version) {
            let block = Self::version_block(file, product);
            let block_rva = rva_of(&section);
            section.extend_from_slice(&block);
            pad4(&mut section);
            let start = rva_of(&section);
            // root (type 16) -> name table (id 1) -> language table (id 0x409) -> data entry.
            let table = |section: &mut Vec<u8>, id: u32, target: u32| {
                section.extend_from_slice(&[0; 12]);
                put_u16(section, 0); // named entries
                put_u16(section, 1); // id entries
                put_u32(section, id);
                put_u32(section, target);
            };
            table(&mut section, 16, 0x8000_0000 | 0x18);
            table(&mut section, 1, 0x8000_0000 | 0x30);
            table(&mut section, 0x409, 72);
            put_u32(&mut section, block_rva);
            put_u32(&mut section, u32::try_from(block.len()).expect("small"));
            put_u32(&mut section, 0);
            put_u32(&mut section, 0);
            resource_dir = (start, rva_of(&section) - start);
        }

        let optional_size: u16 = if self.pe32_plus { 240 } else { 224 };
        let mut file = vec![0u8; 0x80];
        file[..2].copy_from_slice(b"MZ");
        file[0x3C..0x40].copy_from_slice(&0x80u32.to_le_bytes());
        file.extend_from_slice(b"PE\0\0");
        put_u16(&mut file, self.machine);
        put_u16(&mut file, 1); // one section
        put_u32(&mut file, 0x5F00_0000); // time stamp
        put_u32(&mut file, 0);
        put_u32(&mut file, 0);
        put_u16(&mut file, optional_size);
        let mut characteristics = 0x0102u16; // executable, 32-bit machine
        if self.dll {
            characteristics |= 0x2000;
        }
        if self.large_address_aware {
            characteristics |= 0x0020;
        }
        put_u16(&mut file, characteristics);
        // Optional header.
        let optional_start = file.len();
        put_u16(&mut file, if self.pe32_plus { 0x020b } else { 0x010b });
        file.resize(optional_start + 68, 0);
        put_u16(&mut file, 2); // subsystem: GUI
        file.resize(optional_start + if self.pe32_plus { 108 } else { 92 }, 0);
        put_u32(&mut file, 16); // number of data directories
        for (rva, size) in [export_dir, import_dir, resource_dir] {
            put_u32(&mut file, rva);
            put_u32(&mut file, size);
        }
        file.resize(optional_start + usize::from(optional_size), 0);
        // Section table: one section, `.rdata`.
        let raw_size = section.len().div_ceil(FILE_ALIGNMENT).max(1) * FILE_ALIGNMENT;
        file.extend_from_slice(b".rdata\0\0");
        put_u32(&mut file, u32::try_from(section.len()).expect("small")); // virtual size
        put_u32(&mut file, SECTION_RVA);
        put_u32(&mut file, u32::try_from(raw_size).expect("small"));
        put_u32(&mut file, SECTION_FILE_OFFSET);
        file.extend_from_slice(&[0; 12]);
        put_u32(&mut file, 0x4000_0040); // readable, initialised data
        file.resize(SECTION_FILE_OFFSET as usize, 0);
        file.extend_from_slice(&section);
        file.resize(SECTION_FILE_OFFSET as usize + raw_size, 0);
        file
    }
}
