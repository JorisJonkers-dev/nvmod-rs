// SPDX-License-Identifier: GPL-3.0-only
//! `saves transfer`: copy the saves of one profile to another with their `.nvse` co-saves. A save without its
//! partner is refused unless forced, and a file that would overwrite a different one is always refused.

use crate::{Error, Location, copy_atomic, cosave_name, hash_file, io, save_files};
use serde::Serialize;
use std::fs;

/// What `transfer` did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TransferReport {
    /// The profile copied from.
    pub from: String,
    /// The profile copied to.
    pub to: String,
    /// Files written, saves and co-saves.
    pub copied: Vec<String>,
    /// Files that were already there with the same content.
    pub already_there: Vec<String>,
    /// Saves copied without a co-save (only with `force`).
    pub without_cosave: Vec<String>,
}

/// Copy the saves of `from` into the save folder of `to`.
///
/// # Errors
/// `Refused` when the folders are the same, there is nothing to copy, a co-save is missing (without `force`) or
/// a different file is in the way (nothing is written); `Failed` when a copy fails (the files made are removed).
pub fn transfer(
    from: (&str, &Location),
    to: (&str, &Location),
    force: bool,
) -> Result<TransferReport, Error> {
    let same = from.1.dir == to.1.dir
        || fs::canonicalize(&from.1.dir)
            .ok()
            .zip(fs::canonicalize(&to.1.dir).ok())
            .is_some_and(|(a, b)| a == b);
    if same {
        return Err(Error::Refused(format!(
            "`{}` and `{}` use the same save folder",
            from.0, to.0
        )));
    }
    let saves = save_files(&from.1.dir);
    if saves.is_empty() {
        return Err(Error::Refused(format!(
            "`{}` has no saves in {}",
            from.0,
            from.1.dir.display()
        )));
    }
    let mut plan: Vec<String> = Vec::new();
    let mut without = Vec::new();
    for (name, _) in &saves {
        plan.push(name.clone());
        let partner = cosave_name(name);
        if from.1.dir.join(&partner).is_file() {
            plan.push(partner);
        } else {
            without.push(name.clone());
        }
    }
    if !without.is_empty() && !force {
        return Err(Error::Refused(format!(
            "no .nvse co-save next to: {} (use --force to copy them without)",
            without.join(", ")
        )));
    }
    let mut there = Vec::new();
    let mut todo = Vec::new();
    let mut clash = Vec::new();
    for name in plan {
        let target = to.1.dir.join(&name);
        if target.is_file() {
            let same = hash_file(&from.1.dir.join(&name))
                .ok()
                .zip(hash_file(&target).ok())
                .is_some_and(|(a, b)| a == b);
            if same {
                there.push(name);
            } else {
                clash.push(name);
            }
        } else {
            todo.push(name);
        }
    }
    if !clash.is_empty() {
        return Err(Error::Refused(format!(
            "`{}` already has different files named: {} (nothing was copied)",
            to.0,
            clash.join(", ")
        )));
    }
    let made_dir = !to.1.dir.exists();
    fs::create_dir_all(&to.1.dir).map_err(|e| Error::Failed(io(&to.1.dir, &e)))?;
    let mut created: Vec<std::path::PathBuf> = Vec::new();
    for name in &todo {
        let step = nvmod_core::cancel::step_point("saves-transfer").and_then(|()| {
            let target = to.1.dir.join(name);
            copy_atomic(&from.1.dir.join(name), &target).map(|_| created.push(target))
        });
        if let Err(why) = step {
            for path in &created {
                let _ = fs::remove_file(path);
            }
            if made_dir {
                let _ = fs::remove_dir(&to.1.dir);
            }
            return Err(Error::Failed(why));
        }
    }
    Ok(TransferReport {
        from: from.0.to_owned(),
        to: to.0.to_owned(),
        copied: todo,
        already_there: there,
        without_cosave: without,
    })
}
