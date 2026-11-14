// SPDX-License-Identifier: GPL-3.0-only
#![no_main]
use libfuzzer_sys::fuzz_target;
use nvmod_modinstall::fomod::{Answers, Env, FileState, Module, decode_xml};

struct NoPlugins;

impl Env for NoPlugins {
    fn file_state(&self, _file: &str) -> Option<FileState> {
        Some(FileState::Missing)
    }
}

fuzz_target!(|bytes: &[u8]| {
    if let Ok(module) = Module::parse(&decode_xml(bytes)) {
        let _ = module.resolve(&NoPlugins, &Answers::new());
    }
});
