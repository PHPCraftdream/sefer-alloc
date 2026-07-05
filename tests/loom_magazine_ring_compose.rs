//! loom model of the **magazine ↔ RemoteFreeRing composition** and its M2
//! cross-thread-double-free residual hole (task R2 / #154; real fix #164).
//!
//! # Scope — what this models
//!
//! This is a SHADOW model (loom atomics, not the real `HeapCore`/`RemoteFreeRing`)
//! of the three resting-place bits for ONE block P, per agent B's sketch:
//!
//! - `in_magazine: AtomicBool` — owner-only. P is sitting in the tcache.
//! - `bitmap_free: AtomicBool` — owner-only. P is on the BinTable free list.
//! - `ring: AtomicU32` — single-slot channel. A REMOTE thread publishes P's
//!   cross-thread free here.
//!
//! Transitions (today's production rules, faithfully modelled):
//!
//! - REMOTE free (producer): `ring.store(P)` — sets NEITHER owner bit (the
//!   ring push deliberately leaves the bitmap alone; P is not in `slots`).
//! - OWNER own-free of P (`own_free`):
//!   `scan(in_magazine)? no-op : bitmap_free? no-op : set in_magazine` —
//!   the Э6 two-oracle guard. It consults ONLY the two owner bits; it is
//!   blind to `ring`, so with `ring` nonempty and both bits clear it sets
//!   `in_magazine` — the bug.
//! - OWNER drain (`drain`): `ring` nonempty && !bitmap_free → set bitmap_free
//!   (models `reclaim_offset` marking P free on the BinTable).
//! - OWNER drain_checked (`drain_checked`, task #164): `ring` nonempty &&
//!   !bitmap_free → IF `in_magazine` → DROP the ring entry (do NOT set
//!   bitmap_free); ELSE → set bitmap_free (genuine xfree). Models the §5
//!   fallback (a)-closure: the drain sees the magazine.
//!
//! # Test shape
//!
//! - `compose_finds_double_issue_hole_pre164` (`#[should_panic]`): proves the
//!   hole EXISTS under the old drain rules (ring NOT visible to own_free,
//!   drain blind to magazine). Historical counterfactual — the model is
//!   non-vacuous because loom finds the violating interleaving.
//!
//! - `compose_drain_sees_magazine_invariant_holds` (GREEN, task #164): under
//!   the FIXED drain rules (`drain_checked` — drain sees the magazine),
//!   the invariant `!(in_magazine && bitmap_free)` HOLDS on both temporal
//!   legs. This is the green invariant test that the file's own instructions
//!   (lines 60-63) said to add when #164 lands.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --features alloc-global,alloc-xthread \
//!   --test loom_magazine_ring_compose
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use loom::sync::Arc;
use loom::thread;

const RING_EMPTY: u32 = 0;
const P_TOKEN: u32 = 1; // "P's cross-thread free is queued in the ring"

/// Shadow of the three resting-place bits for one block P.
struct Compose {
    in_magazine: AtomicBool,
    bitmap_free: AtomicBool,
    ring: AtomicU32,
}

impl Compose {
    fn new() -> Arc<Self> {
        Arc::new(Compose {
            in_magazine: AtomicBool::new(false),
            bitmap_free: AtomicBool::new(false),
            ring: AtomicU32::new(RING_EMPTY),
        })
    }

    /// REMOTE cross-thread free of P: publish into the ring, touch no owner bit.
    fn remote_free(&self) {
        self.ring.store(P_TOKEN, Ordering::Release);
    }

    /// OWNER own-thread free of P — the Э6 two-oracle guard.
    fn own_free(&self) {
        // (1) in-magazine scan oracle.
        if self.in_magazine.load(Ordering::Acquire) {
            return; // in-magazine double-free — no-op
        }
        // (2) BinTable bitmap oracle.
        if self.bitmap_free.load(Ordering::Acquire) {
            return; // flushed double-free — no-op
        }
        // Both oracles clear → push into the magazine.
        self.in_magazine.store(true, Ordering::Release);
    }

    /// OWNER drain of the ring — OLD rules (pre-#164): blind to the magazine.
    /// Models `reclaim_offset` marking P free on the BinTable.
    fn drain(&self) {
        if self.ring.load(Ordering::Acquire) == P_TOKEN && !self.bitmap_free.load(Ordering::Acquire)
        {
            self.bitmap_free.store(true, Ordering::Release);
            self.ring.store(RING_EMPTY, Ordering::Release);
        }
    }

    /// OWNER drain of the ring — FIXED rules (task #164, §5 fallback (a)):
    /// the drain sees the magazine. When `ring` is nonempty AND `in_magazine`
    /// is set, the ring entry is a duplicate free of a magazine-resident block
    /// → DROP it (do NOT set `bitmap_free`). Only when `in_magazine` is false
    /// does the drain mark `bitmap_free` (genuine cross-thread free).
    fn drain_checked(&self) {
        if self.ring.load(Ordering::Acquire) == P_TOKEN && !self.bitmap_free.load(Ordering::Acquire)
        {
            if self.in_magazine.load(Ordering::Acquire) {
                // Magazine-resident: DROP the ring entry — the magazine copy
                // is the sole canonical reference.
                self.ring.store(RING_EMPTY, Ordering::Release);
            } else {
                // Genuine cross-thread free: link onto the BinTable.
                self.bitmap_free.store(true, Ordering::Release);
                self.ring.store(RING_EMPTY, Ordering::Release);
            }
        }
    }

    /// Safety invariant: P must never be issuable from two sources at once.
    fn invariant_holds(&self) -> bool {
        !(self.in_magazine.load(Ordering::Acquire) && self.bitmap_free.load(Ordering::Acquire))
    }
}

/// HISTORICAL COUNTERFACTUAL (pre-#164): under the OLD drain rules (drain
/// blind to magazine) loom finds an interleaving that violates the invariant —
/// `in_magazine && bitmap_free` — the ring↔magazine double-issue state.
///
/// `#[should_panic]` because loom explores the interleaving where the hole is
/// hit. If this ever passes (no panic) WITHOUT a corresponding fix, the model
/// has gone vacuous.
#[test]
#[should_panic(expected = "RING↔MAGAZINE HOLE")]
fn compose_finds_double_issue_hole_pre164() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let c = Compose::new();

        // REMOTE thread: publish P's cross-thread free into the ring.
        let c_remote = Arc::clone(&c);
        let remote = thread::spawn(move || {
            c_remote.remote_free();
        });

        // OWNER thread: own-free P (may run before or after the ring publish),
        // then drain the ring. Both orderings are explored by loom.
        let c_owner = Arc::clone(&c);
        let owner = thread::spawn(move || {
            c_owner.own_free(); // production: ring NOT visible
            c_owner.drain(); // OLD: blind to magazine
        });

        remote.join().unwrap();
        owner.join().unwrap();

        assert!(
            c.invariant_holds(),
            "RING↔MAGAZINE HOLE (task #164): P is BOTH in the magazine AND on \
             the BinTable free list — double-issue / freelist corruption. The \
             own-thread free landed P in the magazine while its cross-thread \
             free was pending in the ring; the later drain then marked P free."
        );
    });
}

/// GREEN invariant test (task #164): under the FIXED drain rules
/// (`drain_checked` — drain sees the magazine), the invariant
/// `!(in_magazine && bitmap_free)` HOLDS on BOTH temporal legs.
///
/// Leg 1 (remote-before-own): `ring.store → own_free sets in_magazine →
/// drain_checked sees in_magazine → DROP (no bitmap_free)` → invariant holds.
///
/// Leg 2 (own-before-remote): `own_free sets in_magazine → ring.store →
/// drain_checked sees in_magazine → DROP (no bitmap_free)` → invariant holds.
///
/// This encodes DRAIN-SIDE resolution (the drain sees the magazine), not an
/// own_free ring-read (which the companion counterfactual proved insufficient).
#[test]
fn compose_drain_sees_magazine_invariant_holds() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let c = Compose::new();

        let c_remote = Arc::clone(&c);
        let remote = thread::spawn(move || {
            c_remote.remote_free();
        });

        let c_owner = Arc::clone(&c);
        let owner = thread::spawn(move || {
            c_owner.own_free();
            c_owner.drain_checked(); // #164 FIX: drain sees the magazine
        });

        remote.join().unwrap();
        owner.join().unwrap();

        assert!(
            c.invariant_holds(),
            "RING↔MAGAZINE HOLE (task #164): invariant violated under the \
             fixed drain_checked rules — the fix is broken."
        );
    });
}
