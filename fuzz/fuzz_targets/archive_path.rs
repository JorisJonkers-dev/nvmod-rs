// SPDX-License-Identifier: GPL-3.0-only
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|name: &str| {
    if let Ok(path) = nvmod_modinstall::extract::safe_relative(name) {
        assert!(path.is_relative());
        assert!(path.components().all(|c| matches!(c, std::path::Component::Normal(_))));
    }
});
