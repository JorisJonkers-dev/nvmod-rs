// SPDX-License-Identifier: GPL-3.0-only
#![no_main]
use libfuzzer_sys::fuzz_target;
use nvmod_formats::categories::{Categories, format_ids, parse_ids};
fuzz_target!(|data: &[u8]| {
    let categories = Categories::parse(data);
    assert_eq!(categories.to_bytes(), data);
    for c in &categories.entries {
        assert!(categories.name(c.id).is_some());
    }
    let text = String::from_utf8_lossy(data);
    let ids = parse_ids(&text);
    assert_eq!(parse_ids(&format_ids(&ids)), ids);
});
