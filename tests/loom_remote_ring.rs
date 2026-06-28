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
