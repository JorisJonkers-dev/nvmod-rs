// SPDX-License-Identifier: GPL-3.0-only
//! Writes the ids module of the feature catalogue (catalogue/, read by tally) with tally's generator: a copy that
//! `tally check` keeps equal to the pinned tally's (`tally ids --generator catalogue/tally_ids_gen.rs`). Standard
//! library only: no build dependency.

#![allow(missing_docs)]

include!("../../catalogue/tally_ids_gen.rs");

fn main() {
    tally_ids_gen::build("../../catalogue");
}
