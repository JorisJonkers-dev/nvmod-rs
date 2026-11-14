// SPDX-License-Identifier: GPL-3.0-only
#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|text: &str| {
    let _ = nvmod_catalog::Catalog::parse(text);
});
