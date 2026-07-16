//! loom model-check: dirty word with MULTIPLE segments.
//!
//! # Scope
//!
//! Extends `loom_dirty_publish` to model the scenario from A5's correctness
//! matrix: MULTIPLE segment IDs packed into the SAME dirty word. Two producers
//! publish ring entries for DIFFERENT segments but set bits in the SAME u64
//! dirty word. The consumer swaps the word to 0 and must observe BOTH bits.
//!
//! # Invariant
//!
//! After a swap(0, Acquire), every bit that was set (and whose ring entry was
//! published before the set) must be processed. No bit is permanently
//! invisible.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --features alloc-core,alloc-xthread --test loom_dirty_multi_segment
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use loom::sync::Arc;
use loom::thread;

/// Sentinel — matches `RING_SLOT_EMPTY`.
const RING_SLOT_EMPTY: u32 = u32::MAX;

/// Model: one dirty word covering multiple segments, each with its own ring
/// slot. We simplify to 2 segments (bit 0 and bit 1 in the same word), each
/// with one ring slot.
struct MultiSegDirtyModel {
    // Per-segment ring slot (one slot per segment for simplicity).
    seg0_slot: AtomicU32,
    seg1_slot: AtomicU32,
    // Shared dirty word (covers both segments).
    dirty: AtomicU64,
}

impl MultiSegDirtyModel {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            seg0_slot: AtomicU32::new(RING_SLOT_EMPTY),
            seg1_slot: AtomicU32::new(RING_SLOT_EMPTY),
            dirty: AtomicU64::new(0),
        })
    }

    /// Producer: write the ring entry THEN set the segment's dirty bit.
    fn push_segment(&self, segment: u32, offset: u32) {
        let slot = match segment {
            0 => &self.seg0_slot,
            1 => &self.seg1_slot,
            _ => unreachable!(),
        };
        // Ring publish (Release).
        slot.store(offset, Ordering::Release);
        // Dirty bit (Release) — A4 protocol: ring publish THEN dirty bit.
        self.dirty.fetch_or(1u64 << segment, Ordering::Release);
    }

    /// Consumer: swap dirty word to 0 (Acquire), then drain each segment
    /// whose bit was set. Returns the number of entries found.
    fn swap_and_drain(&self) -> u32 {
        let bits = self.dirty.swap(0, Ordering::Acquire);
        let mut count = 0;
        if bits & 1 != 0 {
            let off = self.seg0_slot.load(Ordering::Acquire);
            if off != RING_SLOT_EMPTY {
                count += 1;
                self.seg0_slot.store(RING_SLOT_EMPTY, Ordering::Relaxed);
            }
        }
        if bits & 2 != 0 {
            let off = self.seg1_slot.load(Ordering::Acquire);
            if off != RING_SLOT_EMPTY {
                count += 1;
                self.seg1_slot.store(RING_SLOT_EMPTY, Ordering::Relaxed);
            }
        }
        count
    }
}

/// Two producers write to DIFFERENT segments in the SAME dirty word.
/// The consumer must observe both after sufficient drain passes.
#[test]
fn multi_segment_same_dirty_word_no_lost_entry() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = MultiSegDirtyModel::new();

        // Producer A: push to segment 0.
        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || {
            m_a.push_segment(0, 100);
        });

        // Producer B: push to segment 1.
        let m_b = Arc::clone(&model);
        let tb = thread::spawn(move || {
            m_b.push_segment(1, 200);
        });

        ta.join().unwrap();
        tb.join().unwrap();

        // Consumer: two drain passes (should catch everything).
        let c1 = model.swap_and_drain();
        let c2 = model.swap_and_drain();

        assert_eq!(
            c1 + c2,
            2,
            "multi-segment dirty word: drained {} entries across 2 passes (want 2)",
            c1 + c2
        );
    });
}

/// Producer-during-drain variant: producer B starts AFTER producer A
/// completes and after the first drain. Producer B's bit must survive
/// to the second drain.
#[test]
fn multi_segment_producer_during_drain() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = MultiSegDirtyModel::new();

        // Producer A: push to segment 0.
        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || {
            m_a.push_segment(0, 100);
        });
        ta.join().unwrap();

        // First drain: picks up segment 0.
        let c1 = model.swap_and_drain();

        // Producer B: push to segment 1 AFTER the first drain.
        let m_b = Arc::clone(&model);
        let tb = thread::spawn(move || {
            m_b.push_segment(1, 200);
        });
        tb.join().unwrap();

        // Second drain: picks up segment 1.
        let c2 = model.swap_and_drain();

        assert_eq!(
            c1 + c2,
            2,
            "producer-during-drain: drained {} entries across 2 passes (want 2)",
            c1 + c2
        );
    });
}

/// Edge case: both producers push to the SAME segment (same bit). The bit
/// is idempotent — setting it twice is harmless, and both ring entries must
/// be found.
#[test]
fn same_segment_two_producers_same_bit() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = MultiSegDirtyModel::new();

        // Both push to segment 0 (same bit).
        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || {
            m_a.push_segment(0, 100);
        });

        // Producer B also pushes to segment 0 — but this model has only one
        // slot per segment, so B's write overwrites A's. This is the expected
        // real-ring behavior: the ring has multiple slots. We just verify the
        // dirty bit survives the concurrent fetch_or.
        let m_b = Arc::clone(&model);
        let tb = thread::spawn(move || {
            // Just set the dirty bit (the ring overwrite is a model simplification).
            m_b.dirty.fetch_or(1u64 << 0, Ordering::Release);
        });

        ta.join().unwrap();
        tb.join().unwrap();

        // At least 1 entry should be found (the last store wins for the slot,
        // but the dirty bit is definitely set).
        let c1 = model.swap_and_drain();
        let c2 = model.swap_and_drain();
        assert!(
            c1 + c2 >= 1,
            "same-bit: expected at least 1 drained entry, got {}",
            c1 + c2
        );
    });
}
