// SPDX-License-Identifier: GPL-3.0-only
//! Overwrite: what programs wrote that no Mod owns. List it, make a Mod of it, move files into a Mod, delete files
//! (to the trash, reversible) and sync the files a Mod provides back into that Mod. The Overwrite folder is
//! `<instance>/overwrite/`; paths are `/` separated and relative to it. Every change goes through the lifecycle
//! journal, so `nvmod rollback` takes it back, and the Staging tree (which shows Overwrite) is built again.

use super::{Ctx, Plan, Step, insert_index, refuse, rel_mod, valid_mod_name};
use crate::lists;
use crate::mods::{ModsError, Place, join};
use nvmod_instance::{NvmodInstance, find_ci};
use nvmod_stage::glob::selects;
use serde::Serialize;
use std::fs;
use std::path::Path;

/// The folder of Overwrite, relative to the Instance folder.
const OVERWRITE: &str = "overwrite";

/// One file of Overwrite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OverwriteFile {
    /// The path inside Overwrite, `/` separated.
    pub path: String,
    /// Size in bytes.
    pub size: u64,
    /// The enabled Mods that provide the same path (ignoring case), highest priority first.
    pub providers: Vec<String>,
}

/// What `nvmod overwrite list` shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OverwriteList {
    /// The profile whose Mods are looked at for providers.
    pub profile: String,
    /// The files, sorted by path.
    pub files: Vec<OverwriteFile>,
}

/// Every file under `dir` as a `/` separated path, sorted. A link or other special file is refused: a program
/// that left one in Overwrite needs a person's decision.
fn walk(dir: &Path) -> Result<Vec<(String, u64)>, String> {
    fn go(dir: &Path, prefix: &str, out: &mut Vec<(String, u64)>) -> Result<(), String> {
        let entries = match fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("{}: {e}", dir.display())),
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let rel = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}/{name}")
            };
            let kind = entry
                .file_type()
                .map_err(|e| format!("{}: {e}", entry.path().display()))?;
            if kind.is_dir() {
                go(&entry.path(), &rel, out)?;
            } else if kind.is_file() {
                let size = entry.metadata().map_or(0, |m| m.len());
                out.push((rel, size));
            } else {
                return Err(format!(
                    "{} is a link or a special file; Overwrite holds only files and folders",
                    entry.path().display()
                ));
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    go(dir, "", &mut out)?;
    out.sort();
    Ok(out)
}

/// A path inside Overwrite: `/` separated names, none empty, `.` or `..`.
fn checked(path: &str) -> Result<(), ModsError> {
    let bad = path.is_empty()
        || path.contains('\\')
        || path
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == ".." || p.chars().any(char::is_control));
    if bad {
        return refuse(format!(
            "`{path}` is not a path inside Overwrite (use `/`, no `..`, no empty parts)"
        ));
    }
    Ok(())
}

/// `rel` as it is spelled on disk under `base` (each existing folder or file name is taken ignoring case), and
/// whether the whole path exists.
fn spelled(base: &Path, rel: &str) -> (String, bool) {
    let mut dir = base.to_path_buf();
    let mut out: Vec<String> = Vec::new();
    let mut exists = true;
    for part in rel.split('/') {
        if exists && let Some(found) = find_ci(&dir, part) {
            let name = found
                .file_name()
                .map_or_else(|| part.to_owned(), |n| n.to_string_lossy().into_owned());
            dir = found;
            out.push(name);
        } else {
            exists = false;
            out.push(part.to_owned());
        }
    }
    (out.join("/"), exists)
}

/// The ancestors of `paths` inside Overwrite (relative to the Instance folder), deepest first, without Overwrite.
fn ancestors(paths: &[String]) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();
    for path in paths {
        let mut up = Path::new(path).parent();
        while let Some(dir) = up.filter(|d| !d.as_os_str().is_empty()) {
            let text = dir.to_string_lossy().into_owned();
            if !dirs.contains(&text) {
                dirs.push(text);
            }
            up = dir.parent();
        }
    }
    dirs.sort_by_key(|d| std::cmp::Reverse(d.matches('/').count()));
    dirs.into_iter()
        .map(|d| format!("{OVERWRITE}/{d}"))
        .collect()
}

/// The enabled Mods that hold each of `wanted` (lower-cased `/` paths), highest priority first.
fn providers(ctx: &Ctx<'_>, wanted: &[String]) -> Result<Vec<Vec<String>>, ModsError> {
    let lower: Vec<String> = wanted.iter().map(|p| p.to_lowercase()).collect();
    let mut found: Vec<Vec<String>> = vec![Vec::new(); wanted.len()];
    for entry in ctx.parsed.enabled_mods() {
        let Some(folder) = super::folder_of(&ctx.mods, &entry.name) else {
            continue;
        };
        let files = walk(&ctx.mods.join(&folder)).map_err(ModsError::Refused)?;
        for (path, _) in files {
            let path = path.to_lowercase();
            if let Some(i) = lower.iter().position(|w| *w == path) {
                found[i].push(entry.name.clone());
            }
        }
    }
    Ok(found)
}

/// The files of Overwrite and which enabled Mods of `profile` provide the same paths.
///
/// # Errors
/// Overwrite holds a link or a special file, or a Mod folder cannot be read.
pub fn list(instance: &NvmodInstance, profile: &str) -> Result<OverwriteList, ModsError> {
    let text = fs::read(instance.profile_dir(profile).join("modlist.txt")).unwrap_or_default();
    let text = nvmod_formats::text::decode(&text);
    let ctx = Ctx {
        instance,
        profile,
        doc: crate::mods::Doc::parse(&text),
        parsed: nvmod_formats::Modlist::parse(&text),
        mods: instance.root.join("mods"),
        staged: false,
    };
    let files = walk(&instance.root.join(OVERWRITE)).map_err(ModsError::Refused)?;
    let paths: Vec<String> = files.iter().map(|(p, _)| p.clone()).collect();
    let found = providers(&ctx, &paths)?;
    Ok(OverwriteList {
        profile: profile.to_owned(),
        files: files
            .into_iter()
            .zip(found)
            .map(|((path, size), providers)| OverwriteFile {
                path,
                size,
                providers,
            })
            .collect(),
    })
}

impl Ctx<'_> {
    fn overwrite_dir(&self) -> std::path::PathBuf {
        self.instance.root.join(OVERWRITE)
    }

    /// The files of Overwrite under `path` (the path itself for a file), spelled as on disk.
    fn overwrite_selection(&self, path: &str) -> Result<(String, Vec<String>), ModsError> {
        checked(path)?;
        let (actual, exists) = spelled(&self.overwrite_dir(), path);
        if !exists {
            return refuse(format!("`{path}` is not in Overwrite"));
        }
        let at = self.overwrite_dir().join(&actual);
        if at.is_dir() {
            let files = walk(&at).map_err(ModsError::Refused)?;
            Ok((
                actual.clone(),
                files
                    .into_iter()
                    .map(|(f, _)| format!("{actual}/{f}"))
                    .collect(),
            ))
        } else {
            Ok((actual.clone(), vec![actual]))
        }
    }

    pub(super) fn overwrite_create_mod(
        &self,
        plan: &mut Plan,
        name: &str,
        globs: &[String],
    ) -> Result<(), ModsError> {
        valid_mod_name(name)?;
        if self.taken(name, None) {
            return refuse(format!("`{name}` already exists"));
        }
        let all = walk(&self.overwrite_dir()).map_err(ModsError::Refused)?;
        if all.is_empty() {
            return refuse("Overwrite holds no files");
        }
        for glob in globs {
            checked(&glob.replace("**", "x").replace(['*', '?'], "x"))?;
        }
        let chosen: Vec<String> = all
            .into_iter()
            .map(|(p, _)| p)
            .filter(|p| globs.is_empty() || globs.iter().any(|g| selects(g, p)))
            .collect();
        if chosen.is_empty() {
            return refuse(format!("nothing in Overwrite matches {}", globs.join(", ")));
        }
        plan.steps.push(Step::MakeDir {
            path: rel_mod(name),
            files: vec![(
                "meta.ini".to_owned(),
                super::meta_for_new(&[("gameName", "newvegas")]),
            )],
        });
        for file in &chosen {
            plan.steps.push(Step::Move {
                from: format!("{OVERWRITE}/{file}"),
                to: format!("{}/{file}", rel_mod(name)),
            });
        }
        plan.steps.push(Step::Prune(ancestors(&chosen)));
        let at = insert_index(&self.doc, &Place::Top)?;
        let mut lines = self.lines();
        lines.insert(at, format!("+{name}"));
        plan.modlist = Some(join(&lines));
        let carried: Vec<String> = chosen
            .iter()
            .filter(|p| !p.contains('/'))
            .filter(|p| {
                Path::new(p).extension().is_some_and(|x| {
                    ["esp", "esm", "esl"]
                        .iter()
                        .any(|w| x.eq_ignore_ascii_case(w))
                })
            })
            .cloned()
            .collect();
        if !carried.is_empty() {
            plan.plugins = lists::add_inactive_plugins(
                self.read_profile("plugins.txt").as_deref(),
                self.read_profile("loadorder.txt").as_deref(),
                &carried,
            )
            .map_err(ModsError::Refused)?;
            plan.plugins_dropped = Vec::new();
            plan.detail = Some(format!(
                "plugins listed as inactive: {}",
                carried.join(", ")
            ));
        }
        plan.files = chosen;
        plan.stage = true;
        name.clone_into(&mut plan.name);
        Ok(())
    }

    pub(super) fn overwrite_move(
        &self,
        plan: &mut Plan,
        path: &str,
        to: &str,
    ) -> Result<(), ModsError> {
        let (actual, files) = self.overwrite_selection(path)?;
        let found = self.mod_found(to)?;
        let folder = Self::folder_or_refuse(&found)?.to_owned();
        let mod_dir = self.mods.join(&folder);
        let mut clashes = Vec::new();
        let mut moves = Vec::new();
        for file in &files {
            let (there, exists) = spelled(&mod_dir, file);
            if exists {
                clashes.push(file.clone());
            }
            moves.push((file.clone(), there));
        }
        if !clashes.is_empty() {
            return refuse(format!(
                "`{}` already has: {}; nothing was moved",
                found.name,
                clashes.join(", ")
            ));
        }
        for (file, there) in &moves {
            plan.steps.push(Step::Move {
                from: format!("{OVERWRITE}/{file}"),
                to: format!("{}/{there}", rel_mod(&folder)),
            });
        }
        plan.steps.push(Step::Prune(ancestors(&files)));
        plan.detail = Some(format!("{actual} to {}", found.name));
        plan.files = files;
        plan.stage = true;
        plan.name = actual;
        Ok(())
    }

    pub(super) fn overwrite_delete(
        &self,
        plan: &mut Plan,
        first: &str,
        more: &[String],
    ) -> Result<(), ModsError> {
        let mut chosen: Vec<(String, Vec<String>)> = Vec::new();
        for path in std::iter::once(first).chain(more.iter().map(String::as_str)) {
            let (actual, files) = self.overwrite_selection(path)?;
            if !chosen.iter().any(|(a, _)| *a == actual) {
                chosen.push((actual, files));
            }
        }
        // A path inside another chosen folder goes with that folder.
        let tops: Vec<&(String, Vec<String>)> = chosen
            .iter()
            .filter(|(a, _)| {
                !chosen
                    .iter()
                    .any(|(b, _)| b != a && a.starts_with(&format!("{b}/")))
            })
            .collect();
        let mut all = Vec::new();
        for (actual, files) in tops {
            plan.steps.push(Step::TrashPath {
                from: format!("{OVERWRITE}/{actual}"),
                keep: format!("{OVERWRITE}/{actual}"),
            });
            all.extend(files.iter().cloned());
        }
        let tops: Vec<String> = all.clone();
        plan.steps.push(Step::Prune(ancestors(&tops)));
        plan.files = all;
        first.clone_into(&mut plan.name);
        plan.stage = true;
        Ok(())
    }

    pub(super) fn overwrite_sync(
        &self,
        plan: &mut Plan,
        to: Option<&str>,
    ) -> Result<(), ModsError> {
        let wanted = match to {
            Some(m) => Some(self.mod_found(m)?.name),
            None => None,
        };
        let files: Vec<String> = walk(&self.overwrite_dir())
            .map_err(ModsError::Refused)?
            .into_iter()
            .map(|(p, _)| p)
            .collect();
        let found = providers(self, &files)?;
        let mut moves: Vec<(String, String)> = Vec::new();
        let mut ambiguous = Vec::new();
        for (file, who) in files.iter().zip(&found) {
            let target = match (&wanted, who.as_slice()) {
                (_, []) => continue,
                (Some(w), who) => match who.iter().find(|n| n.eq_ignore_ascii_case(w)) {
                    Some(n) => n.clone(),
                    None => continue,
                },
                (None, [one]) => one.clone(),
                (None, several) => {
                    ambiguous.push(format!("{file} ({})", several.join(", ")));
                    continue;
                }
            };
            moves.push((file.clone(), target));
        }
        if !ambiguous.is_empty() {
            return refuse(format!(
                "several Mods provide: {}; name one with --to, or move the files yourself",
                ambiguous.join("; ")
            ));
        }
        for (file, target) in &moves {
            let folder = super::folder_of(&self.mods, target)
                .ok_or_else(|| ModsError::Refused(format!("the folder of `{target}` is gone")))?;
            let mod_dir = self.mods.join(&folder);
            let (there, exists) = spelled(&mod_dir, file);
            let dest = format!("{}/{there}", rel_mod(&folder));
            if exists {
                plan.steps.push(Step::TrashPath {
                    from: dest.clone(),
                    keep: dest.clone(),
                });
            }
            plan.steps.push(Step::Move {
                from: format!("{OVERWRITE}/{file}"),
                to: dest,
            });
        }
        let paths: Vec<String> = moves.iter().map(|(f, _)| f.clone()).collect();
        if !paths.is_empty() {
            plan.steps.push(Step::Prune(ancestors(&paths)));
            plan.stage = true;
            let mut targets: Vec<String> = moves.iter().map(|(_, t)| t.clone()).collect();
            targets.sort();
            targets.dedup();
            plan.detail = Some(format!("synced to {}", targets.join(", ")));
        }
        plan.files = paths;
        "overwrite".clone_into(&mut plan.name);
        Ok(())
    }
}

impl Ctx<'_> {
    /// `path` (absolute, under the Instance folder) relative to it, `/` separated.
    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.instance.root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/")
    }

    /// The sweep: what programs did to the Staging tree since it was built (see `nvmod_stage::sweep`).
    pub(super) fn sweep(
        &self,
        plan: &mut Plan,
        to: Option<&str>,
        protect: Option<&str>,
    ) -> Result<(), ModsError> {
        use nvmod_stage::sweep::Kind;
        let changes =
            nvmod_stage::sweep::scan(self.instance, self.profile).map_err(ModsError::Refused)?;
        let (base_rel, base_dir) = match to {
            Some(name) => {
                let found = self.mod_found(name)?;
                let folder = Self::folder_or_refuse(&found)?.to_owned();
                (rel_mod(&folder), self.mods.join(&folder))
            }
            None => (OVERWRITE.to_owned(), self.overwrite_dir()),
        };
        let tree = self.relative(&self.instance.staging_dir(self.profile).join("data"));
        let mut taken: Vec<String> = Vec::new();
        let mut moved: Vec<String> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();
        for change in &changes {
            if !matches!(change.kind, Kind::New | Kind::Replaced) {
                continue;
            }
            let (there, exists) = spelled(&base_dir, &change.spelled);
            let lower = there.to_lowercase();
            if taken.contains(&lower) {
                skipped.push(change.path.clone());
                continue;
            }
            taken.push(lower);
            let dest = format!("{base_rel}/{there}");
            if exists {
                plan.steps.push(Step::TrashPath {
                    from: dest.clone(),
                    keep: dest.clone(),
                });
            }
            plan.steps.push(Step::Move {
                from: format!("{tree}/{}", change.path),
                to: dest,
            });
            moved.push(there);
        }
        if let Some(glob) = protect {
            checked(&glob.replace("**", "x").replace(['*', '?'], "x"))?;
            let file = self
                .instance
                .profile_dir(self.profile)
                .join(nvmod_stage::PROTECT_FILE);
            let old = fs::read(&file).unwrap_or_default();
            let text = nvmod_formats::text::decode(&old);
            if !nvmod_stage::sweep::parse_globs(&text)
                .iter()
                .any(|g| g == glob)
            {
                let mut text = text;
                if !text.is_empty() && !text.ends_with('\n') {
                    text.push('\n');
                }
                text.push_str(glob);
                text.push('\n');
                plan.steps.push(Step::Write {
                    path: self.relative(&file),
                    bytes: text.into_bytes(),
                });
            }
        }
        if !skipped.is_empty() {
            plan.detail = Some(format!(
                "left in the tree, same path as another file by letter case: {}",
                skipped.join(", ")
            ));
        }
        plan.sweep = true;
        plan.restage = !changes.is_empty() || !plan.steps.is_empty();
        plan.stage = true;
        plan.files = moved;
        plan.changes = changes;
        "sweep".clone_into(&mut plan.name);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_stay_inside_overwrite() {
        assert!(checked("a/b.txt").is_ok());
        for bad in ["", "../a", "a//b", "/a", "a\\b"] {
            assert!(checked(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn ancestors_come_deepest_first_without_overwrite_itself() {
        let dirs = ancestors(&[
            "a/b/c.txt".to_owned(),
            "a/d.txt".to_owned(),
            "e.txt".to_owned(),
        ]);
        assert_eq!(dirs, ["overwrite/a/b", "overwrite/a"]);
    }
}
