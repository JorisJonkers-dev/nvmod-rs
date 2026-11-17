// SPDX-License-Identifier: GPL-3.0-only
//! The library API of nvmod, for a program that links nvmod and calls it in process instead of starting the
//! `nvmod` command line.
//!
//! Every action is a job of one shape:
//!
//! ```text
//! run(&Request, &mut dyn Progress, &CancelToken) -> Result<Outcome<Output>, JobError>
//! ```
//!
//! * The request and its result are typed structs; requests are `serde` serialisable, so a plan can be shown and a
//!   journal entry written.
//! * [`Progress`] receives [`Event`]s with a step id, a stable label key and optional counts.
//! * [`CancelToken`] is cooperative: a job checks it between its steps and stops at the next safe point. A
//!   cancelled job returns [`Outcome::Cancelled`], which is a result, not an error.
//! * [`JobError`] has a stable [`ErrorKind`], a plain summary and [`JobError::copy_text`]. It never holds a command
//!   line.
//! * Nothing here prints or ends the process. The `nvmod` subcommands `mods list|enable|disable|reorder`, `sort`,
//!   `saves list` and `run --dry-run` are wrappers over these jobs.
//!
//! The jobs:
//!
//! | Job | Request | Result |
//! | --- | --- | --- |
//! | [`mods::list`] | [`mods::ListRequest`] | [`mods::ListOutput`] |
//! | [`mods::change`] | [`mods::ChangeRequest`] | [`mods::Report`] |
//! | [`plugins::list`] | [`plugins::ListRequest`] | [`plugins::ListReport`] |
//! | [`plugins::change`] | [`plugins::ChangeRequest`] | [`plugins::EditReport`] |
//! | [`loadorder::sort`] | [`loadorder::SortRequest`] | [`loadorder::SortOutput`] |
//! | [`saves::list`] | [`saves::ListRequest`] | [`saves::ListOutput`] |
//! | [`run::plan`] | [`run::PlanRequest`] | [`run::RunPlan`] |
//!
//! [`API_VERSION`] is the version of this API: the nvmod release version, which every release's tag and notes
//! carry. While it is 0.x a minor release may change the API and a patch release does not.

pub mod job;
pub mod loadorder;
pub mod mods;
pub mod plugins;
pub mod run;
pub mod saves;

pub use job::{CancelToken, ErrorKind, Event, JobError, NoProgress, Outcome, Progress};

/// The version of this library API: the nvmod release version.
pub const API_VERSION: &str = env!("CARGO_PKG_VERSION");

use nvmod_instance::{InstanceError, NvmodInstance};
use std::path::Path;

/// Open the Instance at `root` (read only, or migrating an older file forward), as the command line does.
pub(crate) fn open(root: &Path, write: bool, job: &'static str) -> Result<NvmodInstance, JobError> {
    let opened = if write {
        NvmodInstance::open(root)
    } else {
        NvmodInstance::open_read_only(root)
    };
    opened.map_err(|e| JobError::new(ErrorKind::Refused, job, describe_open_error(root, &e)))
}

/// The reason an Instance could not be opened, in the words the command line uses.
#[must_use]
pub fn describe_open_error(root: &Path, e: &InstanceError) -> String {
    match e {
        InstanceError::Refused(why) => why.clone(),
        _ => format!("{} is not an nvmod Instance ({e})", root.display()),
    }
}

/// The profile a job works on: `wanted`, or the only one there is.
pub(crate) fn profile(
    instance: &NvmodInstance,
    wanted: Option<&str>,
    job: &'static str,
) -> Result<String, JobError> {
    instance
        .resolve_profile(wanted)
        .map_err(|e| JobError::new(ErrorKind::Refused, job, e.to_string()))
}
