// SPDX-License-Identifier: GPL-3.0-only
//! The question stream of a FOMOD, for a caller that asks the owner step by step.
//!
//! [`next`] is one call of the protocol: it takes the archive and the answers collected so far and returns the
//! next event, either a step to show (every group with its selection rule, and every option with description,
//! image, type and tick state, judged against the flags the earlier answers set and the active Plugins) or the
//! plan (the files the Mod will hold). Nothing is written. When the plan comes back, give the answers of the
//! plan to [`crate::install::install`] as `fomod_answers`.
//!
//! The event and the saved answers file have JSON Schemas: [`events_schema`] (`schemas/fomod-events.v1.json`) and
//! [`answers_schema`] (`schemas/fomod-answers.v1.json`). A call extracts the archive again; a caller that
//! asks many steps of one large archive can hold an [`nvmod_modinstall::Session`] instead.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
pub use nvmod_modinstall::{
    AnswersFile, Decided, FomodEvent, GroupEvent, OptionEvent, PlanEvent, StepEvent, answers_schema,
    events_schema,
};
use nvmod_modinstall::{Limits, Session};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

const NEXT: &str = "fomod.next";

/// What [`next`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NextRequest {
    /// The Instance folder (its profile's Plugin lists decide the file dependencies).
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    #[serde(default)]
    pub profile: Option<String>,
    /// The archive, or a folder, to ask about. Optional with `reconfigure`: the archive the Mod came from.
    #[serde(default)]
    pub archive: Option<PathBuf>,
    /// Answers given so far: question id (`step/group`) to the chosen option names.
    #[serde(default)]
    pub answers: BTreeMap<String, Vec<String>>,
    /// Answers to show ticked in the groups that are not answered yet.
    #[serde(default)]
    pub previous: BTreeMap<String, Vec<String>>,
    /// Reconfigure this installed Mod: its saved answers are the `previous` ones (unless `previous` is given),
    /// and its saved archive is used when `archive` is not.
    #[serde(default)]
    pub reconfigure: Option<String>,
    /// The most bytes the extraction may write; `None` for the default limit.
    #[serde(default)]
    pub max_extract_bytes: Option<u64>,
    /// The other limits of the extraction; see [`crate::install::InstallRequest::extract_limits`].
    #[serde(default)]
    pub extract_limits: nvmod_modinstall::LimitOverrides,
}

/// The next event of the stream: the first step that has a group to answer, or the plan.
///
/// # Errors
/// [`ErrorKind::Refused`] when the archive is refused, an answer breaks the FOMOD's rules, or `reconfigure` names
/// a Mod without saved answers.
pub fn next(
    request: &NextRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<FomodEvent>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, false, NEXT)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), NEXT)?;
    progress.report(&Event::step("read", "fomod.next.reading"));
    let mut overrides = request.extract_limits;
    if request.max_extract_bytes.is_some() {
        overrides.max_bytes = request.max_extract_bytes;
    }
    let limits = Limits::with_overrides(&overrides);
    let refused = |why: String| JobError::new(ErrorKind::Refused, NEXT, why);
    let (archive, mut previous) = match &request.reconfigure {
        Some(name) => {
            let (_, saved) = nvmod_modinstall::saved::read(&instance, name).map_err(refused)?;
            let archive = request
                .archive
                .clone()
                .unwrap_or_else(|| PathBuf::from(&saved.archive));
            (archive, saved.answers)
        }
        None => (
            request
                .archive
                .clone()
                .ok_or_else(|| refused("give the archive to ask about".to_owned()))?,
            BTreeMap::new(),
        ),
    };
    if !request.previous.is_empty() {
        previous.clone_from(&request.previous);
    }
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let session = Session::open(&instance, &profile, &archive, limits).map_err(refused)?;
    let event = session.next(&request.answers, &previous).map_err(refused)?;
    Ok(Outcome::Done(event))
}
