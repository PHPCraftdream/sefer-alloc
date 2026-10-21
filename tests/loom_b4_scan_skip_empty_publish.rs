//! P1-a actual-source loom model for the B4 guarded-scan ∥ publish protocol
//! (docs/perf/B4_MT_COLLAPSE_DIAGNOSIS).
//!
//! This is NOT a shadow model: the PRODUCTION `bitmap_scan.rs`
//! (`src/alloc_core/segment/remote_bitmap/bitmap_scan.rs`) is compiled into
//! this test crate via `#[path]` include, exactly like
//! `tests/loom_sidecar_bitmap_actual.rs` — production files resolve their
//! atomics (`AtomicU64`, `AtomicU8`, `Ordering`) through `super`, so the
//! reexport at this root module substitutes loom atomics. The loom build's
//! feature set (`alloc-core alloc-xthread tagged-index-stack/loom`, no
//! `bench-internals`) means `BitmapScan::next_cut` compiles the
//! `#[cfg(not(all(feature = "bench-internals", r18_sidecar_scan_bench)))]`
//! arm — i.e. the real B4 load-then-conditional-swap guarded arm
//! (~line 70-82). Publication is the production
//! `SidecarBitmap::publish` (`fetch_or(bit, AcqRel)`).
//!
//! Reduced geometry: 2 pending words (`SEGMENT = 2 * 64 * MIN_BLOCK = 2048`),
//! one scanner thread, one publisher thread — a minimal loom state space.
//!
//! Pinned properties (over every interleaving):
//! - Word 0 is zero: its cut is always empty and pops no record (skip path).
//! - A bit published into word 1 concurrently with the scanner's zero skip of
//!   word 0 is NEVER lost: it is observed by exactly one of (first scan of
//!   word 1, second scan after join).
//! - The record is never duplicated across the two scans.
//! - Word 0's cut never yields anything at any point.
//! - After both scans the whole bank is drained: word 1 pending == 0 and
//!   word 0 pending == 0 (a consumed word reads back as zero through a
//!   second scan).
#![cfg(loom)]

mod alloc_core {
    // Mirrors `alloc_core::fatal` (`src/alloc_core/platform/fatal.rs`) for the
    // source-included primitives.
    pub mod fatal {
        pub fn fatal(msg: &'static [u8]) -> ! {
            use std::io::Write;
            let _ = std::io::stderr().write_all(msg);
            std::process::abort()
        }
    }
    pub mod os {
        /// 2 pending words at `MIN_BLOCK = 16` (minimal loom state space).
        pub const SEGMENT: usize = 2 * 64 * 16;
    }
    pub mod size_classes {
        pub const MIN_BLOCK: usize = 16;
        pub const SMALL_CLASS_COUNT: usize = 8;
    }
    pub mod segment_layout {
        pub const SIDECAR_WORD_BYTES: usize =
            crate::alloc_core::size_classes::MIN_BLOCK * u64::BITS as usize;
    }
}

// The included production files resolve `super` at this root module.
pub(crate) use loom::sync::atomic::{AtomicPtr, AtomicU64, AtomicU8, Ordering};

#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_cut.rs"]
mod bitmap_cut;
#[path = "../src/alloc_core/segment/remote_bitmap/bitmap_scan.rs"]
mod bitmap_scan;
#[path = "../src/alloc_core/segment/remote_bitmap/sidecar_bitmap.rs"]
mod sidecar_bitmap;

mod remote_bitmap {
    pub(crate) use super::bitmap_cut::BitmapCut;
    pub(crate) use super::bitmap_scan::BitmapScan;
    pub(crate) use super::sidecar_bitmap::SidecarBitmap;
}

// Root bindings for the names the included production files resolve via
// `super::` (same mechanism as `tests/loom_sidecar_bitmap_actual.rs`);
// `BitmapCut`/`BitmapScan` are referenced by the included sources themselves.
use remote_bitmap::{BitmapCut, BitmapScan, SidecarBitmap};

use loom::sync::Arc;
use loom::thread;

/// Offset 1024 lives in pending word 1 (1024 bytes per bit at MIN_BLOCK=16 ×
/// 64 bits/word), same granule as `tests/b4_scan_zero_word_skip_keeps_bits.rs`.
const OFFSET: u32 = 1024;
const CLASS: u8 = 7;
const HIGH_WATER: usize = 2 * 64 * 16; // covers both words

struct Backing {
    words: Vec<AtomicU64>,
    starts: Vec<AtomicU64>,
}

impl Backing {
    fn new() -> Self {
        Self {
            words: (0..SidecarBitmap::WORDS)
                .map(|_| AtomicU64::new(0))
                .collect(),
            // D1' phase-1a start bank, zero-initialized.
            starts: (0..SidecarBitmap::WORDS)
                .map(|_| AtomicU64::new(0))
                .collect(),
        }
    }

    fn bitmap(&self) -> SidecarBitmap<'_> {
        // payload_start 0: the whole (reduced) segment is payload here.
        SidecarBitmap::from_pending(&self.words, &self.starts).unwrap()
    }
}

#[test]
fn loom_b4_publish_racing_zero_word_skip_found_by_later_scan() {
    loom::model(move || {
        assert_eq!(SidecarBitmap::WORDS, 2);
        let backing = Arc::new(Backing::new());
        let map = backing.bitmap();
        map.issue(OFFSET);

        // Scanner: word 0 (zero → skip), then word 1 (may or may not find the
        // concurrently published bit, depending on the interleaving).
        let scanner = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || {
                let mut scan = backing.bitmap().scan_from(HIGH_WATER, 0).unwrap();
                let mut cut0 = scan.next_cut().unwrap();
                assert!(
                    cut0.is_empty() && cut0.pop().is_none(),
                    "zero word 0 must yield an empty cut that pops nothing"
                );
                let mut cut1 = scan.next_cut().unwrap();
                cut1.pop()
            })
        };

        // Publisher: publishes the (already issued) word-1 granule.
        let publisher = {
            let backing = Arc::clone(&backing);
            thread::spawn(move || assert!(backing.bitmap().publish(OFFSET, 0)))
        };

        let first_word1 = scanner.join().unwrap();
        publisher.join().unwrap();

        // Second scan after the publisher has run: whatever the first scan
        // missed MUST be found here.
        let mut rescan = backing.bitmap().scan_from(HIGH_WATER, 0).unwrap();
        // Word 0 again: still zero, still an empty cut.
        let mut cut0 = rescan.next_cut().unwrap();
        assert!(
            cut0.is_empty() && cut0.pop().is_none(),
            "word 0 stays empty across scans"
        );
        let mut cut1 = rescan.next_cut().unwrap();
        let second_word1 = cut1.pop();
        assert!(rescan.next_cut().is_none(), "two-word window is one-pass");

        // Exactly-once: the published record is found by exactly one of the
        // two word-1 scans — never duplicated, never lost.
        match (first_word1, second_word1) {
            (Some(found), None) | (None, Some(found)) => {
                assert_eq!(found, OFFSET, "the record carries the issued offset")
            }
            (None, None) => panic!("published bit lost by the zero-word skip"),
            (Some(_), Some(_)) => panic!("record duplicated across the two scans"),
        }

        // Fully drained: a third scan over the whole bank finds nothing and
        // both consumed words read back as zero.
        let mut drain = backing.bitmap().scan_from(HIGH_WATER, 0).unwrap();
        assert!(drain.next_cut().unwrap().is_empty(), "word 0 drained");
        assert!(
            drain.next_cut().unwrap().is_empty(),
            "word 1 drained after both discovery scans"
        );
        assert!(drain.next_cut().is_none());
    });
}
