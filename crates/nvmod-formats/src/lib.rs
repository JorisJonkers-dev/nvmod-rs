// SPDX-License-Identifier: GPL-3.0-only
//! Read-only readers for the files of a Mod Organizer 2 instance, following
//! `docs/research/mo2-internals-and-import.md` of NV-Linux-Modding. Nothing here writes: the round-trip rules
//! (write `modlist.txt`, `plugins.txt` and `loadorder.txt` together, CRLF, atomic) belong to the installer
//! milestone. Every reader takes bytes or text, so it can be fuzzed without a file system.

pub mod categories;
pub mod ini;
pub mod instance;
pub mod lockedorder;
pub mod metaini;
pub mod modlist;
pub mod plugins;
pub mod scan;
pub mod scripts;
pub mod text;
pub mod version;
pub mod winpath;

pub use instance::{Executable, Mo2Error, Mo2Instance, Mo2Profile, ModMeta};
pub use lockedorder::LockedOrder;
pub use modlist::{EntryKind, ModState, Modlist, ModlistEntry};
