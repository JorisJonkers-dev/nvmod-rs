// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod tools list|verify|fetch|trust|run`: the pinned tools of `tools.lock.toml`, the verified fetch, and the one
//! runner. The lock file defaults to the one shipped with nvmod; `--lock` names another (the tests point it at a
//! fake release server). Tools are kept by content hash in the store (`--store`, `NVMOD_TOOLS_DIR`, or the user's
//! data folder).

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, VERIFICATION, args::Args};
use nvmod_instance::NvmodInstance;
use nvmod_tools::{
    Acquire, AcquireError, Call, Curl, Kind, Lock, Pin, Runner, Store, StoreState, ToolEntry,
    Trust, builtin_lock, check_sha256, fetch_bytes, pin_of, sha256_hex,
};
use serde_json::json;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

const USAGE_LINE: &str = "usage: nvmod tools list|verify [--json] | fetch <tool> | trust <tool> <version> --instance <dir> [--sha256 <hex>] | run <tool> -- <args...>\n       common options: [--lock <file>] [--store <dir>] [--instance <dir>] [--gh <path>] [--curl <path>] [--retry-delay-ms <n>]";
const OPTIONS: &[&str] = &[
    "lock",
    "store",
    "instance",
    "gh",
    "curl",
    "retry-delay-ms",
    "sha256",
    "timeout-secs",
];

/// Everything one `tools` command needs, read from its options.
struct Context {
    args: Args,
    lock: Lock,
    store: Store,
    trust: Option<Trust>,
    instance: Option<PathBuf>,
    json: bool,
}

fn usage(err: &mut dyn Write, why: Option<&str>) -> u8 {
    match why {
        Some(why) => {
            let _ = writeln!(err, "nvmod tools: {why}\n{USAGE_LINE}");
        }
        None => {
            let _ = writeln!(err, "{USAGE_LINE}");
        }
    }
    USAGE
}

fn refused(err: &mut dyn Write, why: &str) -> u8 {
    let _ = writeln!(err, "nvmod tools: refused: {why}");
    REFUSED
}

fn context(args: Args, err: &mut dyn Write) -> Result<Context, u8> {
    let lock = match args.option("lock") {
        None => builtin_lock(),
        Some(path) => {
            let text =
                std::fs::read_to_string(path).map_err(|e| refused(err, &format!("{path}: {e}")))?;
            Lock::parse(&text).map_err(|why| refused(err, &why))?
        }
    };
    let store = Store::new(
        args.option("store")
            .map_or_else(Store::default_location, PathBuf::from),
    );
    let instance = args.option("instance").map(PathBuf::from);
    let trust = match &instance {
        None => None,
        Some(root) => {
            NvmodInstance::open(root)
                .map_err(|e| refused(err, &crate::install_mod::describe_open_error(root, &e)))?;
            Some(Trust::load(root).map_err(|why| refused(err, &why))?)
        }
    };
    let json = args.flag("json");
    Ok(Context {
        args,
        lock,
        store,
        trust,
        instance,
        json,
    })
}

fn delay(args: &Args) -> Option<Duration> {
    args.option("retry-delay-ms")
        .map_or(Some(1000), |v| v.parse::<u64>().ok())
        .map(Duration::from_millis)
}

/// One row of `list` and `verify`.
struct Row {
    name: String,
    version: String,
    state: &'static str,
    detail: String,
}

fn row(entry: &ToolEntry, ctx: &Context) -> Row {
    let make = |state, detail: String| Row {
        name: entry.name.clone(),
        version: entry.version.clone(),
        state,
        detail,
    };
    if entry.kind == Kind::Linked {
        let tag = entry.source.as_ref().map_or("", |s| s.tag.as_str());
        return make("linked", format!("(tag {tag})"));
    }
    let Some(asset) = entry.asset_here() else {
        return make(
            "no-asset",
            format!("(no file pinned for {})", nvmod_tools::platform()),
        );
    };
    let (Pin::Lock(hash) | Pin::Trusted(hash)) = pin_of(entry, asset, ctx.trust.as_ref()) else {
        return make(
            "needs-trust",
            "(no pinned checksum; run `nvmod tools trust`)".to_owned(),
        );
    };
    match ctx.store.state(hash, &entry.name) {
        StoreState::Missing => make("not fetched", String::new()),
        StoreState::Verified => make("verified", format!("({})", &hash[..12])),
        StoreState::Corrupt(actual) => make(
            "corrupt",
            format!("(the stored file has SHA-256 {actual}, expected {hash})"),
        ),
    }
}

fn print_rows(rows: &[Row], json: bool, out: &mut dyn Write) {
    if json {
        let value: Vec<_> = rows
            .iter()
            .map(|r| json!({"name": r.name, "version": r.version, "state": r.state, "detail": r.detail}))
            .collect();
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
        return;
    }
    for r in rows {
        let _ = writeln!(out, "{} {} {} {}", r.name, r.version, r.state, r.detail);
    }
}

fn list_or_verify(ctx: &Context, verify: bool, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let rows: Vec<Row> = ctx.lock.tools.iter().map(|t| row(t, ctx)).collect();
    print_rows(&rows, ctx.json, out);
    let bad: Vec<&Row> = rows.iter().filter(|r| r.state == "corrupt").collect();
    if verify && !bad.is_empty() {
        for r in bad {
            let _ = writeln!(
                err,
                "nvmod tools: {} {} is corrupt {}; it will not be run",
                r.name, r.version, r.detail
            );
        }
        return VERIFICATION;
    }
    OK
}

fn tool<'a>(ctx: &'a Context, err: &mut dyn Write) -> Result<&'a ToolEntry, u8> {
    let name = ctx.args.positional.first().map_or("", String::as_str);
    ctx.lock
        .tool(name)
        .ok_or_else(|| refused(err, &format!("the lock file pins no tool called `{name}`")))
}

fn acquire_failed(err: &mut dyn Write, e: &AcquireError) -> u8 {
    match e {
        AcquireError::Refused(why) => refused(err, why),
        AcquireError::Verification(why) => {
            let _ = writeln!(
                err,
                "nvmod tools: verification failed: {why}\nnothing was installed"
            );
            VERIFICATION
        }
        AcquireError::Failed(why) => {
            let _ = writeln!(
                err,
                "nvmod tools: step failed, nothing was installed: {why}"
            );
            ROLLED_BACK
        }
    }
}

fn fetch(ctx: &Context, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let entry = match tool(ctx, err) {
        Ok(e) => e,
        Err(code) => return code,
    };
    let Some(retry_delay) = delay(&ctx.args) else {
        return usage(err, Some("--retry-delay-ms needs a number"));
    };
    let curl = Curl {
        program: ctx
            .args
            .option("curl")
            .map_or_else(|| PathBuf::from("curl"), PathBuf::from),
    };
    let gh = ctx
        .args
        .option("gh")
        .map_or_else(|| PathBuf::from("gh"), PathBuf::from);
    let acquire = Acquire {
        fetcher: &curl,
        gh: &gh,
        retry_delay,
        store: &ctx.store,
        trust: ctx.trust.as_ref(),
    };
    match acquire.fetch(entry) {
        Ok(done) => {
            if ctx.json {
                let _ = writeln!(
                    out,
                    "{}",
                    json!({"tool": entry.name, "version": entry.version, "path": done.path.display().to_string(),
                           "sha256": done.sha256, "fetched": done.fetched, "checks": done.checks})
                );
            } else {
                let _ = writeln!(
                    out,
                    "nvmod tools: {} {} is at {}",
                    entry.name,
                    entry.version,
                    done.path.display()
                );
                for check in &done.checks {
                    let _ = writeln!(out, "  {check}");
                }
            }
            OK
        }
        Err(e) => acquire_failed(err, &e),
    }
}

fn trust(ctx: &Context, out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let (Some(name), Some(version)) = (ctx.args.positional.first(), ctx.args.positional.get(1))
    else {
        return usage(err, None);
    };
    let (Some(root), Some(known)) = (&ctx.instance, &ctx.trust) else {
        return usage(err, Some("trust needs --instance <dir>"));
    };
    let Some(entry) = ctx.lock.tool(name).filter(|e| &e.version == version) else {
        return refused(err, &format!("the lock file pins no {name} {version}"));
    };
    let Some(asset) = entry.asset_here() else {
        return refused(
            err,
            &format!(
                "{name} {version} pins no file for {}",
                nvmod_tools::platform()
            ),
        );
    };
    if asset.sha256.is_some() {
        return refused(
            err,
            &format!(
                "{name} {version} has a checksum pinned in the lock file; a confirmation is only for tools without one"
            ),
        );
    }
    let Some(retry_delay) = delay(&ctx.args) else {
        return usage(err, Some("--retry-delay-ms needs a number"));
    };
    let curl = Curl {
        program: ctx
            .args
            .option("curl")
            .map_or_else(|| PathBuf::from("curl"), PathBuf::from),
    };
    let bytes = match fetch_bytes(&curl, &asset.url, retry_delay) {
        Ok(b) => b,
        Err(e) => return acquire_failed(err, &e.into()),
    };
    let seen = sha256_hex(&bytes);
    let Some(confirmed) = ctx.args.option("sha256") else {
        let _ = writeln!(
            out,
            "{name} {version}: the download has SHA-256 {seen}\nnothing was recorded; check that value against the publisher, then run again with --sha256 {seen}"
        );
        return REFUSED;
    };
    if let Err(e) = check_sha256(name, "the checksum you gave", &bytes, confirmed) {
        return acquire_failed(err, &e.into());
    }
    let mut known = known.clone();
    match known.record(root, name, version, &seen) {
        Ok(()) => {
            let _ = writeln!(
                out,
                "nvmod tools: recorded {name} {version} with SHA-256 {seen} in {}",
                Trust::path(root).display()
            );
            OK
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod tools: {why}");
            ROLLED_BACK
        }
    }
}

fn run_tool(ctx: &Context, tool_args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let entry = match tool(ctx, err) {
        Ok(e) => e,
        Err(code) => return code,
    };
    let Some(asset) = entry.asset_here() else {
        return refused(
            err,
            &format!(
                "{} pins no file for {}",
                entry.name,
                nvmod_tools::platform()
            ),
        );
    };
    let pin = pin_of(entry, asset, ctx.trust.as_ref());
    let Some(hash) = pin.sha256() else {
        return refused(
            err,
            &format!(
                "{} {} has no pinned checksum and no confirmation",
                entry.name, entry.version
            ),
        );
    };
    let path = ctx.store.path(hash, &entry.name);
    if ctx.store.state(hash, &entry.name) == StoreState::Missing {
        return refused(
            err,
            &format!(
                "{} {} is not fetched; run `nvmod tools fetch {}`",
                entry.name, entry.version, entry.name
            ),
        );
    }
    let Some(secs) = ctx
        .args
        .option("timeout-secs")
        .map_or(Some(300), |v| v.parse::<u64>().ok())
    else {
        return usage(err, Some("--timeout-secs needs a number"));
    };
    let runner = Runner {
        timeout: Duration::from_secs(secs),
        ..Runner::default()
    };
    let mut call = Call::new(&entry.name, &path, tool_args);
    call.expected_sha256 = Some(hash);
    match runner.run(&call) {
        Ok(output) => {
            let _ = out.write_all(&output.stdout);
            OK
        }
        Err(e) => {
            let _ = writeln!(err, "nvmod tools: {e}");
            e.exit_code()
        }
    }
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    // Everything after `--` belongs to the tool.
    let split = args.iter().position(|a| a == "--");
    let (own, tool_args) = match split {
        Some(at) => (&args[..at], &args[at + 1..]),
        None => (args, &args[args.len()..]),
    };
    let Some((verb, rest)) = own.split_first() else {
        return usage(err, None);
    };
    let args = match Args::parse(rest, &["json"], OPTIONS) {
        Ok(a) => a,
        Err(why) => return usage(err, Some(&why)),
    };
    let wanted = match verb.as_str() {
        "list" | "verify" => args.positional.is_empty(),
        "fetch" => args.positional.len() == 1,
        "trust" => args.positional.len() == 2,
        "run" => args.positional.len() == 1 && split.is_some(),
        _ => return usage(err, Some(&format!("unknown command `{verb}`"))),
    };
    if !wanted || (verb != "run" && split.is_some()) {
        return usage(err, None);
    }
    let ctx = match context(args, err) {
        Ok(c) => c,
        Err(code) => return code,
    };
    match verb.as_str() {
        "list" => list_or_verify(&ctx, false, out, err),
        "verify" => list_or_verify(&ctx, true, out, err),
        "fetch" => fetch(&ctx, out, err),
        "trust" => trust(&ctx, out, err),
        _ => run_tool(&ctx, tool_args, out, err),
    }
}
