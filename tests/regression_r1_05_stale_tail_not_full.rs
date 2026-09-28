//! R1-05 (`docs/reviews/2026-09-28-005939-src-review-oxx-round-1.md`) —
//! deterministic (non-loom) regression: a "stale" tail/reservation snapshot
//! must be classified distinctly from a genuinely full ring, in BOTH of the
//! two MPSC rings the finding names.
//!
//! # Background
//!
//! `RemoteFreeRing::full_check` (`src/alloc_core/segment/remote_free_ring/ops.rs`)
//! and `HeapOverflow::push_impl`'s inline room check
//! (`src/registry/heap_overflow.rs`) each read a producer's `tail` snapshot
//! `t` before comparing it against the consumer's `head` cursor `h`. Because
//! `t` and `h` are read as two SEPARATE (non-atomic) steps, a producer
//! preempted in between can have its `t` overtaken: other producers push
//! further and the owner fully drains, so the freshly-reread `h` ends up
//! `> t`. Both rings' PRE-FIX code treated `t < h` (equivalently, for
//! `HeapOverflow`'s wrapping `usize` cursors, a large-magnitude unsigned
//! `wrapping_sub` result) identically to "the ring is full" — a spurious
//! overflow-counter tick and unnecessary second-tier routing, even though no
//! free was ever lost (see each ring's own fix commit / doc comment for the
//! full argument).
//!
//! # What this file proves, and what it does not
//!
//! This file exercises the FIXED classification function directly (via a
//! `#[doc(hidden)]` test hook on each ring) with a caller-supplied,
//! deliberately stale snapshot — no real thread preemption is needed, because
//! the classification itself is a pure decision over two already-in-hand
//! cursor values. This is a DIRECT, deterministic proof that the exact
//! decision function distinguishes `Stale` from `Full`. It does **not**
//! replace the loom coverage in `tests/loom_remote_ring.rs`
//! (`stale_tail_snapshot_retried_not_reported_full` +
//! its counterfactual), which additionally proves the surrounding concurrent
//! `push` retry composition (CAS/publish orderings, the shared `cached_head`
//! shadow refresh) delivers a stale `t` to the classification function in the
//! first place, under loom's memory model.
//!
//! Counterfactual (verified manually, not committed as a test — see the
//! task report): temporarily reverting `RemoteFreeRing::full_check`'s
//! `Stale`/`Full` split back to the pre-fix merged `Err`, and
//! `HeapOverflow::room_check`'s `(diff as isize) < 0` branch back to the
//! pre-fix bare `diff >= HEAP_OVERFLOW_CAP`, makes both assertions below that
//! expect code `1` (Stale) instead observe code `2` (Full) — i.e. this file's
//! assertions are not vacuously true.

#![cfg(feature = "alloc-core")]

#[cfg(feature = "alloc-xthread")]
mod remote_free_ring_stale_tail {
    use sefer_alloc::alloc_core::remote_free_ring::{RemoteFreeRing, FOOTPRINT, RING_CAP};

    /// A stale tail snapshot (well below the real, current head) must be
    /// classified `Stale` (code `1`), not `Full` (code `2`) — R1-05. Also
    /// pins the two surrounding cases (`Room`, genuine `Full`) so the fix
    /// is shown to narrow, not widen, the set of stale/full classifications.
    #[test]
    fn stale_tail_snapshot_classified_as_stale_not_full() {
        // Fresh isolated buffer — no segment, no allocator (same technique as
        // `tests/regression_ring_overflow_counter.rs`).
        let mut buf = vec![0u8; FOOTPRINT].into_boxed_slice();
        let base = buf.as_mut_ptr();
        assert!((base as usize).is_multiple_of(8));
        // SAFETY: `base` is a FOOTPRINT-sized, 8-byte-aligned, owned buffer.
        let ring = unsafe {
            RemoteFreeRing::init_test_buffer(base);
            RemoteFreeRing::over_test_buffer(base)
        };

        // Quiescent, empty ring with 5 lifetime reservations already drained:
        // head == tail == 5. `dbg_set_cursors` also resets `cached_head` to
        // match `head` (5), so the classification's fast path cannot
        // shortcut the stale check below (see `RemoteFreeRing::full_check`'s
        // `t >= cached_head` fast-path guard — a stale `t = 0 < 5` fails it,
        // forcing the real slow-path comparison this test targets).
        ring.dbg_set_cursors(5, 5);

        // A STALE snapshot: `t = 0`, standing in for a value a producer
        // captured before 5 reservations' worth of activity happened
        // elsewhere and the owner fully drained past it.
        let stale_code = ring.dbg_full_check_code(0);
        assert_eq!(
            stale_code, 1,
            "a tail snapshot (0) below the real head (5) must be classified \
             Stale (1), not Full (2) — R1-05"
        );

        // Room: the real, current tail value shows room (empty ring).
        let room_code = ring.dbg_full_check_code(5);
        assert_eq!(
            room_code, 0,
            "the real, current tail value (5, matching head) must show Room (0)"
        );

        // Full: a genuinely at-capacity COHERENT snapshot (t == head + CAP)
        // must still be reported Full, not accidentally reclassified Stale.
        let full_code = ring.dbg_full_check_code(5 + RING_CAP as u64);
        assert_eq!(
            full_code, 2,
            "a coherent at-capacity snapshot (t == head + RING_CAP) must \
             still be classified Full (2) — the fix narrows Err, it doesn't \
             remove genuine-full detection"
        );
    }
}

#[cfg(all(feature = "alloc-global", feature = "internals"))]
mod heap_overflow_stale_tail {
    use sefer_alloc::registry::heap_overflow::HeapOverflow;

    /// `HeapOverflow::dbg_room_check_code(t, h)` is a PURE function of its
    /// two `usize` arguments (no allocator state), so this proves the fixed
    /// signed-`wrapping_sub` classification directly, without needing to
    /// know `HEAP_OVERFLOW_CAP`'s private value (it differs under
    /// `--cfg miri`; `t=10_000_000` is unambiguously past either build's
    /// value).
    #[test]
    fn room_check_classifies_stale_vs_room_vs_full() {
        // Stale: `t` (0) behind `h` (5) — the R1-05 scenario for
        // `HeapOverflow`'s wrapping `usize` cursors. Pre-fix,
        // `t.wrapping_sub(h)` computes as a huge unsigned wraparound value
        // (`>= HEAP_OVERFLOW_CAP` under ANY build), misclassifying this
        // identically to a genuinely full ring.
        assert_eq!(
            HeapOverflow::dbg_room_check_code(0, 5),
            1,
            "t (0) behind h (5) must be classified Stale (1), not Full (2) — R1-05"
        );

        // Room: coherent, empty (t == h).
        assert_eq!(
            HeapOverflow::dbg_room_check_code(5, 5),
            0,
            "t == h must be classified Room (0)"
        );

        // Full: a large, unambiguously-over-capacity POSITIVE diff (native
        // HEAP_OVERFLOW_CAP == 2048, miri's == 64 — both far below this).
        assert_eq!(
            HeapOverflow::dbg_room_check_code(10_000_000, 0),
            2,
            "a large positive diff (unambiguously >= HEAP_OVERFLOW_CAP under \
             any build) must still be classified Full (2)"
        );
    }
}
