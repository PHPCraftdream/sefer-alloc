//! loom model-check of the **`abandoned_segs` Treiber stack ABA guard**
//! (UBFIX-4 / M-6 — "same tag-reset in `abandoned_segs`", the structural
//! twin of H-2's `free_slots` fix; see `tests/loom_free_slots_aba.rs`, which
//! this file mirrors in structure and technique).
//!
//! # Scope — what loom covers
//!
//! This harness models `pop_abandoned_segment` / `push_abandoned_segment_into`
//! (`src/registry/heap_registry.rs`) and `pack_abandoned_head` /
//! `unpack_abandoned_head` / `abandoned_head_is_empty` (`src/registry/bootstrap.rs`)
//! in isolation using `loom::sync::atomic` (NOT the real functions, which
//! use `core::sync::atomic` and operate on a live `Registry` +
//! `SegmentMeta`-addressed header field). It reproduces the EXACT protocol
//! shape, with a SMALL model alignment shift (4 bits, base = multiple of 16)
//! standing in for the real `ABANDON_SEG_SHIFT` (22, `log2(4 MiB)`) — the
//! shift's magnitude is irrelevant to the ABA protocol itself (only "tag
//! lives in the low N bits below the base's alignment" matters), and a small
//! shift keeps loom's explorable state space tiny:
//!
//! - `abandoned_segs: AtomicU64` head, packed `(base_high_bits | tag_low_bits)`
//!   — base in the high bits (real bases are `SEGMENT`-aligned, so their low
//!   bits are free for the tag), tag in the low `TAG_BITS` bits, bumped on
//!   every successful PUSH.
//! - Per-segment `next_abandoned: AtomicU64` link (mirrors
//!   `SegmentMeta::next_abandoned_atomic`), with `TAIL` (`u64::MAX`) as the
//!   "no next" sentinel and base=0 (null) as the "stack empty" sentinel —
//!   `abandoned_head_is_empty` only inspects the base half, exactly like the
//!   real `bootstrap::abandoned_head_is_empty`.
//! - `pop`: load tagged head, read the segment's `next_abandoned` link, CAS
//!   head to `(next_base, SAME tag)` — a losing CAS retries with the fresh
//!   head.
//! - `push`: write the segment's `next_abandoned` link to the current head's
//!   base (or `TAIL`), CAS head to `(base, tag + 1)`.
//!
//! # M-6 addendum: the empty-transition tag-reset ABA (same shape as H-2)
//!
//! The ORIGINAL `pop_abandoned_segment`'s empty-transition branch (the pop
//! that drains the stack to zero elements) collapsed the head straight to
//! the constant `ABANDONED_HEAD_EMPTY` (base=null, tag=0), discarding
//! whatever running tag was live at the moment of the drain — identical
//! defect shape to H-2's `TaggedPtr::empty()`. `pop_preserves_tag_on_drain`
//! below models both the buggy drain branch (hardcoded tag=0) and the fixed
//! drain branch (preserves the observed tag) via a `preserve_tag_on_drain`
//! flag, and — using the SAME two-flag rendezvous technique as
//! `tests/loom_free_slots_aba.rs::run_h2_interleaving` (forcing thread B's
//! full pop+push cycle to be strictly sandwiched between thread A's head
//! load and A's CAS, eliminating the "innocent ordering" false positive a
//! free race would allow) — asserts:
//! - **buggy branch:** loom finds the schedule where A's stale CAS (fired
//!   against a snapshot from BEFORE B's cycle) spuriously succeeds even
//!   though B's cycle fully completed in the interim (`#[should_panic]`
//!   counterfactual `counterfactual_abandoned_empty_transition_tag_reset_lets_aba_recur`).
//! - **fixed branch:** the same interleaving forces A's CAS to fail (it
//!   observes a tag that has moved on), so it retries
//!   (`pop_abandoned_empty_transition_preserves_tag`).
//!
//! # Reachability note (why this is "lighter" coverage than H-2's)
//!
//! `abandon_segments`/`try_adopt` — the only production callers of this
//! stack — are **test-only** today (Phase 12.5's shard model retired
//! thread-exit abandonment; see the "REACTIVATION HAZARD" note on
//! `HeapRegistry::abandon_segments` in `src/registry/heap_registry.rs` and
//! `tests/loom_registry.rs`'s own honesty note). This file is the
//! `abandoned_segs`-specific loom counterpart to `loom_free_slots_aba.rs`,
//! proving the SAME tag-preservation fix is sound for this stack's
//! (base-in-high-bits, tag-in-low-bits) packing before the primitive is ever
//! reactivated — matching `loom_registry.rs`'s existing "prove the substrate
//! now, even though dead today" precedent for this exact stack.
//!
//! # How to run
//!
//! ```sh
//! RUSTFLAGS="--cfg loom" cargo test --release --features alloc-global --test loom_abandoned_segs_aba
//! ```

#![cfg(loom)]

use loom::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use loom::sync::Arc;
use loom::thread;

/// Model alignment shift: 4 bits (base = multiple of 16). Stands in for the
/// real `ABANDON_SEG_SHIFT` (22) — only the "tag lives below the base's
/// alignment" shape matters for the ABA protocol, and a small shift keeps
/// loom's state space tiny.
const TAG_BITS: u32 = 4;
const TAG_MASK: u64 = (1u64 << TAG_BITS) - 1;

/// "No next" sentinel for a segment's `next_abandoned` link (mirrors
/// `segment_header::ABANDONED_TAIL`, `u64::MAX`).
const TAIL: u64 = u64::MAX;

/// Mirrors `bootstrap::pack_abandoned_head`/`unpack_abandoned_head`/
/// `abandoned_head_is_empty` — pure bit arithmetic on a model-scale
/// alignment. The real functions additionally thread exposed-provenance
/// pointer reconstruction (`with_exposed_provenance_mut`); that machinery is
/// provenance bookkeeping around the SAME integer protocol modelled here,
/// so this harness (like `loom_free_slots_aba.rs`'s `tagged_ptr` module)
/// models the protocol on plain integers.
mod abandoned_head {
    use super::TAG_MASK;

    pub(crate) fn pack(base: u64, tag: u64) -> u64 {
        (base & !TAG_MASK) | (tag & TAG_MASK)
    }

    pub(crate) fn unpack(word: u64) -> (u64, u64) {
        (word & !TAG_MASK, word & TAG_MASK)
    }

    pub(crate) fn is_empty(word: u64) -> bool {
        (word & !TAG_MASK) == 0
    }
}

/// Single-segment model registry: one fake "segment base" (16, the smallest
/// non-zero multiple of the model alignment — 0 is reserved for the empty
/// sentinel, exactly like a real null base) is on the stack at a
/// caller-chosen starting tag (models "not the first push/pop cycle ever").
struct SingleSegRegistry {
    abandoned_segs: AtomicU64,
    next_abandoned: AtomicU64,
}

const BASE: u64 = 16;

impl SingleSegRegistry {
    fn seeded(start_tag: u64) -> Arc<Self> {
        Arc::new(SingleSegRegistry {
            abandoned_segs: AtomicU64::new(abandoned_head::pack(BASE, start_tag)),
            next_abandoned: AtomicU64::new(TAIL),
        })
    }

    /// Mirrors `pop_abandoned_segment`, parameterised on the drain-branch
    /// tag behaviour (`preserve_tag_on_drain = false` reproduces the M-6
    /// bug; `true` is the fix shape).
    fn pop(&self, preserve_tag_on_drain: bool) -> Option<u64> {
        let mut head = self.abandoned_segs.load(Ordering::Acquire);
        loop {
            if abandoned_head::is_empty(head) {
                return None;
            }
            let (base, tag) = abandoned_head::unpack(head);
            let next = self.next_abandoned.load(Ordering::Acquire);
            let new_head = if next == TAIL {
                if preserve_tag_on_drain {
                    // FIX: reuse the tag just read instead of resetting to 0.
                    abandoned_head::pack(0, tag)
                } else {
                    // BUG: `ABANDONED_HEAD_EMPTY` — hardcoded tag 0.
                    0
                }
            } else {
                abandoned_head::pack(next, tag)
            };
            match self.abandoned_segs.compare_exchange(
                head,
                new_head,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Some(base),
                Err(actual) => head = actual,
            }
        }
    }

    /// Mirrors `push_abandoned_segment_into`: reads the tag out of the
    /// CURRENT head unconditionally (empty or not) and bumps it — identical
    /// in both the buggy and fixed builds; the fix lives entirely in `pop`'s
    /// drain branch.
    fn push(&self, base: u64) {
        let mut head = self.abandoned_segs.load(Ordering::Acquire);
        loop {
            let next_link = if abandoned_head::is_empty(head) {
                TAIL
            } else {
                let (cur_base, _tag) = abandoned_head::unpack(head);
                cur_base
            };
            self.next_abandoned.store(next_link, Ordering::Release);
            let (_cur_base, tag) = abandoned_head::unpack(head);
            let new_tag = (tag + 1) & TAG_MASK;
            let new_head = abandoned_head::pack(base, new_tag);
            match self.abandoned_segs.compare_exchange(
                head,
                new_head,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(actual) => head = actual,
            }
        }
    }
}

/// Runs the M-6 interleaving against whichever drain-branch behaviour
/// `preserve_tag_on_drain` selects, and PANICS (`"stale CAS succeeded"`) if
/// loom finds a schedule where thread A's CAS — fired against a snapshot
/// captured BEFORE thread B's full pop+push cycle — spuriously succeeds
/// AFTER that cycle has fully completed. Uses the same two-flag rendezvous
/// as `loom_free_slots_aba.rs::run_h2_interleaving` to force strict
/// sandwiching (see that function's doc comment for why a free race would
/// false-positive on the fixed build).
fn run_m6_interleaving(preserve_tag_on_drain: bool) {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(move || {
        // Seed at tag = 1 so B's drain-then-refill cycle recomputes exactly
        // tag = 1 again in the BUGGY branch (0 -> +1 -> 1), colliding with
        // A's captured `tag = 1` snapshot.
        let reg = SingleSegRegistry::seeded(1);
        let a_loaded = Arc::new(AtomicU32::new(0));
        let b_done = Arc::new(AtomicU32::new(0));

        // Thread B: waits for A's snapshot, then runs a FULL pop (drains
        // the stack — the same base A is targeting) immediately followed by
        // a full push of that SAME base (refills the stack). Signals
        // `b_done` only after both complete.
        let reg_b = Arc::clone(&reg);
        let a_loaded_b = Arc::clone(&a_loaded);
        let b_done_b = Arc::clone(&b_done);
        let tb = thread::spawn(move || {
            while a_loaded_b.load(Ordering::Acquire) == 0 {
                thread::yield_now();
            }
            let popped = reg_b.pop(preserve_tag_on_drain);
            if let Some(base) = popped {
                reg_b.push(base);
            }
            b_done_b.store(1, Ordering::Release);
            popped
        });

        // Thread A: manual split pop (load, read next_abandoned, compute
        // candidate). Signals `a_loaded` right after the load, then BLOCKS
        // on `b_done` before firing its CAS — forcing B's entire cycle to
        // be sandwiched in the gap by construction.
        let head = reg.abandoned_segs.load(Ordering::Acquire);
        let (base, tag) = abandoned_head::unpack(head);
        let next = reg.next_abandoned.load(Ordering::Acquire);
        let new_head = if next == TAIL {
            if preserve_tag_on_drain {
                abandoned_head::pack(0, tag)
            } else {
                0
            }
        } else {
            abandoned_head::pack(next, tag)
        };
        a_loaded.store(1, Ordering::Release);
        while b_done.load(Ordering::Acquire) == 0 {
            thread::yield_now();
        }
        let a_result = reg
            .abandoned_segs
            .compare_exchange(head, new_head, Ordering::Acquire, Ordering::Relaxed)
            .map(|_| base);

        tb.join().unwrap();

        // THE INVARIANT: once B has fully completed a pop+push cycle on
        // this segment (guaranteed by the rendezvous), A's CAS against its
        // PRE-B snapshot must NEVER succeed — the tag half of the live head
        // must have moved on. If A's CAS succeeds anyway, the tag sequence
        // looped back to A's stale value — the M-6 empty-transition
        // tag-reset bug.
        assert!(
            a_result.is_err(),
            "stale CAS succeeded: thread A's compare_exchange used a head \
             snapshot captured BEFORE thread B's full pop+push cycle, yet \
             succeeded AFTER that cycle completed — an empty-transition \
             tag-reset ABA collision (M-6)"
        );
    });
}

/// **The fix (`preserve_tag_on_drain = true`):** loom finds NO schedule
/// where A's stale CAS spuriously succeeds — the running tag keeps climbing
/// across the empty transition, so A's captured tag snapshot can never
/// numerically recur.
#[test]
fn pop_abandoned_empty_transition_preserves_tag() {
    run_m6_interleaving(true);
}

/// **The counterfactual (non-vacuousness proof):** replaying the IDENTICAL
/// interleaving against the BUGGY drain branch (hardcoded tag=0 — the
/// pre-fix `ABANDONED_HEAD_EMPTY` behaviour) — loom finds the schedule where
/// A's stale CAS spuriously succeeds, proving this harness actually
/// exercises the tag-reset-on-empty defect (not an artifact of the test's
/// construction).
#[test]
#[should_panic(expected = "stale CAS succeeded")]
fn counterfactual_abandoned_empty_transition_tag_reset_lets_aba_recur() {
    run_m6_interleaving(false);
}
