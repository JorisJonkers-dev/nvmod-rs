// SPDX-License-Identifier: GPL-3.0-only
//! A journal written before the crate was split out (no `kind`, three profile files) is rolled back unchanged.

use nvmod_instance::NvmodInstance;
use nvmod_journal::{RollbackError, rollback};
use nvmod_testkit::{BaseBuilder, TempDir, write_file};
use nvmod_testkit::{covers, ids};
use std::fs;

#[test]
fn an_install_journal_in_the_old_format_is_rolled_back() {
    covers(&[ids::safety::undo::OLD_JOURNAL]);
    let base = TempDir::new("jr-base");
    BaseBuilder::steam().write(base.path());
    let tmp = TempDir::new("jr-inst");
    let root = tmp.path().join("fnv");
    let instance = NvmodInstance::create(&root, base.path()).expect("create the Instance");
    instance.create_profile("Default").expect("profile");
    let profile = root.join("profiles/Default");
    // State after the install: a changed list and the Mod it added.
    write_file(&profile, "modlist.txt", b"+Added\n");
    write_file(&root, "mods/Added/Data/a.txt", b"mod");
    // The journal as the program wrote it before the split: no `kind`, `existed` has three entries.
    let id = "20260101T000000-0001";
    let dir = root.join(".nvmod/journal").join(id);
    write_file(&dir, "backup/modlist.txt", b"-Kept\n");
    let journal = format!(
        r#"{{"schema":1,"id":"{id}","mod":"Added","profile":"Default","status":"complete","existed":[true,false,false],"staging_existed":false}}"#
    );
    write_file(&dir, "journal.json", journal.as_bytes());

    let report = rollback(&instance).expect("rolled back");
    assert_eq!(
        (
            report.kind.as_str(),
            report.r#mod.as_str(),
            report.journal.as_str()
        ),
        ("install", "Added", id)
    );
    assert_eq!(fs::read(profile.join("modlist.txt")).unwrap(), b"-Kept\n");
    assert!(!root.join("mods/Added").exists());
    assert!(!profile.join("plugins.txt").exists());
    // The journal is marked, and a second rollback has nothing to do.
    let text = fs::read_to_string(dir.join("journal.json")).unwrap();
    assert!(text.contains("\"rolled-back\""), "{text}");
    assert!(matches!(
        rollback(&instance),
        Err(RollbackError::Refused(_))
    ));
}
