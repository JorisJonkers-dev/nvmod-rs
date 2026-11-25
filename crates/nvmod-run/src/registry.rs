// SPDX-License-Identifier: GPL-3.0-only
//! The executables registry: add, edit and remove entries of an Instance's `executables` list, and import
//! MO2's `customExecutables` from a `ModOrganizer.ini` (read only).

use nvmod_core::{Executable, Finding, Severity};
use nvmod_formats::Mo2Instance;
use std::path::Path;

/// Titles `nvmod run` reserves for its built-in entries.
pub const RESERVED: [&str; 2] = ["game", "nv-rs"];

/// A change to an entry: a field that is `Some` is set, one that is `None` is kept. An empty string clears
/// `working_directory`, and `steam_app_id` 0 clears the app id.
#[derive(Debug, Clone, Default)]
pub struct Patch {
    /// The new title (edit only).
    pub title: Option<String>,
    /// The program.
    pub binary: Option<String>,
    /// The arguments.
    pub arguments: Option<String>,
    /// The working directory.
    pub working_directory: Option<String>,
    /// The Steam app id.
    pub steam_app_id: Option<u32>,
    /// Hidden or not.
    pub hidden: Option<bool>,
}

fn same(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// The position of the entry titled `title` (case-insensitive).
#[must_use]
pub fn position(list: &[Executable], title: &str) -> Option<usize> {
    list.iter().position(|e| same(&e.title, title))
}

fn check_title(list: &[Executable], title: &str, except: Option<usize>) -> Result<(), String> {
    if title.trim().is_empty() {
        return Err("a title is needed".to_owned());
    }
    if RESERVED.iter().any(|r| same(r, title)) {
        return Err(format!(
            "`{title}` is a built-in entry of `nvmod run`; choose another title"
        ));
    }
    if position(list, title).is_some_and(|p| Some(p) != except) {
        return Err(format!("an entry titled `{title}` exists already"));
    }
    Ok(())
}

/// Add `entry`.
///
/// # Errors
/// The title is empty, reserved or taken, or the binary is empty.
pub fn add(list: &mut Vec<Executable>, entry: Executable) -> Result<(), String> {
    check_title(list, &entry.title, None)?;
    if entry.binary.trim().is_empty() {
        return Err("a binary is needed".to_owned());
    }
    list.push(entry);
    Ok(())
}

/// Apply `patch` to the entry titled `title`.
///
/// # Errors
/// No such entry, or the new title is empty, reserved or taken, or the new binary is empty.
pub fn edit(list: &mut [Executable], title: &str, patch: &Patch) -> Result<(), String> {
    let at = position(list, title).ok_or_else(|| format!("no entry titled `{title}`"))?;
    if let Some(new) = &patch.title {
        check_title(list, new, Some(at))?;
    }
    if patch.binary.as_deref().is_some_and(|b| b.trim().is_empty()) {
        return Err("a binary is needed".to_owned());
    }
    let e = &mut list[at];
    if let Some(v) = &patch.title {
        v.clone_into(&mut e.title);
    }
    if let Some(v) = &patch.binary {
        v.clone_into(&mut e.binary);
    }
    if let Some(v) = &patch.arguments {
        v.clone_into(&mut e.arguments);
    }
    if let Some(v) = &patch.working_directory {
        e.working_directory = Some(v.clone()).filter(|w| !w.is_empty());
    }
    if let Some(v) = patch.steam_app_id {
        e.steam_app_id = Some(v).filter(|id| *id != 0);
    }
    if let Some(v) = patch.hidden {
        e.hidden = v;
    }
    Ok(())
}

/// Remove the entry titled `title`.
///
/// # Errors
/// No such entry.
pub fn remove(list: &mut Vec<Executable>, title: &str) -> Result<Executable, String> {
    let at = position(list, title).ok_or_else(|| format!("no entry titled `{title}`"))?;
    Ok(list.remove(at))
}

/// What an import found.
#[derive(Debug, Clone, Default)]
pub struct Imported {
    /// The entries added, in MO2's order.
    pub added: Vec<Executable>,
    /// What was refused or skipped, and why.
    pub findings: Vec<Finding>,
}

/// Read the `customExecutables` of the MO2 instance at `mo2` (the instance is only read) and add what maps to
/// this machine to `list`. A path outside `Z:` (any other drive) is a finding and the entry is left out; so is
/// an entry whose title is taken or reserved, which makes a second import add nothing.
///
/// # Errors
/// `mo2` is not an MO2 instance or its `ModOrganizer.ini` cannot be read.
pub fn import_mo2(mo2: &Path, list: &mut Vec<Executable>) -> Result<Imported, String> {
    let instance = Mo2Instance::open(mo2).map_err(|e| e.to_string())?;
    let mut out = Imported::default();
    for e in &instance.executables {
        let file_name = |p: &str| p.rsplit(['/', '\\']).next().unwrap_or_default().to_owned();
        let title = if e.title.trim().is_empty() {
            file_name(&e.binary)
        } else {
            e.title.clone()
        };
        let Some(binary) = &e.binary_host else {
            out.findings.push(Finding::new(
                "exec.unmapped-path",
                Severity::Warning,
                title,
                format!(
                    "the binary `{}` is not on a drive Wine maps to this machine (only Z: is); the entry was left out",
                    e.binary
                ),
            ));
            continue;
        };
        if e.working_directory.is_some() && e.working_directory_host.is_none() {
            out.findings.push(Finding::new(
                "exec.unmapped-path",
                Severity::Warning,
                title,
                format!(
                    "the working directory `{}` is not on a drive Wine maps to this machine (only Z: is); the entry was left out",
                    e.working_directory.as_deref().unwrap_or_default()
                ),
            ));
            continue;
        }
        let entry = Executable {
            title: title.clone(),
            binary: binary.to_string_lossy().into_owned(),
            arguments: e.arguments.clone(),
            working_directory: e
                .working_directory_host
                .as_ref()
                .map(|w| w.to_string_lossy().into_owned()),
            steam_app_id: e.steam_app_id,
            hidden: e.hidden,
        };
        match add(list, entry.clone()) {
            Ok(()) => out.added.push(entry),
            Err(why) => out.findings.push(Finding::new(
                "exec.skipped",
                Severity::Info,
                title,
                format!("not imported: {why}"),
            )),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nvmod_testkit::{covers, ids};

    fn entry(title: &str) -> Executable {
        Executable {
            title: title.to_owned(),
            binary: "/x/tool.exe".to_owned(),
            arguments: String::new(),
            working_directory: None,
            steam_app_id: None,
            hidden: false,
        }
    }

    #[test]
    fn titles_are_unique_without_case_and_not_reserved() {
        covers(&[ids::run::registry::UNIQUE_TITLES]);
        let mut list = Vec::new();
        add(&mut list, entry("FNVEdit")).unwrap();
        assert!(add(&mut list, entry("fnvedit")).is_err());
        assert!(add(&mut list, entry("Game")).is_err());
        assert!(add(&mut list, entry(" ")).is_err());
    }

    #[test]
    fn edit_sets_and_clears_fields_and_remove_returns_the_entry() {
        let mut list = vec![entry("A"), entry("B")];
        edit(
            &mut list,
            "a",
            &Patch {
                title: Some("C".into()),
                working_directory: Some("/w".into()),
                steam_app_id: Some(7),
                hidden: Some(true),
                ..Patch::default()
            },
        )
        .unwrap();
        assert_eq!(list[0].title, "C");
        assert_eq!(list[0].working_directory.as_deref(), Some("/w"));
        assert!(list[0].hidden && list[0].steam_app_id == Some(7));
        edit(
            &mut list,
            "C",
            &Patch {
                working_directory: Some(String::new()),
                steam_app_id: Some(0),
                ..Patch::default()
            },
        )
        .unwrap();
        assert!(list[0].working_directory.is_none() && list[0].steam_app_id.is_none());
        assert!(
            edit(
                &mut list,
                "C",
                &Patch {
                    title: Some("b".into()),
                    ..Patch::default()
                }
            )
            .is_err()
        );
        assert_eq!(remove(&mut list, "B").unwrap().title, "B");
        assert!(remove(&mut list, "B").is_err());
    }
}
