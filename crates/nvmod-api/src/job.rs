// SPDX-License-Identifier: GPL-3.0-only
//! The shape every job of this API has: `run(request, &mut dyn Progress, &CancelToken) -> Result<Outcome<T>,
//! JobError>`.

use serde::Serialize;
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// One progress event of a job.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Event {
    /// The job's step, stable for programs: for example `read-plugins` or `write-lists`.
    pub step: &'static str,
    /// A stable label key a caller words in its own voice (`loadorder.sort.reading-plugins`).
    pub label: &'static str,
    /// Items done in this step, when the step counts.
    pub done: Option<u64>,
    /// Items in this step, when known.
    pub total: Option<u64>,
}

impl Event {
    /// An event of `step` without counts.
    #[must_use]
    pub const fn step(step: &'static str, label: &'static str) -> Self {
        Self {
            step,
            label,
            done: None,
            total: None,
        }
    }
}

/// Where a job's progress goes. A closure `FnMut(&Event)` is one.
pub trait Progress {
    /// One event, in the order the job made them.
    fn report(&mut self, event: &Event);
}

impl<F: FnMut(&Event)> Progress for F {
    fn report(&mut self, event: &Event) {
        self(event);
    }
}

/// Progress that goes nowhere.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoProgress;

impl Progress for NoProgress {
    fn report(&mut self, _event: &Event) {}
}

/// A cooperative stop request, shared between whoever asks and the job that obeys. Clones share the request.
#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    /// A token nobody has used.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask the job to stop at its next safe point. Asking twice is the same as asking once.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    /// Whether a stop was asked for.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }

    /// The same request as loot-rs's token, for a sort that must stop with the job.
    #[must_use]
    pub fn to_loot(&self) -> loot_rs::api::CancelToken {
        loot_rs::api::CancelToken::from(Arc::clone(&self.0))
    }
}

impl From<&Self> for CancelToken {
    fn from(token: &Self) -> Self {
        token.clone()
    }
}

/// How a job ended when it did not fail. A cancelled job is a result, not an error: it stopped at a safe point
/// and wrote nothing after it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome<T> {
    /// The job finished.
    Done(T),
    /// The job stopped because it was asked to.
    Cancelled,
}

impl<T> Outcome<T> {
    /// The result, `None` when the job was cancelled.
    pub fn done(self) -> Option<T> {
        match self {
            Self::Done(value) => Some(value),
            Self::Cancelled => None,
        }
    }
}

/// The class of a [`JobError`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
#[non_exhaustive]
pub enum ErrorKind {
    /// Refused before anything was written.
    Refused,
    /// A check of the data failed (a backup that does not match its manifest); nothing was changed.
    Corrupt,
    /// A step failed and what it started was put back.
    Failed,
    /// A check inside nvmod failed (a panic in the job): a bug, never a property of the input.
    Internal,
}

impl ErrorKind {
    /// A stable code: `refused`, `corrupt`, `failed` or `internal`.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Refused => "refused",
            Self::Corrupt => "corrupt",
            Self::Failed => "failed",
            Self::Internal => "internal",
        }
    }
}

/// Why a job gave no result. It never holds a command line.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct JobError {
    /// The class.
    pub kind: ErrorKind,
    /// The job: `mods.list`, `mods.change`, `loadorder.sort`, `saves.list` or `run.plan`.
    pub job: &'static str,
    /// What went wrong, in plain words.
    pub summary: String,
}

impl JobError {
    /// An error of `kind` in `job`.
    pub fn new(kind: ErrorKind, job: &'static str, summary: impl Into<String>) -> Self {
        Self {
            kind,
            job,
            summary: summary.into(),
        }
    }

    /// The error as one block of plain text to copy into a bug report: `nvmod <version>`, the job, the code and
    /// the summary, one per line, with the home directory written as `~`.
    #[must_use]
    pub fn copy_text(&self) -> String {
        let summary = match std::env::var("HOME") {
            Ok(home) => {
                let home = home.trim_end_matches('/');
                if home.len() > 1 {
                    tilde(&self.summary, home)
                } else {
                    self.summary.clone()
                }
            }
            Err(_) => self.summary.clone(),
        };
        format!(
            "nvmod {}\njob: {}\nerror: {}\n{summary}",
            crate::API_VERSION,
            self.job,
            self.kind.code()
        )
    }
}

impl fmt::Display for JobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.summary)
    }
}

impl std::error::Error for JobError {}

/// `text` with every `home` that starts a path (not inside a longer path, and followed by `/` or the end of a
/// word) written as `~`.
pub(crate) fn tilde(text: &str, home: &str) -> String {
    let path_char = |c: char| c.is_alphanumeric() || matches!(c, '/' | '.' | '_' | '-' | '~');
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(home) {
        let after = &rest[at + home.len()..];
        out.push_str(&rest[..at]);
        let starts = !out.chars().next_back().is_some_and(path_char);
        let ends = after
            .chars()
            .next()
            .is_none_or(|c| c == '/' || !path_char(c));
        if starts && ends {
            out.push('~');
        } else {
            out.push_str(home);
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_home_directory_is_written_as_a_tilde_only_where_a_path_starts_with_it() {
        let home = "/home/someone";
        assert_eq!(tilde("/home/someone/Data", home), "~/Data");
        assert_eq!(
            tilde("/mnt/home/someone/Data", home),
            "/mnt/home/someone/Data"
        );
        assert_eq!(tilde("/home/someone2/Data", home), "/home/someone2/Data");
        assert_eq!(tilde("at /home/someone", home), "at ~");
    }

    #[test]
    fn a_home_directory_with_a_trailing_slash_is_written_as_a_tilde_too() {
        let home = "/home/someone";
        assert_eq!(tilde("/home/someone/Data", home), "~/Data");
        assert_eq!(tilde("at /home/someone/", home), "at ~/");
        assert_eq!(tilde("/home/someone", home), "~");
    }

    #[test]
    fn a_cancel_token_from_a_reference_is_a_clone_of_the_same_request() {
        let token = CancelToken::new();
        let copy = CancelToken::from(&token);
        assert!(!copy.is_cancelled());
        copy.cancel();
        assert!(token.is_cancelled());
    }

    #[test]
    fn a_clone_of_a_cancel_token_shares_the_request_with_loot_rs() {
        let token = CancelToken::new();
        let loot = token.to_loot();
        token.clone().cancel();
        assert!(token.is_cancelled());
        assert!(loot.is_cancelled());
    }

    #[test]
    fn copy_text_names_the_version_the_job_and_the_code() {
        let error = JobError::new(ErrorKind::Refused, "mods.list", "no profile");
        let text = error.copy_text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(
            lines,
            [
                format!("nvmod {}", crate::API_VERSION).as_str(),
                "job: mods.list",
                "error: refused",
                "no profile"
            ]
        );
    }
}
