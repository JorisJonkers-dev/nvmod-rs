// SPDX-License-Identifier: GPL-3.0-only
//! Mod versions as Mod Organizer 2 writes them into `meta.ini`: numeric dotted (`1.9`), dotted with a letter
//! suffix (`5.17a`) and date style, a leading `d` (`d2026.3.1.0`). A version that does not fit is reported, never
//! guessed, and a date version cannot be ordered against a dotted one.

use std::cmp::Ordering;

/// Which family a version belongs to; versions of different families do not order against each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Numeric dotted, optionally with a letter suffix on the last part.
    Dotted,
    /// Date style: `d` and dotted numbers.
    Date,
}

/// A parsed version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Version {
    /// The family.
    pub kind: Kind,
    /// The numeric parts, in order.
    pub parts: Vec<u64>,
    /// Lowercased letters after the last number (`a` of `5.17a`), empty when none.
    pub suffix: String,
}

impl Version {
    /// Parse `text`; `None` when it is not one of the shapes above (empty, `1.0-beta`, `latest`, ...).
    #[must_use]
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim();
        let (kind, body) = match text.strip_prefix(['d', 'D']) {
            Some(rest) if rest.starts_with(|c: char| c.is_ascii_digit()) => (Kind::Date, rest),
            _ => (Kind::Dotted, text.strip_prefix(['v', 'V']).unwrap_or(text)),
        };
        let mut parts = Vec::new();
        let mut suffix = String::new();
        let pieces: Vec<&str> = body.split('.').collect();
        for (i, piece) in pieces.iter().enumerate() {
            let digits = piece.chars().take_while(char::is_ascii_digit).count();
            if digits == 0 {
                return None;
            }
            parts.push(piece[..digits].parse().ok()?);
            let rest = &piece[digits..];
            if !rest.is_empty() {
                let last = i + 1 == pieces.len();
                if !last
                    || kind == Kind::Date
                    || rest.len() > 8
                    || !rest.chars().all(|c| c.is_ascii_alphabetic())
                {
                    return None;
                }
                rest.to_ascii_lowercase().clone_into(&mut suffix);
            }
        }
        Some(Version {
            kind,
            parts,
            suffix,
        })
    }

    /// The order of two versions of the same family; `None` across families.
    #[must_use]
    pub fn order(&self, other: &Version) -> Option<Ordering> {
        if self.kind != other.kind {
            return None;
        }
        for i in 0..self.parts.len().max(other.parts.len()) {
            let a = self.parts.get(i).copied().unwrap_or(0);
            let b = other.parts.get(i).copied().unwrap_or(0);
            if a != b {
                return Some(a.cmp(&b));
            }
        }
        Some(self.suffix.cmp(&other.suffix))
    }
}

/// Why two versions could not be ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    /// One or both strings do not parse (`left`, `right` say which).
    Unparsable {
        /// The first string did not parse.
        left: bool,
        /// The second string did not parse.
        right: bool,
    },
    /// A date version against a dotted one.
    Incomparable,
}

/// The result of [`compare`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Comparison {
    /// The order; `Equal` when `issue` is set (nothing is guessed).
    pub order: Ordering,
    /// Set when the versions could not be ordered.
    pub issue: Option<Issue>,
}

/// Compare two version strings. Unparsable strings and a date against a dotted version compare equal and carry
/// an [`Issue`], so a caller reports them instead of acting on a guess.
#[must_use]
pub fn compare(a: &str, b: &str) -> Comparison {
    let (x, y) = (Version::parse(a), Version::parse(b));
    let (Some(x), Some(y)) = (&x, &y) else {
        return Comparison {
            order: Ordering::Equal,
            issue: Some(Issue::Unparsable {
                left: x.is_none(),
                right: y.is_none(),
            }),
        };
    };
    match x.order(y) {
        Some(order) => Comparison { order, issue: None },
        None => Comparison {
            order: Ordering::Equal,
            issue: Some(Issue::Incomparable),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{covers, ids};

    fn lt(a: &str, b: &str) {
        let c = compare(a, b);
        assert_eq!((c.order, c.issue), (Ordering::Less, None), "{a} < {b}");
        let c = compare(b, a);
        assert_eq!((c.order, c.issue), (Ordering::Greater, None), "{b} > {a}");
    }

    #[test]
    fn dotted_suffixed_and_date_versions_order() {
        covers(&[ids::formats::files::VERSION_ORDER]);
        lt("1.9", "1.10");
        lt("5.17", "5.17a");
        lt("5.17a", "5.17b");
        lt("5.17a", "5.18");
        lt("d2026.3.1.0", "d2026.11.2.0");
        lt("1.0", "1.0.1");
        lt("v1.2", "1.3");
        assert_eq!(compare("1.0", "1.0.0").order, Ordering::Equal);
        assert_eq!(compare("1.0", "1.0.0").issue, None);
    }

    #[test]
    fn a_date_against_a_dotted_version_is_incomparable() {
        let c = compare("d2026.3.1.0", "2.5");
        assert_eq!(
            (c.order, c.issue),
            (Ordering::Equal, Some(Issue::Incomparable))
        );
    }

    #[test]
    fn unparsable_versions_compare_equal_and_are_reported() {
        for bad in [
            "",
            "latest",
            "1.0-beta",
            "1..2",
            "d",
            "1.0.a",
            "d1.0a",
            "99999999999999999999.1",
        ] {
            assert_eq!(Version::parse(bad), None, "{bad}");
        }
        let c = compare("latest", "1.0");
        assert_eq!(
            (c.order, c.issue),
            (
                Ordering::Equal,
                Some(Issue::Unparsable {
                    left: true,
                    right: false
                })
            )
        );
        assert_eq!(
            compare("x", "y").issue,
            Some(Issue::Unparsable {
                left: true,
                right: true
            })
        );
    }
}
