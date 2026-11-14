// SPDX-License-Identifier: GPL-3.0-only
#![no_main]
//! Two modes, chosen by the first byte. Even: the rest is the file, as is. Odd: the rest is a list of
//! (offset, value) edits applied to a valid synthetic plugin, which reaches the export, import and resource
//! parsers far faster than random bytes can.
use libfuzzer_sys::fuzz_target;
use nvmod_testkit::PeBuilder;

fuzz_target!(|data: &[u8]| {
    let Some((&mode, rest)) = data.split_first() else { return };
    if mode % 2 == 0 {
        let _ = nvmod_pe::inspect_bytes(rest);
        return;
    }
    let mut file = PeBuilder::nvse_plugin()
        .nvse_version(1, 1)
        .versions([1, 4, 0, 525], [1, 4, 0, 525])
        .build();
    for edit in rest.chunks_exact(3) {
        let at = usize::from(u16::from_le_bytes([edit[0], edit[1]])) % file.len();
        file[at] = edit[2];
    }
    let _ = nvmod_pe::inspect_bytes(&file);
});
