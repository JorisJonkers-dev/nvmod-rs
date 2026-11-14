// SPDX-License-Identifier: GPL-3.0-only
//! Findings: what a check reports, in a form that is stable in JSON.

use serde::Serialize;
use std::fmt;
use std::str::FromStr;

/// How serious a finding is. The order is the order of seriousness: `Info < Warning < Error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// A fact worth knowing; nothing to do.
    Info,
    /// Probably a problem, or something the check could not decide.
    Warning,
    /// A problem that stops a plugin from loading or the game from starting with it.
    Error,
}

impl Severity {
    /// The lower-case name used in JSON and on the command line.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Info => "info",
            Severity::Warning => "warning",
            Severity::Error => "error",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The text was not `info`, `warning` or `error`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseSeverityError;

impl fmt::Display for ParseSeverityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("expected one of: info, warning, error")
    }
}

impl std::error::Error for ParseSeverityError {}

impl FromStr for Severity {
    type Err = ParseSeverityError;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        match text.trim().to_ascii_lowercase().as_str() {
            "info" => Ok(Severity::Info),
            "warning" | "warn" => Ok(Severity::Warning),
            "error" => Ok(Severity::Error),
            _ => Err(ParseSeverityError),
        }
    }
}

/// One thing a check found. `code` is stable (`pe.not-32-bit`) so scripts can match on it; `message` is for
/// people and may change.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Finding {
    /// Stable identifier, `<area>.<what>`.
    pub code: &'static str,
    /// How serious it is.
    pub severity: Severity,
    /// What it is about: a file, a mod or a plugin, as the owner would name it.
    pub subject: String,
    /// What was found and, where it helps, what to do.
    pub message: String,
}

impl Finding {
    /// A finding with the given parts.
    #[must_use]
    pub fn new(
        code: &'static str,
        severity: Severity,
        subject: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Finding {
            code,
            severity,
            subject: subject.into(),
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severities_are_ordered_by_seriousness() {
        assert!(Severity::Info < Severity::Warning && Severity::Warning < Severity::Error);
    }

    #[test]
    fn severity_parses_its_own_names_and_the_short_form() {
        for s in [Severity::Info, Severity::Warning, Severity::Error] {
            assert_eq!(s.as_str().parse::<Severity>(), Ok(s));
            assert_eq!(s.to_string(), s.as_str());
        }
        assert_eq!(" WARN ".parse::<Severity>(), Ok(Severity::Warning));
    }

    #[test]
    fn severity_refuses_anything_else() {
        assert_eq!("fatal".parse::<Severity>(), Err(ParseSeverityError));
        assert!(ParseSeverityError.to_string().contains("info"));
    }

    #[test]
    fn finding_serialises_with_a_lower_case_severity() {
        let f = Finding::new("pe.x", Severity::Warning, "a.dll", "msg");
        let json = serde_json::to_string(&f).unwrap();
        assert_eq!(
            json,
            r#"{"code":"pe.x","severity":"warning","subject":"a.dll","message":"msg"}"#
        );
    }
}
