// SPDX-License-Identifier: GPL-3.0-only
//! The inspector on synthetic PE files built in code, including damaged ones.

use nvmod_pe::{PeError, PeFormat, Version4, VersionStruct, inspect_bytes, inspect_file};
use nvmod_testkit::PeBuilder;

const OPTIONAL: usize = 0x80 + 24;
const DIRECTORIES: usize = OPTIONAL + 96;

/// File offset of the table of data directory `n`.
fn table_offset(file: &[u8], n: usize) -> usize {
    let rva = u32::from_le_bytes(
        file[DIRECTORIES + 8 * n..DIRECTORIES + 8 * n + 4]
            .try_into()
            .unwrap(),
    );
    0x400 + (rva as usize - 0x1000)
}

fn set_u32(file: &mut [u8], at: usize, value: u32) {
    file[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn find(haystack: &[u8], needle: &[u8]) -> usize {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
        .unwrap()
}

#[test]
fn an_nvse_plugin_is_recognised_with_its_exports_and_imports() {
    let report = inspect_bytes(&PeBuilder::nvse_plugin().build()).unwrap();
    assert!(report.is_x86_32bit() && report.is_dll && !report.large_address_aware);
    assert_eq!(report.machine_name, "i386");
    assert_eq!(report.format, PeFormat::Pe32);
    let mut names: Vec<_> = report.exports.iter().map(|e| e.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["NVSEPlugin_Load", "NVSEPlugin_Query"]);
    assert_eq!(report.imports, ["KERNEL32.dll"]);
    assert!(report.nvse.is_plugin && report.nvse.has_query && report.nvse.has_load);
    assert!(!report.nvse.has_version_export && report.nvse.version_struct.is_none());
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    assert_eq!((report.file_version, report.product_version), (None, None));
}

#[test]
fn the_version_struct_is_read_from_the_exported_data() {
    let file = PeBuilder::dll()
        .export_code("NVSEPlugin_Load")
        .nvse_version(1, 0x0102_0304)
        .build();
    let report = inspect_bytes(&file).unwrap();
    assert!(report.nvse.has_version_export);
    assert_eq!(
        report.nvse.version_struct,
        Some(VersionStruct {
            info_version: 1,
            version: 0x0102_0304
        })
    );
}

#[test]
fn the_version_resource_and_the_large_address_flag_are_read() {
    let file = PeBuilder::exe()
        .versions([1, 4, 0, 525], [1, 4, 0, 525])
        .large_address_aware(true)
        .build();
    let report = inspect_bytes(&file).unwrap();
    assert!(!report.is_dll && report.large_address_aware);
    assert_eq!(report.file_version, Some(Version4([1, 4, 0, 525])));
    assert_eq!(report.product_version.unwrap().to_string(), "1.4.0.525");
    assert!(report.exports.is_empty() && report.imports.is_empty() && !report.nvse.is_plugin);
}

#[test]
fn a_64_bit_dll_is_read_but_is_not_a_32_bit_x86_image() {
    let file = PeBuilder::nvse_plugin().machine(0x8664).pe32_plus().build();
    let report = inspect_bytes(&file).unwrap();
    assert_eq!(
        (report.format, report.machine_name.as_str()),
        (PeFormat::Pe32Plus, "amd64")
    );
    assert!(!report.is_x86_32bit());
    assert!(report.nvse.has_load);
    let odd = inspect_bytes(&PeBuilder::dll().machine(0x1234).build()).unwrap();
    assert_eq!(odd.machine_name, "unknown(0x1234)");
}

#[test]
fn several_imports_and_data_exports_survive() {
    let file = PeBuilder::dll()
        .import("MSVCP140.dll")
        .import("VCRUNTIME140.dll")
        .import("KERNEL32.dll")
        .export_data("Alpha", b"hello")
        .export_code("Beta")
        .build();
    let report = inspect_bytes(&file).unwrap();
    assert_eq!(
        report.imports,
        ["MSVCP140.dll", "VCRUNTIME140.dll", "KERNEL32.dll"]
    );
    assert_eq!(report.exports.len(), 2);
    assert!(report.exports.iter().all(|e| !e.forwarded));
}

#[test]
fn files_that_are_not_pe_files_are_refused_with_a_reason() {
    assert_eq!(inspect_bytes(&[]), Err(PeError::Truncated("DOS header")));
    assert_eq!(inspect_bytes(b"MZ"), Err(PeError::Truncated("DOS header")));
    let good = PeBuilder::nvse_plugin().build();
    let mut file = good.clone();
    file[0] = b'X';
    assert_eq!(inspect_bytes(&file), Err(PeError::NotPe("no MZ signature")));
    let mut file = good.clone();
    set_u32(&mut file, 0x3C, 0x10);
    assert!(matches!(inspect_bytes(&file), Err(PeError::NotPe(_))));
    set_u32(&mut file, 0x3C, u32::MAX);
    assert_eq!(inspect_bytes(&file), Err(PeError::Truncated("COFF header")));
    let mut file = good.clone();
    file[0x80] = b'Q';
    assert_eq!(inspect_bytes(&file), Err(PeError::NotPe("no PE signature")));
    let mut file = good;
    file[OPTIONAL] = 0x99;
    assert_eq!(
        inspect_bytes(&file),
        Err(PeError::NotPe("unknown optional header magic"))
    );
}

#[test]
fn header_fields_beyond_the_limits_are_unsupported_not_allocated() {
    let good = PeBuilder::nvse_plugin().build();
    let mut file = good.clone();
    file[0x86..0x88].copy_from_slice(&5000u16.to_le_bytes()); // section count
    assert!(matches!(inspect_bytes(&file), Err(PeError::Unsupported(_))));
    let mut file = good.clone();
    file[0x80 + 20..0x80 + 22].copy_from_slice(&u16::MAX.to_le_bytes()); // optional header size
    assert!(matches!(inspect_bytes(&file), Err(PeError::Unsupported(_))));
    let mut file = good.clone();
    set_u32(&mut file, OPTIONAL + 92, 1_000_000); // data directories
    assert!(matches!(inspect_bytes(&file), Err(PeError::Unsupported(_))));
    let mut file = good;
    file[0x86..0x88].copy_from_slice(&96u16.to_le_bytes()); // 96 sections do not fit in the file
    assert_eq!(
        inspect_bytes(&file),
        Err(PeError::Truncated("section table"))
    );
}

#[test]
fn a_damaged_export_table_is_an_issue_and_the_rest_still_reads() {
    let good = PeBuilder::nvse_plugin()
        .versions([2, 0, 0, 1], [2, 0, 0, 1])
        .build();
    let export = table_offset(&good, 0);
    for (field, value) in [
        (24usize, u32::MAX),
        (20, u32::MAX),
        (28, 0xDEAD_0000),
        (32, 0xDEAD_0000),
        (36, 0xDEAD_0000),
    ] {
        let mut file = good.clone();
        set_u32(&mut file, export + field, value);
        let report = inspect_bytes(&file).unwrap();
        assert!(
            report.issues.iter().any(|i| i.starts_with("exports:")),
            "field {field}: {:?}",
            report.issues
        );
        assert!(report.exports.is_empty() && !report.nvse.is_plugin);
        assert_eq!(report.imports, ["KERNEL32.dll"]);
        assert_eq!(report.file_version, Some(Version4([2, 0, 0, 1])));
    }
    // A name pointer outside every section, and an ordinal beyond the address table.
    let names = u32::from_le_bytes(good[export + 32..export + 36].try_into().unwrap()) as usize;
    let mut file = good.clone();
    set_u32(&mut file, 0x400 + names - 0x1000, 0xFFFF_FFF0);
    assert!(inspect_bytes(&file).unwrap().issues[0].contains("not inside a section"));
    let ordinals = u32::from_le_bytes(good[export + 36..export + 40].try_into().unwrap()) as usize;
    let mut file = good;
    file[0x400 + ordinals - 0x1000] = 0xFF;
    assert!(inspect_bytes(&file).unwrap().issues[0].contains("ordinal"));
}

#[test]
fn an_unterminated_name_is_an_issue() {
    let good = PeBuilder::nvse_plugin().build();
    let start = find(&good, b"NVSEPlugin_Load");
    let mut file = good;
    // Overwrite the section tail from the first name on with non-zero bytes: no terminator within the section.
    let end = 0x400 + 0x200;
    for byte in &mut file[start..end] {
        *byte = b'A';
    }
    let issues = inspect_bytes(&file).unwrap().issues;
    assert!(
        issues.iter().any(|i| i.contains("not terminated")),
        "{issues:?}"
    );
}

#[test]
fn a_damaged_import_table_is_an_issue() {
    let good = PeBuilder::nvse_plugin().build();
    let imports = table_offset(&good, 1);
    let mut file = good.clone();
    set_u32(&mut file, imports + 12, 0xFFFF_0000); // DLL name outside every section
    assert!(inspect_bytes(&file).unwrap().issues[0].starts_with("imports:"));
    // No terminating all-zero descriptor within the limit: fill the section with non-zero descriptors.
    let mut file = good;
    for byte in &mut file[imports + 20..0x600] {
        *byte = 1;
    }
    for n in 0..1 {
        set_u32(&mut file, imports + 12 + 20 * n, 0x1000);
    }
    let issues = inspect_bytes(&file).unwrap().issues;
    assert!(
        issues.iter().any(|i| i.starts_with("imports:")),
        "{issues:?}"
    );
}

#[test]
fn a_damaged_version_resource_is_an_issue() {
    let good = PeBuilder::exe()
        .versions([1, 4, 0, 525], [1, 4, 0, 525])
        .build();
    let issue = |file: &[u8]| inspect_bytes(file).unwrap().issues.join("; ");

    let key: Vec<u8> = "VS_VERSION_INFO"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    let mut file = good.clone();
    let at = find(&file, &key);
    file[at] = b'X';
    assert!(issue(&file).contains("VS_VERSION_INFO"), "{}", issue(&file));

    let mut file = good.clone();
    let at = find(&file, &0xFEEF_04BDu32.to_le_bytes());
    file[at] = 0;
    assert!(issue(&file).contains("fixed file info"));

    let resources = table_offset(&good, 2);
    let mut file = good.clone();
    set_u32(&mut file, resources + 12, 0x0001_0000 | 0x1388); // 5000 id entries in the root table
    assert!(issue(&file).contains("too large"));
    let mut file = good.clone();
    // The version data entry claims a block larger than 64 KiB.
    set_u32(&mut file, resources + 72 + 4, 0x0002_0000);
    assert!(issue(&file).contains("larger than 64 KiB"));
    // The root table's entry points at a table beyond the directory.
    let mut file = good.clone();
    set_u32(&mut file, resources + 20, 0x8000_0000 | 0x00FF_0000);
    assert!(issue(&file).contains("malformed"));
    // Many entries that all lead to a sub-table are stopped by the table limit, not followed to the end: the
    // section and the directory are enlarged so that the entries fit.
    let mut file = good;
    file.resize(0x400 + 0x1000, 0);
    set_u32(&mut file, 0x80 + 24 + 224 + 8, 0x1000); // section virtual size
    set_u32(&mut file, 0x80 + 24 + 224 + 16, 0x1000); // section raw size
    set_u32(&mut file, DIRECTORIES + 8 * 2 + 4, 0x1000); // resource directory size
    set_u32(&mut file, resources + 12, 70 << 16); // 70 id entries in the root table
    for n in 0..70 {
        set_u32(&mut file, resources + 16 + 8 * n, 16);
        set_u32(&mut file, resources + 16 + 8 * n + 4, 0x8000_0000);
    }
    assert!(
        issue(&file).contains("too many resource directory tables"),
        "{}",
        issue(&file)
    );
}

#[test]
fn the_data_directory_count_may_be_smaller_than_sixteen() {
    let mut file = PeBuilder::nvse_plugin().build();
    set_u32(&mut file, OPTIONAL + 92, 1); // only the export directory is declared
    let report = inspect_bytes(&file).unwrap();
    assert!(report.nvse.is_plugin && report.imports.is_empty());
    set_u32(&mut file, OPTIONAL + 92, 0);
    assert!(!inspect_bytes(&file).unwrap().nvse.is_plugin);
}

#[test]
fn a_file_is_read_from_disk_through_the_bounded_reader() {
    let dir = std::env::temp_dir().join(format!("nvmod-pe-inspect-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("plugin.dll");
    std::fs::write(
        &path,
        PeBuilder::nvse_plugin()
            .versions([1, 2, 3, 4], [1, 2, 3, 4])
            .build(),
    )
    .unwrap();
    let report = inspect_file(&path).unwrap();
    assert_eq!(report.file_version, Some(Version4([1, 2, 3, 4])));
    std::fs::write(dir.join("short.dll"), b"MZ").unwrap();
    assert_eq!(
        inspect_file(&dir.join("short.dll")),
        Err(PeError::Truncated("DOS header"))
    );
    assert!(matches!(
        inspect_file(&dir.join("missing.dll")),
        Err(PeError::Io(_))
    ));
    std::fs::remove_dir_all(&dir).unwrap();
}
