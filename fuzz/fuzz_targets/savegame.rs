// SPDX-License-Identifier: GPL-3.0-only
#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|bytes: &[u8]| {
    // A truncated or damaged save is an error, never a panic.
    if let Ok(h) = nvmod_formats::savegame::parse(bytes) {
        assert!(h.plugins.len() <= 255);
    }
});
