// SPDX-License-Identifier: GPL-3.0-only
#![no_main]
use libfuzzer_sys::fuzz_target;
fuzz_target!(|text: &str| {
    if let Ok(caps) = nvmod_tools::Capabilities::parse(text) {
        assert!(caps.extenders.iter().all(|e| e.implemented <= e.total));
    }
});
