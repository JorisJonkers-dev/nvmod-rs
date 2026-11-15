// SPDX-License-Identifier: GPL-3.0-only
#![no_main]
//! Two modes, chosen by the first byte. Even: the rest is the file, as is. Odd: the rest is a list of
//! (offset, value) edits applied to a valid synthetic archive, which reaches the directory walk far faster than
//! random bytes can.
use libfuzzer_sys::fuzz_target;
use nvmod_testkit::BsaBuilder;

fuzz_target!(|data: &[u8]| {
    let Some((&mode, rest)) = data.split_first() else { return };
    if mode % 2 == 0 {
        let _ = nvmod_bsa::read_bytes(rest);
        return;
    }
    let mut file = BsaBuilder::new()
        .file("meshes/fake/a.nif", 5)
        .file("textures/fake/b.dds", 7)
        .file("c.txt", 1)
        .bytes();
    for edit in rest.chunks_exact(3) {
        let at = usize::from(u16::from_le_bytes([edit[0], edit[1]])) % file.len();
        file[at] = edit[2];
    }
    let _ = nvmod_bsa::read_bytes(&file);
});
