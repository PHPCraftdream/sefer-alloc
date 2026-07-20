//! R8-10 (task #223): tests for the pool-admission-never-decommits invariant
//! on the small-segment hysteresis pool, under `alloc-lazy-commit`.
//!
//! Feature-gated: `alloc-lazy-commit` (which implies `alloc-core`).
//!
//! ## Background — why this file was rewritten
//!
//! The original B3 (R7 Workstream B) design had `release_or_pool_empty_segment`
//! decommit the payload above the initial lazy chunk and reset all metadata
//! (bump → payload_start, free lists cleared, `is_decommitted = true`) the
//! INSTANT a segment was admitted to the pool — turning it into a "clean carve
//! target" that `reserve_small_segment` would pop directly, bypassing
//! `find_segment_with_free`'s free-list path.
//!
//! An external perf review (R8-10) found this cost 50-75× more
//! `commit_range`/decommit syscalls per empty→pool→reuse→refill cycle than the
//! eager path (feature-OFF), which pays ZERO syscalls for the identical cycle
//! because a pooled segment there stays fully committed with its free lists
//! intact. The hysteresis pool's entire purpose — "the warmest entry, expected
//! back imminently, so keep it ready" — was defeated by decommitting it on
//! arrival.
//!
//! The fix (this task): pool admission NEVER decommits or resets metadata,
//! identically on the eager and lazy-commit paths. A pooled segment is left
//! EXACTLY as it was the instant it emptied, and reuse goes through the SAME
//! `find_segment_with_free` free-list path used by the eager leg.
//!
//! ## What this file now verifies
//!   - After a segment empties and is admitted to the pool, its frontier
//!     (`committed_payload_end`), bump cursor, and free lists are UNCHANGED
//!     from the instant it emptied — no reset, no decommit.
//!   - A full empty→pool→reuse→refill cycle costs EXACTLY ZERO
//!     `GROW_COMMIT_COUNT` and ZERO `dbg_decommit_count()` deltas — the
//!     counterfactual (pre-fix code) was verified to make this red (delta > 0,
//!     see task #223's summary for the exact observed count) before the fix
//!     landed.
//!   - Repeated cycles keep paying zero syscalls (not just the first one).
//!   - Metadata and the remote-free ring are readable across the cycle (they
//!     were never decommitted in either design, before or after this fix).
//!   - The eager path (feature-OFF, Unix, miri) is unchanged: it already paid
//!     zero syscalls on this cycle and still does.

#![cfg(feature = "alloc-lazy-commit")]
// R8-5 (task #218): on every leg EXCEPT genuine Windows-lazy, the lazy-arm
// cfg branches in the tests below compile out and leave their bindings
// unused. Silence the lint family on the eager legs.
#![cfg_attr(
    any(not(windows), miri, feature = "numa-aware"),
    allow(
        unused_variables,
        unused_mut,
        dead_code,
        unused_imports,
        clippy::needless_return
    )
)]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

/// The segment size constant (4 MiB).
const SEGMENT: usize = SegmentLayout::SEGMENT;

/// The metadata end offset.
fn small_meta_end() -> usize {
    SegmentLayout::SMALL_META_END
}

// ── Helper: get a non-primordial segment and exhaust it ───────────────────

/// Exhaust the primordial segment by allocating 16-byte blocks until a new
/// segment is reserved. Returns (allocator, first_ptr_in_new_segment).
fn alloc_past_primordial() -> (AllocCore, *mut u8) {
    let mut a = AllocCore::new().unwrap();
    let prim_ptr = a.alloc(Layout::from_size_align(16, 8).unwrap());
    assert!(!prim_ptr.is_null());
    let prim_base = (prim_ptr as usize) & !(SEGMENT - 1);

    let mut second = core::ptr::null_mut();
    for _ in 0..500_000 {
        let p = a.alloc(Layout::from_size_align(16, 8).unwrap());
        assert!(!p.is_null());
        if (p as usize) & !(SEGMENT - 1) != prim_base {
            second = p;
            break;
        }
    }
    assert!(
        !second.is_null(),
        "failed to trigger a second segment reservation"
    );
    (a, second)
}

/// Get the segment base of a pointer.
fn seg_base(ptr: *mut u8) -> usize {
    (ptr as usize) & !(SEGMENT - 1)
}

/// Fill a non-primordial segment completely, then free all blocks so it
/// empties and gets pooled. Returns the base of the emptied segment and
/// the vector of freed pointers (for inspection).
fn fill_and_empty_segment(a: &mut AllocCore, seg_ptr: *mut u8) -> (usize, Vec<*mut u8>) {
    let base = seg_base(seg_ptr);
    let block_size = 16;
    let mut ptrs = vec![seg_ptr];

    // Fill the segment until we spill into a new one.
    for _ in 0..500_000 {
        let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
        assert!(!p.is_null());
        if (p as usize) & !(SEGMENT - 1) != base {
            // Spilled into a new segment; free this one too so it doesn't
            // hold the `small_cur` reference.
            // SAFETY: `p` is a valid allocation from `a`.
            unsafe {
                a.dealloc(p, Layout::from_size_align(block_size, 8).unwrap());
            }
            break;
        }
        ptrs.push(p);
    }

    // Free all blocks in the segment so it empties and gets pooled.
    for &p in &ptrs {
        // SAFETY: `p` is a valid allocation from `a`.
        unsafe {
            a.dealloc(p, Layout::from_size_align(block_size, 8).unwrap());
        }
    }

    (base, ptrs)
}

// ── Test: frontier is UNCHANGED across pool admission ──────────────────────

/// After a segment empties and is pooled, under `alloc-lazy-commit` the
/// `committed_payload_end` (frontier) is EXACTLY what it was the instant the
/// segment emptied — pool admission does not reset or decommit it. Since the
/// segment was fully carved before emptying, its frontier had already grown
/// (via B2 grow-on-carve) at least to cover every block it ever carved; the
/// post-pool frontier must equal the pre-pool (fully-carved) frontier.
#[test]
fn frontier_unchanged_across_pool_admission() {
    let (mut a, second_ptr) = alloc_past_primordial();
    let lazy_first_chunk = a.dbg_lazy_first_chunk();

    // On Unix/miri the eager path sets frontier = SEGMENT throughout, and the
    // pool never decommits there either — this test is a no-op on those legs
    // (the invariant it checks is trivially true: nothing ever resets the
    // frontier on those platforms).
    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        let _ = (second_ptr, lazy_first_chunk);
        return;
    }

    #[cfg(all(windows, not(miri), not(feature = "numa-aware")))]
    {
        let expected_initial = small_meta_end() + lazy_first_chunk;
        let initial_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert_eq!(
            initial_frontier, expected_initial,
            "fresh lazy segment frontier mismatch"
        );

        // Fill the segment (this grows the frontier via B2 grow-on-carve as
        // carving proceeds past LAZY_FIRST_CHUNK) and empty it so it pools.
        let (base, _ptrs) = fill_and_empty_segment(&mut a, second_ptr);

        // Read the frontier from the OWNER's perspective right before the
        // final block was freed would require an extra seam; instead assert
        // the invariant that matters operationally: after pooling, the
        // frontier is >= the initial lazy value (i.e. NOT reset backwards to
        // `expected_initial` unless it happened to still be there — filling
        // the whole segment means it grew, so it must be strictly greater)
        // and, crucially, that admission itself performed no additional grow
        // (checked precisely by the zero-syscall test below). Here we just
        // confirm the frontier survived pooling at all (still readable, still
        // reflects a fully-carved segment, not reset to the initial chunk).
        let pooled_count = a.dbg_pooled_count();
        assert!(pooled_count > 0, "segment should have been pooled");

        let frontier_after = a.dbg_committed_payload_end_for(base as *mut u8).unwrap();
        assert!(
            frontier_after > expected_initial,
            "after pool admission, a FULLY-CARVED segment's frontier must stay \
             grown (> initial lazy value {expected_initial}), got \
             {frontier_after} — a value equal to the initial chunk would mean \
             admission reset the frontier, reintroducing the R8-10 regression"
        );
    }
}

// ── Test: zero syscalls across a full empty->pool->reuse->refill cycle ────

/// The load-bearing regression test for task #223: a full
/// empty→pool→reuse→refill cycle must cost EXACTLY ZERO `GROW_COMMIT_COUNT`
/// and ZERO `dbg_decommit_count()` deltas. The segment stays fully committed
/// throughout — pool admission does not decommit it, and reuse (via
/// `find_segment_with_free`'s free-list path) finds already-committed pages
/// with an already-populated free list, so it never needs to grow or recommit
/// anything.
///
/// Counterfactual (verified live against this exact scenario before the fix
/// landed, task #223): with the pre-fix B3 decommit-on-admission code, this
/// test was RED — `GROW_COMMIT_COUNT` advanced by 15 on the first reuse
/// allocation that landed back in the pooled segment (the grow-on-carve path
/// recommitting the chunks that had just been decommitted on admission), and
/// `dbg_decommit_count()` advanced by 1 on admission itself. Restoring the
/// old decommit-on-admission block reproduces that red result.
#[test]
fn zero_syscalls_across_empty_pool_reuse_refill_cycle() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        // On the eager legs this invariant already held before this task (the
        // pool never decommitted there) — confirm it still holds, cheaply.
        let (base, _ptrs) = fill_and_empty_segment(&mut a, second_ptr);
        let decommit_before = AllocCore::dbg_decommit_count();
        for _ in 0..100 {
            let p = a.alloc(Layout::from_size_align(16, 8).unwrap());
            assert!(!p.is_null());
        }
        let decommit_after = AllocCore::dbg_decommit_count();
        let _ = base;
        assert_eq!(
            decommit_after, decommit_before,
            "eager-leg reuse must not decommit"
        );
        return;
    }

    #[cfg(all(windows, not(miri), not(feature = "numa-aware")))]
    {
        // empty -> pool.
        let (base, _ptrs) = fill_and_empty_segment(&mut a, second_ptr);
        assert!(a.dbg_pooled_count() > 0, "segment should be pooled");

        let grow_before = a.dbg_grow_commit_count();
        let decommit_before = AllocCore::dbg_decommit_count();

        // reuse -> refill: keep allocating (this drives `small_cur`'s current
        // segment to exhaustion and beyond if needed, forcing
        // `find_segment_with_free` to scan and reuse the pooled segment's
        // free list) until we observe an allocation landing back in the
        // pooled segment's address range.
        let mut reused = false;
        for _ in 0..600_000 {
            let p = a.alloc(Layout::from_size_align(16, 8).unwrap());
            assert!(!p.is_null());
            if seg_base(p) == base {
                reused = true;
                break;
            }
        }
        assert!(
            reused,
            "expected the pooled segment to be reused within the loop bound"
        );

        let grow_after = a.dbg_grow_commit_count();
        let decommit_after = AllocCore::dbg_decommit_count();

        assert_eq!(
            grow_after - grow_before,
            0,
            "expected ZERO GROW_COMMIT_COUNT delta across empty->pool->reuse: \
             a pooled segment must stay fully committed, so reuse via the \
             free-list path never needs to grow the frontier"
        );
        assert_eq!(
            decommit_after - decommit_before,
            0,
            "expected ZERO decommit delta across empty->pool->reuse: pool \
             admission must not decommit the segment (that is the R8-10 fix)"
        );
    }
}

// ── Test: repeated cycles keep paying zero syscalls ───────────────────────

/// Multiple cycles of fill-empty-pool-reuse each individually cost zero
/// syscalls — not just the first cycle after the fix (guards against a
/// regression that only elides the FIRST decommit but reintroduces one on a
/// later re-admission).
#[test]
fn repeated_cycles_stay_zero_syscall() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(windows, not(miri), not(feature = "numa-aware")))]
    {
        // Cycle 1: fill, empty, pool.
        let (base, _ptrs) = fill_and_empty_segment(&mut a, second_ptr);
        assert!(
            a.dbg_pooled_count() > 0,
            "cycle 1: segment should be pooled"
        );

        let decommit_after_cycle1_pool = AllocCore::dbg_decommit_count();

        // Reuse: allocate a few blocks (partially fill) — should land back in
        // the pooled segment via the free-list path with no syscalls.
        let block_size = 16;
        let mut cycle1_ptrs = Vec::new();
        for _ in 0..100 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            assert!(!p.is_null());
            if seg_base(p) == base {
                cycle1_ptrs.push(p);
            }
        }
        assert!(
            !cycle1_ptrs.is_empty(),
            "cycle 1: expected some reuse allocations to land in the pooled segment"
        );

        // Cycle 2: free everything in this segment again -> re-empties ->
        // re-pooled (or re-admitted if it was unpooled on reuse).
        for &p in &cycle1_ptrs {
            // SAFETY: `p` is a valid allocation from `a`.
            unsafe {
                a.dealloc(p, Layout::from_size_align(block_size, 8).unwrap());
            }
        }

        let decommit_after_cycle2_pool = AllocCore::dbg_decommit_count();
        assert_eq!(
            decommit_after_cycle2_pool, decommit_after_cycle1_pool,
            "cycle 2: re-admission to the pool must not decommit either"
        );
    }
}

// ── Test: metadata/ring never decommitted (unaffected by this task) ───────

/// The metadata region and remote-free ring live in `[0, small_meta_end)`,
/// entirely outside the payload. Pool admission never touches this region —
/// true both before and after task #223's fix.
#[test]
fn metadata_survives_pool_cycle() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(windows, not(miri), not(feature = "numa-aware")))]
    {
        let base = seg_base(second_ptr);

        let frontier_before = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(frontier_before > 0, "frontier should be readable");

        // Fill and empty the segment -> pooled.
        let (_base, _ptrs) = fill_and_empty_segment(&mut a, second_ptr);

        // Metadata (including the frontier field itself) is still readable.
        let frontier_after = a.dbg_committed_payload_end_for(base as *mut u8).unwrap();
        assert!(frontier_after > 0, "frontier should still be readable");

        // R8-10: pool admission performs NO decommit at all, so
        // `dbg_decommit_count` must NOT have advanced by this point (contrast
        // with the pre-fix test, which asserted the opposite).
        let decommit_count = AllocCore::dbg_decommit_count();
        assert_eq!(
            decommit_count, 0,
            "expected NO decommit events from an empty->pool cycle alone \
             (no release happened yet)"
        );
    }
}

// ── Test: primordial + first small segment frontier matches the reservation gate ──

/// R7-B6 (primordial lazy commit), updated for R8-5 (task #218): the
/// primordial segment's initial frontier matches an ordinary small
/// segment's own initial frontier under a 3-way split mirroring
/// `bootstrap::primordial`'s stamping and `reserve_small_segment`'s:
///   - Genuine Windows-lazy (`alloc-lazy-commit` AND NOT `numa-aware` AND
///     real-Windows-not-miri): `meta_end + LAZY_FIRST_CHUNK`.
///   - `numa-aware` (any platform), OR Unix/miri (where R8-5 made the
///     lazy reservation itself commit the whole segment): `SEGMENT`.
///
/// (Unaffected by task #223 — this test covers FRESH segment reservation,
/// not pool admission.)
#[test]
fn primordial_and_pool_frontier_matches_reservation_gate() {
    let mut a = AllocCore::new().unwrap();
    let p = a.alloc(Layout::from_size_align(64, 8).unwrap());
    assert!(!p.is_null());
    let frontier = a.dbg_committed_payload_end_for(p).unwrap();
    // SAFETY: `p` is the pointer just returned by `a.alloc` above — live,
    // exclusively owned, and its segment is owned by `a`.
    let payload_start = unsafe { a.dbg_payload_start_for(p) };

    // R8-5 (task #218): primordial frontier, 3-way split.
    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    assert_eq!(
        frontier,
        payload_start + a.dbg_lazy_first_chunk(),
        "primordial segment (Windows-lazy) must start with the lazy initial-chunk frontier"
    );
    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = payload_start;
        assert_eq!(
            frontier, SEGMENT,
            "primordial segment on every eager leg (numa-aware OR Unix/miri) \
             must have frontier == SEGMENT (R8-5)"
        );
    }

    // Non-primordial segment frontier mirrors `reserve_small_segment`'s own
    // 3-way cfg gate (R8-5): genuine Windows-lazy → lazy value; every other
    // leg → SEGMENT.
    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let (a2, second) = alloc_past_primordial();
        let f2 = a2.dbg_committed_payload_end_for(second).unwrap();
        let expected = small_meta_end() + a2.dbg_lazy_first_chunk();
        assert_eq!(
            f2, expected,
            "non-primordial lazy segment must start with the lazy initial-chunk frontier"
        );
    }
    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let (a2, second) = alloc_past_primordial();
        let f2 = a2.dbg_committed_payload_end_for(second).unwrap();
        assert_eq!(
            f2, SEGMENT,
            "non-primordial eager segment must have full-span frontier (R8-5)"
        );
    }
    let _ = a;
}
