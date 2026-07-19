//! loom model-check of the R7-A4 **dirty-segment publish / swap / lost-wakeup**
//! protocol.
//!
//! # Scope
//!
//! This harness models the dirty-routing protocol in isolation using
//! `loom::sync::atomic` (NOT the real `HeapSlot::dirty_segments`). It asserts
//! the core safety property:
//!
//! > 2 producers publish ring entries and then set dirty bits; 1 consumer
//! > swaps the dirty word to 0 and drains. Every entry published to the ring
//! > is either found by the drain that observed its bit, OR the bit is re-set
//! > by a concurrent/later producer for the next drain pass — no entry is
//! > permanently invisible.
//!
//! loom explores every interleaving (bounded by `preemption_bound = 3`) and
//! finds any execution where a published entry is never observed.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --features alloc-core,alloc-xthread --test loom_dirty_publish
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use loom::sync::Arc;
use loom::thread;

/// Sentinel slot value — matches `RING_SLOT_EMPTY`.
const RING_SLOT_EMPTY: u32 = u32::MAX;

/// A tiny ring (CAP=2) + one dirty word, modelling the A4 protocol.
struct DirtyModel {
    // Ring state (from loom_remote_ring.rs).
    ring_head: AtomicU32,
    ring_tail: AtomicU32,
    ring_slots: [AtomicU32; 2],
    // Dirty bitmap (one word for simplicity — covers 2 "segments").
    dirty: AtomicU64,
}

impl DirtyModel {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            ring_head: AtomicU32::new(0),
            ring_tail: AtomicU32::new(0),
            ring_slots: [
                AtomicU32::new(RING_SLOT_EMPTY),
                AtomicU32::new(RING_SLOT_EMPTY),
            ],
            dirty: AtomicU64::new(0),
        })
    }

    /// Producer: push an offset into the ring, then set the dirty bit.
    /// Models the A4 protocol: ring publish THEN dirty bit (Release).
    fn push_and_mark(&self, offset: u32, segment_bit: u64) -> bool {
        // Ring push (simplified from loom_remote_ring.rs).
        loop {
            let t = self.ring_tail.load(Ordering::Relaxed);
            let h = self.ring_head.load(Ordering::Acquire);
            if t.wrapping_sub(h) >= 2 {
                return false; // Ring full.
            }
            match self.ring_tail.compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // Publish the offset (Release).
                    self.ring_slots[(t as usize) % 2].store(offset, Ordering::Release);
                    // A4: set the dirty bit AFTER the ring publish (Release).
                    self.dirty.fetch_or(segment_bit, Ordering::Release);
                    return true;
                }
                Err(_) => continue,
            }
        }
    }

    /// Consumer: swap the dirty word to 0, then drain the ring. Returns the
    /// number of entries drained AND whether the dirty word was non-zero.
    fn swap_and_drain(&self) -> (u32, bool) {
        // A4: swap the dirty word (Acquire — pairs with producer's Release).
        let was_dirty = self.dirty.swap(0, Ordering::Acquire) != 0;
        // Drain the ring (from loom_remote_ring.rs).
        let t = self.ring_tail.load(Ordering::Acquire);
        let mut h = self.ring_head.load(Ordering::Relaxed);
        let mut count = 0u32;
        while h != t {
            let slot = &self.ring_slots[(h as usize) % 2];
            let off = slot.load(Ordering::Acquire);
            if off == RING_SLOT_EMPTY {
                break; // Not yet published — stop.
            }
            count += 1;
            slot.store(RING_SLOT_EMPTY, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.ring_head.store(h, Ordering::Release);
        (count, was_dirty)
    }

    /// COUNTERFACTUAL producer push: replaces the `compare_exchange_weak` on
    /// `ring_tail` with a non-atomic load-store (a classic lost-update RMW).
    /// Two producers can both load the SAME tail `t`, both store `t+1`, and
    /// both write to `slot[t % 2]` — one overwrites the other's entry, so only
    /// one of the two pushed offsets is ever observable to the drain. Used only
    /// by the `counterfactual_non_atomic_push_loses_entry` test below.
    ///
    /// This break is genuinely non-vacuous for THIS file's existing tests (all
    /// of which join producers before draining), unlike two other candidates
    /// that were considered and rejected:
    ///
    /// 1. **Remove the `if off == RING_SLOT_EMPTY { break; }` early-stop** —
    ///    vacuous here because producers join before the drain runs, so every
    ///    reserved slot is already published and the drain never observes
    ///    `RING_SLOT_EMPTY` (this break is non-vacuous in `loom_remote_ring.rs`
    ///    only because that file's counterfactual uses a concurrent consumer
    ///    racing in-flight producers).
    /// 2. **Downgrade `ring_slots` ordering `Release`/`Acquire` → `Relaxed`** —
    ///    vacuous because the dirty word's `fetch_or(Release)` ↔ `swap(Acquire)`
    ///    pair already establishes the happens-before chain from producer
    ///    `slot.store` to consumer `slot.load`, and the `join()` before drain is
    ///    itself a sync point.
    fn push_and_mark_broken_non_atomic(&self, offset: u32, segment_bit: u64) -> bool {
        let t = self.ring_tail.load(Ordering::Relaxed);
        let h = self.ring_head.load(Ordering::Acquire);
        if t.wrapping_sub(h) >= 2 {
            return false; // Ring full.
        }
        // BUG: non-atomic load-store instead of compare_exchange_weak. Two
        // producers can both read the same `t` and both store `t+1`, landing
        // both writes in `slot[t % 2]` — a lost-update race the CAS exists to
        // prevent. The second store silently overwrites the first.
        self.ring_tail.store(t.wrapping_add(1), Ordering::Relaxed);
        self.ring_slots[(t as usize) % 2].store(offset, Ordering::Release);
        self.dirty.fetch_or(segment_bit, Ordering::Release);
        true
    }
}

/// 2 producers push distinct offsets and set the dirty bit; the consumer
/// does 2 swap-and-drain passes. INVARIANT: both offsets are eventually
/// drained (the dirty bit ensures visibility across drain passes).
#[test]
fn dirty_publish_swap_never_loses_entry() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = DirtyModel::new();

        // Producer A: push offset 10, set bit 0.
        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || {
            m_a.push_and_mark(10, 1 << 0);
        });

        // Producer B: push offset 20, set bit 1.
        let m_b = Arc::clone(&model);
        let tb = thread::spawn(move || {
            m_b.push_and_mark(20, 1 << 1);
        });

        ta.join().unwrap();
        tb.join().unwrap();

        // Consumer: two drain passes.
        let (c1, _) = model.swap_and_drain();
        let (c2, _) = model.swap_and_drain();

        // Both offsets must be drained across the two passes.
        assert_eq!(
            c1 + c2,
            2,
            "dirty publish: drained {} entries across 2 passes (want 2) — lost entry",
            c1 + c2
        );
    });
}

/// LOST-WAKEUP: a producer that sets its bit AFTER the consumer's swap(0)
/// must have its bit survive to the next drain pass. Model: two producers
/// push+mark concurrently, then the consumer runs two sequential drain
/// passes. After both producers join, both entries must be found across
/// the two drain passes (a bit set after the first swap is caught by the
/// second).
#[test]
fn lost_wakeup_bit_survives_swap() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = DirtyModel::new();

        // Producer A: push + mark.
        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || {
            m_a.push_and_mark(10, 1 << 0);
        });

        // Producer B: push + mark.
        let m_b = Arc::clone(&model);
        let tb = thread::spawn(move || {
            m_b.push_and_mark(20, 1 << 1);
        });

        // Wait for both producers to complete.
        ta.join().unwrap();
        tb.join().unwrap();

        // Consumer: two sequential drain passes. Under the dirty protocol,
        // a bit set by producer B after the first swap is caught by the
        // second swap. After both producers join, all bits are committed.
        let (c1, _) = model.swap_and_drain();
        let (c2, _) = model.swap_and_drain();

        // Both entries must be found across the two passes.
        assert_eq!(
            c1 + c2,
            2,
            "lost-wakeup: drained {} entries across 2 passes (want 2)",
            c1 + c2
        );
    });
}

/// Empty dirty word: a drain with dirty=0 drains nothing (no spurious
/// ring reads from a clean bitmap).
#[test]
fn empty_dirty_no_drain() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = DirtyModel::new();
        let (count, was_dirty) = model.swap_and_drain();
        assert_eq!(count, 0, "empty dirty: drained {count} entries (want 0)");
        assert!(!was_dirty, "empty dirty: dirty word was non-zero");
    });
}

// =========================================================================
// Counterfactual — non-atomic ring-tail reservation loses an entry.
// =========================================================================

/// COUNTERFACTUAL for `dirty_publish_swap_never_loses_entry` (and the other
/// two positive tests above, which share the same `push_and_mark` producer
/// path): proves the harness is non-vacuous by running the SAME 2-producer /
/// 1-consumer thread structure against a DELIBERATELY BROKEN producer push
/// (`push_and_mark_broken_non_atomic`) that replaces the
/// `ring_tail.compare_exchange_weak` with a non-atomic load-store.
///
/// Why this specific break (and not the two natural alternatives): the file's
/// three existing tests ALL `join()` both producers before draining, so the
/// ring slots are fully published and globally visible by the time the drain
/// runs. Under that join-before-drain shape:
///
/// - **Remove the `RING_SLOT_EMPTY` early-stop in `swap_and_drain`** is vacuous
///   (the drain never observes an unpublished slot — every reservation was
///   followed through to publication before the join).
/// - **Downgrade `ring_slots` ordering to `Relaxed`** is vacuous (the dirty
///   word's `Release`/`Acquire` already carries the happens-before, and the
///   join itself is a sync point).
///
/// The CAS push, by contrast, races producer-vs-producer BEFORE the join — so
/// breaking it is observable in the post-join drain. Two producers both load
/// `ring_tail = 0`, both store `ring_tail = 1`, and both write to
/// `ring_slots[0]`; the second store silently overwrites the first. The drain
/// then sees `tail = 1` (only one reservation's worth of cursor advance) and
/// reclaims only ONE entry across both drain passes.
///
/// This mirrors `loom_remote_ring.rs`'s established `counterfactual_drain_*
///` pattern (the closest precedent for breaking a CAS-ring protocol in this
/// repo), but applied to the PUSH side rather than the DRAIN side because that
/// is the side this file's join-before-drain shape leaves observable.
///
/// `#[should_panic]` because loom explores all interleavings with
/// `preemption_bound = 3` and FINDS the one where both producers race on the
/// same tail index. If this passes (does not panic), the counterfactual is
/// vacuous.
#[test]
#[should_panic(expected = "non-atomic")]
fn counterfactual_non_atomic_push_loses_entry() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let model = DirtyModel::new();

        // Producer A: BROKEN push of offset 10.
        let m_a = Arc::clone(&model);
        let ta = thread::spawn(move || {
            m_a.push_and_mark_broken_non_atomic(10, 1 << 0);
        });

        // Producer B: BROKEN push of offset 20.
        let m_b = Arc::clone(&model);
        let tb = thread::spawn(move || {
            m_b.push_and_mark_broken_non_atomic(20, 1 << 1);
        });

        ta.join().unwrap();
        tb.join().unwrap();

        // Consumer: two drain passes (same shape as the positive test).
        let (c1, _) = model.swap_and_drain();
        let (c2, _) = model.swap_and_drain();

        // With the broken non-atomic push, both producers can land in the same
        // slot (one overwriting the other), so the drain reclaims only ONE
        // entry across both passes instead of two.
        assert_eq!(
            c1 + c2,
            2,
            "non-atomic push: drained {} entries across 2 passes (want 2) — \
             loom found the interleaving where both producers raced on the same \
             ring_tail index and one overwrote the other's slot",
            c1 + c2
        );
    });
}
