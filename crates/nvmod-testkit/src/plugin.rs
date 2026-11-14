// SPDX-License-Identifier: GPL-3.0-only
//! Synthetic plugin files: a header record (`TES4`) with the master flag and the master list, nothing else. The
//! layout is the one `loot-rs` reads (its `synthetic_plugin`); no game data is involved.

/// Builds the bytes of a minimal Fallout: New Vegas plugin.
#[derive(Debug, Clone, Default)]
pub struct PluginBuilder {
    master_flag: bool,
    masters: Vec<String>,
}

impl PluginBuilder {
    /// A plugin without the master flag (an `.esp`) and without masters.
    #[must_use]
    pub fn new() -> PluginBuilder {
        PluginBuilder::default()
    }

    /// A plugin with the master flag (an `.esm`).
    #[must_use]
    pub fn master() -> PluginBuilder {
        PluginBuilder {
            master_flag: true,
            masters: Vec::new(),
        }
    }

    /// Add a master the plugin declares in its header.
    #[must_use]
    pub fn requires(mut self, master: &str) -> PluginBuilder {
        self.masters.push(master.to_owned());
        self
    }

    /// The file's bytes.
    ///
    /// # Panics
    /// When a master name is longer than a header field can hold.
    #[must_use]
    pub fn build(&self) -> Vec<u8> {
        fn subrecord(out: &mut Vec<u8>, kind: [u8; 4], payload: &[u8]) {
            out.extend_from_slice(&kind);
            let length = u16::try_from(payload.len()).expect("subrecord fits in u16");
            out.extend_from_slice(&length.to_le_bytes());
            out.extend_from_slice(payload);
        }
        let mut data = Vec::new();
        let mut hedr = Vec::new();
        hedr.extend_from_slice(&1.34f32.to_le_bytes());
        hedr.extend_from_slice(&0u32.to_le_bytes());
        hedr.extend_from_slice(&0x800u32.to_le_bytes());
        subrecord(&mut data, *b"HEDR", &hedr);
        subrecord(&mut data, *b"CNAM", b"tester\0");
        for master in &self.masters {
            let mut payload = master.as_bytes().to_vec();
            payload.push(0);
            subrecord(&mut data, *b"MAST", &payload);
            subrecord(&mut data, *b"DATA", &0u64.to_le_bytes());
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"TES4");
        out.extend_from_slice(
            &u32::try_from(data.len())
                .expect("header fits")
                .to_le_bytes(),
        );
        out.extend_from_slice(&u32::from(self.master_flag).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&15u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&data);
        out
    }
}
