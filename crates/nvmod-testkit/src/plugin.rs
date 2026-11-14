// SPDX-License-Identifier: GPL-3.0-only
//! Synthetic plugin files: a header record (`TES4`) with the master flag and the master list, nothing else. The
//! layout is the one `loot-rs` reads (its `synthetic_plugin`); no game data is involved.

/// Builds the bytes of a minimal Fallout: New Vegas plugin.
#[derive(Debug, Clone, Default)]
pub struct PluginBuilder {
    master_flag: bool,
    masters: Vec<String>,
    scripts: Vec<(Vec<u8>, String)>,
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
            ..PluginBuilder::default()
        }
    }

    /// Add a master the plugin declares in its header.
    #[must_use]
    pub fn requires(mut self, master: &str) -> PluginBuilder {
        self.masters.push(master.to_owned());
        self
    }

    /// Add a script record (`SCPT`) with the compiled bytes `compiled` (see [`ScdaBuilder`]) and the source text
    /// `source`. The records go into one `SCPT` group after the header.
    #[must_use]
    pub fn script(mut self, compiled: Vec<u8>, source: &str) -> PluginBuilder {
        self.scripts.push((compiled, source.to_owned()));
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
        if !self.scripts.is_empty() {
            let mut group = Vec::new();
            for (n, (compiled, source)) in self.scripts.iter().enumerate() {
                let mut body = Vec::new();
                let mut schr = vec![0u8; 4];
                schr.extend_from_slice(&0u32.to_le_bytes());
                schr.extend_from_slice(
                    &u32::try_from(compiled.len())
                        .expect("script fits")
                        .to_le_bytes(),
                );
                schr.extend_from_slice(&0u32.to_le_bytes());
                schr.extend_from_slice(&[0, 0, 0, 0]);
                subrecord(&mut body, *b"SCHR", &schr);
                subrecord(&mut body, *b"SCDA", compiled);
                subrecord(&mut body, *b"SCTX", source.as_bytes());
                group.extend_from_slice(b"SCPT");
                group.extend_from_slice(
                    &u32::try_from(body.len())
                        .expect("record fits")
                        .to_le_bytes(),
                );
                group.extend_from_slice(&0u32.to_le_bytes());
                group.extend_from_slice(
                    &(0x0100_0000 + u32::try_from(n).expect("few")).to_le_bytes(),
                );
                group.extend_from_slice(&0u32.to_le_bytes());
                group.extend_from_slice(&15u16.to_le_bytes());
                group.extend_from_slice(&0u16.to_le_bytes());
                group.extend_from_slice(&body);
            }
            out.extend_from_slice(b"GRUP");
            out.extend_from_slice(
                &u32::try_from(group.len() + 24)
                    .expect("group fits")
                    .to_le_bytes(),
            );
            out.extend_from_slice(b"SCPT");
            out.extend_from_slice(&[0; 12]);
            out.extend_from_slice(&group);
        }
        out
    }
}

/// Builds the compiled bytes (`SCDA`) of a synthetic script: statements of the layout the game's compiler writes
/// (a 16-bit code and a 16-bit length, then the data), with command opcodes of the caller's choosing. The commands
/// do nothing; only the call sites are real.
#[derive(Debug, Clone, Default)]
pub struct ScdaBuilder {
    bytes: Vec<u8>,
}

fn statement(out: &mut Vec<u8>, code: u16, data: &[u8]) {
    out.extend_from_slice(&code.to_le_bytes());
    out.extend_from_slice(&u16::try_from(data.len()).expect("fits").to_le_bytes());
    out.extend_from_slice(data);
}

/// An expression (its length word, then the tokens) that calls each of `opcodes` once.
fn expression(opcodes: &[u16], literal: Option<&str>) -> Vec<u8> {
    let mut tokens = Vec::new();
    for (n, opcode) in opcodes.iter().enumerate() {
        tokens.push(b'X');
        tokens.extend_from_slice(&0u16.to_le_bytes());
        tokens.extend_from_slice(&opcode.to_le_bytes());
        tokens.extend_from_slice(&2u16.to_le_bytes());
        if n > 0 {
            tokens.push(1);
        }
    }
    if let Some(text) = literal {
        tokens.push(b'S');
        tokens.extend_from_slice(&u16::try_from(text.len()).expect("fits").to_le_bytes());
        tokens.extend_from_slice(text.as_bytes());
    }
    let mut out = u16::try_from(tokens.len() + 2)
        .expect("fits")
        .to_le_bytes()
        .to_vec();
    out.extend_from_slice(&tokens);
    out
}

impl ScdaBuilder {
    /// A script with its `ScriptName` statement.
    #[must_use]
    pub fn new() -> ScdaBuilder {
        let mut bytes = Vec::new();
        statement(&mut bytes, 0x1d, &[]);
        ScdaBuilder { bytes }
    }

    /// A statement that calls `opcode`.
    #[must_use]
    pub fn call(mut self, opcode: u16) -> ScdaBuilder {
        statement(&mut self.bytes, opcode, &[0, 0]);
        self
    }

    /// A statement that calls `opcode` on a reference (`ref.Command`: the `0x1C` prefix and the reference index).
    #[must_use]
    pub fn call_on(mut self, reference: u16, opcode: u16) -> ScdaBuilder {
        self.bytes.extend_from_slice(&0x1cu16.to_le_bytes());
        self.bytes.extend_from_slice(&reference.to_le_bytes());
        statement(&mut self.bytes, opcode, &[0, 0]);
        self
    }

    /// `Set var to <calls>`: an expression that calls each of `opcodes`.
    #[must_use]
    pub fn set_to(mut self, opcodes: &[u16]) -> ScdaBuilder {
        let mut data = vec![b'f', 1, 0];
        data.extend_from_slice(&expression(opcodes, None));
        statement(&mut self.bytes, 0x15, &data);
        self
    }

    /// `If <calls>` and its `EndIf`, with a string literal `literal` in the expression when given.
    #[must_use]
    pub fn if_calls(mut self, opcodes: &[u16], literal: Option<&str>) -> ScdaBuilder {
        let mut data = vec![0, 0];
        data.extend_from_slice(&expression(opcodes, literal));
        statement(&mut self.bytes, 0x16, &data);
        statement(&mut self.bytes, 0x19, &[]);
        self
    }

    /// A statement that calls `opcode` with arguments that are expressions (`version 1`, a count, then a length
    /// and an expression each); the expressions call `nested`.
    ///
    /// # Panics
    /// With more than 255 expressions.
    #[must_use]
    pub fn call_with_expressions(mut self, opcode: u16, nested: &[&[u16]]) -> ScdaBuilder {
        let mut data = vec![0, 0, 1, u8::try_from(nested.len()).expect("few")];
        for calls in nested {
            data.extend_from_slice(&expression(calls, None));
        }
        statement(&mut self.bytes, opcode, &data);
        self
    }

    /// The compiled bytes.
    #[must_use]
    pub fn build(&self) -> Vec<u8> {
        self.bytes.clone()
    }
}
