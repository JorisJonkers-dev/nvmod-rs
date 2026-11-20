// SPDX-License-Identifier: GPL-3.0-only
//! The write side of `nvmod sort`: `--keep-order` writes the Load order the profile already has, so only the plugin
//! file times of the Staging tree change (and the lists, when their bytes differ). nv-rs orders plugins by file
//! time until the fork has a `plugins.txt` switch (docs/DECISIONS.md), so an imported MO2 order needs this. The
//! `--apply` path of the masterlist sort is [`crate::loadorder::sort`].

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
pub use nvmod_loadorder::Applied;
use nvmod_loadorder::Error;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const KEEP_ORDER: &str = "sort.keep_order";

/// What [`keep_order`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeepOrderRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
}

/// The result of [`keep_order`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeepOrderOutput {
    /// The profile.
    pub profile: String,
    /// The plugins of the Load order that was kept.
    pub plugins: usize,
    /// What was written.
    pub applied: Applied,
}

fn failure(error: Error) -> JobError {
    match error {
        Error::Refused(why) => JobError::new(ErrorKind::Refused, KEEP_ORDER, why),
        Error::Failed(why) => JobError::new(ErrorKind::Failed, KEEP_ORDER, why),
    }
}

/// Write the Load order of `request.profile` as it is: `plugins.txt` and `loadorder.txt` when their bytes differ,
/// then the plugin file times of the Staging tree, strictly increasing in Load order. The write is journaled, so
/// `nvmod rollback` undoes it, and nothing is written when the lists and the times are already right.
///
/// # Errors
/// [`ErrorKind::Refused`] when the Instance, the profile, the Staging tree or the lists are missing or unreadable,
/// before anything was written; [`ErrorKind::Failed`] when a write failed and the profile was put back.
pub fn keep_order(
    request: &KeepOrderRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<KeepOrderOutput>, JobError> {
    let _scope = cancel.scope();
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, true, KEEP_ORDER)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), KEEP_ORDER)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("read-order", "sort.keep-order.reading-order"));
    let data = nvmod_loadorder::staging_data(&instance, &profile).map_err(failure)?;
    let order = nvmod_loadorder::read_slots(&instance.profile_dir(&profile)).map_err(failure)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("write-times", "sort.keep-order.writing-times"));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let applied = nvmod_loadorder::write_order(
        &instance,
        &profile,
        Some(&data),
        &order,
        None,
        nvmod_journal::KIND_SORT,
    )
    .map_err(failure)?;
    Ok(Outcome::Done(KeepOrderOutput {
        profile,
        plugins: order.len(),
        applied,
    }))
}
