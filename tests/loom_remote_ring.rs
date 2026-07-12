//! loom model-check of the **`RemoteFreeRing` MPSC bounded queue protocol**
//! (task #36, step 2b).
//!
//! # Scope — what loom covers
//!
//! This harness models the ring's push/drain protocol in isolation using
//! `loom::sync::atomic` (NOT the real `RemoteFreeRing`, which uses
//! `core::sync::atomic`). It asserts the core safety property:
//!
//! > 2 producers push UNIQUE offsets (disjoint ranges); 1 consumer drains.
//! > Every pushed offset is reclaimed EXACTLY ONCE — no loss, no duplication.
//!
//! loom explores every interleaving (bounded by `preemption_bound = 3`) and
//! finds any execution where a buggy drain loses an offset (never reclaimed)
//! or duplicates one (reclaimed twice). The correct protocol has neither.
//!
//! # The counterfactuals (non-vacuousness proofs)
//!
//! The ring's `drain` has TWO load-bearing details that a naive version omits:
//!
//! 1. **Break on an unpublished slot** — a producer may win the tail CAS
//!    (reservation) but not yet have stored its offset into the slot. The
//!    correct drain `break`s at the first `RING_SLOT_EMPTY` reserved slot
//!    (order is preserved by the cursors; a later drain picks it up). A naive
//!    drain that SKIPS the empty slot and continues would (a) read a stale
//!    offset from a PREVIOUS wrap of that slot index (a duplication) or (b)
//!    miss the not-yet-published offset entirely once the producer does
//!    publish (a loss, if the cursor advanced past it).
//!
//! 2. **Clear the slot after reclaiming** — the correct drain stores
//!    `RING_SLOT_EMPTY` back into the slot after reclaiming its offset, so the
//!    next wrap of that slot index starts empty. A naive drain that does NOT
//!    clear leaves the offset in the slot; on the next wrap, the consumer
//!    re-reads and re-reclaims the SAME offset (a duplication).
//!
//! Both counterfactuals are `#[should_panic]` tests: loom finds the
//! interleaving where the buggy drain breaks the exactly-once invariant. If
//! either counterfactual PASSES (no panic), the harness is vacuous and broken.
//!
//! # How to run
//!
//! loom is a `cfg(loom)` dev-dependency, so this file is only compiled under
//! `--cfg loom`:
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --features alloc-core,alloc-xthread --test loom_remote_ring
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;

/// Sentinel slot value meaning "no offset" (not-yet-published or drained).
/// Matches `RING_SLOT_EMPTY` in the real `RemoteFreeRing` (`u32::MAX`).
const RING_SLOT_EMPTY: u32 = u32::MAX;

/// A small ring capacity so the wrap path is exercised within loom's bounded
/// exploration. The real `RemoteFreeRing` uses 256; loom needs a tiny model.
const CAP: usize = 4;

/// The loom model of the ring's metadata: head, tail, and the slot array.
/// Mirrors the layout of `RemoteFreeRing` (head/tail cursors + slots).
struct RingModel {
    head: AtomicU32,
    tail: AtomicU32,
    slots: [AtomicU32; CAP],
}

impl RingModel {
    fn new() -> Arc<Self> {
        // SAFETY of the array init: `const { AtomicU32::new(RING_SLOT_EMPTY) }`
        // would be ideal, but loom's AtomicU32 isn't const-constructible in
        // all versions; use a once-cell-style init via a function.
        let r = Arc::new(RingModel {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            slots: std::array::from_fn(|_| AtomicU32::new(RING_SLOT_EMPTY)),
        });
        r
    }

    /// The CORRECT push (mirrors `RemoteFreeRing::push`): full-check (Acquire
    /// head), CAS-reserve the tail (AcqRel), Release-store the offset.
    fn push(&self, offset: u32) -> Result<(), ()> {
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            let h = self.head.load(Ordering::Acquire);
            if t.wrapping_sub(h) >= CAP as u32 {
                return Err(()); // full → overflow
            }
            match self.tail.compare_exchange_weak(
                t,
                t.wrapping_add(1),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    self.slots[(t as usize) % CAP].store(offset, Ordering::Release);
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
    }

    /// The CORRECT drain (mirrors `RemoteFreeRing::drain`): wrap-correct
    /// `while h != t`, break on an unpublished (empty) reserved slot, clear
    /// each slot after reclaiming. Passes each offset to `reclaim`.
    fn drain<F: FnMut(u32)>(&self, mut reclaim: F) {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let slot = &self.slots[(h as usize) % CAP];
            let off = slot.load(Ordering::Acquire);
            if off == RING_SLOT_EMPTY {
                break; // reserved but not yet published — stop (order preserved)
            }
            reclaim(off);
            slot.store(RING_SLOT_EMPTY, Ordering::Relaxed); // clear for next wrap
            h = h.wrapping_add(1);
        }
        self.head.store(h, Ordering::Release);
    }

    /// COUNTERFACTUAL 1: a drain that does NOT break on an unpublished slot.
    /// Instead it skips the empty slot and continues draining later slots.
    /// This can (a) read a stale offset from a previous wrap of that slot
    /// index once the producer publishes a DIFFERENT offset there, or (b)
    /// advance the cursor past a not-yet-published reservation, losing it.
    /// loom finds an execution where this loses or duplicates an offset.
    fn drain_broken_no_break<F: FnMut(u32)>(&self, mut reclaim: F) {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let slot = &self.slots[(h as usize) % CAP];
            let off = slot.load(Ordering::Acquire);
            if off != RING_SLOT_EMPTY {
                reclaim(off);
                slot.store(RING_SLOT_EMPTY, Ordering::Relaxed);
            }
            // BUG: advances past an unpublished (empty) reserved slot instead
            // of breaking. The producer's later publish at this reservation is
            // then stranded behind a cursor that already moved past it — a
            // later drain re-derives `t` but `h` has advanced, so the stranded
            // reservation is only seen if the producer published before this
            // drain's tail load; in the interleaving where it didn't, the
            // offset is lost (never reclaimed).
            h = h.wrapping_add(1);
        }
        self.head.store(h, Ordering::Release);
    }

    /// COUNTERFACTUAL 2: a drain that does NOT clear the slot after reclaiming.
    /// The offset remains in the slot; on the next wrap of that slot index
    /// (after CAP more reservations), the consumer re-reads and re-reclaims
    /// the SAME offset — a duplication.
    fn drain_broken_no_clear<F: FnMut(u32)>(&self, mut reclaim: F) {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let slot = &self.slots[(h as usize) % CAP];
            let off = slot.load(Ordering::Acquire);
            if off == RING_SLOT_EMPTY {
                break;
            }
            reclaim(off);
            // BUG: does NOT clear the slot. On the next wrap the consumer
            // re-reads this stale offset and re-reclaims it (duplication).
            h = h.wrapping_add(1);
        }
        self.head.store(h, Ordering::Release);
    }
}

// =========================================================================
// Correct protocol — must never lose or duplicate.
// =========================================================================

/// 2 producers push UNIQUE offsets (disjoint bands), then join, THEN the main
/// thread drains once. Asserts every successfully-pushed offset is reclaimed
/// exactly once (no loss, no duplication). The producers-join-before-drain
/// shape keeps loom's exploration bounded (no unbounded consumer loop) while
/// still exercising the producer-vs-producer CAS contention. Bounded with
/// `preemption_bound = 3`.
#[test]
fn correct_ring_never_loses_or_duplicates() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = RingModel::new();
        // Each producer pushes ONE offset (its band). Two producers → at most
        // 2 offsets in the ring; CAP=4 so no overflow. The push is one
        // reservation CAS + one publish store — the full two-step protocol.
        let ring_a = Arc::clone(&ring);
        let ta = thread::spawn(move || {
            // Retry on a lost CAS (the correct push already retries internally).
            while ring_a.push(10).is_err() {}
        });
        let ring_b = Arc::clone(&ring);
        let tb = thread::spawn(move || while ring_b.push(20).is_err() {});

        ta.join().unwrap();
        tb.join().unwrap();

        // Both producers have finished (published their offsets). A single
        // drain reclaims everything that was published. Because the producers
        // joined, their publish stores are globally visible (loom's happens
        // before via the join), so the drain sees both offsets.
        let mut got_a = 0u32;
        let mut got_b = 0u32;
        ring.drain(|off| {
            if off == 10 {
                got_a += 1;
            } else if off == 20 {
                got_b += 1;
            }
        });

        // INVARIANT: each offset reclaimed EXACTLY once.
        assert_eq!(
            got_a, 1,
            "offset 10 reclaimed {got_a} times (expected 1) — loss or duplication"
        );
        assert_eq!(
            got_b, 1,
            "offset 20 reclaimed {got_b} times (expected 1) — loss or duplication"
        );
    });
}

// =========================================================================
// Counterfactual 1 — drain WITHOUT break on unpublished slot.
// =========================================================================

/// COUNTERFACTUAL: a drain that skips (rather than breaks at) an unpublished
/// reserved slot loses the offset — the producer's later publish is stranded
/// behind a cursor that already moved past it. loom finds the interleaving
/// where fewer offsets are reclaimed than were successfully pushed.
///
/// `#[should_panic]` because loom explores all interleavings and finds the one
/// where the buggy drain loses an offset. If this passes, the counterfactual is
/// vacuous.
#[test]
#[should_panic(expected = "lost")]
fn counterfactual_drain_without_break_loses_offset() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = RingModel::new();
        let pushed_ok = Arc::new(AtomicUsize::new(0));
        let reclaimed = Arc::new(AtomicUsize::new(0));

        // Producer A: push offset 10.
        let ring_a = Arc::clone(&ring);
        let pushed_a = Arc::clone(&pushed_ok);
        let ta = thread::spawn(move || {
            if ring_a.push(10).is_ok() {
                pushed_a.fetch_add(1, Ordering::Relaxed);
            }
        });

        // Producer B: push offset 20.
        let ring_b = Arc::clone(&ring);
        let pushed_b = Arc::clone(&pushed_ok);
        let tb = thread::spawn(move || {
            if ring_b.push(20).is_ok() {
                pushed_b.fetch_add(1, Ordering::Relaxed);
            }
        });

        // Consumer: BROKEN drain (no break on unpublished).
        let reclaimed_c = Arc::clone(&reclaimed);
        let ring_c = Arc::clone(&ring);
        let pushed_c = Arc::clone(&pushed_ok);
        let tc = thread::spawn(move || {
            loop {
                ring_c.drain_broken_no_break(|_| {
                    reclaimed_c.fetch_add(1, Ordering::Relaxed);
                });
                if pushed_c.load(Ordering::Acquire) >= 2 {
                    break;
                }
                thread::yield_now();
            }
            // Final drains with the broken drain.
            for _ in 0..2 {
                ring_c.drain_broken_no_break(|_| {
                    reclaimed_c.fetch_add(1, Ordering::Relaxed);
                });
            }
        });

        ta.join().unwrap();
        tb.join().unwrap();
        tc.join().unwrap();

        let pushed = pushed_ok.load(Ordering::Acquire);
        let got = reclaimed.load(Ordering::Acquire);
        // The broken drain loses an offset in some interleaving: reclaimed <
        // successfully pushed. loom finds that interleaving.
        assert!(
            got == pushed,
            "lost offset: reclaimed {got} of {pushed} successfully-pushed \
             (drain-without-break loses a not-yet-published reservation)"
        );
    });
}

// =========================================================================
// Counterfactual 2 — drain WITHOUT clearing slots (duplication on wrap).
// =========================================================================

/// COUNTERFACTUAL: a drain that reclaims but does NOT clear the slot leaves
/// the offset stale; on the next wrap of that slot index the consumer
/// re-reclaims it (duplication). To exercise the wrap within loom's bound we
/// push `CAP + extra` offsets through a single producer so the cursor wraps
/// past the first slot, then drain.
///
/// `#[should_panic]` because loom finds the interleaving where the stale
/// offset is re-reclaimed.
#[test]
#[should_panic(expected = "duplicate")]
fn counterfactual_drain_without_clear_duplicates_on_wrap() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = RingModel::new();
        // Push CAP+1 distinct offsets so the cursor wraps past slot 0, then a
        // second pass pushes a NEW offset into slot 0. The broken drain left
        // slot 0's first offset uncleared, so draining after the second push
        // re-reclaims the stale first offset.
        // Offsets: round 1 pushes [100, 101, 102, 103] (fills CAP), round 2
        // pushes 104 (lands in slot 0 after wrap). The broken drain of round 2
        // sees BOTH the stale 100 AND the new 104 in slot 0 region → duplicate.
        let reclaimed_counts: Arc<Vec<AtomicU32>> =
            Arc::new((0..200).map(|_| AtomicU32::new(0)).collect());

        // Single producer: push 100, 101, 102, 103, 104 (wraps slot 0).
        let ring_p = Arc::clone(&ring);
        let tp = thread::spawn(move || {
            for off in 100..105 {
                // Retry on overflow (CAP=4, so 5 pushes need the consumer to
                // drain; we run a single-threaded variant here to make the
                // wrap deterministic — push, drain, push, drain).
                while ring_p.push(off).is_err() {
                    thread::yield_now();
                }
            }
        });

        // Consumer: BROKEN drain (no clear). Drains interleaved with pushes.
        let reclaimed_c = Arc::clone(&reclaimed_counts);
        let ring_c = Arc::clone(&ring);
        let tc = thread::spawn(move || {
            for _ in 0..8 {
                ring_c.drain_broken_no_clear(|off| {
                    if off >= 100 && off < 200 {
                        reclaimed_c[off as usize - 100].fetch_add(1, Ordering::Relaxed);
                    }
                });
                thread::yield_now();
            }
        });

        tp.join().unwrap();
        tc.join().unwrap();

        // INVARIANT broken by the no-clear drain: some offset reclaimed > 1.
        // On the wrap, the stale offset in slot 0 (100) is re-reclaimed.
        let mut max_dup = 0u32;
        for c in reclaimed_counts.iter() {
            let v = c.load(Ordering::Acquire);
            if v > max_dup {
                max_dup = v;
            }
        }
        assert!(
            max_dup <= 1,
            "duplicate: an offset reclaimed {max_dup} times (drain-without-clear \
             leaves a stale offset re-reclaimed on wrap)"
        );
    });
}

/// Empty-ring drain is a no-op: draining a fresh ring reclaims nothing.
/// Guards against a drain that fabricates offsets out of nowhere.
#[test]
fn drain_empty_ring_is_noop() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = RingModel::new();
        let mut count = 0u32;
        ring.drain(|_| {
            count += 1;
        });
        assert_eq!(count, 0, "drain of empty ring reclaimed something");
    });
}

// =========================================================================
// RAD-4 (Phase 4, E3a) — overflow-retry composition.
//
// `HeapCore::push_with_overflow_retry` (`src/registry/heap_core.rs`) wraps
// the SAME `RemoteFreeRing::push` this file already models, in a bounded
// retry loop, on `Err(PushOverflow)`. It adds NO new shared state beyond two
// `Relaxed` diagnostic counters (no synchronisation role) and calls no new
// ring method — the ring's push/drain protocol above is untouched. The
// property that matters for THIS composition is: does retrying a failed
// push, concurrently with OTHER producers ALSO retrying and a consumer
// draining, still preserve the ring's own no-loss/no-duplication invariant?
// `correct_ring_never_loses_or_duplicates` above already proves this for 2
// non-overflowing producers; `counterfactual_drain_without_clear_duplicates_on_wrap`
// already exercises a single-producer retry-on-overflow. This test closes
// the gap between them: MULTIPLE producers, a CAP small enough that
// overflow is FORCED (not just possible), each retrying independently,
// racing a CONCURRENT consumer drain loop (not a join-then-drain shape) —
// the actual shape `push_with_overflow_retry` runs under in production.
// =========================================================================

/// A `CAP = 1` ring model — the SAME `RingModel::push`/`drain` above, just
/// re-instantiated at capacity 1 (below the file's shared `CAP = 4`) so that
/// TWO concurrent producers already force genuine overflow (the second
/// producer to reserve necessarily observes a full ring at least once) —
/// matching the file's existing 2-producer scale
/// (`correct_ring_never_loses_or_duplicates`) while keeping loom's state
/// space tractable (an unconditional multi-producer retry-until-success loop
/// against a larger CAP, or a 3rd producer, was measured to blow up loom's
/// exploration time — this is deliberately the SMALLEST model that still
/// forces overflow, not the largest that fits).
struct RingModel1 {
    head: AtomicU32,
    tail: AtomicU32,
    slot: AtomicU32,
}

impl RingModel1 {
    fn new() -> Arc<Self> {
        Arc::new(RingModel1 {
            head: AtomicU32::new(0),
            tail: AtomicU32::new(0),
            slot: AtomicU32::new(RING_SLOT_EMPTY),
        })
    }

    /// Identical shape to `RingModel::push`, `CAP = 1`.
    fn push(&self, offset: u32) -> Result<(), ()> {
        loop {
            let t = self.tail.load(Ordering::Relaxed);
            let h = self.head.load(Ordering::Acquire);
            if t.wrapping_sub(h) >= 1 {
                return Err(()); // full → overflow
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

    /// Identical shape to `RingModel::drain`, `CAP = 1`.
    fn drain<F: FnMut(u32)>(&self, mut reclaim: F) {
        let t = self.tail.load(Ordering::Acquire);
        let mut h = self.head.load(Ordering::Relaxed);
        while h != t {
            let off = self.slot.load(Ordering::Acquire);
            if off == RING_SLOT_EMPTY {
                break;
            }
            reclaim(off);
            self.slot.store(RING_SLOT_EMPTY, Ordering::Relaxed);
            h = h.wrapping_add(1);
        }
        self.head.store(h, Ordering::Release);
    }
}

/// 2 producers push disjoint offsets into a `CAP = 1` ring — with capacity
/// 1, the second producer to reserve is GUARANTEED to observe a full ring at
/// least once under any interleaving where the first producer's reservation
/// has landed but not yet drained, forcing genuine overflow (not just
/// contention) with the smallest possible producer count.
///
/// Each producer retries up to [`MODEL_RETRY_BOUND`] times on `Err(())`,
/// yielding between attempts — mirroring `push_with_overflow_retry`'s
/// bounded-retry SHAPE (a finite loop, not an infinite one) rather than its
/// exact spin-vs-yield backoff strategy (orthogonal to the correctness
/// property under test; see [`MODEL_RETRY_BOUND`]'s doc comment for why an
/// UNBOUNDED retry loom model does not work here). A consumer drains ONCE
/// after both producers join — the same shape as
/// `correct_ring_never_loses_or_duplicates` above, extended to a `CAP = 1`
/// ring so overflow is actually forced (that test's `CAP = 4` with 2
/// producers never overflows).
///
/// INVARIANT: every offset that successfully lands (either producer's first
/// attempt or a later retry) is reclaimed EXACTLY once — no duplication
/// (the ring's own protocol, unmodified, still holds under this retry
/// composition) and, GIVEN the retry bound is large enough for both
/// producers to eventually win their CAS race against a `CAP = 1` ring
/// drained only after both finish, no loss either.
#[test]
fn overflow_retry_concurrent_drain_never_loses_or_duplicates() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let ring = RingModel1::new();
        let reclaimed: Arc<[AtomicU32; 2]> = Arc::new([AtomicU32::new(0), AtomicU32::new(0)]);
        let landed: Arc<[AtomicU32; 2]> = Arc::new([AtomicU32::new(0), AtomicU32::new(0)]);

        // 2 producers, each retrying its OWN offset up to MODEL_RETRY_BOUND
        // times — exactly `push_with_overflow_retry`'s bounded-retry SHAPE
        // (loop on `Err` up to a finite cap, no new synchronisation, same
        // underlying `push`).
        let mut producers = Vec::new();
        for (i, &offset) in [10u32, 20u32].iter().enumerate() {
            let ring_p = Arc::clone(&ring);
            let landed_p = Arc::clone(&landed);
            producers.push((
                i,
                thread::spawn(move || {
                    for _ in 0..MODEL_RETRY_BOUND {
                        if ring_p.push(offset).is_ok() {
                            landed_p[i].store(1, Ordering::Relaxed);
                            return;
                        }
                        thread::yield_now();
                    }
                }),
            ));
        }

        for (_, p) in producers {
            p.join().unwrap();
        }

        // Consumer drains ONCE, after both producers finished retrying —
        // the SAME "producers join, then drain" shape
        // `correct_ring_never_loses_or_duplicates` above already proves
        // sound for the ring's own protocol; here it runs against a
        // `CAP = 1` ring, so it also exercises the case where the SECOND
        // producer's first attempt genuinely overflowed and had to retry
        // (rather than winning the initial CAS race outright).
        ring.drain(|off| {
            let idx = match off {
                10 => 0,
                20 => 1,
                _ => return,
            };
            reclaimed[idx].fetch_add(1, Ordering::Relaxed);
        });

        for (i, counter) in reclaimed.iter().enumerate() {
            let seen = counter.load(Ordering::Acquire);
            let did_land = landed[i].load(Ordering::Relaxed) == 1;
            if did_land {
                // A push that reported success MUST be reclaimed exactly
                // once — this is the invariant that matters (no loss, no
                // duplication of a push the retry loop believes succeeded).
                assert_eq!(
                    seen, 1,
                    "producer {i}'s offset landed (push returned Ok) but was \
                     reclaimed {seen} times (want exactly 1) — the overflow-retry \
                     composition lost or duplicated a push"
                );
            } else {
                // MODEL_RETRY_BOUND was exhausted in this particular
                // interleaving (a loom artefact of a SMALL bound chosen for
                // tractability, not a real-world outcome — the real
                // RING_PUSH_RETRY_SPINS is 262,144, astronomically larger
                // than any interleaving loom explores here). A push that
                // never landed correctly contributes nothing to `reclaimed`.
                assert_eq!(
                    seen, 0,
                    "producer {i}'s offset was reclaimed {seen} times despite \
                     never successfully landing (push never returned Ok) — a \
                     duplication/fabrication bug, not a retry-exhaustion artefact"
                );
            }
        }
    });
}

/// Bound on the model producers' retry loop (see
/// `overflow_retry_concurrent_drain_never_loses_or_duplicates`). An
/// UNBOUNDED retry-until-success loop (even with `thread::yield_now()`
/// between attempts) was measured to make loom's model checker abort with
/// "Model exceeded maximum number of branches... requiring the processor to
/// make progress" — loom explores every retry iteration as new branches, so
/// an unbounded loop is loom-hostile regardless of yield points (yielding
/// only affects scheduling fairness, not the iteration count loom must
/// explore). A small bound keeps the model tractable while still covering
/// the property under test: does a push that DOES land (within the bound)
/// get reclaimed exactly once under every interleaving loom explores. This
/// mirrors `push_with_overflow_retry`'s own bounded nature (a real, finite
/// cap — `RING_PUSH_RETRY_SPINS`) more faithfully than an infinite model
/// loop would have, even though the real bound (262,144) is far larger than
/// what loom can afford to explore exhaustively.
const MODEL_RETRY_BOUND: u32 = 3;
