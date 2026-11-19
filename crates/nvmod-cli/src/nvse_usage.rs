// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod nvse-usage --instance <dir> [--profile <name>] [--jip-table <file>] [--nvse-names <file>] [--plugin-names <file>] [--json]`: which
//! extender opcodes the compiled scripts of the active plugins call, how often, and which plugins call them (G-01
//! of the nv-rs gap analysis). Read only. Nothing but opcode numbers, command names from the tables the caller
//! names, call counts and plugin file names is printed; script text, form data and record contents are never
//! read into the report and nothing is written anywhere.

use crate::{OK, REFUSED, USAGE, args::Args};
use nvmod_formats::{plugins::parse_plugin_list, scripts};
use nvmod_instance::NvmodInstance;
use nvmod_tools::Capabilities;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Write;
use std::path::{Path, PathBuf};

/// The schema of the `--json` output.
const SCHEMA: u32 = 1;
const USAGE_LINE: &str = "usage: nvmod nvse-usage --instance <dir> [--profile <name>] [--jip-table <file>] [--nvse-names <file>] [--plugin-names <file>] [--json]";
/// The first opcode of xNVSE; below it are the game's own commands.
const XNVSE_FIRST: u32 = 0x1400;
/// Largest plugin read, as a bound on memory.
const MAX_PLUGIN_BYTES: u64 = 1 << 30;

/// The owner of an opcode block.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Block {
    Xnvse,
    JipLn,
    Mcm,
    JohnnyGuitar,
    Knvse,
    ShowOff,
    Unknown,
}

impl Block {
    /// The block of `opcode`, or `None` below xNVSE's first opcode (the game's own).
    fn of(opcode: u32) -> Option<Block> {
        match opcode {
            ..XNVSE_FIRST => None,
            0x1400..=0x1fff => Some(Block::Xnvse),
            0x2200..=0x233a | 0x2600..=0x2942 => Some(Block::JipLn),
            0x21c0..=0x21c8 => Some(Block::Mcm),
            0x3100..=0x329b => Some(Block::JohnnyGuitar),
            0x3920..=0x3929 => Some(Block::Knvse),
            0x3c93..=0x3d74 => Some(Block::ShowOff),
            _ => Some(Block::Unknown),
        }
    }

    fn id(self) -> &'static str {
        match self {
            Block::Xnvse => "xnvse",
            Block::JipLn => "jip-ln",
            Block::Mcm => "mcm",
            Block::JohnnyGuitar => "johnnyguitar",
            Block::Knvse => "knvse",
            Block::ShowOff => "showoff",
            Block::Unknown => "unknown",
        }
    }

    /// Whether this tool has a name table (and a `capabilities.json` extender) for the block.
    fn has_table(self) -> bool {
        matches!(self, Block::Xnvse | Block::JipLn)
    }
}

/// Command names per opcode, from the files the caller names.
#[derive(Default)]
struct Names {
    by_opcode: BTreeMap<u32, Vec<String>>,
}

impl Names {
    /// The JIP LN table of the fork (`source-57.30.tsv`): opcode in hex, registration, name, macro, alias.
    fn add_jip(&mut self, text: &str) {
        for line in text.lines() {
            if line.starts_with('#') || line.starts_with("opcode") {
                continue;
            }
            let cols: Vec<&str> = line.split('\t').collect();
            let Some(opcode) = cols
                .first()
                .and_then(|c| u32::from_str_radix(c.trim(), 16).ok())
            else {
                continue;
            };
            if cols.get(1).is_none_or(|r| *r == "EMPTY") {
                continue;
            }
            let entry = self.by_opcode.entry(opcode).or_default();
            for name in [cols.get(2), cols.get(4)].into_iter().flatten() {
                if !name.is_empty() {
                    entry.push((*name).to_owned());
                }
            }
        }
    }

    /// A plain table for the plugins without one of the fork's (MCM, JohnnyGuitar, kNVSE, ShowOff): one
    /// `opcode<TAB>name` per line, the opcode in hex with or without `0x`; `#` lines and extra columns ignored.
    fn add_plain(&mut self, text: &str) {
        for line in text.lines() {
            if line.starts_with('#') {
                continue;
            }
            let mut cols = line.split('\t');
            let Some(opcode) = cols.next().map(|c| c.trim().trim_start_matches("0x")) else {
                continue;
            };
            let (Ok(opcode), Some(name)) = (u32::from_str_radix(opcode, 16), cols.next()) else {
                continue;
            };
            let name = name.trim();
            if !name.is_empty() {
                self.by_opcode
                    .entry(opcode)
                    .or_default()
                    .push(name.to_owned());
            }
        }
    }

    /// The xNVSE names of the fork (`names.rs`): `("Long", "Short"),` lines of `NAMES`, from opcode `0x1400`.
    fn add_nvse(&mut self, text: &str) {
        let mut opcode = XNVSE_FIRST;
        let mut inside = false;
        for line in text.lines() {
            let line = line.trim();
            if line.contains("static NAMES") {
                inside = true;
            } else if inside && line.starts_with("(\"") {
                let parts: Vec<&str> = line.split('"').collect();
                let entry = self.by_opcode.entry(opcode).or_default();
                for name in [parts.get(1), parts.get(3)].into_iter().flatten() {
                    if !name.is_empty() {
                        entry.push((*name).to_owned());
                    }
                }
                opcode += 1;
            } else if inside && line.starts_with("];") {
                break;
            }
        }
    }
}

/// What `capabilities.json` says is built, per extender id, as lowercase command names.
enum Built {
    Unavailable(&'static str),
    Known(BTreeMap<String, BTreeSet<String>>),
}

fn read_built(path: &Path) -> Built {
    match Capabilities::load(path) {
        Err(nvmod_tools::CapabilitiesError::Missing) => return Built::Unavailable("missing"),
        Err(nvmod_tools::CapabilitiesError::Invalid(_)) => return Built::Unavailable("invalid"),
        Ok(_) => {}
    }
    let value: serde_json::Value = std::fs::read_to_string(path)
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let mut map = BTreeMap::new();
    for extender in value["extenders"].as_array().into_iter().flatten() {
        let Some(id) = extender["id"].as_str() else {
            continue;
        };
        let names = extender["commands"]["implemented_names"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|n| n.as_str().map(str::to_lowercase))
            .collect();
        map.insert(id.to_owned(), names);
    }
    Built::Known(map)
}

#[derive(Serialize)]
struct OpcodeRow {
    opcode: String,
    block: &'static str,
    name: Option<String>,
    /// `built`, `missing`, `no-table` or `unchecked`.
    status: &'static str,
    calls: u64,
    plugins: Vec<String>,
}

#[derive(Serialize)]
struct Report {
    schema: u32,
    capabilities: &'static str,
    plugins_listed: usize,
    plugins_scanned: usize,
    scripts_scanned: u64,
    opcodes: Vec<OpcodeRow>,
}

fn find_plugin(data_dir: &Path, name: &str) -> Option<PathBuf> {
    let direct = data_dir.join(name.to_lowercase());
    if direct.is_file() {
        return Some(direct);
    }
    std::fs::read_dir(data_dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
        .filter(|p| p.is_file())
}

fn status(block: Block, names: &[String], built: &Built) -> &'static str {
    if !block.has_table() {
        return "no-table";
    }
    let Built::Known(map) = built else {
        return "unchecked";
    };
    let Some(implemented) = map.get(block.id()) else {
        return "unchecked";
    };
    if names.is_empty() {
        return "unchecked";
    }
    if names
        .iter()
        .any(|n| implemented.contains(&n.to_lowercase()))
    {
        "built"
    } else {
        "missing"
    }
}

/// Calls and calling plugin file names per opcode.
type Tally = BTreeMap<u32, (u64, BTreeSet<String>)>;

/// Read each listed plugin that is staged and count the extender opcodes of its compiled scripts. Returns the
/// tally, the number of plugins read and the number of scripts.
fn tally_plugins(list: &[String], data_dir: &Path) -> (Tally, usize, u64) {
    let mut tally = Tally::new();
    let mut scanned = 0;
    let mut scripts_scanned = 0u64;
    for plugin in list {
        let Some(path) = find_plugin(data_dir, plugin) else {
            continue;
        };
        let Ok(file) = std::fs::File::open(&path) else {
            continue;
        };
        let mut bytes = Vec::new();
        if std::io::Read::read_to_end(&mut std::io::Read::take(file, MAX_PLUGIN_BYTES), &mut bytes)
            .is_err()
        {
            continue;
        }
        scanned += 1;
        for script in scripts::compiled_scripts(&bytes) {
            scripts_scanned += 1;
            for opcode in scripts::command_calls(&script) {
                let opcode = u32::from(opcode);
                if Block::of(opcode).is_some() {
                    let entry = tally.entry(opcode).or_default();
                    entry.0 += 1;
                    entry.1.insert(plugin.clone());
                }
            }
        }
    }
    (tally, scanned, scripts_scanned)
}

fn rows_of(tally: Tally, names: &Names, built: &Built) -> Vec<OpcodeRow> {
    tally
        .into_iter()
        .filter_map(|(opcode, (calls, plugins))| {
            let block = Block::of(opcode)?;
            let known = names.by_opcode.get(&opcode).cloned().unwrap_or_default();
            Some(OpcodeRow {
                opcode: format!("0x{opcode:04X}"),
                block: block.id(),
                name: known.first().cloned(),
                status: status(block, &known, built),
                calls,
                plugins: plugins.into_iter().collect(),
            })
        })
        .collect()
}

fn refuse(err: &mut dyn Write, why: impl std::fmt::Display) -> u8 {
    let _ = writeln!(err, "nvmod nvse-usage: {why}");
    REFUSED
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(
        args,
        &["json"],
        &["instance", "profile", "jip-table", "nvse-names", "plugin-names"],
    ) {
        Ok(a) if a.positional.is_empty() && a.option("instance").is_some() => a,
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_LINE}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod nvse-usage: {why}\n{USAGE_LINE}");
            return USAGE;
        }
    };
    let root = Path::new(args.option("instance").unwrap_or_default());
    let instance = match NvmodInstance::open_read_only(root) {
        Ok(i) => i,
        Err(why) => return refuse(err, why),
    };
    let profile = match instance.resolve_profile(args.option("profile")) {
        Ok(p) => p,
        Err(why) => return refuse(err, why),
    };
    let mut names = Names::default();
    for (option, add) in [
        ("jip-table", Names::add_jip as fn(&mut Names, &str)),
        ("nvse-names", Names::add_nvse),
        ("plugin-names", Names::add_plain),
    ] {
        if let Some(file) = args.option(option) {
            match std::fs::read_to_string(file) {
                Ok(text) => add(&mut names, &text),
                Err(e) => return refuse(err, format!("--{option} {file}: {e}")),
            }
        }
    }
    let list = match std::fs::read(instance.profile_dir(&profile).join("plugins.txt")) {
        Ok(bytes) => parse_plugin_list(&bytes),
        Err(e) => {
            return refuse(
                err,
                format!("profile `{profile}` has no readable plugins.txt: {e}"),
            );
        }
    };
    let data_dir = instance.staging_dir(&profile).join("data");
    if !data_dir.is_dir() {
        return refuse(
            err,
            format!("profile `{profile}` is not staged; run `nvmod stage` first"),
        );
    }
    let built = read_built(&instance.nv_rs_capabilities());

    let (tally, scanned, scripts_scanned) = tally_plugins(&list, &data_dir);
    let mut rows = rows_of(tally, &names, &built);
    rows.sort_by(|a, b| b.calls.cmp(&a.calls).then_with(|| a.opcode.cmp(&b.opcode)));
    let report = Report {
        schema: SCHEMA,
        capabilities: match &built {
            Built::Unavailable(why) => why,
            Built::Known(_) => "used",
        },
        plugins_listed: list.len(),
        plugins_scanned: scanned,
        scripts_scanned,
        opcodes: rows,
    };
    if args.flag("json") {
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&report).unwrap_or_default()
        );
    } else {
        print_text(out, &report);
    }
    OK
}

fn print_text(out: &mut dyn Write, report: &Report) {
    let _ = writeln!(
        out,
        "nvmod nvse-usage: {} of {} listed plugin(s) read, {} compiled script(s), capabilities.json: {}",
        report.plugins_scanned, report.plugins_listed, report.scripts_scanned, report.capabilities
    );
    let _ = writeln!(
        out,
        "{:>7}  {:<8} {:<13} {:<10} {:<30} plugins",
        "calls", "opcode", "block", "status", "name"
    );
    for row in &report.opcodes {
        let _ = writeln!(
            out,
            "{:>7}  {:<8} {:<13} {:<10} {:<30} {}",
            row.calls,
            row.opcode,
            row.block,
            row.status,
            row.name.as_deref().unwrap_or("-"),
            row.plugins.join(", ")
        );
    }
}
