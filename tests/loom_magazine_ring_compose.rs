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
//!
//! # The invariant and the hole
//!
//! Safety invariant checked after each step:
//!
//! > `!(in_magazine && bitmap_free)`
//!
//! A block must never be simultaneously issuable from the magazine AND resting
//! on the BinTable free list — that is exactly the double-issue / freelist-
//! corruption state. Under TODAY's transition rules loom FINDS the interleaving
//! that reaches `in_magazine && bitmap_free`:
//!
//!   REMOTE: ring.store(P)  →  OWNER own_free: bits clear → set in_magazine
//!                          →  OWNER drain: ring nonempty & !bitmap_free → set bitmap_free
//!   ⇒ in_magazine && bitmap_free — VIOLATION.
//!
//! # Test shape
//!
//! Because the model documents a hole that is REAL today, the primary test is a
//! `#[should_panic]` COUNTERFACTUAL: it asserts loom DOES find the violation
//! (proving the hole is present and the model is non-vacuous). If it ever stops
//! panicking, either the hole was fixed (update the model per #164's new
//! transition rules and flip this to the green invariant test) or the model
//! went vacuous.
//!
//! A companion `#[ignore]`d test (`compose_naive_ring_check_still_holed`) shows
//! WHY the fix is a genuine design task (#164) and not a one-line guard: the
//! obvious "make `own_free` also consult the ring" patch (its `ring_visible`
//! flag) does NOT close the hole. loom still finds a SYMMETRIC leg — the
//! interleaving where the own-thread free runs BEFORE the remote free is even
//! published (`own_free` sees an empty ring → sets `in_magazine`), then the
//! remote publishes and the owner drains (→ sets `bitmap_free`) → the same
//! `in_magazine && bitmap_free` violation. So this companion test is ALSO a
//! `#[should_panic]` counterfactual (the naive fix is still holed), marked
//! `#[ignore]` so it does not clutter the default run; it is the concrete
//! evidence #164 must reason about (a correct fix needs the drain to also see
//! the magazine, or a per-block conflict record — not merely a ring-read on the
//! own-free path). When #164 lands, replace it with the green invariant test
//! for whatever transition rules the real fix establishes.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --features alloc-global,alloc-xthread \
//!   --test loom_magazine_ring_compose -- --ignored   # the should_panic ones run normally
//! ```
//!
//! NOTE (for the orchestrator): this file is intentionally NOT registered in
//! `scripts/loom.mjs`'s runner list — its primary test PANICS BY DESIGN
//! (documenting the #164 hole), which would turn `npm run loom` CI red. Wire it
//! into the runner only once #164 flips it to the green invariant form.

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
    ///
    /// `ring_visible` models the #164 FIX: when true, `own_free` also treats a
    /// pending ring entry as "already freed" and no-ops (closing the hole).
    /// Today (production) it is FALSE: the guard is blind to the ring.
    fn own_free(&self, ring_visible: bool) {
        // (1) in-magazine scan oracle.
        if self.in_magazine.load(Ordering::Acquire) {
            return; // in-magazine double-free — no-op
        }
        // (2) BinTable bitmap oracle.
        if self.bitmap_free.load(Ordering::Acquire) {
            return; // flushed double-free — no-op
        }
        // (#164 fix) ring oracle — absent in production today.
        if ring_visible && self.ring.load(Ordering::Acquire) == P_TOKEN {
            return; // cross-thread free already pending — no-op
        }
        // Both (all) oracles clear → push into the magazine.
        self.in_magazine.store(true, Ordering::Release);
    }

    /// OWNER drain of the ring — models `reclaim_offset` marking P free on the
    /// BinTable and consuming the ring entry.
    fn drain(&self) {
        if self.ring.load(Ordering::Acquire) == P_TOKEN && !self.bitmap_free.load(Ordering::Acquire)
        {
            self.bitmap_free.store(true, Ordering::Release);
            self.ring.store(RING_EMPTY, Ordering::Release);
        }
    }

    /// Safety invariant: P must never be issuable from two sources at once.
    fn invariant_holds(&self) -> bool {
        !(self.in_magazine.load(Ordering::Acquire) && self.bitmap_free.load(Ordering::Acquire))
    }
}

/// COUNTERFACTUAL / documentation test: under TODAY's rules (ring NOT visible
/// to `own_free`) loom finds an interleaving that violates the invariant —
/// `in_magazine && bitmap_free` — the ring↔magazine double-issue state.
///
/// `#[should_panic]` because loom explores the interleaving where the hole is
/// hit. If this ever passes (no panic) WITHOUT a corresponding #164 fix, the
/// model has gone vacuous — investigate before trusting it.
#[test]
#[should_panic(expected = "RING↔MAGAZINE HOLE")]
fn compose_finds_double_issue_hole_today() {
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
            c_owner.own_free(false); // production: ring NOT visible
            c_owner.drain();
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

/// WHY #164 is a design task, not a one-line guard: the obvious "make
/// `own_free` also consult the ring" patch (`ring_visible = true`) does NOT
/// close the hole. loom still finds a SYMMETRIC leg — the own-thread free runs
/// BEFORE the remote free is published (`own_free` sees an empty ring → sets
/// `in_magazine`), then the remote publishes and the owner drains (→ sets
/// `bitmap_free`) → the same `in_magazine && bitmap_free` violation.
///
/// So this is a `#[should_panic]` counterfactual (the NAIVE fix is still
/// holed), `#[ignore]`d so it stays out of the default run. It is the concrete
/// evidence #164 must reason about: a correct fix needs the DRAIN to also see
/// the magazine (or a per-block conflict record), not merely a ring-read on the
/// own-free path. When #164 lands, replace this with the green invariant test
/// for the real fix's transition rules and retire
/// `compose_finds_double_issue_hole_today`.
#[test]
#[ignore = "documents the #164 hole; naive ring-read fix is still holed — replaced by a green invariant test when #164 lands"]
#[should_panic(expected = "RING↔MAGAZINE HOLE")]
fn compose_naive_ring_check_still_holed() {
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
            c_owner.own_free(true); // NAIVE fix: ring visible to the guard
            c_owner.drain();
        });

        remote.join().unwrap();
        owner.join().unwrap();

        assert!(
            c.invariant_holds(),
            "RING↔MAGAZINE HOLE (task #164): even with own_free reading the ring, \
             the own-free-before-remote-publish leg still reaches \
             in_magazine && bitmap_free — the naive ring-read fix is insufficient."
        );
    });
}
