// SPDX-License-Identifier: GPL-3.0-only
//! The run plan: what starting the game or a tool on a profile would do. Planning starts nothing; the launch
//! itself is the game launcher's.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
pub use nvmod_run::{Platform, RunPlan, StagingAction};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const PLAN: &str = "run.plan";

/// What [`plan`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
    /// `game`, `nv-rs`, a registry title or a path to a program.
    pub target: String,
    /// Arguments added after the entry's own.
    #[serde(default)]
    pub args: Vec<String>,
    /// The Proton program, `None` for `proton` on `PATH`.
    #[serde(default)]
    pub proton: Option<String>,
    /// The Steam client folder, when known.
    #[serde(default)]
    pub steam_client: Option<PathBuf>,
    /// `game` on an nv-rs Instance runs through Proton instead of natively.
    #[serde(default)]
    pub via_proton: bool,
    /// The nv-viewer to start, instead of the one found.
    #[serde(default)]
    pub viewer: Option<PathBuf>,
    /// The folder of shared nv-rs builds, searched when the Instance has none of its own.
    #[serde(default)]
    pub shared_root: Option<PathBuf>,
    /// Build the Staging tree again even when it is current.
    #[serde(default)]
    pub restage: bool,
    /// Leave the Staging tree as it is.
    #[serde(default)]
    pub no_stage: bool,
}

/// The words of a refusal of the run plan. nvmod-run names the command line options it refuses (`--proton`); a
/// caller of this API gave a field of the request instead, so a reason that names an option is said again without
/// it.
fn refusal(why: &str) -> String {
    match why.strip_prefix("--proton ") {
        Some(rest) => format!("the Proton program {rest}"),
        None => why.to_owned(),
    }
}

/// Plan a run for the host platform: the program, its arguments and environment, the staged `Data` folder and
/// what happens to the Staging tree first. Reads only and starts nothing.
///
/// # Errors
/// [`ErrorKind::Refused`] when the folder is not an Instance, the profile cannot be chosen, or the target cannot
/// be run (an unknown entry, no program found).
pub fn plan(
    request: &PlanRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<RunPlan>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("plan", "run.plan.planning"));
    let instance = crate::open(&request.instance, false, PLAN)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), PLAN)?;
    let inner = nvmod_run::Request {
        instance: &instance,
        profile: &profile,
        target: &request.target,
        args: &request.args,
        proton: request.proton.as_deref(),
        platform: Platform::host(),
        steam_client: request.steam_client.clone(),
        via_proton: request.via_proton,
        viewer: request.viewer.clone(),
        shared_root: request.shared_root.clone(),
    };
    let mut plan =
        nvmod_run::plan(&inner).map_err(|why| JobError::new(ErrorKind::Refused, PLAN, refusal(&why)))?;
    plan.staging = nvmod_run::plan_staging(&instance, &profile, request.restage, request.no_stage);
    Ok(Outcome::Done(plan))
}
