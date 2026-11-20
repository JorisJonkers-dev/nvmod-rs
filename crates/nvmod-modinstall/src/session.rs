// SPDX-License-Identifier: GPL-3.0-only
//! The question stream of a FOMOD, for a caller that asks the owner step by step (a terminal prompt, the
//! Toolbox's Mods page).
//!
//! A [`Session`] extracts the archive once. [`Session::next`] takes the answers given so far and returns the next
//! [`FomodEvent`]: a [`StepEvent`] (one install step with every group, option, description, image, type and
//! the tick state, all judged against the flags the earlier answers set and the active Plugins of the profile), or the
//! final [`PlanEvent`] with the files the Mod will hold. Nothing is written to the Instance; answering again with
//! one more step of answers is the whole protocol. The events and the answers file have JSON Schemas in
//! `schemas/` ([`events_schema`], [`answers_schema`]).

use crate::fomod::{Answers, Inferred, Module, Next, Outcome, StepEvent};
use crate::{
    InstallError, ListsEnv, Plan, Scratch, content_root, extract, final_paths, fomod, plan_fomod,
    plan_plain, resolve_ci,
};
use nvmod_instance::NvmodInstance;
use schemars::JsonSchema;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

/// The files the Mod will hold, and how the FOMOD was answered: the last event of a session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
pub struct PlanEvent {
    /// Files of the Mod, relative paths with `/`, sorted.
    pub files: Vec<String>,
    /// Choices the installer made without asking.
    pub inferred: Vec<Inferred>,
    /// The answers that counted, the content of the `answers` of the saved answers file. Empty for an archive
    /// that is not a FOMOD.
    pub answers: Answers,
}

/// One event of the stream: `{"event": "step", ...}` or `{"event": "plan", ...}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(tag = "event", rename_all = "kebab-case")]
pub enum FomodEvent {
    /// A step with something to answer.
    Step(StepEvent),
    /// Everything is decided; the Mod's file list.
    Plan(PlanEvent),
}

/// What [`Session::resolve`] works out.
pub(crate) struct Resolved {
    pub plan: Plan,
    pub inferred: Vec<Inferred>,
    /// `Some` for a FOMOD: the answers that counted and the module's name.
    pub fomod: Option<(Answers, String)>,
}

/// An archive (or folder) read once and ready to be asked about.
#[derive(Debug)]
pub struct Session {
    _scratch: Scratch,
    root: PathBuf,
    module: Option<Module>,
    env: ListsEnv,
}

impl Session {
    /// Extract `archive` (or read the folder in place) and read its `fomod/ModuleConfig.xml`; the active
    /// Plugins come from the profile's `plugins.txt` and `loadorder.txt`.
    ///
    /// # Errors
    /// The archive is refused (a limit, a hostile path, an unknown format), or the XML is not valid.
    pub fn open(
        instance: &NvmodInstance,
        profile: &str,
        archive: &Path,
        limits: extract::Limits,
    ) -> Result<Session, String> {
        let dir = instance.profile_dir(profile);
        let read = |file: &str| fs::read(dir.join(file)).ok();
        Session::open_with(
            archive,
            limits,
            read("plugins.txt").as_deref(),
            read("loadorder.txt").as_deref(),
        )
    }

    pub(crate) fn open_with(
        archive: &Path,
        limits: extract::Limits,
        plugins: Option<&[u8]>,
        order: Option<&[u8]>,
    ) -> Result<Session, String> {
        let scratch = Scratch::new()?;
        // A folder (such as the output of the TTW installer) is read in place; an archive is extracted first.
        let root = if archive.is_dir() {
            content_root(archive)
        } else {
            let extracted = scratch.0.join("x");
            fs::create_dir_all(&extracted).map_err(|e| e.to_string())?;
            extract::extract(archive, &extracted, limits)?;
            content_root(&extracted)
        };
        refuse_scripted(&root)?;
        let module = match resolve_ci(&root, "fomod/ModuleConfig.xml") {
            Some(config) => {
                let bytes = fs::read(&config).map_err(|e| e.to_string())?;
                Some(Module::parse(&fomod::decode_xml(&bytes))?)
            }
            None => None,
        };
        Ok(Session {
            _scratch: scratch,
            root,
            module,
            env: ListsEnv {
                active: nvmod_formats::plugins::parse_plugin_list(plugins.unwrap_or_default()),
                order: nvmod_formats::plugins::parse_plugin_list(order.unwrap_or_default()),
            },
        })
    }

    /// Whether the archive has a `fomod/ModuleConfig.xml`.
    #[must_use]
    pub fn is_fomod(&self) -> bool {
        self.module.is_some()
    }

    /// The module's name, when the archive is a FOMOD.
    #[must_use]
    pub fn module_name(&self) -> Option<&str> {
        self.module.as_ref().map(Module::name)
    }

    /// The next event, given the `answers` so far (`step name/group name` to the chosen option names).
    /// `previous` only decides which options show ticked in a group that is not answered yet, so a reconfigure
    /// can start from the old choices. An archive that is not a FOMOD gives the plan at once.
    ///
    /// # Errors
    /// An answer names a group or option that does not exist, breaks the group's rule or picks an option that
    /// cannot be used; or the FOMOD installs a file the archive does not hold.
    pub fn next(&self, answers: &Answers, previous: &Answers) -> Result<FomodEvent, String> {
        let Some(module) = &self.module else {
            return Ok(FomodEvent::Plan(PlanEvent {
                files: final_paths(&plan_plain(&self.root)?),
                inferred: Vec::new(),
                answers: Answers::new(),
            }));
        };
        match module.next(&self.env, answers, previous)? {
            Next::Step(step) => Ok(FomodEvent::Step(step)),
            Next::Done(selection) => Ok(FomodEvent::Plan(PlanEvent {
                files: final_paths(&plan_fomod(&self.root, &selection.files)?),
                inferred: selection.inferred,
                answers: selection.answers,
            })),
        }
    }

    /// Work out the plan for `answers`, or the questions that remain.
    pub(crate) fn resolve(&self, answers: &Answers) -> Result<Resolved, InstallError> {
        let Some(module) = &self.module else {
            return Ok(Resolved {
                plan: plan_plain(&self.root).map_err(InstallError::Refused)?,
                inferred: Vec::new(),
                fomod: None,
            });
        };
        match module
            .resolve(&self.env, answers)
            .map_err(InstallError::Refused)?
        {
            Outcome::Questions(questions) => Err(InstallError::Questions(questions)),
            Outcome::Ready(selection) => Ok(Resolved {
                plan: plan_fomod(&self.root, &selection.files).map_err(InstallError::Refused)?,
                inferred: selection.inferred,
                fomod: Some((selection.answers, module.name().to_owned())),
            }),
        }
    }
}

/// Installer scripts are programs, which nvmod does not run: a BAIN `wizard.txt` and a C# FOMOD (a `fomod`
/// folder with a `.cs` script and no `ModuleConfig.xml`) are refused with advice.
fn refuse_scripted(root: &Path) -> Result<(), String> {
    if nvmod_instance::find_ci(root, "wizard.txt").is_some_and(|p| p.is_file()) {
        return Err(
            "this archive is a BAIN package with a `wizard.txt` installer script; nvmod does not run installer scripts. Unpack it, choose the folders by hand and install the folder"
                .to_owned(),
        );
    }
    if resolve_ci(root, "fomod/ModuleConfig.xml").is_none()
        && let Some(folder) = nvmod_instance::find_ci(root, "fomod").filter(|p| p.is_dir())
    {
        let has_script = fs::read_dir(folder)
            .into_iter()
            .flatten()
            .flatten()
            .any(|e| {
                Path::new(&e.file_name())
                    .extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("cs"))
            });
        if has_script {
            return Err(
                "this archive is a scripted (C#) FOMOD; nvmod does not run installer scripts. Unpack it, choose the files by hand and install the folder"
                    .to_owned(),
            );
        }
    }
    Ok(())
}

fn schema_text(schema: &schemars::Schema) -> String {
    let mut text = serde_json::to_string_pretty(schema).expect("a schema serialises");
    text.push('\n');
    text
}

/// The JSON Schema of [`FomodEvent`], as pretty JSON with a trailing newline (`schemas/fomod-events.v1.json`).
///
/// # Panics
/// Never in practice: a generated schema always serialises.
#[must_use]
pub fn events_schema() -> String {
    schema_text(&schemars::schema_for!(FomodEvent))
}

/// The JSON Schema of the saved answers file ([`crate::AnswersFile`]), as pretty JSON with a trailing newline
/// (`schemas/fomod-answers.v1.json`).
///
/// # Panics
/// Never in practice: a generated schema always serialises.
#[must_use]
pub fn answers_schema() -> String {
    schema_text(&schemars::schema_for!(crate::AnswersFile))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Compare a checked-in schema with the generated one; `NVMOD_BLESS=1` rewrites the file.
    fn check_schema(file: &str, generated: &str) {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../schemas")
            .join(file);
        if std::env::var_os("NVMOD_BLESS").is_some() {
            fs::write(&path, generated).unwrap();
        }
        let checked_in = fs::read_to_string(&path).unwrap_or_default();
        assert!(
            checked_in == generated,
            "schemas/{file} differs from the schema generated from the Rust types; if the change is intended, \
             bump the schema version and run the tests with NVMOD_BLESS=1 to rewrite it"
        );
    }

    #[test]
    fn the_checked_in_event_schema_is_the_generated_one() {
        check_schema("fomod-events.v1.json", &events_schema());
    }

    #[test]
    fn the_checked_in_answers_schema_is_the_generated_one() {
        check_schema("fomod-answers.v1.json", &answers_schema());
    }
}
