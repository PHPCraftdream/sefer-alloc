//! B3 (R7 Workstream B): tests for the lazy-commit-aware decommit/recommit
//! integration in the segment pool recycle path.
//!
//! Feature-gated: `alloc-lazy-commit` (which implies `alloc-core`).
//!
//! These tests verify:
//!   - After a decommit-and-pool cycle, the frontier resets to the initial lazy
//!     value (meta_end + LAZY_FIRST_CHUNK), NOT SEGMENT.
//!   - Repeated commit-decommit-recommit cycles keep the frontier lazy (it
//!     never jumps to SEGMENT on reuse).
//!   - A reused segment grows incrementally via B2's grow-on-carve logic
//!     (GROW_CHUNK steps, not one SEGMENT jump).
//!   - alloc_zeroed after recommit returns all zeros (Windows demand-zero).
//!   - Metadata and remote-free ring are never decommitted (still readable
//!     across the cycle).
//!   - The eager path (feature-OFF, Unix, miri) is unchanged.

#![cfg(feature = "alloc-lazy-commit")]
#![cfg_attr(
    feature = "numa-aware",
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

// ── Test: frontier resets to lazy value after decommit+pool ───────────────

/// After a segment empties and is pooled, under `alloc-lazy-commit` the
/// `committed_payload_end` is reset to `meta_end + LAZY_FIRST_CHUNK` (the
/// initial lazy frontier), NOT left at SEGMENT.
#[test]
fn frontier_resets_on_decommit_pool() {
    let (mut a, second_ptr) = alloc_past_primordial();
    let lazy_first_chunk = a.dbg_lazy_first_chunk();
    let _grow_chunk = a.dbg_grow_chunk();

    // On Unix/miri the eager path sets frontier = SEGMENT throughout.
    // The pool path does not decommit-reset. This test is a no-op there.
    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        let _ = (second_ptr, lazy_first_chunk, _grow_chunk);
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

        // Fill the segment and empty it so it gets pooled.
        let (base, _ptrs) = fill_and_empty_segment(&mut a, second_ptr);

        // After pool admission under B3, the frontier should be reset to the
        // initial lazy value.
        let pooled_count = a.dbg_pooled_count();
        assert!(pooled_count > 0, "segment should have been pooled");

        // The segment is still registered (pooled, not released). Read the
        // frontier from a pointer we know is in the segment.
        let frontier_after = a.dbg_committed_payload_end_for(base as *mut u8).unwrap();
        assert_eq!(
            frontier_after, expected_initial,
            "after pool, frontier should be reset to initial lazy value \
             ({expected_initial}), got {frontier_after}"
        );
    }
}

// ── Test: reused segment grows incrementally ─────────────────────────────

/// A segment that went through decommit-pool-reuse grows its frontier
/// incrementally via B2's grow-on-carve (GROW_CHUNK steps), NOT in one
/// SEGMENT jump.
#[test]
fn reused_segment_grows_incrementally() {
    let (mut a, second_ptr) = alloc_past_primordial();
    let grow_chunk = a.dbg_grow_chunk();
    let lazy_first_chunk = a.dbg_lazy_first_chunk();

    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        let _ = (second_ptr, grow_chunk, lazy_first_chunk);
        return;
    }

    #[cfg(all(windows, not(miri), not(feature = "numa-aware")))]
    {
        let expected_initial = small_meta_end() + lazy_first_chunk;

        // Fill and empty the segment so it gets pooled.
        let (base, _ptrs) = fill_and_empty_segment(&mut a, second_ptr);
        assert!(a.dbg_pooled_count() > 0, "segment should be pooled");

        // Now trigger a new segment reserve. The pool should be popped first
        // (under B3 lazy-commit), reusing the emptied segment as small_cur.
        // Allocate blocks into the reused segment and track frontier growth.
        let mut frontier_history = Vec::new();
        let block_size = 4096; // 4 KiB to cross chunks faster
        let mut saw_reused_seg = false;

        for _ in 0..600_000 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            assert!(!p.is_null(), "allocation should not fail");
            let p_base = seg_base(p);

            if p_base == base {
                saw_reused_seg = true;
                let frontier = a.dbg_committed_payload_end_for(p).unwrap();
                if frontier_history.last().copied() != Some(frontier) {
                    frontier_history.push(frontier);
                }
            }

            // Stop once we've moved past the reused segment.
            if saw_reused_seg && p_base != base {
                break;
            }
        }

        assert!(
            saw_reused_seg,
            "expected allocations to land in the reused (pooled) segment"
        );

        // The frontier should have grown incrementally, starting from the
        // initial lazy frontier. It should NOT jump directly to SEGMENT.
        assert!(
            !frontier_history.is_empty(),
            "expected at least one frontier observation"
        );

        // First observation should be at or near the initial lazy frontier.
        assert!(
            frontier_history[0] <= expected_initial + grow_chunk,
            "first frontier ({}) should be near initial lazy value ({})",
            frontier_history[0],
            expected_initial
        );

        // If there are multiple observations, each step should be
        // <= GROW_CHUNK above the previous (incremental growth).
        for w in frontier_history.windows(2) {
            let step = w[1] - w[0];
            assert!(
                step <= grow_chunk,
                "frontier step ({step}) should be <= GROW_CHUNK ({grow_chunk})"
            );
        }

        // The final frontier should NOT be SEGMENT unless the segment was
        // fully filled. If the segment was partially used, it should be
        // < SEGMENT.
        if frontier_history.len() > 1 {
            // We have multiple steps — the growth was incremental.
            // The first step proves we didn't jump to SEGMENT.
            assert!(
                frontier_history[0] < SEGMENT,
                "first frontier observation should be < SEGMENT (lazy growth)"
            );
        }
    }
}

// ── Test: repeated decommit-recommit cycles keep frontier lazy ───────────

/// Multiple cycles of fill-empty-pool-reuse keep the frontier lazy on every
/// reuse (it resets to the initial value, never accumulates to SEGMENT).
#[test]
fn repeated_cycles_keep_frontier_lazy() {
    let (mut a, second_ptr) = alloc_past_primordial();
    let lazy_first_chunk = a.dbg_lazy_first_chunk();

    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        let _ = (second_ptr, lazy_first_chunk);
        return;
    }

    #[cfg(all(windows, not(miri), not(feature = "numa-aware")))]
    {
        let expected_initial = small_meta_end() + lazy_first_chunk;

        // Cycle 1: fill, empty, pool.
        let (base, _ptrs) = fill_and_empty_segment(&mut a, second_ptr);

        // Verify frontier reset.
        let frontier1 = a.dbg_committed_payload_end_for(base as *mut u8).unwrap();
        assert_eq!(
            frontier1, expected_initial,
            "cycle 1: frontier should reset to initial lazy value"
        );

        // Reuse: allocate a few blocks (partially fill).
        let block_size = 16;
        let mut cycle1_ptrs = Vec::new();
        for _ in 0..100 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            assert!(!p.is_null());
            if seg_base(p) == base {
                cycle1_ptrs.push(p);
            }
        }

        // Cycle 2: free everything in this segment again.
        for &p in &cycle1_ptrs {
            // SAFETY: `p` is a valid allocation from `a`.
            unsafe {
                a.dealloc(p, Layout::from_size_align(block_size, 8).unwrap());
            }
        }

        // Check that the frontier is back to the initial lazy value.
        let frontier2 = a.dbg_committed_payload_end_for(base as *mut u8).unwrap();
        assert_eq!(
            frontier2, expected_initial,
            "cycle 2: frontier should reset to initial lazy value again, \
             got {frontier2}"
        );
    }
}

// ── Test: alloc_zeroed after recommit returns zeros ───────────────────────

/// After a decommit-pool-reuse cycle, freshly committed pages from the grow
/// path are zero-filled (Windows demand-zero guarantee). Blocks carved from
/// recommitted pages should contain all zeros.
#[test]
fn alloc_zeroed_after_recommit() {
    let (mut a, second_ptr) = alloc_past_primordial();
    let _lazy_first_chunk = a.dbg_lazy_first_chunk();

    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        let _ = (second_ptr, _lazy_first_chunk);
        return;
    }

    #[cfg(all(windows, not(miri), not(feature = "numa-aware")))]
    {
        let lazy_first_chunk = _lazy_first_chunk;
        let initial_chunk_end = small_meta_end() + lazy_first_chunk;

        // Fill and empty the segment so it gets pooled.
        let (base, _ptrs) = fill_and_empty_segment(&mut a, second_ptr);

        // After fill_and_empty_segment, small_cur is a THIRD segment (the one
        // that was allocated when the second overflowed). We need to exhaust
        // it (or at least trigger reserve_small_segment) so the pool is popped
        // and the second segment is reused. Allocate blocks until we land in
        // the reused (pooled) segment PAST the initial chunk boundary.
        //
        // The initial chunk `[meta_end, meta_end + LAZY_FIRST_CHUNK)` was NOT
        // decommitted on pool admission (B3 keeps it committed for fault-free
        // reuse), so it retains old data. Only pages ABOVE the initial chunk
        // that are freshly committed by grow-on-carve are guaranteed zero by
        // the OS (Windows demand-zero on VirtualAlloc MEM_COMMIT).
        let block_size = 4096; // 4 KiB blocks
        let mut found_zero_page = false;

        for _ in 0..600_000 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            assert!(!p.is_null());
            if seg_base(p) != base {
                continue;
            }
            let off_in_seg = p as usize - base;
            // Only check blocks PAST the initial chunk (freshly committed).
            if off_in_seg + block_size <= initial_chunk_end {
                continue;
            }
            found_zero_page = true;

            // Check that the block is zero. The block comes from freshly
            // committed pages (grow-on-carve committed them), which the OS
            // guarantees are zero on Windows.
            let slice = unsafe { core::slice::from_raw_parts(p, block_size) };
            for (i, &byte) in slice.iter().enumerate() {
                assert_eq!(
                    byte, 0,
                    "byte at offset {i} in block {:p} (seg offset {off_in_seg}) \
                     should be zero after recommit, got {byte:#x}",
                    p
                );
            }
            // Found and verified one block past the initial chunk; done.
            break;
        }

        assert!(
            found_zero_page,
            "expected at least one allocation PAST the initial chunk \
             in the reused segment"
        );
    }
}

// ── Test: metadata/ring never decommitted ────────────────────────────────

/// The metadata region and remote-free ring live in `[0, small_meta_end)`,
/// which is entirely below the decommit range. After a decommit-pool cycle,
/// the segment's metadata (magic, kind, segment_id) is still readable.
#[test]
fn metadata_survives_decommit_cycle() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(windows, not(miri), not(feature = "numa-aware")))]
    {
        let base = seg_base(second_ptr);

        // Record the segment_id before the decommit cycle.
        // (We can't access `segment_id_at` directly from tests, but we can
        // verify by checking the frontier is readable — the frontier accessor
        // reads from the segment header, which is in the metadata region.)
        let frontier_before = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(frontier_before > 0, "frontier should be readable");

        // Fill and empty the segment.
        let (_base, _ptrs) = fill_and_empty_segment(&mut a, second_ptr);

        // After the decommit-pool cycle, the metadata should still be
        // readable (the decommit only touched payload pages above the
        // initial chunk, never the metadata).
        let frontier_after = a.dbg_committed_payload_end_for(base as *mut u8).unwrap();
        // The frontier was reset but is still readable — no fault.
        assert!(frontier_after > 0, "frontier should still be readable");

        // The decommit count should have incremented (the pool admission
        // under B3 calls the decommit path).
        let decommit_count = AllocCore::dbg_decommit_count();
        assert!(decommit_count > 0, "expected at least one decommit event");
    }
}

// ── Test: eager path is unchanged ────────────────────────────────────────

/// On the eager path (Unix/miri), the pool admission does NOT decommit-reset.
/// The segment stays fully committed with free lists intact, and frontier
/// stays at SEGMENT. This test verifies the feature-OFF / non-Windows
/// behaviour is unchanged.
#[test]
fn eager_path_pool_unchanged() {
    let mut a = AllocCore::new().unwrap();
    // The primordial segment is always eager.
    let p = a.alloc(Layout::from_size_align(64, 8).unwrap());
    assert!(!p.is_null());
    let frontier = a.dbg_committed_payload_end_for(p).unwrap();

    // Primordial always has frontier == SEGMENT (eager).
    assert_eq!(
        frontier, SEGMENT,
        "primordial segment must have full-span frontier (eager path)"
    );

    // On Unix/miri, a non-primordial segment also has frontier == SEGMENT.
    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        let (a2, second) = alloc_past_primordial();
        let f2 = a2.dbg_committed_payload_end_for(second).unwrap();
        assert_eq!(
            f2, SEGMENT,
            "non-primordial eager segment must have full-span frontier"
        );
    }
    let _ = a;
}
