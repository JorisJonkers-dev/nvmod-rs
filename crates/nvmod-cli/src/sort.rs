// SPDX-License-Identifier: GPL-3.0-only
//! `nvmod sort --instance <dir> [--profile <name>] --masterlist <file> [--apply] [--json]`.

use crate::{OK, REFUSED, ROLLED_BACK, USAGE, args::Args};
use nvmod_instance::NvmodInstance;
use nvmod_loadorder::{Applied, Error, Proposal};
use std::io::Write;
use std::path::Path;

const USAGE_TEXT: &str =
    "usage: nvmod sort --instance <dir> [--profile <name>] --masterlist <file> [--apply] [--json]";

fn kind_name(kind: &impl std::fmt::Debug) -> String {
    format!("{kind:?}").to_lowercase()
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
                    .map(|m| serde_json::json!({ "kind": kind_name(&m.kind), "text": m.text }))
                    .collect()
            };
            serde_json::json!({
                "position": index + 1,
                "name": slot.name,
                "active": slot.active,
                "installed": result.is_some(),
                "group": result.map(|r| r.group.clone()),
                "messages": result.map(|r| messages(&r.messages)).unwrap_or_default(),
                "generated_messages": result.map(|r| messages(&r.generated_messages)).unwrap_or_default(),
                "findings": result.map(|r| r.findings.iter().map(|f| serde_json::json!({
                    "kind": kind_name(&f.kind), "text": f.text, "hard": f.hard,
                })).collect::<Vec<_>>()).unwrap_or_default(),
            })
        })
        .collect();
    serde_json::json!({
        "profile": proposal.profile,
        "reordered": proposal.reordered(),
        "order": order,
        "general_messages": output.general_messages.iter()
            .chain(&output.generated_general_messages)
            .map(|m| serde_json::json!({ "kind": kind_name(&m.kind), "text": m.text }))
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
            let _ = writeln!(out, "       [{}] {}", kind_name(&m.kind), m.text);
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
        let _ = writeln!(out, "message [{}]: {}", kind_name(&m.kind), m.text);
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

pub(crate) fn run(args: &[String], out: &mut dyn Write, err: &mut dyn Write) -> u8 {
    let args = match Args::parse(
        args,
        &["apply", "json"],
        &["instance", "profile", "masterlist"],
    ) {
        Ok(a)
            if a.positional.is_empty()
                && a.option("instance").is_some()
                && a.option("masterlist").is_some() =>
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
    let masterlist = match nvmod_loadorder::load_masterlist(Path::new(
        args.option("masterlist").unwrap_or_default(),
    )) {
        Ok(m) => m,
        Err(Error::Refused(why) | Error::Failed(why)) => return refused(&why, err),
    };
    let proposal = match nvmod_loadorder::propose(&instance, &profile, &masterlist) {
        Ok(p) => p,
        Err(Error::Refused(why) | Error::Failed(why)) => return refused(&why, err),
    };
    let applied = if args.flag("apply") {
        match nvmod_loadorder::apply(&instance, &proposal) {
            Ok(a) => Some(a),
            Err(Error::Refused(why)) => return refused(&why, err),
            Err(Error::Failed(why)) => {
                let _ = writeln!(err, "nvmod sort: {why}");
                return ROLLED_BACK;
            }
        }
    } else {
        None
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
