// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod sort --instance <dir> [--profile <name>] --masterlist <file> [--apply] [--json]`, and
//! `nvmod sort --instance <dir> [--profile <name>] --keep-order [--json]`, which keeps the Load order as it is and
//! only writes the plugin file times (what an imported MO2 order needs).

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, args::Args};
use nvmod_instance::NvmodInstance;
use nvmod_loadorder::{Applied, Proposal};
use std::io::Write;
use std::path::Path;

const USAGE_TEXT: &str = "usage: nvmod sort --instance <dir> [--profile <name>] (--masterlist <file> | --fetch-masterlist)
                 [--apply] [--json]
       nvmod sort --instance <dir> [--profile <name>] --keep-order [--json]   (keep the order, write only the plugin file times)
       masterlist source options: [--lock <file>] [--store <dir>] [--gh <path>] [--curl <path>]";

/// The tool of `tools.lock.toml` that pins the masterlist.
const MASTERLIST_TOOL: &str = "loot-masterlist-falloutnv";

/// The masterlist of the pinned source, fetched (checksum verified) into the tool store when it is not there yet.
/// The error is the exit code and the reason: a missing pin is a refusal (2), a bad checksum 3, a failed fetch 4.
fn fetch_masterlist(
    args: &Args,
    instance: &NvmodInstance,
) -> Result<std::path::PathBuf, (u8, String)> {
    use nvmod_tools::{Acquire, AcquireError, Curl, Lock, Store, Trust, builtin_lock};
    let refused = |why: String| (REFUSED, why);
    let lock = match args.option("lock") {
        None => builtin_lock(),
        Some(path) => {
            let text =
                std::fs::read_to_string(path).map_err(|e| refused(format!("{path}: {e}")))?;
            Lock::parse(&text).map_err(refused)?
        }
    };
    let entry = lock.tool(MASTERLIST_TOOL).ok_or_else(|| {
        refused(format!(
            "the lock file pins no tool called `{MASTERLIST_TOOL}`, so no masterlist source is pinned: \
             give --masterlist <file> instead (docs/DECISIONS.md)"
        ))
    })?;
    let store = Store::new(
        args.option("store")
            .map_or_else(Store::default_location, std::path::PathBuf::from),
    );
    let curl = Curl {
        program: args
            .option("curl")
            .map_or_else(|| "curl".into(), std::path::PathBuf::from),
    };
    let gh = args
        .option("gh")
        .map_or_else(|| "gh".into(), std::path::PathBuf::from);
    let trust = Trust::load(&instance.root).map_err(refused)?;
    let acquire = Acquire {
        fetcher: &curl,
        gh: &gh,
        retry_delay: std::time::Duration::from_millis(
            args.option("retry-delay-ms")
                .and_then(|v| v.parse().ok())
                .unwrap_or(1000),
        ),
        store: &store,
        trust: Some(&trust),
    };
    acquire
        .fetch(entry)
        .map(|done| done.path)
        .map_err(|e| match &e {
            AcquireError::Refused(why) => (REFUSED, why.clone()),
            AcquireError::Verification(why) => {
                (crate::VERIFICATION, format!("verification failed: {why}"))
            }
            AcquireError::Failed(why) => (
                ROLLED_BACK,
                format!("the masterlist could not be fetched: {why}"),
            ),
        })
}

/// The JSON word of a finding kind: loot-rs's label without spaces, as nvmod has always written it.
fn finding_name(kind: loot_rs::api::FindingKind) -> String {
    kind.label().replace(' ', "")
}

fn proposal_json(proposal: &Proposal, applied: Option<&Applied>) -> serde_json::Value {
    let output = &proposal.output;
    let order: Vec<_> = proposal
        .proposed
        .iter()
        .enumerate()
        .map(|(index, slot)| {
            let result = output
                .plugins
                .iter()
                .find(|p| p.name.eq_ignore_ascii_case(&slot.name));
            let messages = |list: &[loot_rs::api::Message]| -> Vec<_> {
                list.iter()
                    .map(|m| serde_json::json!({ "kind": m.kind.label(), "text": m.text }))
                    .collect()
            };
            serde_json::json!({
                "position": index + 1,
                "name": slot.name,
                "active": slot.active,
                "installed": result.is_some(),
                "group": result.map(|r| r.group.clone()),
                "messages": result.map_or_default(|r| messages(&r.messages)),
                "generated_messages": result.map_or_default(|r| messages(&r.generated_messages)),
                "findings": result.map_or_default(|r| r.findings.iter().map(|f| serde_json::json!({
                    "kind": finding_name(f.kind), "text": f.text, "hard": f.hard,
                })).collect::<Vec<_>>()),
            })
        })
        .collect();
    serde_json::json!({
        "profile": proposal.profile,
        "reordered": proposal.reordered(),
        "positions": { "changed": proposal.positions_changed(), "total": proposal.proposed.len() },
        "order": order,
        "general_messages": output.general_messages.iter()
            .chain(&output.generated_general_messages)
            .map(|m| serde_json::json!({ "kind": m.kind.label(), "text": m.text }))
            .collect::<Vec<_>>(),
        "not_installed": output.not_installed,
        "warnings": output.warnings,
        "applied": applied.map(|a| serde_json::json!({
            "lists_written": a.lists_written,
            "stamped": a.stamped,
            "detached": a.detached,
            "journal": a.journal,
        })),
    })
}

fn print_text(out: &mut dyn Write, proposal: &Proposal, applied: Option<&Applied>) {
    let _ = writeln!(
        out,
        "nvmod sort: profile `{}`, {} plugin(s), {}",
        proposal.profile,
        proposal.proposed.len(),
        if proposal.reordered() {
            "the Load order changes"
        } else {
            "the Load order is already sorted"
        }
    );
    let _ = writeln!(
        out,
        "{} of {} position(s) would change",
        proposal.positions_changed(),
        proposal.proposed.len()
    );
    let output = &proposal.output;
    for (index, slot) in proposal.proposed.iter().enumerate() {
        let note = if output
            .not_installed
            .iter()
            .any(|n| n.eq_ignore_ascii_case(&slot.name))
        {
            " (not installed)"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "{:>4} {} {}{note}",
            index + 1,
            if slot.active { '*' } else { ' ' },
            slot.name
        );
        let Some(result) = output
            .plugins
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(&slot.name))
        else {
            continue;
        };
        for m in result.messages.iter().chain(&result.generated_messages) {
            let _ = writeln!(out, "       [{}] {}", m.kind.label(), m.text);
        }
        for f in &result.findings {
            let _ = writeln!(
                out,
                "       [{}] {}",
                if f.hard { "problem" } else { "note" },
                f.text
            );
        }
    }
    for m in output
        .general_messages
        .iter()
        .chain(&output.generated_general_messages)
    {
        let _ = writeln!(out, "message [{}]: {}", m.kind.label(), m.text);
    }
    for w in &output.warnings {
        let _ = writeln!(out, "warning: {w}");
    }
    match applied {
        None => {
            let _ = writeln!(out, "nothing was written; pass --apply to write it");
        }
        Some(a) if a.journal.is_none() => {
            let _ = writeln!(out, "nothing to change: the lists and file times are right");
        }
        Some(a) => {
            let _ = writeln!(
                out,
                "applied: plugins.txt and loadorder.txt {}; {} plugin file time(s) set ({} link(s) replaced by copies); undo with `nvmod rollback`",
                if a.lists_written {
                    "written"
                } else {
                    "unchanged"
                },
                a.stamped,
                a.detached
            );
        }
    }
}

/// `--keep-order`: the Load order stays exactly as the profile has it; only the plugin file times of the Staging
/// tree are written (and the lists, if their bytes differ), so nv-rs, which orders by file time, loads the order.
fn keep_order(
    args: &Args,
    root: &Path,
    profile: &str,
    out: &mut dyn Write,
    err: &mut dyn Write,
) -> u8 {
    let request = nvmod_api::sort_apply::KeepOrderRequest {
        instance: root.to_path_buf(),
        profile: Some(profile.to_owned()),
    };
    let done = match nvmod_api::sort_apply::keep_order(
        &request,
        &mut nvmod_api::NoProgress,
        &nvmod_api::CancelToken::new(),
    ) {
        Ok(nvmod_api::Outcome::Done(done)) => done,
        Ok(nvmod_api::Outcome::Cancelled) => return REFUSED,
        Err(e) => return crate::job_failure("sort", &e, err),
    };
    if args.flag("json") {
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "profile": done.profile,
                "kept_order": true,
                "plugins": done.plugins,
                "applied": {
                    "lists_written": done.applied.lists_written,
                    "stamped": done.applied.stamped,
                    "detached": done.applied.detached,
                    "journal": done.applied.journal,
                },
            }))
            .unwrap_or_default()
        );
    } else {
        let _ = writeln!(
            out,
            "nvmod sort: profile `{}`, {} plugin(s), the Load order is kept; {} plugin file time(s) set ({} link(s) replaced by copies)",
            done.profile, done.plugins, done.applied.stamped, done.applied.detached
        );
    }
    OK
}

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(
        args,
        &["apply", "json", "fetch-masterlist", "keep-order"],
        &[
            "instance",
            "profile",
            "masterlist",
            "lock",
            "store",
            "gh",
            "curl",
            "retry-delay-ms",
        ],
    ) {
        Ok(a)
            if a.positional.is_empty()
                && a.option("instance").is_some()
                && if a.flag("keep-order") {
                    a.option("masterlist").is_none()
                        && !a.flag("fetch-masterlist")
                        && !a.flag("apply")
                } else {
                    a.option("masterlist").is_some() != a.flag("fetch-masterlist")
                } =>
        {
            a
        }
        Ok(_) => {
            let _ = writeln!(err, "{USAGE_TEXT}");
            return USAGE;
        }
        Err(why) => {
            let _ = writeln!(err, "nvmod sort: {why}");
            return USAGE;
        }
    };
    let refused = |why: &str, err: &mut dyn Write| {
        let _ = writeln!(err, "nvmod sort: refused: {why}");
        REFUSED
    };
    let root = Path::new(args.option("instance").unwrap_or_default());
    let instance = match NvmodInstance::open(root) {
        Ok(i) => i,
        Err(e) => {
            return refused(&crate::install_mod::describe_open_error(root, &e), err);
        }
    };
    let profile = match instance.resolve_profile(args.option("profile")) {
        Ok(p) => p,
        Err(e) => return refused(&e.to_string(), err),
    };
    if args.flag("keep-order") {
        return keep_order(&args, root, &profile, out, err);
    }
    let masterlist_path = if args.flag("fetch-masterlist") {
        match fetch_masterlist(&args, &instance) {
            Ok(path) => path,
            Err((code, why)) => {
                let _ = writeln!(
                    err,
                    "nvmod sort: {}{why}",
                    if code == REFUSED { "refused: " } else { "" }
                );
                return code;
            }
        }
    } else {
        std::path::PathBuf::from(args.option("masterlist").unwrap_or_default())
    };
    let request = nvmod_api::loadorder::SortRequest {
        instance: root.to_path_buf(),
        profile: Some(profile),
        masterlist: masterlist_path,
        apply: args.flag("apply"),
    };
    let (proposal, applied) = match nvmod_api::loadorder::sort(
        &request,
        &mut nvmod_api::NoProgress,
        &nvmod_api::CancelToken::new(),
    ) {
        Ok(nvmod_api::Outcome::Done(done)) => (done.proposal, done.applied),
        Ok(nvmod_api::Outcome::Cancelled) => return REFUSED,
        Err(e) => return crate::job_failure("sort", &e, err),
    };
    if args.flag("json") {
        let _ = writeln!(
            out,
            "{}",
            serde_json::to_string_pretty(&proposal_json(&proposal, applied.as_ref()))
                .unwrap_or_default()
        );
    } else {
        print_text(out, &proposal, applied.as_ref());
    }
    OK
}
