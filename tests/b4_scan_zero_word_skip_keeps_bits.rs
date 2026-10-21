//! B4 (docs/perf/B4_MT_COLLAPSE_DIAGNOSIS): the guarded swap in
//! `BitmapScan::next_cut` (load-then-conditional-swap) must NOT over-skip — a
//! NONZERO word still yields its full bit pattern through `BitmapCut::pop`.
//! Green with the fix AND without it (the unconditional swap also finds the
//! bits); the counterfactual for the fix itself (the RMW-storm removal) is the
//! measurement series, not this test.
//! Conventions/geometry mirror `tests/r6_sidecar_bitmap.rs`.
#![cfg(feature = "alloc-core")]

// Compile the actual primitive in this integration crate. These aliases are
// pinned to the production geometry, not independent test constants.
mod alloc_core {
    // Mirrors `alloc_core::fatal` (`src/alloc_core/platform/fatal.rs`).
    pub mod fatal {
        pub fn fatal(msg: &'static [u8]) -> ! {
            use std::io::Write;
            let _ = std::io::stderr().write_all(msg);
            std::process::abort()
        }
    }
    pub mod os {
        pub const SEGMENT: usize = sefer_alloc::SegmentLayout::SEGMENT;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = sefer_alloc::SegmentLayout::MIN_BLOCK;
    }
    pub mod segment_layout {
        pub const SIDECAR_WORD_BYTES: usize =
            crate::alloc_core::size_classes::MIN_BLOCK * u64::BITS as usize;
    }
}

// Included scanner siblings unused by this test (same pattern as
// `tests/class_leaves_publication_miri.rs`).
#[allow(dead_code)]
#[path = "../src/alloc_core/segment/remote_bitmap/mod.rs"]
mod remote_bitmap;

mod tests {
    use super::remote_bitmap::SidecarBitmap;

    /// Word 0 of the pending bank is zero; word 1 carries bits. One scan from
    /// word 0 must: return an empty cut for word 0 (the B4 skip path), return
    /// the full pattern for word 1 (the guard must not over-skip), and pop the
    /// record out of the word-1 cut.
    #[test]
    fn b4_zero_word_is_skipped_but_nonzero_word_yields_its_bits() {
        let words: Vec<std::sync::atomic::AtomicU64> = (0..SidecarBitmap::WORDS)
            .map(|_| std::sync::atomic::AtomicU64::new(0))
            .collect();
        let starts: Vec<std::sync::atomic::AtomicU64> = (0..SidecarBitmap::WORDS)
            .map(|_| std::sync::atomic::AtomicU64::new(0))
            .collect();
        // payload_start 0: the whole segment is payload in this harness.
        let map = SidecarBitmap::from_pending(&words, &starts).unwrap();

        // Offset 1024 lives in pending word 1 (1024 bytes per bit at
        // MIN_BLOCK=16 × 64 bits/word), class 7 is an arbitrary payload.
        map.issue(1024);
        assert!(map.publish(1024, 0));

        let mut scan = map.scan_from(2048, 0).unwrap();

        // Word 0: pending == 0 → empty cut, no records.
        let mut cut0 = scan.next_cut().unwrap();
        assert!(cut0.is_empty(), "zero word must produce an empty cut");
        assert!(cut0.pop().is_none(), "zero word must not pop a record");

        // Word 1: nonzero → the FULL published pattern must survive the guard.
        let mut cut1 = scan.next_cut().unwrap();
        assert!(!cut1.is_empty(), "nonzero word must not be over-skipped");
        assert_eq!(
            cut1.pop(),
            Some(1024),
            "word-1 cut must yield the record offset"
        );
        assert!(cut1.pop().is_none(), "record must not be duplicated");

        // Scan is one-pass over the two-word window.
        assert!(scan.next_cut().is_none());

        // The swap consumed the bits: a second scan sees nothing pending.
        let mut rescan = map.scan_from(2048, 0).unwrap();
        assert!(rescan.next_cut().unwrap().is_empty());
        assert!(rescan.next_cut().unwrap().is_empty());
        assert!(rescan.next_cut().is_none());
    }
}
