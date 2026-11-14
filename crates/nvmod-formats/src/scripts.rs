// SPDX-License-Identifier: GPL-3.0-only
//! The compiled scripts (`SCDA`) of a plugin and the command opcodes they call. Only numbers leave this module: no
//! script text (`SCTX`), no names and no form data is returned. Every reader takes bytes, bounds every read, and
//! stops at the first thing that does not fit, so a hostile plugin cannot make it run away or panic.
//!
//! The statement layout is the game's compiler's: a 16-bit code and a 16-bit length, then the data; `0x1C` is a
//! reference prefix (the reference index follows, then the real statement). A command is a statement whose code is
//! its opcode. Commands also appear in the expressions of `Set`, `If` and `ElseIf` (token `X` or `x`), and in the
//! expression arguments of a command (version byte 1, a count, then a length-prefixed expression each); those are
//! followed with a strict shape check. Lambda bodies and arguments of another shape are not followed, so a count
//! is a lower bound of the real call sites.

use std::io::Read;

/// Most plugin nesting (`GRUP` in `GRUP`) followed.
const MAX_GROUP_DEPTH: usize = 16;
/// Most compiled scripts returned from one plugin.
pub const MAX_SCRIPTS: usize = 200_000;
/// Most bytes one compressed record may inflate to.
const MAX_INFLATED: u64 = 64 * 1024 * 1024;
/// How deep expressions nest inside command arguments.
const MAX_NESTING: usize = 8;
/// The record flag for a zlib-compressed record body.
const COMPRESSED: u32 = 0x0004_0000;
/// Operator tokens of an expression are single bytes below this.
const OPERATOR_LIMIT: u8 = 45;

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

/// The `SCDA` payloads of every record of `plugin` (scripts, dialogue results, quest stages, terminals and so on),
/// in file order.
#[must_use]
pub fn compiled_scripts(plugin: &[u8]) -> Vec<Vec<u8>> {
    let mut found = Vec::new();
    walk_records(plugin, 0, &mut found);
    found
}

fn walk_records(data: &[u8], depth: usize, found: &mut Vec<Vec<u8>>) {
    let mut at = 0;
    while at + 24 <= data.len() && found.len() < MAX_SCRIPTS {
        let kind = &data[at..at + 4];
        let Some(size) = u32_at(data, at + 4).map(|n| n as usize) else {
            return;
        };
        if kind == b"GRUP" {
            // The size of a group includes its own 24 byte header.
            let Some(end) = at
                .checked_add(size)
                .filter(|&e| e <= data.len() && e >= at + 24)
            else {
                return;
            };
            if depth < MAX_GROUP_DEPTH {
                walk_records(&data[at + 24..end], depth + 1, found);
            }
            at = end;
        } else {
            let Some(end) = (at + 24).checked_add(size).filter(|&e| e <= data.len()) else {
                return;
            };
            let flags = u32_at(data, at + 8).unwrap_or(0);
            let body = &data[at + 24..end];
            if flags & COMPRESSED == 0 {
                walk_subrecords(body, found);
            } else if let Some(inflated) = inflate(body) {
                walk_subrecords(&inflated, found);
            }
            at = end;
        }
    }
}

fn inflate(body: &[u8]) -> Option<Vec<u8>> {
    let expected = u64::from(u32_at(body, 0)?);
    if expected > MAX_INFLATED {
        return None;
    }
    let mut out = Vec::new();
    flate2::read::ZlibDecoder::new(body.get(4..)?)
        .take(MAX_INFLATED)
        .read_to_end(&mut out)
        .ok()?;
    Some(out)
}

fn walk_subrecords(body: &[u8], found: &mut Vec<Vec<u8>>) {
    let mut at = 0;
    let mut large: Option<usize> = None;
    while at + 6 <= body.len() && found.len() < MAX_SCRIPTS {
        let kind = &body[at..at + 4];
        let Some(small) = u16_at(body, at + 4) else {
            return;
        };
        let size = large.take().unwrap_or(usize::from(small));
        if kind == b"XXXX" {
            large = u32_at(body, at + 6).map(|n| n as usize);
            at += 6 + usize::from(small);
            continue;
        }
        let Some(end) = (at + 6).checked_add(size).filter(|&e| e <= body.len()) else {
            return;
        };
        if kind == b"SCDA" {
            found.push(body[at + 6..end].to_vec());
        }
        at = end;
    }
}

/// The opcodes of the commands one compiled script calls, one entry per call site, in script order. Opcodes below
/// `0x1000` (the game's own statements and commands) are included; the caller picks the ranges it wants.
#[must_use]
pub fn command_calls(script: &[u8]) -> Vec<u16> {
    let mut calls = Vec::new();
    let mut at = 0;
    while at + 4 <= script.len() {
        let (code, length, data_at) = if u16_at(script, at) == Some(0x1c) {
            let (Some(code), Some(length)) = (u16_at(script, at + 4), u16_at(script, at + 6))
            else {
                break;
            };
            (code, usize::from(length), at + 8)
        } else {
            let (Some(code), Some(length)) = (u16_at(script, at), u16_at(script, at + 2)) else {
                break;
            };
            (code, usize::from(length), at + 4)
        };
        let Some(data) = script.get(data_at..data_at + length) else {
            break;
        };
        match code {
            0x15 => set_calls(data, &mut calls),
            0x16 | 0x18 => {
                if let Some(expr) = data.get(2..) {
                    expression_calls(expr, 0, &mut calls);
                }
            }
            // The statements of the language itself: `Begin`, `End`, `Else`, `EndIf`, `Return`, `ScriptName`...
            0x10..=0x1f => {}
            opcode => {
                calls.push(opcode);
                argument_calls(data, 0, &mut calls);
            }
        }
        at = data_at + length;
    }
    calls
}

/// `Set <variable> to <expression>`: skip the variable (`r` and a reference first for another script's, the type
/// byte, an index for a local; or `G` and a global's index), then read the expression.
fn set_calls(data: &[u8], calls: &mut Vec<u16>) {
    let mut at = 0;
    if data.first() == Some(&b'G') {
        at = 3;
    } else {
        if data.first() == Some(&b'r') {
            at = 3;
        }
        match data.get(at) {
            Some(b'f' | b's') => at += 3,
            Some(b'r') | None => return,
            Some(_) => at += 1,
        }
    }
    if let Some(expr) = data.get(at..) {
        expression_calls(expr, 0, calls);
    }
}

/// The calls of `args` when it has the shape of expression arguments, at either of the two usual starts.
fn argument_calls(args: &[u8], depth: usize, calls: &mut Vec<u16>) {
    if depth >= MAX_NESTING {
        return;
    }
    for start in [0usize, 2] {
        if args.get(start) != Some(&1) {
            continue;
        }
        let Some(&count) = args.get(start + 1) else {
            continue;
        };
        let mut at = start + 2;
        let mut parts = Vec::new();
        for _ in 0..count {
            let Some(length) = u16_at(args, at).map(usize::from) else {
                break;
            };
            if length < 2 || at + length > args.len() {
                break;
            }
            parts.push(&args[at..at + length]);
            at += length;
        }
        if parts.len() == usize::from(count) && at == args.len() {
            for part in parts {
                expression_calls(part, depth + 1, calls);
            }
            return;
        }
    }
}

/// One expression: its length word (which counts itself), then tokens. Stops at a token it does not know.
fn expression_calls(expr: &[u8], depth: usize, calls: &mut Vec<u16>) {
    let Some(length) = u16_at(expr, 0).map(usize::from) else {
        return;
    };
    let Some(tokens) = expr.get(2..length.min(expr.len())) else {
        return;
    };
    let mut at = 0;
    while let Some(&token) = tokens.get(at) {
        at += 1;
        let step = match token {
            b'B' | b'b' => 1,
            b'I' | b'i' | b'R' | b'G' => 2,
            b'L' | b'l' => 4,
            b'Z' => 8,
            b'V' => 5,
            b'K' => 0,
            b'S' => match u16_at(tokens, at) {
                Some(n) => 2 + usize::from(n),
                None => return,
            },
            b'F' => match u32_at(tokens, at) {
                Some(n) => 4 + n as usize,
                None => return,
            },
            b'X' | b'x' => {
                let (Some(opcode), Some(args_len)) =
                    (u16_at(tokens, at + 2), u16_at(tokens, at + 4))
                else {
                    return;
                };
                let args_len = usize::from(args_len);
                if args_len < 2 {
                    return;
                }
                calls.push(opcode);
                if let Some(args) = tokens.get(at + 6..at + 4 + args_len) {
                    argument_calls(args, depth, calls);
                }
                4 + args_len
            }
            t if t < OPERATOR_LIMIT => 0,
            _ => return,
        };
        at += step;
        if at > tokens.len() {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{PluginBuilder, ScdaBuilder};

    #[test]
    fn statements_expressions_and_nested_arguments_are_counted() {
        let script = ScdaBuilder::new()
            .call(0x1401)
            .call_on(2, 0x2201)
            .set_to(&[0x2202, 0x1401])
            .if_calls(&[0x3101], Some("marker"))
            .call_with_expressions(0x1500, &[&[0x2203], &[0x3c93]])
            .build();
        assert_eq!(
            command_calls(&script),
            [
                0x1401, 0x2201, 0x2202, 0x1401, 0x3101, 0x1500, 0x2203, 0x3c93
            ]
        );
    }

    #[test]
    fn plugin_scripts_are_found_in_their_group() {
        let a = ScdaBuilder::new().call(0x1401).build();
        let b = ScdaBuilder::new().call(0x1402).build();
        let plugin = PluginBuilder::new()
            .script(a.clone(), "secret one")
            .script(b.clone(), "secret two")
            .build();
        assert_eq!(compiled_scripts(&plugin), [a, b]);
    }

    #[test]
    fn a_compressed_record_is_read() {
        use flate2::{Compression, write::ZlibEncoder};
        use std::io::Write;
        let script = ScdaBuilder::new().call(0x1401).build();
        let mut body = Vec::new();
        body.extend_from_slice(b"SCDA");
        body.extend_from_slice(&u16::try_from(script.len()).unwrap().to_le_bytes());
        body.extend_from_slice(&script);
        let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
        z.write_all(&body).unwrap();
        let packed = z.finish().unwrap();
        let mut record = Vec::new();
        record.extend_from_slice(b"INFO");
        record.extend_from_slice(&u32::try_from(packed.len() + 4).unwrap().to_le_bytes());
        record.extend_from_slice(&COMPRESSED.to_le_bytes());
        record.extend_from_slice(&[0; 12]);
        record.extend_from_slice(&u32::try_from(body.len()).unwrap().to_le_bytes());
        record.extend_from_slice(&packed);
        assert_eq!(compiled_scripts(&record), [script]);
    }

    #[test]
    fn truncated_and_hostile_input_stops_without_panic() {
        let plugin = PluginBuilder::new()
            .script(ScdaBuilder::new().call(0x1401).build(), "x")
            .build();
        for cut in 0..plugin.len() {
            let _ = compiled_scripts(&plugin[..cut]);
        }
        let script = ScdaBuilder::new()
            .set_to(&[0x2202])
            .call_with_expressions(0x1500, &[&[0x2203]])
            .build();
        for cut in 0..script.len() {
            let _ = command_calls(&script[..cut]);
        }
        let _ = command_calls(&[0xff; 64]);
        let _ = compiled_scripts(&[0xff; 64]);
    }
}
