// SPDX-License-Identifier: GPL-3.0-only
#![no_main]
use libfuzzer_sys::fuzz_target;
use nvmod_modinstall::Limits;
use nvmod_testkit::TempDir;

/// Magic numbers of every format that is read in process. One is put in front of the input so the fuzzer always
/// reaches a reader and never the external extraction tool.
const MAGICS: [&[u8]; 6] = [
    b"PK\x03\x04",
    b"7z\xbc\xaf\x27\x1c",
    b"\x1f\x8b",
    b"\xfd7zXZ\x00",
    b"BZh",
    b"\x28\xb5\x2f\xfd",
];

fuzz_target!(|data: &[u8]| {
    let Some((&pick, rest)) = data.split_first() else {
        return;
    };
    let mut bytes = MAGICS[usize::from(pick) % MAGICS.len()].to_vec();
    bytes.extend_from_slice(rest);
    let dir = TempDir::new("fuzz-archive");
    let archive = dir.path().join("a.bin");
    std::fs::write(&archive, &bytes).unwrap();
    let dest = dir.path().join("out");
    std::fs::create_dir(&dest).unwrap();
    let limits = Limits {
        max_bytes: 1 << 20,
        max_entries: 256,
    };
    // Any refusal is fine; a panic, a hang or a write outside `dest` is not.
    let _ = nvmod_modinstall::extract::extract(&archive, &dest, None, limits);
    assert!(
        std::fs::read_dir(dir.path())
            .unwrap()
            .flatten()
            .all(|e| e.file_name() == "a.bin" || e.file_name() == "out")
    );
});
