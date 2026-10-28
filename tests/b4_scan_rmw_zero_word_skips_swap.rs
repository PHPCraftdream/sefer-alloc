//! B4 (docs/perf/B4_MT_COLLAPSE_DIAGNOSIS) counterfactual for FIX 1: the
//! production `BitmapScan::next_cut` branch must NOT execute a RMW swap on a
//! zero pending word. Armed with the `measure_rmw_swaps` counter, a full scan
//! of an all-zero window must perform ZERO RMW swaps, and a scan consuming a
//! single published bit must perform EXACTLY ONE.
//! Conventions/geometry mirror `tests/b4_scan_zero_word_skip_keeps_bits.rs`.
#![cfg(all(feature = "alloc-core", feature = "bench-internals"))]

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
        pub const SMALL_CLASS_COUNT: usize = sefer_alloc::SegmentLayout::SIZE_CLASS_TABLE.len();
    }
    pub mod segment_layout {
        pub const SIDECAR_WORD_BYTES: usize =
            crate::alloc_core::size_classes::MIN_BLOCK * u64::BITS as usize;
    }
}

#[allow(dead_code)]
#[path = "../src/alloc_core/segment/remote_bitmap/mod.rs"]
mod remote_bitmap;

// Ghost checks in the included production files (docs/GHOST_CHECKS_PLAN.md):
// ghost builds (miri, --cfg sefer_ghost) compile them.
#[allow(dead_code, unused_imports)]
#[path = "../src/ghost/mod.rs"]
mod ghost;

mod tests {
    use super::remote_bitmap::{BitmapRecord, BitmapScan, SidecarBitmap};

    /// High-water covering the whole pending bank.
    const FULL_WINDOW: usize =
        SidecarBitmap::WORDS * crate::alloc_core::segment_layout::SIDECAR_WORD_BYTES;

    /// Counterfactual: with nothing published (every pending word zero), a
    /// full-window scan must execute NO RMW swap at all. The pre-B4
    /// unconditional swap would increment the counter once per word.
    #[test]
    fn b4_zero_word_scan_executes_no_rmw_swap() {
        let words: Vec<std::sync::atomic::AtomicU64> = (0..SidecarBitmap::WORDS)
            .map(|_| std::sync::atomic::AtomicU64::new(0))
            .collect();
        let classes: Vec<std::sync::atomic::AtomicU8> = (0..SidecarBitmap::GRANULES)
            .map(|_| std::sync::atomic::AtomicU8::new(0))
            .collect();
        let map = SidecarBitmap::from_initialized(&words, &classes).unwrap();

        BitmapScan::measure_rmw_swaps(true);

        let mut scan = map.scan_from(FULL_WINDOW, 0).unwrap();
        let mut cuts = 0;
        while let Some(mut cut) = scan.next_cut() {
            assert!(cut.is_empty(), "zero word must produce an empty cut");
            assert!(cut.pop().is_none(), "zero word must not pop a record");
            cuts += 1;
        }
        assert!(cuts > 0, "scan must visit at least one word");

        let rmw = BitmapScan::measure_rmw_swaps(false);
        assert_eq!(
            rmw, 0,
            "zero-word scan must not execute a single RMW swap (B4 FIX 1 counterfactual)"
        );
    }

    /// Exactly one RMW swap for the one nonzero word: the published bit is
    /// consumed exactly once, and a rescan of the drained bitmap performs no
    /// further RMW (the counter must not grow).
    #[test]
    fn b4_nonzero_word_consumed_exactly_once_with_single_rmw() {
        let words: Vec<std::sync::atomic::AtomicU64> = (0..SidecarBitmap::WORDS)
            .map(|_| std::sync::atomic::AtomicU64::new(0))
            .collect();
        let classes: Vec<std::sync::atomic::AtomicU8> = (0..SidecarBitmap::GRANULES)
            .map(|_| std::sync::atomic::AtomicU8::new(0))
            .collect();
        let map = SidecarBitmap::from_initialized(&words, &classes).unwrap();

        // Offset 1024 lives in pending word 1 (1024 bytes per bit at
        // MIN_BLOCK=16 × 64 bits/word), class 7 is an arbitrary payload.
        assert!(map.issue(1024, 7));
        assert!(map.publish(1024));

        BitmapScan::measure_rmw_swaps(true);

        let mut scan = map.scan_from(FULL_WINDOW, 0).unwrap();
        let mut record = None;
        while let Some(mut cut) = scan.next_cut() {
            if let Some(rec) = cut.pop() {
                assert!(record.is_none(), "record must be popped exactly once");
                record = Some(rec);
            }
        }
        assert_eq!(
            record,
            Some(BitmapRecord {
                offset: 1024,
                class: 7
            }),
            "the published word-1 record must survive the guard"
        );

        let rmw = BitmapScan::measure_rmw_swaps(false);
        assert_eq!(rmw, 1, "exactly one nonzero word → exactly one RMW swap");

        // One-pass scan: a rescan of the drained bitmap is empty and free.
        BitmapScan::measure_rmw_swaps(true);
        let mut rescan = map.scan_from(FULL_WINDOW, 0).unwrap();
        while let Some(mut cut) = rescan.next_cut() {
            assert!(cut.pop().is_none(), "drained bitmap must stay empty");
        }
        let rmw2 = BitmapScan::measure_rmw_swaps(false);
        assert_eq!(rmw2, 0, "rescan of a drained bitmap must not swap");
    }
}
