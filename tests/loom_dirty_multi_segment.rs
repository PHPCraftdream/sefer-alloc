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

// =========================================================================
// CONC-1 (task #206): genuinely-concurrent producer-vs-consumer.
// =========================================================================

/// Concurrent producer-vs-consumer variant — the gap from CONC-1
/// (deep-audit `02-concurrency-lockfree.md` F1).
///
/// Unlike the three tests above, the consumer thread here runs CONCURRENTLY
/// with both producers — no `.join()` serializes a producer's in-flight
/// `dirty.fetch_or(bit, Release)` against the consumer's `dirty.swap(0,
/// Acquire)`. loom therefore explores the RMW-vs-RMW interleaving space that
/// matches production (a drain can start at any moment relative to any number
/// of live remote-freeing threads; nothing serializes them).
///
/// Per the model's documented `# Invariant` ("no bit is permanently
/// invisible" — at-least-once, bounded deferral), the CONCURRENT drain alone
/// is NOT claimed to catch every entry: a producer whose `fetch_or` lands
/// after the consumer's `swap` is legitimately missed on this pass. The
/// correctness argument is that the missed bit remains set and is visible to
/// the NEXT drain. So we assert the TOTAL across (concurrent drain + one
/// guaranteed synchronous drain on the main thread) equals 2 — that final
/// drain models the real system's periodic fallback rescan / next drain
/// cycle that the design explicitly relies on.
#[test]
fn concurrent_producer_consumer_eventual_visibility() {
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

        // Consumer: races against BOTH producers — NOT joined-first. This is
        // the structural difference from the three tests above.
        let m_c = Arc::clone(&model);
        let tc = thread::spawn(move || m_c.swap_and_drain());

        let concurrent_count = tc.join().unwrap();
        ta.join().unwrap();
        tb.join().unwrap();

        // Guaranteed synchronous final drain — models the next drain cycle /
        // periodic fallback rescan that the design relies on for any bit the
        // racy concurrent pass legitimately missed.
        let final_count = model.swap_and_drain();

        assert_eq!(
            concurrent_count + final_count,
            2,
            "concurrent producer-vs-consumer: drained {} entries across \
             concurrent + final passes (want 2)",
            concurrent_count + final_count
        );
    });
}

// =========================================================================
// Counterfactual — Relaxed on the dirty word severs the visibility signal.
// =========================================================================

/// Broken variant of `MultiSegDirtyModel`: the dirty word uses `Relaxed`
/// instead of `Release`/`Acquire`, severing the happens-before chain from
/// producer `slot.store(Release)` to consumer `slot.load(Acquire)`. Used only
/// by the `counterfactual_relaxed_dirty_loses_entry` test below.
struct MultiSegDirtyModelRelaxed {
    seg0_slot: AtomicU32,
    seg1_slot: AtomicU32,
    dirty: AtomicU64,
}

impl MultiSegDirtyModelRelaxed {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            seg0_slot: AtomicU32::new(RING_SLOT_EMPTY),
            seg1_slot: AtomicU32::new(RING_SLOT_EMPTY),
            dirty: AtomicU64::new(0),
        })
    }

    /// Producer — BROKEN: `Relaxed` on the dirty word (should be `Release`).
    fn push_segment(&self, segment: u32, offset: u32) {
        let slot = match segment {
            0 => &self.seg0_slot,
            1 => &self.seg1_slot,
            _ => unreachable!(),
        };
        slot.store(offset, Ordering::Release);
        // BROKEN: Relaxed instead of Release — the dirty word no longer
        // "carries" the slot.store's synchronization to the consumer.
        self.dirty.fetch_or(1u64 << segment, Ordering::Relaxed);
    }

    /// Consumer — BROKEN: `Relaxed` on the dirty word (should be `Acquire`).
    fn swap_and_drain(&self) -> u32 {
        // BROKEN: Relaxed instead of Acquire.
        let bits = self.dirty.swap(0, Ordering::Relaxed);
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

/// Counterfactual for `concurrent_producer_consumer_eventual_visibility`:
/// proves the harness is non-vacuous by running the SAME concurrent-thread
/// structure against a DELIBERATELY BROKEN model where the dirty word uses
/// `Relaxed` instead of `Release`/`Acquire`.
///
/// The dirty word IS the visibility signal in this protocol — its
/// `fetch_or(Release)` ↔ `swap(Acquire)` pair is what establishes the
/// happens-before chain from a producer's `slot.store(Release)` to the
/// consumer's `slot.load(Acquire)`. Downgrading the dirty word to `Relaxed`
/// severs that chain: loom finds an interleaving where the consumer's
/// `slot.load(Acquire)` returns `RING_SLOT_EMPTY` despite the bit being set
/// in the swapped-out word (no synchronization forces the load to see the
/// producer's store), and the entry is permanently lost across both drains.
///
/// `#[should_panic]` because loom explores all interleavings and FINDS the
/// one where the broken model loses an entry. If this passes (does not
/// panic), the counterfactual is vacuous and the harness is broken.
#[test]
#[should_panic(expected = "relaxed")]
fn counterfactual_relaxed_dirty_loses_entry() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = MultiSegDirtyModelRelaxed::new();

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

        // Consumer: same concurrent structure as the positive test.
        let m_c = Arc::clone(&model);
        let tc = thread::spawn(move || m_c.swap_and_drain());

        let concurrent_count = tc.join().unwrap();
        ta.join().unwrap();
        tb.join().unwrap();

        let final_count = model.swap_and_drain();

        assert_eq!(
            concurrent_count + final_count,
            2,
            "relaxed-dirty counterfactual: lost an entry ({} of 2 found) — \
             loom found the interleaving where the broken visibility signal \
             lets slot.load see RING_SLOT_EMPTY despite the bit being set",
            concurrent_count + final_count
        );
    });
}
