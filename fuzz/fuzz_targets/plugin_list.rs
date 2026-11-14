// SPDX-License-Identifier: GPL-3.0-only
#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|bytes: &[u8]| {
    for name in nvmod_formats::plugins::parse_plugin_list(bytes) {
        assert!(!name.is_empty() && name == name.trim());
    }
});
