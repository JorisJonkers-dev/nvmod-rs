// SPDX-License-Identifier: GPL-3.0-only
//! The life of one Mod folder: remove, rename, create empty, clone as a backup, hide and unhide a file, set a
//! note, set categories, and the separators (a folder `<label>_separator` with a `meta.ini` and no files).
//!
//! Every change is journaled (kind `lifecycle`, undone by `nvmod rollback`): the four profile files are saved and
//! each file operation is recorded before it is done. A removed Mod folder is moved to
//! `.nvmod/trash/<journal id>/`, so a rollback restores it byte for byte. Only `modlist.txt`, `plugins.txt`,
//! `loadorder.txt` and the named folder's files change; every other line of the lists, the comments and the CRLF
//! line ends are kept. The profile is staged again when it had a Staging tree and the change touches what the
//! Staging tree shows. A dry run computes the same plan and writes nothing.

use crate::journal;
use crate::lists;
use crate::mods::{Doc, ModsError, Place, join};
use nvmod_formats::metaini::{MetaIni, quote_value};
use nvmod_formats::text::{decode, read_limited};
use nvmod_formats::{EntryKind, ModState, Modlist};
use nvmod_instance::{NvmodInstance, find_ci};
use serde::Serialize;
use std::fs;
use std::path::Path;

/// What to do. The Mod (or separator label, or new name) is named separately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Move the Mod folder to the trash and drop its line and the plugin entries only it provides.
    Remove {
        /// Remove although active plugins of other Mods need its plugins.
        force: bool,
        /// The active plugins that need a plugin of this Mod (the caller works them out from the headers).
        dependants: Vec<String>,
    },
    /// Rename the folder and its `modlist.txt` line.
    Rename {
        /// The new name.
        to: String,
    },
    /// Make an empty Mod; the target is its name.
    Create {
        /// Where its line goes; the top when `None`.
        place: Option<Place>,
    },
    /// Copy the Mod folder. A name that matches MO2's backup rule is never listed.
    Clone {
        /// The new name; `<mod> backup`, `<mod> backup1`, ... (the first free) when `None`.
        name: Option<String>,
        /// Hard-link the files instead of copying them (a copy where a link is not possible).
        hardlink: bool,
    },
    /// Rename a file of the Mod to `<name>.mohidden`.
    Hide {
        /// The path inside the Mod folder.
        path: String,
    },
    /// Rename `<name>.mohidden` back to `<name>`.
    Unhide {
        /// The path inside the Mod folder, with or without the suffix.
        path: String,
    },
    /// Set (or, when empty, clear) the Mod's note.
    Note {
        /// The text.
        text: String,
    },
    /// Make a separator; the target is its label.
    SeparatorAdd {
        /// Where its line goes.
        place: Place,
    },
    /// Rename a separator (the target is its label).
    SeparatorRename {
        /// The new label.
        to: String,
    },
    /// Remove a separator (the target is its label).
    SeparatorRemove,
    /// Set the categories of a Mod (the first is the primary one); empty clears them.
    Categories {
        /// The category ids.
        ids: Vec<u32>,
        /// The ids `categories.dat` has; an id outside it is refused.
        known: Vec<u32>,
    },
}

impl Action {
    fn name(&self) -> &'static str {
        match self {
            Action::Remove { .. } => "remove",
            Action::Rename { .. } => "rename",
            Action::Create { .. } => "create",
            Action::Clone { .. } => "clone",
            Action::Hide { .. } => "hide",
            Action::Unhide { .. } => "unhide",
            Action::Note { .. } => "note",
            Action::SeparatorAdd { .. } => "separator-add",
            Action::SeparatorRename { .. } => "separator-rename",
            Action::SeparatorRemove => "separator-remove",
            Action::Categories { .. } => "category-set",
        }
    }
}

/// The result of a change or a dry run.
#[derive(Debug, Clone, Serialize)]
pub struct Report {
    /// `changed`, `unchanged` or `dry-run`.
    pub status: &'static str,
    /// True for a dry run.
    pub dry_run: bool,
    /// The action: `remove`, `rename`, `create`, `clone`, `hide`, `unhide`, `note`, `separator-add`,
    /// `separator-rename`, `separator-remove` or `category-set`.
    pub action: &'static str,
    /// The profile.
    pub profile: String,
    /// The Mod or separator the action is about (for a create or a clone, the one made).
    pub name: String,
    /// The new name of a rename.
    pub new_name: Option<String>,
    /// Whether the change touched `modlist.txt`.
    pub listed: bool,
    /// The plugins dropped from `plugins.txt` and `loadorder.txt`.
    pub plugins_dropped: Vec<String>,
    /// Active plugins of other Mods that need a plugin of the removed Mod (only with `--force`).
    pub dependants: Vec<String>,
    /// The file hidden or unhidden, or the categories set, as text.
    pub detail: Option<String>,
    /// Files in the Staging tree after the change; `None` when it was not staged again.
    pub staged_files: Option<usize>,
    /// The journal `nvmod rollback` uses; `None` for a dry run or when nothing changed.
    pub journal: Option<String>,
}

fn refuse<T>(why: impl Into<String>) -> Result<T, ModsError> {
    Err(ModsError::Refused(why.into()))
}

/// MO2's backup rule: a name that ends in `backup` and digits is a backup Mod, never listed in `modlist.txt`.
#[must_use]
pub fn is_backup_name(name: &str) -> bool {
    name.to_ascii_lowercase()
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .ends_with("backup")
}

fn valid_name(name: &str, what: &str) -> Result<(), ModsError> {
    if name.is_empty()
        || name != name.trim()
        || name == "."
        || name == ".."
        || name.contains(['/', '\\'])
        || name.chars().any(char::is_control)
        || name.len() > 200
    {
        return refuse(format!(
            "`{name}` is not a usable {what} name (empty, padded with blanks, a dot name, a path or a control character)"
        ));
    }
    Ok(())
}

fn valid_mod_name(name: &str) -> Result<(), ModsError> {
    valid_name(name, "Mod")?;
    if name.ends_with("_separator") {
        return refuse(format!(
            "`{name}` ends in `_separator`, which names a separator; use `nvmod mods separator add`"
        ));
    }
    if is_backup_name(name) {
        return refuse(format!(
            "`{name}` matches MO2's backup rule (ends in `backup` and digits), so it would never be listed; use `nvmod mods clone`"
        ));
    }
    Ok(())
}

fn separator_name(label: &str) -> String {
    format!("{label}_separator")
}

/// A relative path of `/` separated names that stays inside the Mod folder.
fn inner_path(path: &str) -> Result<Vec<&str>, ModsError> {
    let parts: Vec<&str> = path.split('/').collect();
    if path.contains('\\')
        || parts
            .iter()
            .any(|p| p.is_empty() || *p == "." || *p == ".." || p.chars().any(char::is_control))
    {
        return refuse(format!(
            "`{path}` is not a path inside the Mod (use `/`, no `..`, no empty parts)"
        ));
    }
    Ok(parts)
}

/// A step on the Instance's files. Paths are relative to the Instance folder.
#[derive(Debug)]
enum Step {
    /// Move the Mod folder to the journal's trash.
    Trash(String),
    Move {
        from: String,
        to: String,
    },
    MakeDir {
        path: String,
        files: Vec<(String, Vec<u8>)>,
    },
    Copy {
        from: String,
        to: String,
        hardlink: bool,
    },
    Write {
        path: String,
        bytes: Vec<u8>,
    },
}

#[derive(Default)]
struct Plan {
    steps: Vec<Step>,
    modlist: Option<Vec<u8>>,
    plugins: Option<lists::NewLists>,
    /// The Staging tree shows the change.
    stage: bool,
    new_name: Option<String>,
    plugins_dropped: Vec<String>,
    dependants: Vec<String>,
    detail: Option<String>,
    name: String,
}

struct Found {
    /// The name as the list (or the folder, for an unlisted Mod) spells it.
    name: String,
    /// The folder under `mods/`, when it exists.
    folder: Option<String>,
    /// The index among the entries of `modlist.txt`, when listed.
    entry: Option<usize>,
}

fn folder_of(mods: &Path, name: &str) -> Option<String> {
    find_ci(mods, name).and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
}

fn resolve(doc: &Doc<'_>, parsed: &Modlist, mods: &Path, wanted: &str) -> Result<Found, ModsError> {
    if doc.names.iter().any(|n| n.eq_ignore_ascii_case(wanted)) {
        let i = doc.find(wanted)?;
        let name = parsed.entries[i].name.clone();
        return Ok(Found {
            folder: folder_of(mods, &name),
            name,
            entry: Some(i),
        });
    }
    match folder_of(mods, wanted) {
        Some(folder) => Ok(Found {
            name: folder.clone(),
            folder: Some(folder),
            entry: None,
        }),
        None => refuse(format!(
            "`{wanted}` is neither in modlist.txt nor a folder under mods/"
        )),
    }
}

/// The line index in the full list of lines where a new entry for `place` goes.
fn insert_index(doc: &Doc<'_>, place: &Place) -> Result<usize, ModsError> {
    let after_last = doc.entries.last().map_or(doc.all.len(), |i| i + 1);
    Ok(match place {
        Place::Top => doc.entries.first().copied().unwrap_or(doc.all.len()),
        Place::Bottom => after_last,
        Place::Position(n) => {
            if *n == 0 {
                return refuse("position 0 is outside the list");
            }
            doc.entries.get(n - 1).copied().unwrap_or(after_last)
        }
        Place::Before(other) => doc.entries[doc.find(other)?],
        Place::After(other) => doc.entries[doc.find(other)?] + 1,
    })
}

/// The plugin files at the top of a Mod folder (`.esp`, `.esm`, `.esl`).
fn plugins_of(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| {
            Path::new(n).extension().is_some_and(|x| {
                ["esp", "esm", "esl"]
                    .iter()
                    .any(|w| x.eq_ignore_ascii_case(w))
            })
        })
        .collect();
    names.sort();
    names
}

fn meta_for_new(extra: &[(&str, &str)]) -> Vec<u8> {
    let mut meta = MetaIni::default();
    for (key, value) in extra {
        meta.set("General", key, value);
    }
    meta.to_bytes()
}

fn separator_meta() -> Vec<u8> {
    b"[General]\r\nmodid=0\r\nversion=\r\nnewestVersion=\r\ncategory=0\r\ninstallationFile=\r\n"
        .to_vec()
}

fn rel_mod(folder: &str) -> String {
    format!("mods/{folder}")
}

/// `line` with its `+`, `-` or `*` prefix kept and the name replaced.
fn renamed_line(line: &str, to: &str) -> String {
    let line = line.trim();
    match line.chars().next() {
        Some(c @ ('+' | '-' | '*')) => format!("{c}{to}"),
        _ => to.to_owned(),
    }
}

struct Ctx<'a> {
    instance: &'a NvmodInstance,
    profile: &'a str,
    doc: Doc<'a>,
    parsed: Modlist,
    mods: std::path::PathBuf,
    staged: bool,
}

impl Ctx<'_> {
    fn lines(&self) -> Vec<String> {
        self.doc.all.iter().map(|l| (*l).to_owned()).collect()
    }

    fn taken(&self, name: &str, except: Option<&str>) -> bool {
        let same = |n: &str| except.is_some_and(|e| e.eq_ignore_ascii_case(n));
        (self.doc.names.iter().any(|n| n.eq_ignore_ascii_case(name)) && !same(name))
            || (folder_of(&self.mods, name).is_some() && !same(name))
    }

    fn mod_found(&self, wanted: &str) -> Result<Found, ModsError> {
        let found = resolve(&self.doc, &self.parsed, &self.mods, wanted)?;
        if let Some(i) = found.entry {
            let entry = &self.parsed.entries[i];
            if entry.kind == EntryKind::Separator {
                return refuse(format!(
                    "`{}` is a separator; use `nvmod mods separator`",
                    entry.name
                ));
            }
            if entry.state == ModState::Foreign {
                return refuse(format!("`{}` is not a Mod", entry.name));
            }
        }
        Ok(found)
    }

    fn enabled(&self, found: &Found) -> bool {
        found
            .entry
            .is_some_and(|i| self.parsed.entries[i].state == ModState::Enabled)
    }

    fn folder_or_refuse(found: &Found) -> Result<&str, ModsError> {
        found.folder.as_deref().map_or_else(
            || {
                refuse(format!(
                    "the Mod folder of `{}` is not in the Instance",
                    found.name
                ))
            },
            Ok,
        )
    }

    fn read_profile(&self, file: &str) -> Option<Vec<u8>> {
        fs::read(self.instance.profile_dir(self.profile).join(file)).ok()
    }

    fn plan(&self, target: &str, action: &Action) -> Result<Plan, ModsError> {
        let mut plan = Plan {
            name: target.to_owned(),
            ..Plan::default()
        };
        match action {
            Action::Remove { force, dependants } => {
                self.remove(&mut plan, target, *force, dependants)?;
            }
            Action::Rename { to } => self.rename(&mut plan, target, to)?,
            Action::Create { place } => self.create(&mut plan, target, place.as_ref())?,
            Action::Clone { name, hardlink } => {
                self.clone_mod(&mut plan, target, name.as_deref(), *hardlink)?;
            }
            Action::Hide { path } => self.hide(&mut plan, target, path, true)?,
            Action::Unhide { path } => self.hide(&mut plan, target, path, false)?,
            Action::Note { text } => self.note(&mut plan, target, text)?,
            Action::Categories { ids, known } => self.categories(&mut plan, target, ids, known)?,
            Action::SeparatorAdd { place } => self.separator_add(&mut plan, target, place)?,
            Action::SeparatorRename { to } => self.separator_rename(&mut plan, target, to)?,
            Action::SeparatorRemove => self.separator_remove(&mut plan, target)?,
        }
        Ok(plan)
    }

    fn remove(
        &self,
        plan: &mut Plan,
        target: &str,
        force: bool,
        dependants: &[String],
    ) -> Result<(), ModsError> {
        let found = self.mod_found(target)?;
        plan.name.clone_from(&found.name);
        if !dependants.is_empty() && !force {
            return refuse(format!(
                "active plugin(s) of other Mods need a plugin of `{}`: {}; pass --force to remove it anyway",
                found.name,
                dependants.join(", ")
            ));
        }
        plan.dependants = dependants.to_vec();
        if let Some(folder) = &found.folder {
            plan.steps.push(Step::Trash(folder.clone()));
        }
        if let Some(i) = found.entry {
            let mut lines = self.lines();
            lines.remove(self.doc.entries[i]);
            plan.modlist = Some(join(&lines));
            plan.stage = true;
            if let Some(folder) = &found.folder {
                let theirs = plugins_of(&self.mods.join(folder));
                let others: Vec<String> = self
                    .parsed
                    .enabled_mods()
                    .filter(|e| e.name != found.name)
                    .filter_map(|e| folder_of(&self.mods, &e.name))
                    .flat_map(|f| plugins_of(&self.mods.join(f)))
                    .collect();
                plan.plugins_dropped = theirs
                    .into_iter()
                    .filter(|p| !others.iter().any(|o| o.eq_ignore_ascii_case(p)))
                    .collect();
                if !plan.plugins_dropped.is_empty() {
                    plan.plugins = lists::remove_plugins(
                        self.read_profile("plugins.txt").as_deref(),
                        self.read_profile("loadorder.txt").as_deref(),
                        &plan.plugins_dropped,
                    )
                    .map_err(ModsError::Refused)?;
                }
            }
        }
        Ok(())
    }

    fn rename(&self, plan: &mut Plan, target: &str, to: &str) -> Result<(), ModsError> {
        let found = self.mod_found(target)?;
        plan.name.clone_from(&found.name);
        valid_mod_name(to)?;
        if to == found.name {
            return refuse(format!("`{to}` is already the name"));
        }
        if self.taken(to, Some(&found.name)) {
            return refuse(format!("`{to}` already exists"));
        }
        let folder = Self::folder_or_refuse(&found)?;
        plan.steps.push(Step::Move {
            from: rel_mod(folder),
            to: rel_mod(to),
        });
        if let Some(i) = found.entry {
            let mut lines = self.lines();
            let at = self.doc.entries[i];
            lines[at] = renamed_line(&lines[at], to);
            plan.modlist = Some(join(&lines));
            plan.stage = true;
        }
        plan.new_name = Some(to.to_owned());
        Ok(())
    }

    fn create(&self, plan: &mut Plan, name: &str, place: Option<&Place>) -> Result<(), ModsError> {
        valid_mod_name(name)?;
        if self.taken(name, None) {
            return refuse(format!("`{name}` already exists"));
        }
        let at = insert_index(&self.doc, place.unwrap_or(&Place::Top))?;
        let mut lines = self.lines();
        lines.insert(at, format!("+{name}"));
        plan.modlist = Some(join(&lines));
        plan.steps.push(Step::MakeDir {
            path: rel_mod(name),
            files: vec![(
                "meta.ini".to_owned(),
                meta_for_new(&[("gameName", "newvegas")]),
            )],
        });
        plan.stage = true;
        Ok(())
    }

    fn clone_mod(
        &self,
        plan: &mut Plan,
        target: &str,
        name: Option<&str>,
        hardlink: bool,
    ) -> Result<(), ModsError> {
        let found = self.mod_found(target)?;
        let folder = Self::folder_or_refuse(&found)?;
        let dest = if let Some(name) = name {
            valid_name(name, "Mod")?;
            if name.ends_with("_separator") {
                return refuse("a clone cannot be named like a separator");
            }
            if self.taken(name, None) {
                return refuse(format!("`{name}` already exists"));
            }
            name.to_owned()
        } else {
            let base = format!("{} backup", found.name);
            let mut candidate = base.clone();
            let mut n = 0_u32;
            while self.taken(&candidate, None) {
                n += 1;
                candidate = format!("{base}{n}");
            }
            candidate
        };
        valid_name(&dest, "Mod")?;
        plan.name.clone_from(&dest);
        plan.detail = Some(format!("copy of {}", found.name));
        plan.steps.push(Step::Copy {
            from: rel_mod(folder),
            to: rel_mod(&dest),
            hardlink,
        });
        if !is_backup_name(&dest) {
            // Listed but disabled, just below the original (the top when the original is not listed).
            let at = match found.entry {
                Some(i) => self.doc.entries[i] + 1,
                None => insert_index(&self.doc, &Place::Top)?,
            };
            let mut lines = self.lines();
            lines.insert(at, format!("-{dest}"));
            plan.modlist = Some(join(&lines));
            plan.stage = true;
        }
        Ok(())
    }

    fn hide(&self, plan: &mut Plan, target: &str, path: &str, hide: bool) -> Result<(), ModsError> {
        let found = self.mod_found(target)?;
        plan.name.clone_from(&found.name);
        let folder = Self::folder_or_refuse(&found)?;
        let suffix = nvmod_formats::scan::HIDDEN_SUFFIX;
        let has_suffix = path.to_ascii_lowercase().ends_with(suffix);
        let inner = if has_suffix {
            &path[..path.len() - suffix.len()]
        } else {
            path
        };
        inner_path(inner)?;
        if inner.eq_ignore_ascii_case("meta.ini") {
            return refuse("meta.ini is not a Mod file and cannot be hidden");
        }
        let (visible, hidden) = (
            self.mods.join(folder).join(inner),
            self.mods.join(folder).join(format!("{inner}{suffix}")),
        );
        let (from, to, from_rel, to_rel) = if hide {
            if has_suffix {
                return refuse(format!("`{path}` is already hidden"));
            }
            (
                visible,
                hidden,
                format!("{}/{inner}", rel_mod(folder)),
                format!("{}/{inner}{suffix}", rel_mod(folder)),
            )
        } else {
            (
                hidden,
                visible,
                format!("{}/{inner}{suffix}", rel_mod(folder)),
                format!("{}/{inner}", rel_mod(folder)),
            )
        };
        let is_file = fs::symlink_metadata(&from).is_ok_and(|m| m.is_file());
        if !is_file {
            return refuse(format!(
                "`{}` is not a file of `{}`",
                from_rel.trim_start_matches(&format!("{}/", rel_mod(folder))),
                found.name
            ));
        }
        if fs::symlink_metadata(&to).is_ok() {
            return refuse(format!(
                "`{}` already exists in `{}`",
                to_rel.trim_start_matches(&format!("{}/", rel_mod(folder))),
                found.name
            ));
        }
        plan.steps.push(Step::Move {
            from: from_rel,
            to: to_rel,
        });
        plan.detail = Some(inner.to_owned());
        plan.stage = self.enabled(&found);
        Ok(())
    }

    fn meta_write(&self, plan: &mut Plan, folder: &str, edit: impl FnOnce(&mut MetaIni)) {
        let path = self.mods.join(folder).join("meta.ini");
        let old = read_limited(&path).unwrap_or_default();
        let mut meta = MetaIni::parse(&old);
        edit(&mut meta);
        let bytes = meta.to_bytes();
        if bytes != old {
            plan.steps.push(Step::Write {
                path: format!("{}/meta.ini", rel_mod(folder)),
                bytes,
            });
        }
    }

    fn note(&self, plan: &mut Plan, target: &str, text: &str) -> Result<(), ModsError> {
        let found = self.mod_found(target)?;
        plan.name.clone_from(&found.name);
        let folder = Self::folder_or_refuse(&found)?;
        if text.chars().any(char::is_control) {
            return refuse("a note is one line without control characters");
        }
        self.meta_write(plan, folder, |meta| {
            if text.is_empty() {
                meta.remove("General", "notes");
            } else {
                meta.set("General", "notes", &quote_value(text));
            }
        });
        plan.detail = Some(text.to_owned());
        Ok(())
    }

    fn categories(
        &self,
        plan: &mut Plan,
        target: &str,
        ids: &[u32],
        known: &[u32],
    ) -> Result<(), ModsError> {
        let found = self.mod_found(target)?;
        plan.name.clone_from(&found.name);
        let folder = Self::folder_or_refuse(&found)?;
        let unknown: Vec<String> = ids
            .iter()
            .filter(|id| !known.contains(id))
            .map(ToString::to_string)
            .collect();
        if !unknown.is_empty() {
            return refuse(format!(
                "category id(s) {} are not in categories.dat of the Instance",
                unknown.join(", ")
            ));
        }
        let value = nvmod_formats::categories::format_ids(ids);
        self.meta_write(plan, folder, |meta| {
            if ids.is_empty() {
                meta.remove("General", "category");
            } else {
                meta.set("General", "category", &quote_value(&value));
            }
        });
        plan.detail = Some(value);
        Ok(())
    }

    fn separator_found(&self, label: &str) -> Result<Found, ModsError> {
        let name = if label.ends_with("_separator") {
            label.to_owned()
        } else {
            separator_name(label)
        };
        let found = resolve(&self.doc, &self.parsed, &self.mods, &name)?;
        match found.entry {
            Some(i) if self.parsed.entries[i].kind == EntryKind::Separator => Ok(found),
            _ => refuse(format!("`{label}` is not a separator")),
        }
    }

    fn separator_add(&self, plan: &mut Plan, label: &str, place: &Place) -> Result<(), ModsError> {
        valid_name(label, "separator")?;
        let name = separator_name(label);
        if self.taken(&name, None) {
            return refuse(format!("a separator `{label}` already exists"));
        }
        plan.name.clone_from(&name);
        let at = insert_index(&self.doc, place)?;
        let mut lines = self.lines();
        lines.insert(at, format!("-{name}"));
        plan.modlist = Some(join(&lines));
        plan.steps.push(Step::MakeDir {
            path: rel_mod(&name),
            files: vec![("meta.ini".to_owned(), separator_meta())],
        });
        plan.stage = true;
        Ok(())
    }

    fn separator_rename(&self, plan: &mut Plan, label: &str, to: &str) -> Result<(), ModsError> {
        let found = self.separator_found(label)?;
        valid_name(to, "separator")?;
        let new = separator_name(to);
        plan.name.clone_from(&found.name);
        if new == found.name {
            return refuse(format!("`{to}` is already the label"));
        }
        if self.taken(&new, Some(&found.name)) {
            return refuse(format!("a separator `{to}` already exists"));
        }
        if let Some(folder) = &found.folder {
            plan.steps.push(Step::Move {
                from: rel_mod(folder),
                to: rel_mod(&new),
            });
        }
        let i = found.entry.unwrap_or_default();
        let mut lines = self.lines();
        let at = self.doc.entries[i];
        lines[at] = renamed_line(&lines[at], &new);
        plan.modlist = Some(join(&lines));
        plan.new_name = Some(new);
        plan.stage = true;
        Ok(())
    }

    fn separator_remove(&self, plan: &mut Plan, label: &str) -> Result<(), ModsError> {
        let found = self.separator_found(label)?;
        plan.name.clone_from(&found.name);
        if let Some(folder) = &found.folder {
            plan.steps.push(Step::Trash(folder.clone()));
        }
        let mut lines = self.lines();
        lines.remove(self.doc.entries[found.entry.unwrap_or_default()]);
        plan.modlist = Some(join(&lines));
        plan.stage = true;
        Ok(())
    }
}

/// One category of a Mod as `mods list` reports it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CategoryRef {
    /// The id in `meta.ini`.
    pub id: u32,
    /// Its name in `categories.dat`; `None` when the file does not have the id (it is kept, not dropped).
    pub name: Option<String>,
}

/// A row of `mods list` with the categories of the Mod.
#[derive(Debug, Clone, Serialize)]
pub struct CategorisedRow {
    /// The row.
    #[serde(flatten)]
    pub row: crate::mods::ModRow,
    /// The categories in `meta.ini`, the primary one first.
    pub categories: Vec<CategoryRef>,
    /// Whether the Mod is generated (its `meta.ini` carries the marker of the Toolbox's generated LOD Mod). Such a
    /// Mod is left out of update checks and conflict results. Only written when true.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub generated: bool,
}

/// The rows of `profile` with their categories (from each `meta.ini`, named by `categories.dat` at the Instance
/// root when there is one). With `only`, just the Mods that have that category id.
#[must_use]
pub fn list_categorised(
    instance: &NvmodInstance,
    profile: &str,
    only: Option<u32>,
) -> Vec<CategorisedRow> {
    let cats = fs::read(instance.root.join("categories.dat"))
        .map_or_default(|b| nvmod_formats::categories::Categories::parse(&b));
    let mods = instance.root.join("mods");
    crate::mods::list(instance, profile)
        .into_iter()
        .map(|row| {
            let ids = (row.kind == "mod")
                .then(|| folder_of(&mods, &row.name))
                .flatten()
                .and_then(|f| read_limited(&mods.join(f).join("meta.ini")).ok())
                .and_then(|b| MetaIni::parse(&b).fields().category)
                .map_or_default(|c| nvmod_formats::categories::parse_ids(&c));
            let categories = ids
                .into_iter()
                .map(|id| CategoryRef {
                    id,
                    name: cats.name(id).map(str::to_owned),
                })
                .collect();
            let generated = row.kind == "mod"
                && folder_of(&mods, &row.name)
                    .is_some_and(|f| nvmod_formats::metaini::is_generated_mod_dir(&mods.join(f)));
            CategorisedRow {
                row,
                categories,
                generated,
            }
        })
        .filter(|r| only.is_none_or(|id| r.categories.iter().any(|c| c.id == id)))
        .collect()
}

pub(crate) fn copy_tree(from: &Path, to: &Path, hardlink: bool) -> Result<(), String> {
    let io = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
    fs::create_dir_all(to).map_err(|e| io(to, e))?;
    for entry in fs::read_dir(from).map_err(|e| io(from, e))? {
        let entry = entry.map_err(|e| io(from, e))?;
        let (source, dest) = (entry.path(), to.join(entry.file_name()));
        let kind = entry.file_type().map_err(|e| io(&source, e))?;
        if kind.is_dir() {
            copy_tree(&source, &dest, hardlink)?;
        } else if kind.is_file() {
            nvmod_core::cancel::step_point("copy")?;
            if !(hardlink && fs::hard_link(&source, &dest).is_ok()) {
                fs::copy(&source, &dest).map_err(|e| io(&source, e))?;
            }
        } else {
            return Err(format!(
                "{} is a link or a special file; a Mod folder holds neither",
                source.display()
            ));
        }
    }
    Ok(())
}

fn do_step(
    instance: &NvmodInstance,
    entry: &mut journal::Entry,
    step: &Step,
) -> Result<(), String> {
    let io = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
    let at = |rel: &str| instance.root.join(rel);
    match step {
        Step::Trash(folder) => {
            let from = rel_mod(folder);
            let to = format!(".nvmod/trash/{}/{folder}", entry.id);
            entry.record_move(instance, &from, &to)?;
            let target = at(&to);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|e| io(parent, e))?;
            }
            fs::rename(at(&from), &target).map_err(|e| io(&target, e))
        }
        Step::Move { from, to } => {
            entry.record_move(instance, from, to)?;
            fs::rename(at(from), at(to)).map_err(|e| io(&at(to), e))
        }
        Step::MakeDir { path, files } => {
            entry.record_create(instance, path)?;
            fs::create_dir_all(at(path)).map_err(|e| io(&at(path), e))?;
            for (name, bytes) in files {
                journal::write_atomic(&at(path).join(name), bytes)?;
            }
            Ok(())
        }
        Step::Copy { from, to, hardlink } => {
            entry.record_create(instance, to)?;
            copy_tree(&at(from), &at(to), *hardlink)
        }
        Step::Write { path, bytes } => {
            entry.record_file(instance, path)?;
            journal::write_atomic(&at(path), bytes)
        }
    }
}

/// Carry out `action` on `target` in `profile`, or plan to.
///
/// # Errors
/// See [`ModsError`]. A refusal writes nothing; a failure puts the Instance back.
pub fn apply(
    instance: &NvmodInstance,
    profile: &str,
    target: &str,
    action: &Action,
    dry_run: bool,
) -> Result<Report, ModsError> {
    let list = instance.profile_dir(profile).join("modlist.txt");
    let bytes = fs::read(&list).map_err(|_| {
        ModsError::Refused(format!(
            "profile `{profile}` has no modlist.txt; nothing to change"
        ))
    })?;
    let text = decode(&bytes);
    let ctx = Ctx {
        instance,
        profile,
        doc: Doc::parse(&text),
        parsed: Modlist::parse(&text),
        mods: instance.root.join("mods"),
        staged: journal::staging_exists(instance, profile),
    };
    let plan = ctx.plan(target, action)?;
    let unchanged = plan.steps.is_empty() && plan.modlist.is_none() && plan.plugins.is_none();
    let mut report = Report {
        status: if dry_run {
            "dry-run"
        } else if unchanged {
            "unchanged"
        } else {
            "changed"
        },
        dry_run,
        action: action.name(),
        profile: profile.to_owned(),
        name: plan.name.clone(),
        new_name: plan.new_name.clone(),
        listed: plan.modlist.is_some(),
        plugins_dropped: plan.plugins_dropped.clone(),
        dependants: plan.dependants.clone(),
        detail: plan.detail.clone(),
        staged_files: None,
        journal: None,
    };
    if dry_run || unchanged {
        return Ok(report);
    }
    let mut entry = journal::begin_kind(instance, profile, "", journal::KIND_LIFECYCLE)
        .map_err(ModsError::Refused)?;
    let dir = instance.profile_dir(profile);
    let outcome = (|| -> Result<Option<usize>, String> {
        for step in &plan.steps {
            crate::fail_point("mod")?;
            do_step(instance, &mut entry, step)?;
        }
        crate::fail_point("lists")?;
        if let Some((plugins, order)) = &plan.plugins {
            journal::write_atomic(&dir.join("plugins.txt"), plugins)?;
            journal::write_atomic(&dir.join("loadorder.txt"), order)?;
        }
        if let Some(new) = &plan.modlist {
            journal::write_atomic(&list, new)?;
        }
        if !(ctx.staged && plan.stage) {
            return Ok(None);
        }
        crate::fail_point("stage")?;
        nvmod_stage::stage(instance, profile, nvmod_stage::Options::default())
            .map(|s| Some(s.manifest.files))
            .map_err(|e| match e {
                nvmod_stage::StageError::Refused(m) | nvmod_stage::StageError::Failed(m) => m,
            })
    })();
    match outcome {
        Ok(files) => {
            journal::complete(instance, &entry).map_err(ModsError::Failed)?;
            report.staged_files = files;
            report.journal = Some(entry.id);
            Ok(report)
        }
        Err(why) => match journal::undo(instance, &entry) {
            Ok(()) => Err(ModsError::Failed(format!(
                "{why}; the change was rolled back"
            ))),
            Err(undo) => Err(ModsError::Failed(format!(
                "{why}; the rollback failed too ({undo}); run `nvmod rollback`"
            ))),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backup_rule_matches_mo2() {
        for yes in [
            "Mod backup",
            "Mod backup1",
            "x Backup22",
            "backup",
            "Foo BACKUP7",
        ] {
            assert!(is_backup_name(yes), "{yes}");
        }
        for no in ["Mod backups", "Backup mod", "Mod", "backup x", "b4ckup"] {
            assert!(!is_backup_name(no), "{no}");
        }
    }

    #[test]
    fn names_are_checked() {
        assert!(valid_mod_name("Weapons [NV] v1.2").is_ok());
        for bad in [
            "",
            " x",
            "a/b",
            "a\\b",
            "..",
            "G_separator",
            "Old backup2",
            "a\nb",
        ] {
            assert!(valid_mod_name(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_renamed_line_keeps_its_prefix() {
        assert_eq!(renamed_line("-Old", "New"), "-New");
        assert_eq!(renamed_line("*DLC", "New"), "*New");
        assert_eq!(renamed_line("Old", "New"), "New");
    }

    #[test]
    fn paths_stay_inside_the_mod() {
        assert!(inner_path("a/b.dds").is_ok());
        for bad in ["", "../a", "a//b", "/a", "a\\b", "./a"] {
            assert!(inner_path(bad).is_err(), "{bad:?}");
        }
    }
}
