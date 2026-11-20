// SPDX-License-Identifier: GPL-3.0-only
//! The missing-requirement lines of a sort preview, made to fit the Instance's target.
//!
//! loot-rs reports a requirement the masterlist names (`req`) that is not installed twice: as a message ("This
//! plugin requires X to be installed, but it is missing.") and as a hard finding ("requires X, which is not
//! present."). nvmod keeps one line per requirement. On the nv-rs target a requirement the catalogue maps to
//! something the engine has built in is not an error: it becomes a note that says so, with the coverage from the
//! installed nv-rs's `capabilities.json`, as the Doctor does. Nothing else about a vanilla Instance changes.

use loot_rs::api::{Message, MessageKind, SortOutput};
use nvmod_catalog::{Catalog, Kind, Plugin};
use nvmod_core::Target;
use nvmod_tools::Capabilities;

/// A requirement as the masterlist names it, read from a loot-rs line.
struct Requirement {
    /// The shown name: the display text of a link, the quoted file name, or the text as it is.
    name: String,
    /// The Nexus mod id of a link to nexusmods.com, when the line has one.
    nexus: Option<u64>,
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

fn nexus_id(url: &str) -> Option<u64> {
    let rest = url.split("/mods/").nth(1)?;
    let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

fn requirement(shown: &str) -> Requirement {
    let shown = shown.trim();
    if let Some(link) = shown.strip_prefix('[')
        && let Some((text, url)) = link.split_once("](")
    {
        let url = url.split(')').next().unwrap_or_default();
        return Requirement {
            name: unescape(text),
            nexus: nexus_id(url),
        };
    }
    let unquoted = shown
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(shown);
    Requirement {
        name: unescape(unquoted),
        nexus: None,
    }
}

/// The requirement a message states, when it is loot-rs's missing-requirement message.
fn of_message(text: &str) -> Option<Requirement> {
    let rest = text.strip_prefix("This plugin requires ")?;
    let (shown, _) = rest.split_once(" to be installed, but it is missing.")?;
    Some(requirement(shown))
}

/// The requirement a finding states, when it is a missing-requirement finding.
fn of_finding(text: &str) -> Option<Requirement> {
    let rest = text.strip_prefix("requires ")?;
    let (shown, _) = rest.split_once(", which is not present.")?;
    Some(requirement(shown))
}

fn normal(text: &str) -> String {
    text.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

fn same(a: &Requirement, b: &Requirement) -> bool {
    match (a.nexus, b.nexus) {
        (Some(x), Some(y)) if x != y => false,
        _ => normal(&a.name) == normal(&b.name),
    }
}

/// The catalogue entry a requirement names: by Nexus id, else by the entry's name or DLL file name (compared
/// ignoring case and punctuation).
fn catalogue_entry<'a>(catalog: &'a Catalog, wanted: &Requirement) -> Option<&'a Plugin> {
    let by_id = wanted
        .nexus
        .and_then(|id| catalog.plugins.iter().find(|p| p.nexus_mod_id == Some(id)));
    by_id.or_else(|| {
        let name = normal(&wanted.name);
        catalog
            .plugins
            .iter()
            .find(|p| normal(&p.name) == name || normal(&p.dll) == name)
    })
}

fn built_in_note(plugin: &Plugin, caps: Option<&Capabilities>) -> String {
    let coverage = plugin
        .provided_by
        .nv_rs
        .extender
        .as_deref()
        .and_then(|id| caps.and_then(|c| c.extender(id)));
    match coverage {
        Some(c) if c.missing() == 0 => format!(
            "{}: built in on nv-rs, {} of {} commands implemented.",
            plugin.name, c.implemented, c.total
        ),
        Some(c) => format!(
            "{}: built in on nv-rs, {} of {} commands implemented, {} missing.",
            plugin.name,
            c.implemented,
            c.total,
            c.missing()
        ),
        None => format!(
            "{}: built in on nv-rs (the catalogue says so; no coverage was read from capabilities.json).",
            plugin.name
        ),
    }
}

/// Make the missing-requirement lines of `output` fit `target`. `caps` is the installed nv-rs's
/// `capabilities.json`, when it could be read.
///
/// Each requirement is reported once (the finding is dropped when a message states the same requirement). On
/// nv-rs a requirement on a catalogue entry the engine builds in becomes a note, and one on an entry that is
/// absent there says it cannot load; any other keeps loot-rs's message.
pub fn adjust(
    output: &mut SortOutput,
    catalog: &Catalog,
    target: Target,
    caps: Option<&Capabilities>,
) {
    for plugin in &mut output.plugins {
        let stated: Vec<Requirement> = plugin
            .messages
            .iter()
            .chain(&plugin.generated_messages)
            .filter(|m| m.kind == MessageKind::Error)
            .filter_map(|m| of_message(&m.text))
            .collect();
        plugin.findings.retain(|f| {
            f.kind != loot_rs::api::FindingKind::MissingRequirement
                || of_finding(&f.text).is_none_or(|r| !stated.iter().any(|s| same(s, &r)))
        });
        if target != Target::NvRs {
            continue;
        }
        let mut notes = Vec::new();
        for list in [&mut plugin.messages, &mut plugin.generated_messages] {
            list.retain_mut(|m| {
                let Some(wanted) = of_message(&m.text) else {
                    return true;
                };
                let Some(entry) = catalogue_entry(catalog, &wanted) else {
                    return true;
                };
                match entry.provided_by.nv_rs.kind {
                    kind if kind.is_builtin() => {
                        let mut note: Message = m.clone();
                        note.kind = MessageKind::Say;
                        note.text = built_in_note(entry, caps);
                        notes.push(note);
                        false
                    }
                    Kind::Absent => {
                        m.text.push_str(" It cannot load on nv-rs.");
                        true
                    }
                    _ => true,
                }
            });
        }
        plugin.messages.extend(notes);
    }
}
