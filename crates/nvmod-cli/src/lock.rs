// SPDX-License-Identifier: GPL-3.0-only
//! The Instance lock around every command that writes to an Instance, so nothing rebuilds or edits the tree
//! while a program runs through it (nv-linux #148). `nvmod run` takes the lock itself.

use crate::REFUSED;
use nvmod_instance::NvmodInstance;
use nvmod_run::{InstanceLock, LockError};
use std::io::Write;
use std::path::Path;

thread_local! {
    /// The Instances this thread holds the lock of, so a nested command run under the lock does not try to
    /// take it again.
    static HELD: std::cell::RefCell<Vec<std::path::PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn value_of<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == name)
        .and_then(|i| args.get(i + 1))
        .map(String::as_str)
}

/// Whether `args` is a command that changes files of an existing Instance (a dry run does not).
fn writes(args: &[String]) -> bool {
    let word = |i: usize| args.get(i).map(String::as_str);
    let has = |flag: &str| args.iter().any(|a| a == flag);
    match word(0) {
        Some("stage" | "target" | "rollback") => true,
        Some("install") => !has("--dry-run"),
        Some("downloads") => {
            matches!(word(1), Some("install" | "hide" | "unhide" | "forget")) && !has("--dry-run")
        }
        Some("mods") => {
            matches!(word(1), Some("enable" | "disable" | "reorder")) && !has("--dry-run")
        }
        Some("sort") => has("--apply") && !has("--dry-run"),
        Some("backup") => {
            matches!(word(1), Some("create" | "restore" | "prune")) && !has("--dry-run")
        }
        Some("profile") => {
            matches!(
                word(1),
                Some(
                    "create" | "apply" | "finish" | "copy" | "rename" | "delete" | "select" | "set"
                )
            ) && !has("--dry-run")
        }
        Some("plugins") => matches!(
            word(1),
            Some("enable" | "disable" | "move" | "lock" | "unlock")
        ),
        Some("overwrite") => {
            matches!(word(1), Some("create-mod" | "move" | "delete" | "sync")) && !has("--dry-run")
        }
        Some("exec") => matches!(word(1), Some("add" | "edit" | "remove" | "import")),
        Some("tools") => word(1) == Some("trust"),
        _ => false,
    }
}

/// Run `command` holding the Instance lock when `args` is a writing command on an Instance. A held lock exits 2
/// naming the holder; a lock of a dead process is taken over with an info line.
pub(crate) fn around(
    args: &[String],
    err: &mut dyn Write,
    command: impl FnOnce(&mut dyn Write) -> u8,
) -> u8 {
    if !writes(args) {
        return command(err);
    }
    let Some(root) = value_of(args, "--instance").map(Path::new) else {
        return command(err);
    };
    if !NvmodInstance::is_instance(root) {
        return command(err);
    }
    let key = std::path::absolute(root).unwrap_or_else(|_| root.to_path_buf());
    if HELD.with(|h| h.borrow().contains(&key)) {
        return command(err);
    }
    let name = args.first().map_or("nvmod", String::as_str);
    let lock = match InstanceLock::acquire(root, name) {
        Ok(lock) => lock,
        Err(LockError::Held(h)) => {
            let _ = writeln!(
                err,
                "nvmod {name}: refused: the Instance is in use by {h}; wait for it to end"
            );
            return REFUSED;
        }
        Err(LockError::Io(why)) => {
            let _ = writeln!(
                err,
                "nvmod {name}: refused: cannot take the Instance lock: {why}"
            );
            return REFUSED;
        }
    };
    if let Some(old) = &lock.took_over {
        let _ = writeln!(
            err,
            "nvmod {name}: info: took over the lock of {old}, which is no longer running"
        );
    }
    HELD.with(|h| h.borrow_mut().push(key));
    let code = command(err);
    HELD.with(|h| h.borrow_mut().pop());
    drop(lock);
    code
}
