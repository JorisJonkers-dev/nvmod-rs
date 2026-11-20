// SPDX-License-Identifier: GPL-3.0-only
//! The conflicts job: the file-level conflicts of a profile, and of its archives, as `nvmod conflicts` shows them,
//! run in process. Read only: nothing is written.
//!
//! Cancellation is honoured before the scan starts. The scan and, when asked for, the archive axes run as one
//! call to `nvmod-conflicts`, which is not interrupted once it has started.

use crate::{CancelToken, ErrorKind, Event, JobError, Outcome, Progress};
use nvmod_conflicts::{ConflictError, Report};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

const REPORT: &str = "conflicts.report";

/// What [`report`] needs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportRequest {
    /// The nvmod Instance folder, read as it is: an older Instance file is read as migrated, not rewritten.
    pub instance: PathBuf,
    /// The profile, `None` for the only one there is.
    #[serde(default)]
    pub profile: Option<String>,
    /// Only what involves this enabled Mod: its own row and the contested paths it is in. `None` for every Mod.
    #[serde(default, rename = "mod")]
    pub mod_name: Option<String>,
    /// Whether to add the archive axes: which archives load, and where an archive collides with a loose file or
    /// another archive.
    #[serde(default)]
    pub archives: bool,
}

/// The result of [`report`].
#[derive(Debug, Clone, Serialize)]
pub struct ReportOutput {
    /// The report: the Mods, the contested paths and, when asked for, the archive axes. In JSON it is the `report` member of the output.
    pub report: Report,
}

/// Read the conflicts of the request's profile. Nothing is written.
///
/// A profile with no contested path is a result with an empty `conflicts` list, not a refusal.
///
/// Cancellation is honoured before the scan starts; a running scan (and the archive axes, which are part of the same
/// call) is not interrupted.
///
/// # Errors
/// [`ErrorKind::Refused`] for every refusal of a report that cannot be made from its input: the folder cannot be
/// opened as an nvmod Instance (including an Instance file newer than this nvmod reads), the Instance has no such
/// profile, the profile or its `modlist.txt` cannot be read, the base copy has no `Data` folder, or `mod_name` is
/// not an enabled Mod of the profile. Nothing else fails: the job makes no other error kind.
pub fn report(
    request: &ReportRequest,
    progress: &mut dyn Progress,
    cancel: &CancelToken,
) -> Result<Outcome<ReportOutput>, JobError> {
    let _scope = cancel.scope();
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    progress.report(&Event::step("open-instance", "conflicts.report.opening"));
    let instance = crate::open(&request.instance, false, REPORT)?;
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let profile_name = crate::profile(&instance, request.profile.as_deref(), REPORT)?;
    progress.report(&Event::step("read-layers", "conflicts.report.reading"));
    if cancel.is_cancelled() {
        return Ok(Outcome::Cancelled);
    }
    let made = if request.archives {
        nvmod_conflicts::conflicts_with_archives(&instance, &profile_name)
    } else {
        nvmod_conflicts::conflicts(&instance, &profile_name)
    };
    let report = made
        .and_then(|r| match &request.mod_name {
            Some(name) => r.only(name),
            None => Ok(r),
        })
        .map_err(|ConflictError::Refused(why)| JobError::new(ErrorKind::Refused, REPORT, why))?;
    Ok(Outcome::Done(ReportOutput { report }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_request_names_the_mod_field_as_mod_and_the_rest_default() {
        let request: ReportRequest =
            serde_json::from_str(r#"{"instance": "/games/fnv", "mod": "A"}"#).expect("parses");
        assert_eq!(request.mod_name.as_deref(), Some("A"));
        assert_eq!(request.profile, None);
        assert!(!request.archives);
    }
}
