// SPDX-License-Identifier: GPL-3.0-only
//! The Mods of a profile as `modlist.txt` holds them: list them, enable or disable some, move one in the
//! priority order. A change writes a journal and a backup first (kind `mods`, undone by `nvmod rollback`), edits
//! only `modlist.txt` (every other line, comment and the CRLF line ends are kept) and stages the profile again
//! when it had a Staging tree. A dry run computes the same result and writes nothing.

use crate::journal;
use nvmod_formats::text::{decode, lines};
use nvmod_formats::{EntryKind, ModState, Modlist};
use nvmod_instance::{NvmodInstance, find_ci};
use serde::Serialize;
use std::fs;
use std::path::Path;

/// One line of the priority list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModRow {
    /// 1 is the highest priority (it wins a file conflict).
    pub position: usize,
    /// The folder name under `mods/`.
    pub name: String,
    /// `enabled`, `disabled` or `foreign`.
    pub state: &'static str,
    /// `mod` or `separator`.
    pub kind: &'static str,
    /// Whether the Mod folder exists (always true for a separator and a foreign entry).
    pub installed: bool,
}

/// What to change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Enable these Mods.
    Enable(Vec<String>),
    /// Disable these Mods.
    Disable(Vec<String>),
    /// Move one entry.
    Move {
        /// The entry to move.
        name: String,
        /// Where it goes.
        to: Place,
    },
}

/// A place in the priority list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Place {
    /// The highest priority.
    Top,
    /// The lowest priority.
    Bottom,
    /// Just above (higher priority than) another entry.
    Before(String),
    /// Just below (lower priority than) another entry.
    After(String),
    /// This 1-based position, counted before the move.
    Position(usize),
}

/// Why nothing changed.
#[derive(Debug, PartialEq, Eq)]
pub enum ModsError {
    /// Refused before anything was written (exit code 2).
    Refused(String),
    /// A step failed and the Instance was put back (exit code 4).
    Failed(String),
}

/// The result of a change or a dry run.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// `changed`, `unchanged` or `dry-run`.
    pub status: &'static str,
    /// True for a dry run.
    pub dry_run: bool,
    /// `enable`, `disable` or `reorder`.
    pub action: &'static str,
    /// The profile.
    pub profile: String,
    /// The entries whose state or place changed (or would).
    pub changed: Vec<String>,
    /// The list after the change (or as it would be).
    pub mods: Vec<ModRow>,
    /// Files in the Staging tree after the change; `None` when the profile has no Staging tree or nothing was written.
    pub staged_files: Option<usize>,
    /// The journal `nvmod rollback` uses; `None` for a dry run or when nothing changed.
    pub journal: Option<String>,
}

fn refuse<T>(why: impl Into<String>) -> Result<T, ModsError> {
    Err(ModsError::Refused(why.into()))
}

/// The Mods of `profile`, in priority order. A missing `modlist.txt` is an empty list.
#[must_use]
pub fn list(instance: &NvmodInstance, profile: &str) -> Vec<ModRow> {
    let bytes = fs::read(instance.profile_dir(profile).join("modlist.txt")).unwrap_or_default();
    rows(instance, &Modlist::parse(&decode(&bytes)))
}

fn rows(instance: &NvmodInstance, list: &Modlist) -> Vec<ModRow> {
    let mods = instance.root.join("mods");
    list.entries
        .iter()
        .enumerate()
        .map(|(i, e)| ModRow {
            position: i + 1,
            name: e.name.clone(),
            state: match e.state {
                ModState::Enabled => "enabled",
                ModState::Disabled => "disabled",
                ModState::Foreign => "foreign",
            },
            kind: match e.kind {
                EntryKind::Mod => "mod",
                EntryKind::Separator => "separator",
            },
            installed: e.state == ModState::Foreign
                || e.kind == EntryKind::Separator
                || find_ci(&mods, &e.name).is_some(),
        })
        .collect()
}

/// The entry lines (`+Name`, `-Name`, `*Name`, `Name`) and where in `all` they are.
pub(crate) struct Doc<'a> {
    pub(crate) all: Vec<&'a str>,
    /// Indexes into `all` of the entry lines kept by the parser (first occurrence of a name).
    pub(crate) entries: Vec<usize>,
    pub(crate) names: Vec<String>,
}

impl<'a> Doc<'a> {
    pub(crate) fn parse(text: &'a str) -> Doc<'a> {
        let all: Vec<&str> = lines(text).filter(|l| !l.trim().is_empty()).collect();
        let mut entries = Vec::new();
        let mut names: Vec<String> = Vec::new();
        for (i, line) in all.iter().enumerate() {
            let line = line.trim();
            if line.starts_with('#') {
                continue;
            }
            let name = line.strip_prefix(['+', '-', '*']).unwrap_or(line).trim();
            if name.is_empty() || names.iter().any(|n| n == name) {
                continue;
            }
            entries.push(i);
            names.push(name.to_owned());
        }
        Doc {
            all,
            entries,
            names,
        }
    }

    /// The index among entries of `wanted`: an exact name, or the only name equal ignoring case.
    pub(crate) fn find(&self, wanted: &str) -> Result<usize, ModsError> {
        if let Some(i) = self.names.iter().position(|n| n == wanted) {
            return Ok(i);
        }
        let hits: Vec<usize> = (0..self.names.len())
            .filter(|&i| self.names[i].eq_ignore_ascii_case(wanted))
            .collect();
        match hits.as_slice() {
            [one] => Ok(*one),
            [] => refuse(format!("`{wanted}` is not in modlist.txt")),
            _ => refuse(format!(
                "`{wanted}` matches several entries; use the exact name"
            )),
        }
    }
}

pub(crate) fn join(lines: &[String]) -> Vec<u8> {
    let mut out = String::new();
    for line in lines {
        out.push_str(line);
        out.push_str("\r\n");
    }
    out.into_bytes()
}

/// The new text of `modlist.txt` and the entries that change.
fn plan(
    mods_dir: &Path,
    text: &str,
    change: &Change,
) -> Result<(Vec<String>, Vec<String>), ModsError> {
    let doc = Doc::parse(text);
    let parsed = Modlist::parse(text);
    let mut out: Vec<String> = doc.all.iter().map(|l| (*l).to_owned()).collect();
    let mut changed = Vec::new();
    match change {
        Change::Enable(names) | Change::Disable(names) => {
            let enable = matches!(change, Change::Enable(_));
            if names.is_empty() {
                return refuse("name at least one Mod");
            }
            for wanted in names {
                let i = doc.find(wanted)?;
                let entry = &parsed.entries[i];
                if entry.kind == EntryKind::Separator || entry.state == ModState::Foreign {
                    return refuse(format!("`{}` is not a Mod", entry.name));
                }
                if enable && find_ci(mods_dir, &entry.name).is_none() {
                    return refuse(format!(
                        "the Mod folder of `{}` is not in the Instance, so it cannot be enabled",
                        entry.name
                    ));
                }
                let want = if enable {
                    ModState::Enabled
                } else {
                    ModState::Disabled
                };
                if entry.state != want && !changed.contains(&entry.name) {
                    out[doc.entries[i]] =
                        format!("{}{}", if enable { '+' } else { '-' }, entry.name);
                    changed.push(entry.name.clone());
                }
            }
        }
        Change::Move { name, to } => {
            let from = doc.find(name)?;
            if parsed.entries[from].state == ModState::Foreign {
                return refuse(format!("`{}` is not a Mod", parsed.entries[from].name));
            }
            let count = doc.entries.len();
            // The slot among the entries (0-based, after the entry is taken out) the entry goes to.
            let slot = match to {
                Place::Top => 0,
                Place::Bottom => count - 1,
                Place::Position(n) => {
                    if *n == 0 || *n > count {
                        return refuse(format!("position {n} is outside 1..={count}"));
                    }
                    n - 1
                }
                Place::Before(other) | Place::After(other) => {
                    let at = doc.find(other)?;
                    if at == from {
                        return refuse("an entry cannot be moved relative to itself");
                    }
                    let at = if at > from { at - 1 } else { at };
                    if matches!(to, Place::After(_)) {
                        at + 1
                    } else {
                        at
                    }
                }
            };
            if slot != from {
                // Rebuild: entry lines in a new order, every other line where it was.
                let mut order: Vec<usize> = (0..count).collect();
                order.remove(from);
                order.insert(slot, from);
                let lines_now: Vec<String> = doc.entries.iter().map(|&i| out[i].clone()).collect();
                for (place, &source) in order.iter().enumerate() {
                    out[doc.entries[place]].clone_from(&lines_now[source]);
                }
                changed.push(parsed.entries[from].name.clone());
            }
        }
    }
    Ok((out, changed))
}

/// Apply `change` to the Mods of `profile`, or plan to.
///
/// # Errors
/// See [`ModsError`].
pub fn apply(
    instance: &NvmodInstance,
    profile: &str,
    change: &Change,
    dry_run: bool,
) -> Result<Report, ModsError> {
    let file = instance.profile_dir(profile).join("modlist.txt");
    let bytes = fs::read(&file).map_err(|_| {
        ModsError::Refused(format!(
            "profile `{profile}` has no modlist.txt; nothing to change"
        ))
    })?;
    let text = decode(&bytes);
    let (new_lines, changed) = plan(&instance.root.join("mods"), &text, change)?;
    let new_bytes = join(&new_lines);
    let mods = rows(instance, &Modlist::parse(&decode(&new_bytes)));
    let mut report = Report {
        status: if dry_run {
            "dry-run"
        } else if changed.is_empty() {
            "unchanged"
        } else {
            "changed"
        },
        dry_run,
        action: match change {
            Change::Enable(_) => "enable",
            Change::Disable(_) => "disable",
            Change::Move { .. } => "reorder",
        },
        profile: profile.to_owned(),
        changed,
        mods,
        staged_files: None,
        journal: None,
    };
    if dry_run || report.changed.is_empty() {
        return Ok(report);
    }
    let staged = journal::staging_exists(instance, profile);
    let entry = journal::begin_kind(instance, profile, "", journal::KIND_MODS)
        .map_err(ModsError::Refused)?;
    let outcome = (|| -> Result<Option<usize>, String> {
        crate::fail_point("lists")?;
        journal::write_atomic(&file, &new_bytes)?;
        if !staged {
            return Ok(None);
        }
        crate::fail_point("stage")?;
        nvmod_stage::stage(instance, profile, nvmod_stage::Options::default())
            .map(|s| Some(s.manifest.files))
            .map_err(|e| match e {
                nvmod_stage::StageError::Refused(m) | nvmod_stage::StageError::Failed(m) => m,
            })
    })();
    match outcome {
        Ok(files) => {
            journal::complete(instance, &entry).map_err(ModsError::Failed)?;
            report.staged_files = files;
            report.journal = Some(entry.id);
            Ok(report)
        }
        Err(why) => match journal::undo(instance, &entry) {
            Ok(()) => Err(ModsError::Failed(format!(
                "{why}; the change was rolled back"
            ))),
            Err(undo) => Err(ModsError::Failed(format!(
                "{why}; the rollback failed too ({undo}); run `nvmod rollback`"
            ))),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: &str = "# header\r\n+A\r\n-B\r\nGroup_separator\r\n*DLC\r\n+C\r\n";

    fn run(change: &Change) -> Result<String, ModsError> {
        let (lines, _) = plan(Path::new("/nonexistent"), LIST, change)?;
        Ok(lines.join("|"))
    }

    fn mv(name: &str, to: Place) -> Change {
        Change::Move {
            name: name.to_owned(),
            to,
        }
    }

    #[test]
    fn a_move_keeps_the_comment_and_the_prefixes() {
        assert_eq!(
            run(&mv("C", Place::Top)).unwrap(),
            "# header|+C|+A|-B|Group_separator|*DLC"
        );
        assert_eq!(
            run(&mv("A", Place::After("B".into()))).unwrap(),
            "# header|-B|+A|Group_separator|*DLC|+C"
        );
        assert_eq!(
            run(&mv("C", Place::Before("B".into()))).unwrap(),
            "# header|+A|+C|-B|Group_separator|*DLC"
        );
        assert_eq!(
            run(&mv("A", Place::Bottom)).unwrap(),
            "# header|-B|Group_separator|*DLC|+C|+A"
        );
        assert_eq!(
            run(&mv("C", Place::Position(2))).unwrap(),
            "# header|+A|+C|-B|Group_separator|*DLC"
        );
    }

    #[test]
    fn a_move_to_where_it_is_changes_nothing() {
        let (_, changed) = plan(Path::new("/x"), LIST, &mv("A", Place::Top)).unwrap();
        assert_eq!(changed, Vec::<String>::new());
        let (_, changed) = plan(Path::new("/x"), LIST, &mv("B", Place::After("A".into()))).unwrap();
        assert_eq!(changed, Vec::<String>::new());
    }

    #[test]
    fn bad_moves_are_refused() {
        for change in [
            mv("Nope", Place::Top),
            mv("DLC", Place::Top),
            mv("A", Place::Position(0)),
            mv("A", Place::Position(9)),
            mv("A", Place::Before("A".into())),
            mv("A", Place::After("Nope".into())),
        ] {
            assert!(
                matches!(run(&change), Err(ModsError::Refused(_))),
                "{change:?}"
            );
        }
    }

    #[test]
    fn disable_flips_the_prefix_and_names_are_found_ignoring_case() {
        let (lines, changed) = plan(
            Path::new("/x"),
            LIST,
            &Change::Disable(vec!["a".into(), "B".into()]),
        )
        .unwrap();
        assert_eq!(lines.join("|"), "# header|-A|-B|Group_separator|*DLC|+C");
        assert_eq!(changed, ["A"]);
        assert!(plan(Path::new("/x"), LIST, &Change::Disable(vec!["DLC".into()])).is_err());
        assert!(
            plan(
                Path::new("/x"),
                LIST,
                &Change::Disable(vec!["Group_separator".into()])
            )
            .is_err()
        );
        assert!(plan(Path::new("/x"), LIST, &Change::Disable(vec![])).is_err());
    }

    #[test]
    fn enabling_needs_the_folder() {
        let r = plan(
            Path::new("/nonexistent"),
            LIST,
            &Change::Enable(vec!["B".into()]),
        );
        assert!(matches!(r, Err(ModsError::Refused(_))));
    }
}
