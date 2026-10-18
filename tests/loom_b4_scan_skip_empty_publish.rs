//! Loom model of the B4 guarded-swap scan∥publish protocol
//! (docs/perf/B4_MT_COLLAPSE_DIAGNOSIS).
//!
//! Pins the protocol shape of `BitmapScan::next_cut`'s production (B4) arm —
//! `src/alloc_core/segment/remote_bitmap/bitmap_scan.rs`, the
//! `#[cfg(not(all(feature = "bench-internals", r18_sidecar_scan_bench)))]`
//! branch (~line 70): a scan step is `load(Acquire)` then a conditional
//! `swap(0, AcqRel)` only when the word is nonzero; publication is
//! `fetch_or(bit, AcqRel)`. Modeled over raw `loom` atomics (the real
//! `SidecarBitmap`/`BitmapScan` are `pub(crate)`, unreachable from a test
//! crate — same level as `tests/loom_sidecar_bitmap.rs`).
//!
//! Non-loss property pinned: a bit published AFTER a scanner's zero read of a
//! word stays pending and is observed by a SECOND scan of that word; the bit
//! is consumed exactly once across both scans (never duplicated, never lost),
//! and a consumed word always reads back zero.
#![cfg(loom)]

use loom::sync::atomic::{AtomicU64, Ordering};
use loom::sync::Arc;
use loom::thread;

const BIT: u64 = 1 << 3;

/// One pending word of the sidecar bank.
struct Word {
    pending: AtomicU64,
}

impl Word {
    fn new() -> Self {
        Self {
            pending: AtomicU64::new(0),
        }
    }

    /// Publisher side: `BitmapScan`'s counterpart of the terminal publication
    /// (`fetch_or(AcqRel)` — see `SidecarBitmap::publish`).
    fn publish(&self) {
        self.pending.fetch_or(BIT, Ordering::AcqRel);
    }

    /// Scanner side: exactly the B4 guarded arm of `BitmapScan::next_cut`
    /// (load Acquire; swap(0, AcqRel) only when nonzero).
    fn scan_step(&self) -> u64 {
        if self.pending.load(Ordering::Acquire) == 0 {
            0
        } else {
            self.pending.swap(0, Ordering::AcqRel)
        }
    }
}

#[test]
fn loom_b4_guarded_scan_never_loses_a_late_publication() {
    loom::model(|| {
        // Word 0 models a permanently empty word (the skip path); word 1 is
        // the race word between the scanner and the publisher.
        let word0 = Arc::new(Word::new());
        let word1 = Arc::new(Word::new());

        let s0 = Arc::clone(&word0);
        let s1 = Arc::clone(&word1);
        let scanner = thread::spawn(move || {
            let first = s0.scan_step();
            assert_eq!(first, 0, "an empty word must never yield bits");
            s1.scan_step()
        });

        let p1 = Arc::clone(&word1);
        let publisher = thread::spawn(move || p1.publish());

        let first_word1 = scanner.join().unwrap();
        publisher.join().unwrap();

        // Second scan after the publisher has run: the one-pass window has
        // closed, so the bit MUST be observed by exactly one of the two scans.
        let second_word1 = word1.scan_step();

        // (b) non-loss: published exactly once → seen exactly once.
        assert_eq!(
            first_word1 | second_word1,
            BIT,
            "bit published after a zero read must be found by the later scan"
        );
        assert_eq!(
            first_word1 & second_word1,
            0,
            "bit consumed by both scans would be a double publication"
        );

        // (a) consumed-without-swap: a swap always returns what it consumed,
        // so a fully drained word can never still carry bits.
        assert_eq!(
            word1.pending.load(Ordering::Acquire),
            0,
            "pending bits survived a completed scan pair"
        );
        assert_eq!(
            word0.scan_step(),
            0,
            "skipped (zero) word must stay empty and never fabricate a cut"
        );
    });
}
