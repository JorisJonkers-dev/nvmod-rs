// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod lod plan|generate --instance <dir> [--profile <name>] ...`: LOD through lodwright-rs, a pinned subprocess.
//!
//! `plan` builds the request (the active plugins in Load order, the Staging tree's `Data` folder and the settings;
//! no Instance type), runs the pinned `lodwright-rs plan` through the one verified runner, validates what it prints
//! against the vendored `lodwright-plan` contract (a plan of an unknown schema version is refused) and prints the
//! planned blocks. It is read only: nothing is written into the Instance. `generate` exits 2 with the reason until a
//! lodwright-rs release can generate (`docs/architecture/integration.md`, gate L2).

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, args::Args};
use nvmod_instance::NvmodInstance;
use nvmod_loadorder::{Error, read_slots, staging_data};
use nvmod_tools::lodwright::{self, Request};
use nvmod_tools::{
    Call, Lock, Negotiation, Runner, Store, StoreState, ToolEntry, ToolError, builtin,
    builtin_lock, call_negotiated, pin_of,
};
use serde_json::{Value, json};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

const USAGE_LINE: &str = "usage: nvmod lod plan|generate --instance <dir> [--profile <name>] [--world <name>] [--blocks] [--json]\n       tool options: [--lodwright <path>] [--lock <file>] [--store <dir>] [--timeout-secs <n>]";
const OPTIONS: &[&str] = &[
    "instance",
    "profile",
    "world",
    "lodwright",
    "lock",
    "store",
    "timeout-secs",
];
/// A plan reads every plugin of the Load order once; ten minutes covers a large one.
const DEFAULT_TIMEOUT_SECS: u64 = 600;

fn refused(err: &mut dyn Write, why: &str) -> u8 {
    let _ = writeln!(err, "nvmod lod: refused: {why}");
    REFUSED
}

/// The lock entry of lodwright-rs, or a reason.
fn entry_of(lock: &Lock) -> Result<&ToolEntry, String> {
    lock.tool(lodwright::TOOL).ok_or_else(|| {
        format!(
            "the lock file pins no tool called `{}`: no lodwright-rs release carries a binary with a checksum yet, \
             so nothing can be run (docs/DECISIONS.md)",
            lodwright::TOOL
        )
    })
}

fn load_lock(args: &Args) -> Result<Lock, String> {
    match args.option("lock") {
        None => Ok(builtin_lock()),
        Some(path) => {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{path}: {e}"))?;
            Lock::parse(&text)
        }
    }
}

/// The program and its pinned checksum.
fn program_of(args: &Args, entry: &ToolEntry) -> Result<(PathBuf, String), String> {
    let asset = entry.asset_here().ok_or_else(|| {
        format!(
            "{} {} pins no file for {}",
            entry.name,
            entry.version,
            nvmod_tools::platform()
        )
    })?;
    let sha256 = pin_of(entry, asset, None)
        .sha256()
        .map(str::to_owned)
        .ok_or_else(|| format!("{} {} pins no checksum", entry.name, entry.version))?;
    let program = if let Some(path) = args.option("lodwright") {
        PathBuf::from(path)
    } else {
        let store = Store::new(
            args.option("store")
                .map_or_else(Store::default_location, PathBuf::from),
        );
        if store.state(&sha256, &entry.name) == StoreState::Missing {
            return Err(format!(
                "{} {} is not fetched; run `nvmod tools fetch {}` or give --lodwright <path>",
                entry.name, entry.version, entry.name
            ));
        }
        store.path(&sha256, &entry.name)
    };
    Ok((program, sha256))
}

fn tool_failed(err: &mut dyn Write, e: &ToolError) -> u8 {
    let code = e.exit_code();
    let _ = match e {
        ToolError::Unverifiable { .. } => {
            writeln!(err, "nvmod lod: verification failed: {e}; nothing was run")
        }
        ToolError::Refused { .. } => writeln!(err, "nvmod lod: refused: {e}"),
        _ => writeln!(err, "nvmod lod: the tool failed: {e}"),
    };
    code
}

fn generate(args: &Args, err: &mut dyn Write) -> u8 {
    let lock = match load_lock(args) {
        Ok(l) => l,
        Err(why) => return refused(err, &why),
    };
    let why = match entry_of(&lock) {
        Err(why) => why,
        Ok(entry) => format!(
            "{} {} {}: LOD generation waits for the writers and the real-data comparison (gate L2); \
             use the xLODGen route outside nvmod until then",
            entry.name,
            entry.version,
            if entry.contracts.contains_key(lodwright::GENERATE_CONTRACT) {
                "speaks `lodwright-generate`, but this nvmod does not drive it yet"
            } else {
                "reports that it cannot generate yet (it speaks no `lodwright-generate` contract)"
            }
        ),
    };
    refused(err, &why)
}

fn str_of(v: &Value) -> &str {
    v.as_str().unwrap_or_default()
}

fn print_text(out: &mut dyn Write, tool: &ToolEntry, plan: &Value, blocks: bool) {
    let worlds = plan["worlds"].as_array().map_or(&[][..], Vec::as_slice);
    let _ = writeln!(
        out,
        "nvmod lod plan: {} worldspace(s), planned by {} {}; nothing was written",
        worlds.len(),
        tool.name,
        tool.version
    );
    for world in worlds {
        let bounds = if world["bounds"].is_null() {
            "no bounds".to_owned()
        } else {
            let b = &world["bounds"];
            format!(
                "cells {}..{} x {}..{}",
                b["min_x"], b["max_x"], b["min_y"], b["max_y"]
            )
        };
        let _ = writeln!(
            out,
            "{} ({}): {} cell(s), {bounds}, max seam error {}",
            str_of(&world["name"]),
            str_of(&world["form_id"]),
            world["cells"],
            world["max_seam_error"]
        );
        for level in world["levels"].as_array().into_iter().flatten() {
            let _ = writeln!(
                out,
                "  level {}: {} block(s), {} vertices, {} triangles, max error {}",
                level["level"],
                level["blocks"],
                level["vertices"],
                level["triangles"],
                level["max_error"]
            );
        }
        if blocks {
            for b in world["blocks"].as_array().into_iter().flatten() {
                let _ = writeln!(
                    out,
                    "  block level {} at ({}, {}): {}",
                    b["level"],
                    b["x"],
                    b["y"],
                    str_of(&b["mesh"])
                );
            }
        }
    }
}

#[allow(clippy::too_many_lines)] // one flow: refuse, build the request, run, validate, print
pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let usage = |err: &mut dyn Write, why: Option<&str>| {
        let _ = match why {
            Some(why) => writeln!(err, "nvmod lod: {why}\n{USAGE_LINE}"),
            None => writeln!(err, "{USAGE_LINE}"),
        };
        USAGE
    };
    let args = match Args::parse(args, &["json", "blocks"], OPTIONS) {
        Ok(a)
            if a.positional.len() == 1
                && matches!(a.positional[0].as_str(), "plan" | "generate")
                && a.option("instance").is_some() =>
        {
            a
        }
        Ok(_) => return usage(err, None),
        Err(why) => return usage(err, Some(&why)),
    };
    let Some(secs) = args
        .option("timeout-secs")
        .map_or(Some(DEFAULT_TIMEOUT_SECS), |v| v.parse::<u64>().ok())
    else {
        return usage(err, Some("--timeout-secs needs a number"));
    };
    if args.positional[0] == "generate" {
        return generate(&args, err);
    }

    let root = Path::new(args.option("instance").unwrap_or_default());
    let instance = match NvmodInstance::open_read_only(root) {
        Ok(i) => i,
        Err(e) => {
            return refused(err, &crate::install_mod::describe_open_error(root, &e));
        }
    };
    let profile = match instance.resolve_profile(args.option("profile")) {
        Ok(p) => p,
        Err(e) => return refused(err, &e.to_string()),
    };
    let data = match staging_data(&instance, &profile) {
        Ok(d) => d,
        Err(Error::Refused(why) | Error::Failed(why)) => return refused(err, &why),
    };
    let plugins: Vec<String> = match read_slots(&instance.profile_dir(&profile)) {
        Ok(slots) => slots
            .into_iter()
            .filter(|s| s.active)
            .map(|s| s.name)
            .collect(),
        Err(Error::Refused(why) | Error::Failed(why)) => return refused(err, &why),
    };
    if plugins.is_empty() {
        return refused(err, &format!("profile `{profile}` has no active plugin"));
    }
    let lock = match load_lock(&args) {
        Ok(l) => l,
        Err(why) => return refused(err, &why),
    };
    let entry = match entry_of(&lock) {
        Ok(e) => e,
        Err(why) => return refused(err, &why),
    };
    let (program, sha256) = match program_of(&args, entry) {
        Ok(p) => p,
        Err(why) => return refused(err, &why),
    };
    let request = Request {
        data: std::fs::canonicalize(&data).map_or(data, |p| nvmod_core::plain_path(&p)),
        plugins,
        world: args.option("world").map(str::to_owned),
        blocks: args.flag("blocks"),
    };
    let words = request.plan_args();
    let runner = Runner {
        timeout: Duration::from_secs(secs),
        ..Runner::default()
    };
    let mut call = Call::new(&entry.name, &program, &words);
    call.expected_sha256 = Some(&sha256);
    let Some(contract) = builtin(lodwright::PLAN_CONTRACT) else {
        let _ = writeln!(
            err,
            "nvmod lod: the lodwright-plan contract is not built in"
        );
        return ROLLED_BACK;
    };
    // A pin from the release that has `contracts --json` on is negotiated, clamped to the lock's version; an
    // earlier pin is grandfathered (ADR-0006).
    let negotiation = Negotiation {
        lock_version: entry
            .contracts
            .get(lodwright::PLAN_CONTRACT)
            .copied()
            .unwrap_or_else(|| contract.current()),
        required: entry.handshake_required(),
    };
    let (version, plan) = match call_negotiated(&runner, &call, &contract, negotiation) {
        Ok(r) => r,
        Err(e) => return tool_failed(err, &e),
    };
    if args.flag("json") {
        let doc = json!({
            "status": "planned",
            "profile": profile,
            "tool": { "name": entry.name, "version": entry.version },
            "schema_version": version,
            "request": request.to_json(),
            "plan": plan,
        });
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&doc).unwrap_or_default()
        );
    } else {
        print_text(out, entry, &plan, request.blocks);
    }
    OK
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usage_is_exit_5() {
        let (mut out, mut err) = (Vec::new(), Vec::new());
        for words in [&[][..], &["plan"], &["--nope"], &["fly", "--instance", "x"]] {
            let args: Vec<String> = words.iter().map(|w| (*w).to_owned()).collect();
            assert_eq!(run(&args, &mut out, &mut err), USAGE, "{words:?}");
        }
    }
}
