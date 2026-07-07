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

use loom::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};
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

// ===========================================================================
// X7 Ф3 (task #191) — generation-state model + counterfactual.
//
// The `Compose` model above has THREE resting-place bits but NO generation
// state. The `drain_checked` closure (task #164) closes the IN-MAGAZINE leg
// but NOT the RE-ISSUE-BEFORE-DRAIN leg: a block that was popped from the
// magazine (re-issued to the caller) before the drain runs is
// information-theoretically indistinguishable from a delayed genuine
// cross-thread free — `RING_MAGAZINE_XTHREAD_DOUBLE_FREE_FIX.md` §8.
//
// X7 adds a per-block generation counter (u8, wraps at 256). The three
// touches (X7 plan §3-Ф3):
//   (a) BUMP at issue: when a block leaves the magazine (pop to caller),
//       `gen += 1`.
//   (b) STAMP at remote free: the ring note carries the block's
//       THEN-current gen.
//   (c) COMPARE at drain: if `stamped_gen != current_gen`, the note refers
//       to a PAST life → DROP it.
//
// This section adds a `ComposeGen` model with generation state and two
// tests:
//   - `compose_gen_guard_holds` (GREEN): under the three touches, the
//     invariant "a drain never links a block whose stamped gen differs from
//     its current gen" HOLDS on the A→B→I→D interleaving.
//   - `compose_gen_guard_removed_finds_hole` (`#[should_panic]`): WITHOUT
//     touch (c) (the gen-comparison at drain), loom finds the A→B→I→D
//     interleaving where the stale note is honoured → invariant violated.
//     Reproduces the residual test's pinned-red state inside the loom model,
//     proving the gen-check is load-bearing at the model level too.
// ===========================================================================

/// Shadow model with generation state: the three resting-place bits PLUS
/// `gen` (the block's current generation) and `ring_gen` (the generation
/// stamped into the ring note at remote-free time).
struct ComposeGen {
    in_magazine: AtomicBool,
    bitmap_free: AtomicBool,
    /// The block's CURRENT generation. Bumped (mod 256) on every issue
    /// (magazine pop → caller). Models `bump_gen` (touch a).
    gen: AtomicU8,
    /// The ring channel: 0 = empty, 1 = P's note is queued. Models the
    /// `RemoteFreeRing` slot.
    ring: AtomicU32,
    /// The generation STAMPED into the ring note at remote-free time.
    /// Models `pack_entry_hardened`'s gen field (touch b). Read by the
    /// drain to compare against `gen` (touch c).
    ring_gen: AtomicU8,
}

impl ComposeGen {
    fn new() -> Arc<Self> {
        Arc::new(ComposeGen {
            in_magazine: AtomicBool::new(false),
            bitmap_free: AtomicBool::new(false),
            // The block starts in its 0th life (the bootstrap zeroes the gen
            // table — `init_gen_table_in_place`).
            gen: AtomicU8::new(0),
            ring: AtomicU32::new(RING_EMPTY),
            ring_gen: AtomicU8::new(0),
        })
    }

    /// REMOTE cross-thread free of P — touch (b): stamp the CURRENT gen into
    /// the ring note. The block's `gen` is NOT advanced here (free does not
    /// bump — only issue bumps; X7 plan §2.3 decision 3).
    fn remote_free_gen(&self) {
        let g = self.gen.load(Ordering::Acquire);
        self.ring_gen.store(g, Ordering::Release);
        self.ring.store(P_TOKEN, Ordering::Release);
    }

    /// OWNER own-thread free of P — the Э6 two-oracle guard (same as
    /// `Compose::own_free`). Generation is NOT bumped here (the block enters
    /// the magazine, still allocator-owned — not issued).
    fn own_free_gen(&self) {
        if self.in_magazine.load(Ordering::Acquire) {
            return;
        }
        if self.bitmap_free.load(Ordering::Acquire) {
            return;
        }
        self.in_magazine.store(true, Ordering::Release);
    }

    /// OWNER issue P (magazine pop → caller) — touch (a): BUMP the
    /// generation. The block leaves the allocator's bookkeeping and enters
    /// the caller's hands; this is the life transition.
    fn issue_gen(&self) {
        // The block leaves the magazine.
        self.in_magazine.store(false, Ordering::Release);
        // Bump the generation (mod 256 — the X7 §2.5 wrap residual).
        let g = self.gen.load(Ordering::Acquire);
        self.gen.store(g.wrapping_add(1), Ordering::Release);
    }

    /// OWNER drain — WITH the gen-guard (touch c). After the existing
    /// `drain_checked` logic (X2 magazine-resident drop), compare the
    /// stamped gen against the current gen. A mismatch → DROP the note
    /// (do NOT set bitmap_free). Only a match → link (set bitmap_free).
    fn drain_gen_checked(&self) {
        if self.ring.load(Ordering::Acquire) == P_TOKEN && !self.bitmap_free.load(Ordering::Acquire)
        {
            // X2 check first (magazine-resident drop).
            if self.in_magazine.load(Ordering::Acquire) {
                self.ring.store(RING_EMPTY, Ordering::Release);
                return;
            }
            // X7 Ф3 touch (c): compare stamped gen vs current gen.
            let stamped = self.ring_gen.load(Ordering::Acquire);
            let current = self.gen.load(Ordering::Acquire);
            if stamped != current {
                // Stale note (past life) → DROP.
                self.ring.store(RING_EMPTY, Ordering::Release);
                return;
            }
            // Genuine cross-thread free of the CURRENT life → link.
            self.bitmap_free.store(true, Ordering::Release);
            self.ring.store(RING_EMPTY, Ordering::Release);
        }
    }

    /// OWNER drain — WITHOUT the gen-guard (counterfactual). Identical to
    /// `drain_gen_checked` MINUS the gen comparison. Used by the
    /// `#[should_panic]` test to prove the gen-check is load-bearing.
    fn drain_gen_no_guard(&self) {
        if self.ring.load(Ordering::Acquire) == P_TOKEN && !self.bitmap_free.load(Ordering::Acquire)
        {
            // X2 check (magazine-resident drop) — kept.
            if self.in_magazine.load(Ordering::Acquire) {
                self.ring.store(RING_EMPTY, Ordering::Release);
                return;
            }
            // NO gen-check — the pre-X7 drain. A stale note is honoured.
            self.bitmap_free.store(true, Ordering::Release);
            self.ring.store(RING_EMPTY, Ordering::Release);
        }
    }

    /// X7 invariant: a drain must never link (bitmap_free) a block whose
    /// CURRENT generation differs from the generation stamped in the ring
    /// note — UNLESS the note was for the current life and the block was
    /// genuinely freed (in which case gen hasn't advanced since the stamp).
    /// Concretely: `bitmap_free == true` implies the drain linked the block,
    /// which requires `stamped_gen == current_gen` at drain time. After the
    /// link the block is on the freelist; no issue runs to bump gen in this
    /// model's post-drain state, so `stamped == current` must still hold
    /// whenever `bitmap_free` is set.
    fn gen_invariant_holds(&self) -> bool {
        if self.bitmap_free.load(Ordering::Acquire) {
            let stamped = self.ring_gen.load(Ordering::Acquire);
            let current = self.gen.load(Ordering::Acquire);
            // The block was linked: its stamped gen must match the gen that
            // was current at link time. Since no issue (bump) happens after
            // the link in this model, `current` is unchanged → still equals
            // `stamped`. A violation means a stale note (past life) was
            // linked — the exact corruption the gen-guard prevents.
            stamped == current
        } else {
            true
        }
    }
}

/// GREEN invariant test (X7 Ф3): under the THREE touches (bump at issue,
/// stamp at remote free, compare at drain), the gen invariant HOLDS on the
/// A→B→I→D interleaving — the stale note (stamped before the re-issue) is
/// dropped at drain time because its gen no longer matches.
///
/// The model explores the critical interleaving:
///   1. issue_gen (A → pops from mag, bumps gen: 0 → 1).
///   2. remote_free_gen (B → stamps gen=1 into the ring note).
///   3. issue_gen (I → re-issue, bumps gen: 1 → 2). [requires the block
///      to have come back into the magazine via own_free_gen between B and I]
///   4. drain_gen_checked (D → compares stamped=1 vs current=2 → DROP).
///
/// loom explores ALL interleavings of the two threads, including the one
/// where B stamps BEFORE I bumps — the exact A→B→I→D window. The gen-guard
/// drops the stale note in every such interleaving → invariant holds.
#[test]
fn compose_gen_guard_holds() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let c = ComposeGen::new();

        let c_remote = Arc::clone(&c);
        let remote = thread::spawn(move || {
            // REMOTE: publish P's cross-thread free, stamping the current gen.
            c_remote.remote_free_gen();
        });

        let c_owner = Arc::clone(&c);
        let owner = thread::spawn(move || {
            // OWNER: the block was previously issued (gen=0 at bootstrap,
            // bumped to 1 by a prior issue not modelled here — we start the
            // block at gen=0 and the first issue_gen bumps to 1). Sequence:
            //   own_free (block enters magazine) → issue (bumps gen) → drain.
            //
            // This covers BOTH temporal legs:
            //   Leg 1 (remote-before-issue): B stamps gen=0, then I bumps to
            //     1 → drain compares 0 vs 1 → DROP.
            //   Leg 2 (issue-before-remote): I bumps to 1, then B stamps
            //     gen=1 → drain compares 1 vs 1 → genuine free → LINK
            //     (correct: the note refers to the current life).
            c_owner.own_free_gen();
            c_owner.issue_gen();
            c_owner.drain_gen_checked();
        });

        remote.join().unwrap();
        owner.join().unwrap();

        assert!(
            c.gen_invariant_holds(),
            "X7 gen-guard invariant violated: a stale ring note (stamped gen \
             != current gen) was linked onto the BinTable — the generational \
             guard failed to drop it. This is the re-issue-before-drain \
             corruption the X7 arc exists to close."
        );
    });
}

/// COUNTERFACTUAL (X7 Ф3): WITHOUT touch (c) — the gen-comparison at drain —
/// loom finds the A→B→I→D interleaving where the stale note is honoured
/// (bitmap_free set despite stamped gen != current gen) → invariant
/// violated. `#[should_panic]` because loom explores the violating
/// interleaving. Proves the gen-check is load-bearing at the model level.
///
/// Mirrors `compose_finds_double_issue_hole_pre164`'s `#[should_panic]`
/// pattern: the model is non-vacuous because loom finds the interleaving
/// that hits the hole.
#[test]
#[should_panic(expected = "X7 GEN-GUARD HOLE")]
fn compose_gen_guard_removed_finds_hole() {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let c = ComposeGen::new();

        let c_remote = Arc::clone(&c);
        let remote = thread::spawn(move || {
            c_remote.remote_free_gen();
        });

        let c_owner = Arc::clone(&c);
        let owner = thread::spawn(move || {
            c_owner.own_free_gen();
            c_owner.issue_gen();
            // NO gen-check — the pre-X7 drain. A stale note is honoured.
            c_owner.drain_gen_no_guard();
        });

        remote.join().unwrap();
        owner.join().unwrap();

        assert!(
            c.gen_invariant_holds(),
            "X7 GEN-GUARD HOLE (task #191): the drain linked a stale ring \
             note whose stamped generation differs from the block's current \
             generation — the re-issue-before-drain corruption. Without the \
             gen-comparison at drain (touch c), the A→B→I→D interleaving is \
             undecidable and loom finds the violation."
        );
    });
}
