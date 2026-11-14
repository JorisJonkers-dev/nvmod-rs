// SPDX-License-Identifier: GPL-3.0-only
#![no_main]
use libfuzzer_sys::fuzz_target;
use nvmod_formats::ini::{Ini, qt_string};
use nvmod_formats::ModMeta;
fuzz_target!(|text: &str| {
    let ini = Ini::parse(text);
    let _ = ini.get_string("General", "gamePath");
    let _ = qt_string(text);
    let _ = ModMeta::parse(text);
});
