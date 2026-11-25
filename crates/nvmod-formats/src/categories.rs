// SPDX-License-Identifier: GPL-3.0-only
//! `categories.dat` of a Mod Organizer 2 instance: one category per line, `ID|Name|NexusIDs|ParentID`. The file is
//! only read. The reader keeps the original bytes, so a file that nvmod never edits round-trips exactly, and a
//! line it cannot read is skipped without dropping anything from those bytes.

use crate::text::{decode, lines};

/// One category.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Category {
    /// The id a `meta.ini` `category` key refers to.
    pub id: u32,
    /// The name shown in MO2.
    pub name: String,
    /// The Nexus category ids it maps to (as written).
    pub nexus_ids: String,
    /// The parent category, 0 for none.
    pub parent: u32,
}

/// A parsed `categories.dat` with its original bytes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Categories {
    raw: Vec<u8>,
    /// The categories, in file order.
    pub entries: Vec<Category>,
}

impl Categories {
    /// Read `bytes`. Never fails: a line that is blank or not `ID|Name|...` with a numeric id is skipped.
    #[must_use]
    pub fn parse(bytes: &[u8]) -> Categories {
        let mut entries = Vec::new();
        for line in lines(&decode(bytes)) {
            let mut parts = line.trim().split('|');
            let (Some(id), Some(name)) = (parts.next(), parts.next()) else {
                continue;
            };
            let Ok(id) = id.trim().parse::<u32>() else {
                continue;
            };
            entries.push(Category {
                id,
                name: name.to_owned(),
                nexus_ids: parts.next().unwrap_or_default().to_owned(),
                parent: parts
                    .next()
                    .and_then(|p| p.trim().parse().ok())
                    .unwrap_or(0),
            });
        }
        Categories {
            raw: bytes.to_vec(),
            entries,
        }
    }

    /// The name of category `id`, if the file has it.
    #[must_use]
    pub fn name(&self, id: u32) -> Option<&str> {
        self.entries
            .iter()
            .find(|c| c.id == id)
            .map(|c| c.name.as_str())
    }

    /// The bytes that were read, unchanged.
    #[must_use]
    pub fn to_bytes(&self) -> &[u8] {
        &self.raw
    }
}

/// The ids of a `meta.ini` `category` value (`"<id>,<id>,"`, as `MetaFields::category` holds it): every number in
/// the comma separated list, in order, without a repeated id. Text that is not a number is ignored.
#[must_use]
pub fn parse_ids(value: &str) -> Vec<u32> {
    let mut ids: Vec<u32> = Vec::new();
    for part in value.split(',') {
        if let Ok(id) = part.trim().trim_matches('"').parse::<u32>()
            && !ids.contains(&id)
        {
            ids.push(id);
        }
    }
    ids
}

/// The `category` value for `ids`: the ids in order, each followed by a comma (`5,8,`); the first is the primary.
#[must_use]
pub fn format_ids(ids: &[u32]) -> String {
    let mut out = String::new();
    for id in ids {
        out.push_str(&id.to_string());
        out.push(',');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{covers, ids};

    #[test]
    fn names_are_read_and_the_bytes_survive() {
        covers(&[ids::formats::files::CATEGORIES_DAT]);
        let bytes = b"1|Animations|4,51|0\r\n2|Armour|5|0\r\nnot a line\r\n\xff\xfe|x\r\n7|Sub|9|2";
        let c = Categories::parse(bytes);
        assert_eq!(c.entries.len(), 3);
        assert_eq!(c.name(2), Some("Armour"));
        assert_eq!(c.name(3), None);
        assert_eq!(c.entries[2].parent, 2);
        assert_eq!(c.to_bytes(), bytes);
    }

    #[test]
    fn ids_round_trip() {
        assert_eq!(parse_ids("5,8,"), [5, 8]);
        assert_eq!(parse_ids("0,"), [0]);
        assert_eq!(parse_ids("5,5,x,"), [5]);
        assert_eq!(format_ids(&[5, 8]), "5,8,");
        assert_eq!(format_ids(&[]), "");
    }
}
