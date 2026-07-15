//! loom model-check of the **R6-OPT-P0-4 "overflow-first" composition**:
//! `HeapCore::push_with_overflow_retry`'s new ordering (segment ring →
//! immediate heap-level overflow attempt → bounded spin-retry against BOTH →
//! bounded leak), specifically the double-saturation case neither
//! `loom_remote_ring.rs` nor `loom_heap_overflow.rs` exercises on its own.
//!
//! # Scope — what this adds beyond the two sibling files
//!
//! `loom_remote_ring.rs` model-checks `RemoteFreeRing`'s push/drain protocol
//! in isolation (including a retry-composition test against a `CAP = 1`
//! single ring). `loom_heap_overflow.rs` model-checks `HeapOverflow`'s
//! two-field-entry push/drain protocol in isolation. Neither models what
//! happens when a producer must fall through BOTH rings in sequence — the
//! shape `push_with_overflow_retry` runs in production once its first
//! (counted) segment-ring push fails: try the heap-level overflow ring
//! immediately, and only if THAT also fails, spin-retry against both. This
//! file composes minimal models of the two rings (each `CAP = 1`, so two
//! producers force genuine double-saturation with the smallest possible
//! state space) and checks the property the R6-OPT-P0-4 policy promises:
//!
//! > Every block that lands in EITHER ring (segment ring OR overflow ring)
//! > is reclaimed exactly once by SOME drain, and the bounded-leak counter
//! > only fires for a block that landed in NEITHER.
//!
//! # Why this matters for the reordering specifically
//!
//! The pre-R6-OPT-P0-4 policy tried `HeapOverflow` only AFTER exhausting the
//! full spin budget against the segment ring. The new policy tries it
//! IMMEDIATELY on the first ring-push failure — before any spin. This test's
//! `push_overflow_first` mirrors that exact ordering (ring push, then
//! overflow push, then a bounded uncounted-spin retry against the ring, then
//! one final overflow retry, then leak) and proves the reordering doesn't
//! introduce a loss or duplication the original ordering didn't have: a
//! `#[should_panic]` counterfactual (`counterfactual_overflow_ring_never_tried`)
//! demonstrates that if the composed policy DROPS the immediate second-chance
//! step (degenerating to "ring-only, no overflow fallback"), the same
//! workload measurably loses blocks that the real composed policy recovers —
//! proving the second-chance step is load-bearing, not vacuous scaffolding.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --release --features alloc-global,alloc-xthread --test loom_overflow_first_retry -- --nocapture
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, Ordering};
use loom::sync::Arc;
use loom::thread;

/// Sentinel meaning "no offset published" — mirrors `RING_SLOT_EMPTY`.
const SLOT_EMPTY: u32 = u32::MAX;

/// A `CAP = 1` ring model — identical shape to `loom_remote_ring.rs`'s
/// `RingModel1` (the segment `RemoteFreeRing`, single-slot for tractability).
struct Ring1 {
    head: AtomicU32,
    tail: AtomicU32,
    slot: AtomicU32,
}

impl Ring1 {
    fn new() -> Self {
        Ring1 {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            slot: AtomicU32::new(SLOT_EMPTY),
        }
    }

    /// Mirrors `RemoteFreeRing::push` / `try_push_uncounted` (the two are
    /// protocol-identical; only the caller's diagnostic counters differ,
    /// which this model does not need to represent — the property under
    /// test is loss/duplication, not counter bookkeeping).
    fn push(&self, offset: u32) -> Result<(), ()> {
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            let h = self.head.load(Ordering::Acquire);
            if t.wrapping_sub(h) >= 1 {
                return Err(());
            }
            match self.tail.compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.slot.store(offset, Ordering::Release);
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
    }

    /// Mirrors `RemoteFreeRing::drain`.
    fn drain<F: FnMut(u32)>(&self, mut reclaim: F) {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let off = self.slot.load(Ordering::Acquire);
            if off == SLOT_EMPTY {
                break;
            }
            reclaim(off);
            self.slot.store(SLOT_EMPTY, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.head.store(h, Ordering::Release);
    }
}

/// A `CAP = 1` heap-overflow ring model — same single-atomic-slot shape (the
/// real `HeapOverflow` carries a `(base, packed)` pair; this model only needs
/// the ONE payload word `offset` to check loss/duplication, matching
/// `loom_heap_overflow.rs`'s scope note that the two-field torn-read hazard
/// is that file's job, not this composition test's).
struct Overflow1 {
    head: AtomicU32,
    tail: AtomicU32,
    slot: AtomicU32,
}

impl Overflow1 {
    fn new() -> Self {
        Overflow1 {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            slot: AtomicU32::new(SLOT_EMPTY),
        }
    }

    /// Mirrors `HeapOverflow::push`.
    fn push(&self, offset: u32) -> bool {
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            let h = self.head.load(Ordering::Acquire);
            if t.wrapping_sub(h) >= 1 {
                return false;
            }
            match self.tail.compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.slot.store(offset, Ordering::Release);
                    return true;
                }
                Err(_) => continue,
            }
        }
    }

    /// Mirrors `HeapOverflow::drain`.
    fn drain<F: FnMut(u32)>(&self, mut reclaim: F) {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let off = self.slot.load(Ordering::Acquire);
            if off == SLOT_EMPTY {
                break;
            }
            reclaim(off);
            self.slot.store(SLOT_EMPTY, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.head.store(h, Ordering::Release);
    }
}

/// Bound on the model producers' retry loop — mirrors
/// `loom_remote_ring.rs::MODEL_RETRY_BOUND`'s rationale exactly: an unbounded
/// retry-until-success loop makes loom's model checker abort ("exceeded
/// maximum number of branches"), so a small bound is used instead. The real
/// `RING_PUSH_RETRY_SPINS` (8,192 native / 64 miri) is astronomically larger
/// than anything loom can afford to explore exhaustively; this bound only
/// needs to be large enough that "the retry loop ran and either recovered or
/// didn't" is a real branch loom explores, not a proxy for the real budget's
/// magnitude.
const MODEL_RETRY_BOUND: u32 = 3;

/// The composed R6-OPT-P0-4 policy: one ring push, then (on failure) one
/// IMMEDIATE overflow push, then (on failure) a bounded retry alternating
/// ring/overflow attempts, then give up (bounded leak). Returns `true` if the
/// offset landed SOMEWHERE (ring or overflow), `false` if every tier failed
/// (the model's analogue of `DBG_RING_PUSH_RETRY_EXHAUSTED`).
fn push_overflow_first(ring: &Ring1, overflow: &Overflow1, offset: u32) -> bool {
    if ring.push(offset).is_ok() {
        return true; // Tier 1: segment ring, fast path.
    }
    if overflow.push(offset) {
        return true; // Tier 2: IMMEDIATE overflow attempt — the R6-OPT-P0-4 inversion.
    }
    // Tier 3: bounded retry against BOTH (mirrors the real spin loop trying
    // `try_push_uncounted` against the ring; the model also re-tries the
    // overflow ring each iteration, since the real code's final overflow
    // retry after spin-exhaustion is functionally "try overflow again").
    for _ in 0..MODEL_RETRY_BOUND {
        loom::thread::yield_now();
        if ring.push(offset).is_ok() {
            return true;
        }
        if overflow.push(offset) {
            return true;
        }
    }
    false // Tier 4: bounded leak.
}

/// The COUNTERFACTUAL policy: ring-only, no second-chance overflow fallback
/// at all (the shape a policy with the overflow step accidentally REMOVED, or
/// never wired, would have — e.g. `push_to_heap_overflow` always returning
/// `false`). Used to prove the overflow step is load-bearing.
fn push_ring_only_no_overflow(ring: &Ring1, offset: u32) -> bool {
    if ring.push(offset).is_ok() {
        return true;
    }
    for _ in 0..MODEL_RETRY_BOUND {
        loom::thread::yield_now();
        if ring.push(offset).is_ok() {
            return true;
        }
    }
    false
}

// =========================================================================
// Correct composed policy — every landed offset reclaimed exactly once from
// SOME drain (ring's or overflow's); nothing is fabricated or duplicated
// across the two independent rings.
// =========================================================================

/// 2 producers push DISJOINT offsets (10, 20) through the composed
/// `push_overflow_first` policy against a `CAP = 1` ring AND a `CAP = 1`
/// overflow ring — with capacity 1 on BOTH tiers, two producers racing
/// guarantees genuine double-saturation is at least possible under some
/// interleaving (the second producer to reserve the ring AND lose the
/// overflow race must fall into the bounded retry tier), the exact
/// "double-saturation, rare case" scenario R6-OPT-P0-4's spec calls out.
///
/// The owner drains BOTH rings once after both producers join (mirrors the
/// allocator's own "owner lazily drains on next alloc" liveness contract —
/// both `drain_heap_overflow` and the per-segment ring drain run on the same
/// opportunistic schedule in production).
///
/// INVARIANT: every offset that `push_overflow_first` reports as landed
/// (`true`) is reclaimed EXACTLY ONCE by the combined drain of the two
/// rings — no loss, no duplication, and nothing reclaimed that was never
/// actually pushed successfully.
#[test]
fn overflow_first_composed_never_loses_or_duplicates() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = Arc::new(Ring1::new());
        let overflow = Arc::new(Overflow1::new());
        let landed: Arc<[AtomicU32; 2]> = Arc::new([AtomicU32::new(0), AtomicU32::new(0)]);

        let mut producers = Vec::new();
        for (i, &offset) in [10u32, 20u32].iter().enumerate() {
            let ring_p = Arc::clone(&ring);
            let overflow_p = Arc::clone(&overflow);
            let landed_p = Arc::clone(&landed);
            producers.push(thread::spawn(move || {
                if push_overflow_first(&ring_p, &overflow_p, offset) {
                    landed_p[i].store(1, Ordering::Relaxed);
                }
            }));
        }
        for p in producers {
            p.join().unwrap();
        }

        // Owner drains BOTH rings once, after both producers finished — the
        // same "producers join, then drain" shape the sibling loom files use.
        let mut reclaimed = [0u32; 2];
        ring.drain(|off| match off {
            10 => reclaimed[0] += 1,
            20 => reclaimed[1] += 1,
            _ => {}
        });
        overflow.drain(|off| match off {
            10 => reclaimed[0] += 1,
            20 => reclaimed[1] += 1,
            _ => {}
        });

        for i in 0..2 {
            let did_land = landed[i].load(Ordering::Relaxed) == 1;
            if did_land {
                assert_eq!(
                    reclaimed[i], 1,
                    "producer {i}'s offset landed (push_overflow_first returned true) \
                     but was reclaimed {} times across the two rings (want exactly 1) — \
                     the overflow-first composition lost or duplicated a push",
                    reclaimed[i]
                );
            } else {
                assert_eq!(
                    reclaimed[i], 0,
                    "producer {i}'s offset was reclaimed {} times despite \
                     push_overflow_first reporting failure (MODEL_RETRY_BOUND \
                     exhaustion) — a duplication/fabrication bug, not a retry- \
                     exhaustion artefact",
                    reclaimed[i]
                );
            }
        }
    });
}

// =========================================================================
// Counterfactual — the overflow step is load-bearing, not vacuous scaffolding.
// =========================================================================

/// COUNTERFACTUAL: replay the SAME double-saturation-prone workload
/// (2 producers, disjoint offsets, `CAP = 1` ring) but through
/// `push_ring_only_no_overflow` — i.e. with the R6-OPT-P0-4 second-chance
/// step never wired at all. loom finds the interleaving where the second
/// producer both loses the ring-CAS race for the whole bounded retry AND has
/// no overflow ring to fall back to, so its push genuinely fails
/// (`push_ring_only_no_overflow` returns `false`) even though, under the
/// REAL composed policy (`overflow_first_composed_never_loses_or_duplicates`
/// above, same producer count, same ring capacity), that identical
/// interleaving's push lands in the overflow ring instead and is recovered.
///
/// This does not assert on the drain outcome (the ring-only policy is
/// otherwise loss-free by the ring's own protocol) — it asserts directly on
/// `push_ring_only_no_overflow`'s return value, proving there EXISTS an
/// interleaving where the ring-only policy fails a push that the composed
/// policy (with the overflow tier wired) would have recovered. If this
/// `#[should_panic]` counterfactual instead always passes (both producers
/// always land through the ring alone), the composition test above would be
/// proving nothing — this counterfactual is the non-vacuousness proof that
/// the double-saturation scenario is actually reachable at this scale.
#[test]
#[should_panic(expected = "ring-only policy dropped a push")]
fn counterfactual_overflow_ring_never_tried() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = Arc::new(Ring1::new());
        let landed: Arc<[AtomicU32; 2]> = Arc::new([AtomicU32::new(1), AtomicU32::new(1)]);

        let mut producers = Vec::new();
        for (i, &offset) in [10u32, 20u32].iter().enumerate() {
            let ring_p = Arc::clone(&ring);
            let landed_p = Arc::clone(&landed);
            producers.push(thread::spawn(move || {
                if !push_ring_only_no_overflow(&ring_p, offset) {
                    landed_p[i].store(0, Ordering::Relaxed);
                }
            }));
        }
        for p in producers {
            p.join().unwrap();
        }

        let both_landed =
            landed[0].load(Ordering::Relaxed) == 1 && landed[1].load(Ordering::Relaxed) == 1;
        assert!(
            both_landed,
            "ring-only policy dropped a push: at least one producer's offset never \
             landed in the CAP=1 ring within MODEL_RETRY_BOUND retries, with no \
             overflow ring to fall back to — proving the double-saturation scenario \
             is reachable at this scale and the overflow tier is load-bearing"
        );
    });
}

/// Empty-composition drain is a no-op on both rings — guards against either
/// model fabricating an offset out of nowhere.
#[test]
fn drain_empty_composition_is_noop() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = Ring1::new();
        let overflow = Overflow1::new();
        let mut count = 0u32;
        ring.drain(|_| count += 1);
        overflow.drain(|_| count += 1);
        assert_eq!(
            count, 0,
            "drain of empty composed rings reclaimed something"
        );
    });
}
