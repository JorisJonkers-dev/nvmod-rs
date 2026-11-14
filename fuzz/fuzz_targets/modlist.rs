// SPDX-License-Identifier: GPL-3.0-only
#![no_main]
use libfuzzer_sys::fuzz_target;
use nvmod_formats::Modlist;
fuzz_target!(|text: &str| {
    let list = Modlist::parse(text);
    let mut seen = std::collections::HashSet::new();
    for entry in &list.entries {
        assert!(seen.insert(&entry.name) && !entry.name.is_empty());
    }
});
