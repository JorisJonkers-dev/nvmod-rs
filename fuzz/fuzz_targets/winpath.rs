// SPDX-License-Identifier: GPL-3.0-only
#![no_main]
use libfuzzer_sys::fuzz_target;
use std::collections::BTreeMap;
fuzz_target!(|text: &str| {
    let _ = nvmod_formats::winpath::to_host_path(text, &BTreeMap::new());
});
