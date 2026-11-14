// SPDX-License-Identifier: GPL-3.0-only
#![no_main]
use libfuzzer_sys::fuzz_target;
use nvmod_core::Severity;

fuzz_target!(|text: &str| {
    if let Ok(severity) = text.parse::<Severity>() {
        assert_eq!(severity.as_str().parse::<Severity>(), Ok(severity));
    }
});
