// SPDX-License-Identifier: GPL-3.0-only
//! The typed calls of the Mods screen for the Mod lifecycle: `nvmod mods remove|rename|create|
//! clone|hide|unhide|note`, as requests that print their command
//! line and the typed JSON the commands answer with.

use crate::service::{push, push_opt};
use serde::Deserialize;

/// Where a new Mod goes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Where {
    /// The highest priority.
    Top,
    /// The lowest priority.
    Bottom,
    /// Just above this entry.
    Before(String),
    /// Just below this entry.
    After(String),
}

/// What one lifecycle call does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LifecycleStep {
    /// `mods remove <mod> [--force]`.
    Remove {
        /// The Mod.
        name: String,
        /// Remove although active plugins of other Mods need it.
        force: bool,
    },
    /// `mods rename <mod> <new>`.
    Rename {
        /// The Mod.
        name: String,
        /// Its new name.
        to: String,
    },
    /// `mods create <name> [place]`.
    Create {
        /// The new Mod.
        name: String,
        /// Where it goes (the top when `None`).
        at: Option<Where>,
    },
    /// `mods clone <mod> [--name <name>] [--hardlink]`.
    Clone {
        /// The Mod.
        name: String,
        /// The clone's name (`<mod> backup...` when `None`).
        as_name: Option<String>,
        /// Hard-link the files.
        hardlink: bool,
    },
    /// `mods hide|unhide <mod> <path>`.
    Hide {
        /// The Mod.
        name: String,
        /// The file inside it.
        path: String,
        /// Hide (`true`) or unhide.
        hide: bool,
    },
    /// `mods note <mod> <text>`.
    Note {
        /// The Mod.
        name: String,
        /// The note; empty clears it.
        text: String,
    },
}

/// What one lifecycle call needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleRequest {
    /// The Instance folder.
    pub instance: String,
    /// The profile, `None` for the default.
    pub profile: Option<String>,
    /// What to do.
    pub step: LifecycleStep,
    /// Plan only: nothing is written.
    pub dry_run: bool,
}

impl LifecycleRequest {
    /// The words after `nvmod mods`, before `--json`.
    pub fn args(&self) -> Vec<String> {
        let mut rest: Vec<String> = Vec::new();
        let words =
            |rest: &mut Vec<String>, w: &[&str]| rest.extend(w.iter().map(|s| (*s).to_owned()));
        match &self.step {
            LifecycleStep::Remove { name, force } => {
                words(&mut rest, &["remove", name]);
                if *force {
                    rest.push("--force".to_owned());
                }
            }
            LifecycleStep::Rename { name, to } => words(&mut rest, &["rename", name, to]),
            LifecycleStep::Create { name, at } => {
                words(&mut rest, &["create", name]);
                if let Some(at) = at {
                    place(&mut rest, at);
                }
            }
            LifecycleStep::Clone {
                name,
                as_name,
                hardlink,
            } => {
                words(&mut rest, &["clone", name]);
                push_opt(&mut rest, "name", as_name.as_ref());
                if *hardlink {
                    rest.push("--hardlink".to_owned());
                }
            }
            LifecycleStep::Hide { name, path, hide } => {
                words(
                    &mut rest,
                    &[if *hide { "hide" } else { "unhide" }, name, path],
                );
            }
            LifecycleStep::Note { name, text } => words(&mut rest, &["note", name, text]),
        }
        push(&mut rest, "instance", &self.instance);
        push_opt(&mut rest, "profile", self.profile.as_ref());
        if self.dry_run {
            rest.push("--dry-run".to_owned());
        }
        rest
    }
}

fn place(rest: &mut Vec<String>, at: &Where) {
    match at {
        Where::Top => rest.push("--top".to_owned()),
        Where::Bottom => rest.push("--bottom".to_owned()),
        Where::Before(n) => push(rest, "before", n),
        Where::After(n) => push(rest, "after", n),
    }
}

/// What `nvmod mods remove|rename|...` print with `--json`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct LifecycleReport {
    /// `changed`, `unchanged` or `dry-run`.
    pub status: String,
    /// True for a dry run.
    pub dry_run: bool,
    /// The action (`remove`, `rename`, `create`, `clone`, `hide`, `unhide` or `note`).
    pub action: String,
    /// The profile.
    pub profile: String,
    /// The Mod the action is about (the new one for a create or a clone).
    pub name: String,
    /// The new name of a rename.
    pub new_name: Option<String>,
    /// Whether `modlist.txt` changed.
    pub listed: bool,
    /// Plugins dropped from the plugin lists.
    pub plugins_dropped: Vec<String>,
    /// Active plugins of other Mods that need a removed plugin (a forced remove).
    pub dependants: Vec<String>,
    /// The file hidden or unhidden, or the note.
    pub detail: Option<String>,
    /// Files in the Staging tree after the change.
    pub staged_files: Option<usize>,
    /// The journal `nvmod rollback` undoes.
    pub journal: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(step: LifecycleStep) -> Vec<String> {
        LifecycleRequest {
            instance: "/i".into(),
            profile: Some("P".into()),
            step,
            dry_run: true,
        }
        .args()
    }

    #[test]
    fn requests_print_the_command_line() {
        assert_eq!(
            req(LifecycleStep::Remove {
                name: "A b".into(),
                force: true
            }),
            [
                "remove",
                "A b",
                "--force",
                "--instance",
                "/i",
                "--profile",
                "P",
                "--dry-run"
            ]
        );
        assert_eq!(
            req(LifecycleStep::Hide {
                name: "M".into(),
                path: "a/b".into(),
                hide: false
            })[..3],
            ["unhide", "M", "a/b"]
        );
    }

    #[test]
    fn a_report_reads_what_the_command_prints() {
        let text = r#"{"status":"changed","dry_run":false,"action":"remove","profile":"Default","name":"A",
            "new_name":null,"listed":true,"plugins_dropped":["a.esp"],"dependants":[],"detail":null,
            "staged_files":3,"journal":"j"}"#;
        let r: LifecycleReport = serde_json::from_str(text).unwrap();
        assert_eq!(r.plugins_dropped, ["a.esp"]);
        assert_eq!(r.staged_files, Some(3));
    }
}
