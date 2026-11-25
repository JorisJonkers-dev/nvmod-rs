// SPDX-License-Identifier: GPL-3.0-only
//! [`Secret`]: a value that can be held and passed around but never printed, logged or serialised.

use std::fmt;

/// What a [`Secret`] prints instead of its value.
pub const REDACTED: &str = "[redacted]";

/// A secret value (a token, a password). `Debug`, `Display` and `Serialize` all give `[redacted]`, so a
/// `tracing` field, a `format!` or a JSON report cannot carry the value by accident. The only way to the value is
/// [`Secret::expose`], which a call site has to write out.
#[derive(Clone, Default)]
pub struct Secret<T>(T);

impl<T> Secret<T> {
    /// Wrap `value`.
    #[must_use]
    pub fn new(value: T) -> Secret<T> {
        Secret(value)
    }

    /// The value, for the one place that has to use it (an `Authorization` header, say).
    #[must_use]
    pub fn expose(&self) -> &T {
        &self.0
    }
}

impl<T> fmt::Debug for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

impl<T> fmt::Display for Secret<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(REDACTED)
    }
}

impl<T> serde::Serialize for Secret<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(REDACTED)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{covers, ids};

    #[test]
    fn a_secret_never_prints_its_value() {
        covers(&[ids::api::cli::SECRET_NEVER_PRINTS]);
        let s = Secret::new("hunter2-token".to_owned());
        assert_eq!(format!("{s:?}"), "[redacted]");
        assert_eq!(format!("{s}"), "[redacted]");
        assert_eq!(serde_json::to_string(&s).unwrap(), "\"[redacted]\"");
        assert_eq!(s.expose(), "hunter2-token");
        assert_eq!(format!("{:?}", Some(&s)), "Some([redacted])");
    }
}
