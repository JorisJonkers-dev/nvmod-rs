// SPDX-License-Identifier: GPL-3.0-only
//! Properties of the inspector: it never panics, never reads outside the file, and reads back what was built.

use nvmod_pe::{MAX_READ, ReadAt, SliceSource, Version4, inspect, inspect_bytes};
use nvmod_testkit::PeBuilder;
use proptest::prelude::*;

/// A source that records every read it is asked for and checks it against the file.
struct Recording<'a> {
    inner: SliceSource<'a>,
    largest: usize,
}

impl ReadAt for Recording<'_> {
    fn len(&self) -> u64 {
        self.inner.len()
    }

    fn read_at(&mut self, offset: u64, len: usize) -> Option<Vec<u8>> {
        let out = self.inner.read_at(offset, len);
        if out.is_some() {
            assert!(offset + len as u64 <= self.inner.len());
            self.largest = self.largest.max(len);
        }
        out
    }
}

fn valid() -> Vec<u8> {
    PeBuilder::nvse_plugin()
        .nvse_version(1, 9)
        .versions([1, 2, 3, 4], [5, 6, 7, 8])
        .build()
}

proptest! {
    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let _ = inspect_bytes(&bytes);
    }

    #[test]
    fn arbitrary_bytes_behind_a_valid_dos_header_never_panic(tail in proptest::collection::vec(any::<u8>(), 0..2048)) {
        let mut file = valid()[..0x80].to_vec();
        file.extend_from_slice(&tail);
        let _ = inspect_bytes(&file);
    }

    #[test]
    fn changing_any_bytes_of_a_valid_file_never_panics_and_reads_stay_in_the_file(
        edits in proptest::collection::vec((0usize..0x600, any::<u8>()), 1..8),
    ) {
        let mut file = valid();
        for (at, value) in edits {
            file[at] = value;
        }
        let mut source = Recording { inner: SliceSource(&file), largest: 0 };
        let _ = inspect(&mut source);
        prop_assert!(source.largest <= MAX_READ);
    }

    #[test]
    fn every_prefix_of_a_valid_file_is_handled(len in 0usize..0x600) {
        let file = valid();
        let _ = inspect_bytes(&file[..len]);
    }

    #[test]
    fn what_the_builder_wrote_is_what_the_inspector_reads(
        names in proptest::collection::btree_set("[A-Za-z_][A-Za-z0-9_]{0,40}", 0..12),
        imports in proptest::collection::vec("[A-Za-z0-9_]{1,12}\\.dll", 0..6),
        file in any::<[u16; 4]>(),
        product in any::<[u16; 4]>(),
        laa in any::<bool>(),
    ) {
        let mut builder = PeBuilder::dll().large_address_aware(laa).versions(file, product);
        for name in &names {
            builder = builder.export_code(name);
        }
        for dll in &imports {
            builder = builder.import(dll);
        }
        let report = inspect_bytes(&builder.build()).unwrap();
        let mut read: Vec<_> = report.exports.iter().map(|e| e.name.clone()).collect();
        read.sort();
        prop_assert_eq!(read, names.iter().cloned().collect::<Vec<_>>());
        prop_assert_eq!(report.imports, imports);
        prop_assert_eq!(report.file_version, Some(Version4(file)));
        prop_assert_eq!(report.product_version, Some(Version4(product)));
        prop_assert_eq!(report.large_address_aware, laa);
        prop_assert!(report.issues.is_empty());
    }
}
