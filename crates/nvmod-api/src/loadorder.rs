// SPDX-License-Identifier: GPL-3.0-only
//! The Load order of a profile: sort it with loot-rs, and write it.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
use loot_rs::api::{ProgressHook, SortOptions, Stage};
use nvmod_loadorder::Error;
pub use nvmod_loadorder::{Applied, Proposal, Slot};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc;

const SORT: &str = "loadorder.sort";

/// What [`sort`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SortRequest {
    /// The Instance folder.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    pub profile: Option<String>,
    /// The masterlist file (the caller fetches and verifies it).
    pub masterlist: PathBuf,
    /// Write the order: `plugins.txt`, `loadorder.txt` and the plugin file times. `false` only proposes.
    pub apply: bool,
}

/// The result of [`sort`].
#[derive(Debug)]
pub struct SortOutput {
    /// The order loot-rs proposes, with everything it reported.
    pub proposal: Proposal,
    /// What was written, when the request asked to apply.
    pub applied: Option<Applied>,
}

const fn step_of(stage: Stage) -> (&'static str, &'static str) {
    match stage {
        Stage::ReadingPlugins => ("read-plugins", "loadorder.sort.reading-plugins"),
        Stage::ReadingArchives => ("read-archives", "loadorder.sort.reading-archives"),
        Stage::ComparingPlugins => ("compare-plugins", "loadorder.sort.comparing-plugins"),
        Stage::Checking => ("check", "loadorder.sort.checking"),
        _ => ("sort", "loadorder.sort.sorting"),
    }
}

fn failure(error: Error) -> JobError {
    match error {
        Error::Refused(why) => JobError::new(ErrorKind::Refused, SORT, why),
        Error::Failed(why) => JobError::new(ErrorKind::Failed, SORT, why),
    }
}

/// Sort the Load order of a profile with loot-rs from its Staging tree, and with `apply` write it (journaled, so
/// `nvmod rollback` undoes it). Locked plugins keep their position. The sort's own stages are reported as
/// progress, and the token stops the sort between files; once the write has started it runs to its end.
///
/// # Errors
/// [`ErrorKind::Refused`] when the Instance, the profile, the Staging tree, the lists or the masterlist are
/// missing or unreadable, or loot-rs refuses the input (rules that contradict each other); [`ErrorKind::Failed`]
/// when a write failed and the profile was put back; [`ErrorKind::Internal`] when the sort panicked and no stop
/// had been asked for.
pub fn sort(
    request: &SortRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<SortOutput>, JobError> {
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let instance = crate::open(&request.instance, true, SORT)?;
    let profile = crate::profile(&instance, request.profile.as_deref(), SORT)?;
    progress.report(&Event::step(
        "read-masterlist",
        "loadorder.sort.reading-masterlist",
    ));
    let masterlist = nvmod_loadorder::load_masterlist(&request.masterlist).map_err(failure)?;
    let Some(proposal) = propose(&instance, &profile, &masterlist, progress, cancel)? else {
        return Ok(Outcome::Cancelled);
    };
    if !request.apply {
        return Ok(Outcome::Done(SortOutput {
            proposal,
            applied: None,
        }));
    }
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("write-lists", "loadorder.sort.writing"));
    let applied = nvmod_loadorder::apply(&instance, &proposal).map_err(failure)?;
    Ok(Outcome::Done(SortOutput {
        proposal,
        applied: Some(applied),
    }))
}

/// Let loot-rs propose the order, reporting its stages as they happen. `Ok(None)` is a cancelled sort: loot-rs
/// stopped because the token was used, or the worker panicked after the token was used (the panic of a sort that
/// was asked to stop is not a bug worth an [`ErrorKind::Internal`]).
///
/// # Errors
/// As [`sort`] without the write: a refusal of the Staging tree, the lists or the rules, and
/// [`ErrorKind::Internal`] when the worker panicked and no stop had been asked for.
fn propose(
    instance: &nvmod_instance::NvmodInstance,
    profile: &str,
    masterlist: &loot_rs::api::Masterlist,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Option<Proposal>, JobError> {
    // loot-rs reports from its own threads; the events cross to this thread, which owns `progress`. The hook
    // stops reporting once the token is used, so a cancelled sort does not wait for a progress event first.
    let (send, receive) = mpsc::channel();
    let stop = cancel.clone();
    let options = SortOptions::default()
        .with_cancel(cancel.to_loot())
        .with_progress(ProgressHook::new(move |p| {
            if !stop.is_cancelled() {
                let _ = send.send(p);
            }
        }));
    let proposed = std::thread::scope(|scope| {
        // The worker owns the options, and with them the only sender: when it ends, the channel ends and the loop
        // below finishes, whether the sort returned, refused or panicked.
        let worker = scope.spawn(move || {
            nvmod_loadorder::propose_with(instance, profile, masterlist, &options)
        });
        for p in receive {
            let (step, label) = step_of(p.stage);
            progress.report(&Event {
                step,
                label,
                done: Some(p.done as u64),
                total: Some(p.total as u64),
            });
        }
        worker.join()
    });
    match proposed {
        Ok(Ok(proposal)) => Ok(Some(proposal)),
        // A cancelled sort is a result, whether loot-rs said so or the worker died on the way out.
        Ok(Err(_) | Err(_)) if cancel.is_cancelled() => Ok(None),
        Ok(Err(e)) => Err(failure(e)),
        Err(_) => Err(JobError::new(
            ErrorKind::Internal,
            SORT,
            "the sort stopped on an internal error; nothing was written",
        )),
    }
}
