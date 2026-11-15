// SPDX-License-Identifier: GPL-3.0-only
//! Synthetic save files for the tests and the fuzz seeds: a header in the layout `nvmod_formats::savegame`
//! documents (an unverified assumption), a tiny screenshot and a plugin list. No game data is involved.

/// Builds the bytes of a synthetic `.fos` save.
#[derive(Debug, Clone)]
pub struct SaveBuilder {
    number: u32,
    player: String,
    plugins: Vec<String>,
    body: Vec<u8>,
}

impl SaveBuilder {
    /// A save numbered `number` of the player `Courier` with no plugins.
    #[must_use]
    pub fn new(number: u32) -> SaveBuilder {
        SaveBuilder {
            number,
            player: "Courier".to_owned(),
            plugins: Vec::new(),
            body: Vec::new(),
        }
    }

    /// The plugins the save was made with.
    #[must_use]
    pub fn plugins(mut self, plugins: &[&str]) -> SaveBuilder {
        self.plugins = plugins.iter().map(|p| (*p).to_owned()).collect();
        self
    }

    /// Bytes after the header, so that two saves can differ.
    #[must_use]
    pub fn body(mut self, body: &[u8]) -> SaveBuilder {
        self.body = body.to_vec();
        self
    }

    /// The file's bytes.
    #[must_use]
    pub fn build(&self) -> Vec<u8> {
        let string = |out: &mut Vec<u8>, text: &str| {
            out.extend_from_slice(&u16::try_from(text.len()).unwrap_or(0).to_le_bytes());
            out.extend_from_slice(text.as_bytes());
            out.push(b'|');
        };
        let mut b = b"FO3SAVEGAME".to_vec();
        b.extend_from_slice(&0_u32.to_le_bytes());
        b.extend_from_slice(&48_u32.to_le_bytes());
        b.push(b'|');
        for n in [2, 2, self.number] {
            b.extend_from_slice(&u32::to_le_bytes(n));
            b.push(b'|');
        }
        string(&mut b, &self.player);
        string(&mut b, "Title");
        b.extend_from_slice(&1_u32.to_le_bytes());
        b.push(b'|');
        string(&mut b, "Somewhere");
        string(&mut b, "00.00.01");
        b.extend_from_slice(&[0; 12]);
        b.push(0x30);
        b.extend_from_slice(&0_u32.to_le_bytes());
        b.push(u8::try_from(self.plugins.len()).unwrap_or(0));
        for p in &self.plugins {
            string(&mut b, p);
        }
        b.extend_from_slice(&self.body);
        b
    }
}
