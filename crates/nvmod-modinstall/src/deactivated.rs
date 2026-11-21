// SPDX-License-Identifier: GPL-3.0-only
//! The plugins that `nvmod mods disable` took out of `plugins.txt`, kept in the Profile's `deactivated.txt` so
//! `nvmod mods enable` can put each back on its own line, in its own place, active as it was. Every other byte of
//! `plugins.txt` is left alone by both steps, so a disable followed by an enable is the identity on the file.

use nvmod_formats::text::{decode, encode_windows_1252};
use std::fmt::Write as _;

/// The file in the Profile folder. The journal of a Mods change saves and restores it with `plugins.txt`.
pub const FILE: &str = "deactivated.txt";

const HEADER: &str = "# plugins taken out of plugins.txt by `nvmod mods disable`; `nvmod mods enable` puts them back.";

/// One line taken out of `plugins.txt`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Record {
    /// The line above it in `plugins.txt` (without its line end), empty when it was the first line.
    pub after: String,
    /// The line itself (without its line end), as it was: `*Name.esp` for an active plugin.
    pub line: String,
    /// Its line end: `\r\n`, `\n` or empty (last line of a file without a final newline).
    pub eol: String,
}

impl Record {
    /// The plugin name of the line.
    #[must_use]
    pub fn name(&self) -> &str {
        name_of(&self.line)
    }
}

fn name_of(line: &str) -> &str {
    line.trim().trim_start_matches('*').trim()
}

fn eol_word(eol: &str) -> &'static str {
    match eol {
        "\r\n" => "crlf",
        "\n" => "lf",
        _ => "none",
    }
}

fn eol_of(word: &str) -> String {
    match word {
        "crlf" => "\r\n",
        "lf" => "\n",
        _ => "",
    }
    .to_owned()
}

/// The records of a `deactivated.txt` (none for a missing file; a line that does not parse is skipped).
#[must_use]
pub fn parse(bytes: Option<&[u8]>) -> Vec<Record> {
    let Some(bytes) = bytes else {
        return Vec::new();
    };
    decode(bytes)
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let mut parts = l.splitn(3, '\t');
            let eol = parts.next()?;
            let after = parts.next()?;
            let line = parts.next()?;
            Some(Record {
                after: after.to_owned(),
                line: line.to_owned(),
                eol: eol_of(eol),
            })
        })
        .collect()
}

/// The text of a `deactivated.txt` for `records`; `None` when there are none (the file is then removed).
#[must_use]
pub fn render(records: &[Record]) -> Option<Vec<u8>> {
    if records.is_empty() {
        return None;
    }
    let mut out = format!("{HEADER}\n");
    for r in records {
        let _ = writeln!(out, "{}\t{}\t{}", eol_word(&r.eol), r.after, r.line);
    }
    Some(out.into_bytes())
}

/// The raw lines of `bytes`, each with its line end.
fn raw_lines(bytes: &[u8]) -> Vec<&[u8]> {
    bytes.split_inclusive(|b| *b == b'\n').collect()
}

fn split_eol(raw: &[u8]) -> (&[u8], &'static str) {
    if let Some(rest) = raw.strip_suffix(b"\r\n") {
        (rest, "\r\n")
    } else if let Some(rest) = raw.strip_suffix(b"\n") {
        (rest, "\n")
    } else {
        (raw, "")
    }
}

/// `plugins.txt` without the active lines of `names` (ignoring case), and what was taken out. Every other line,
/// comment and line end stays byte for byte. `None` when `plugins.txt` lists none of them.
#[must_use]
pub fn deactivate(old: Option<&[u8]>, names: &[String]) -> Option<(Vec<u8>, Vec<Record>)> {
    let old = old?;
    let mut kept: Vec<u8> = Vec::with_capacity(old.len());
    let mut taken = Vec::new();
    let mut previous = String::new();
    for raw in raw_lines(old) {
        let (body, eol) = split_eol(raw);
        let text = decode(body);
        let trimmed = text.trim();
        let hit = !trimmed.starts_with('#')
            && !trimmed.is_empty()
            && names
                .iter()
                .any(|n| name_of(trimmed).eq_ignore_ascii_case(n));
        if hit {
            taken.push(Record {
                after: previous.clone(),
                line: trimmed.to_owned(),
                eol: eol.to_owned(),
            });
        } else {
            kept.extend_from_slice(raw);
        }
        trimmed.clone_into(&mut previous);
    }
    if taken.is_empty() {
        None
    } else {
        Some((kept, taken))
    }
}

fn bytes_of(text: &str, utf8: bool) -> Vec<u8> {
    if utf8 {
        text.as_bytes().to_vec()
    } else {
        encode_windows_1252(text).unwrap_or_else(|| text.as_bytes().to_vec())
    }
}

/// `plugins.txt` with each of `records` put back: after the line it was below (when that line is still there),
/// else after the nearest earlier plugin of `loadorder.txt` that is listed, else at the end. A plugin that is
/// listed already is not added twice. In the order they were taken out, so a run of neighbours returns whole.
#[must_use]
pub fn reactivate(old: &[u8], records: &[Record], loadorder: Option<&[u8]>) -> Vec<u8> {
    let utf8 = std::str::from_utf8(old).is_ok();
    let mut lines: Vec<Vec<u8>> = raw_lines(old).into_iter().map(<[u8]>::to_vec).collect();
    let order: Vec<String> = loadorder.map_or_default(|b| {
        decode(b)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(|l| name_of(l).to_owned())
            .collect()
    });
    for rec in records {
        let texts: Vec<String> = lines
            .iter()
            .map(|l| decode(split_eol(l).0).trim().to_owned())
            .collect();
        if texts
            .iter()
            .any(|t| !t.starts_with('#') && name_of(t).eq_ignore_ascii_case(rec.name()))
        {
            continue;
        }
        let at = if rec.after.is_empty() {
            Some(0)
        } else {
            texts.iter().position(|t| *t == rec.after).map(|i| i + 1)
        }
        .or_else(|| {
            let pos = order
                .iter()
                .position(|n| n.eq_ignore_ascii_case(rec.name()))?;
            order[..pos].iter().rev().find_map(|n| {
                texts
                    .iter()
                    .position(|t| !t.starts_with('#') && name_of(t).eq_ignore_ascii_case(n))
                    .map(|i| i + 1)
            })
        })
        .unwrap_or(lines.len());
        let mut line = bytes_of(&rec.line, utf8);
        if rec.eol.is_empty() && at < lines.len() {
            line.extend_from_slice(b"\r\n");
        } else {
            line.extend_from_slice(rec.eol.as_bytes());
        }
        if at == lines.len()
            && let Some(last) = lines.last_mut()
            && !last.ends_with(b"\n")
        {
            last.extend_from_slice(b"\r\n");
        }
        lines.insert(at, line);
    }
    lines.concat()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn deactivate_then_reactivate_is_the_identity() {
        for old in [
            &b"# list\r\n*A.esm\r\n*b.esp\r\n*C.esp\r\n"[..],
            b"# list\r\n*A.esm\r\n*b.esp\r\n*C.esp",
            b"*b.esp\n*A.esm\n*C.esp\n",
            b"# list\r\n\r\n*A.esm\r\nB.esp\r\n*C.esp\r\n\r\n",
            b"# list\r\n*A.esm\r\n*b.esp\r\n*c.esp\r\n*D.esp\r\n",
        ] {
            let (new, records) = deactivate(Some(old), &names(&["B.ESP", "c.esp"])).unwrap();
            let back = reactivate(&new, &records, None);
            assert_eq!(decode(&back), decode(old));
            assert_eq!(back, old);
            // and through the file that keeps the records
            let again = parse(render(&records).as_deref());
            assert_eq!(again, records);
        }
    }

    #[test]
    fn only_the_active_lines_of_the_named_plugins_go_and_nothing_else_changes() {
        let old = b"# list\r\n*A.esm\r\n*b.esp\r\n*C.esp\r\n";
        let (new, records) = deactivate(Some(old), &names(&["B.ESP"])).unwrap();
        assert_eq!(new, b"# list\r\n*A.esm\r\n*C.esp\r\n");
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].after, "*A.esm");
        assert!(deactivate(Some(old), &names(&["Z.esp"])).is_none());
        assert!(deactivate(None, &names(&["A.esm"])).is_none());
    }

    #[test]
    fn a_moved_neighbour_falls_back_to_the_load_order_and_then_the_end() {
        let rec = |after: &str, line: &str| Record {
            after: after.into(),
            line: line.into(),
            eol: "\r\n".into(),
        };
        let now = b"*A.esm\r\n*C.esp\r\n";
        let order = b"A.esm\r\nB.esp\r\nC.esp\r\n";
        assert_eq!(
            reactivate(now, &[rec("*Gone.esp", "*B.esp")], Some(order)),
            b"*A.esm\r\n*B.esp\r\n*C.esp\r\n"
        );
        assert_eq!(
            reactivate(now, &[rec("*Gone.esp", "*Z.esp")], Some(order)),
            b"*A.esm\r\n*C.esp\r\n*Z.esp\r\n"
        );
        // already listed: not added twice
        assert_eq!(reactivate(now, &[rec("*A.esm", "*C.esp")], None), now);
    }
}
