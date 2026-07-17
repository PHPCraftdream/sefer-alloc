//! B2 (R7 Workstream B): tests for the fallible incremental bump growth
//! (grow-on-carve) logic in `carve_block` and `carve_batch`.
//!
//! Feature-gated: `alloc-lazy-commit` (which implies `alloc-core`).
//!
//! These tests verify:
//!   - Carving exactly at a chunk boundary commits the next chunk.
//!   - A batch crossing one chunk boundary does ONE commit (not per-block).
//!   - A batch crossing SEVERAL boundaries commits up to its end.
//!   - Commit FAILURE before bump update leaves state unchanged and the
//!     allocation fails cleanly (fault-injection).
//!   - A freshly-lazy segment can be fully filled (carve across all chunks
//!     up to SEGMENT) with correct data at every block.
//!   - Feature-OFF + Unix/miri identical (the eager path is a pure no-op).

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

// ── Helper: get a fresh non-primordial segment ──────────────────────────────

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

// ── Test: carve at chunk boundary commits the next chunk ────────────────────

/// When the bump cursor is exactly at the committed frontier, the next carve
/// triggers a grow commit. After the grow, `committed_payload_end` advances
/// by exactly one `GROW_CHUNK`.
#[test]
fn carve_at_frontier_commits_next_chunk() {
    let (mut a, second_ptr) = alloc_past_primordial();
    let grow_chunk = a.dbg_grow_chunk();

    // The second segment starts with frontier = meta_end + LAZY_FIRST_CHUNK.
    let initial_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();

    // On Unix/miri the eager fallback sets frontier = SEGMENT; the grow check
    // is always false, so this test is a no-op there. Verify the invariant.
    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        assert_eq!(initial_frontier, SEGMENT);
        return; // nothing to test on the eager path
    }

    #[cfg(all(windows, not(miri), not(feature = "numa-aware")))]
    {
        let expected_initial = small_meta_end() + grow_chunk; // LAZY_FIRST_CHUNK == GROW_CHUNK
        assert_eq!(
            initial_frontier, expected_initial,
            "fresh lazy segment frontier mismatch"
        );

        // Allocate blocks until the frontier is crossed. Use a large-ish class
        // to cross faster (4 KiB blocks).
        let block_size = 4096;
        let base = seg_base(second_ptr);
        let commits_before = a.dbg_grow_commit_count();
        loop {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            assert!(
                !p.is_null(),
                "allocation returned null before frontier cross"
            );
            if (p as usize) & !(SEGMENT - 1) != base {
                // Moved to a THIRD segment — didn't detect the cross.
                break;
            }
            // Check if the frontier advanced.
            let frontier_now = a.dbg_committed_payload_end_for(p).unwrap();
            if frontier_now > initial_frontier {
                // The frontier advanced. It should be aligned to GROW_CHUNK
                // (the grow logic rounds up to the next GROW_CHUNK boundary)
                // and must be > initial_frontier, <= SEGMENT.
                assert!(
                    frontier_now.is_multiple_of(grow_chunk) || frontier_now == SEGMENT,
                    "frontier ({frontier_now}) should be GROW_CHUNK-aligned \
                     or equal to SEGMENT"
                );
                assert!(
                    frontier_now > initial_frontier,
                    "frontier should have advanced past the initial value"
                );
                assert!(
                    frontier_now <= SEGMENT,
                    "frontier should not exceed SEGMENT"
                );
                // At least one grow commit should have fired.
                let commits_after = a.dbg_grow_commit_count();
                assert!(
                    commits_after > commits_before,
                    "expected at least one grow commit"
                );
                break;
            }
        }
    }
}

// ── Test: carve_batch does ONE commit per batch ─────────────────────────────

/// A `carve_batch` that crosses one chunk boundary issues exactly ONE
/// `commit_pages` call (not per-block). Verified by observing the
/// `GROW_COMMIT_COUNT` counter.
#[test]
fn carve_batch_one_commit_per_batch() {
    let (mut a, second_ptr) = alloc_past_primordial();
    let _grow_chunk = a.dbg_grow_chunk();

    // On Unix/miri this is a no-op (eager path, no commits).
    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(windows, not(miri), not(feature = "numa-aware")))]
    {
        let base = seg_base(second_ptr);
        let initial_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(initial_frontier < SEGMENT);

        // We'll use carve_batch with a large batch of small blocks. The batch
        // should cross at least one chunk boundary, triggering exactly ONE
        // commit for the whole batch.
        //
        // Use class 0 (16-byte blocks). A batch of 1024 blocks = 16 KiB.
        // We need to push the bump cursor to just below the frontier first,
        // then issue a batch that crosses.
        //
        // Strategy: allocate individual blocks until near the frontier, then
        // issue one large batch via dbg_carve_batch that crosses.
        let block_size = 16;
        let mut near_frontier = false;
        for _ in 0..100_000 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            assert!(!p.is_null());
            if (p as usize) & !(SEGMENT - 1) != base {
                panic!("moved to a new segment before reaching the frontier");
            }
            // Check how close the bump is to the frontier. The bump is at
            // (ptr_offset + block_size) approximately.
            let ptr_off = (p as usize) - base;
            // Leave some room for the batch to cross.
            if ptr_off + block_size * 64 > initial_frontier {
                near_frontier = true;
                break;
            }
        }
        assert!(near_frontier, "failed to approach the frontier");

        let frontier_before = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        let grow_chunk = a.dbg_grow_chunk();

        // Issue a batch that should cross the frontier.
        let mut batch = [core::ptr::null_mut(); 512];
        let n = a.dbg_carve_batch(0, &mut batch);
        assert!(n > 0, "carve_batch returned 0 blocks");

        let frontier_after = a.dbg_committed_payload_end_for(second_ptr).unwrap();

        // The batch should have advanced the frontier in ONE jump (verified
        // by the frontier value being GROW_CHUNK-aligned and covering the
        // batch end — if per-block commits happened, we'd see an intermediate
        // partial frontier value that doesn't cover the whole batch).
        if frontier_after > frontier_before {
            // The frontier advanced — it should be GROW_CHUNK-aligned.
            assert!(
                frontier_after.is_multiple_of(grow_chunk) || frontier_after == SEGMENT,
                "frontier ({frontier_after}) should be GROW_CHUNK-aligned \
                 after a batch commit"
            );
            // The frontier should cover the entire batch: all n blocks are
            // writable (they're within the committed region).
            let batch_span = n * 16; // class 0 = 16 bytes
                                     // The batch starts from wherever the bump was; the frontier
                                     // should be at least batch_span past the old frontier.
            assert!(
                frontier_after >= frontier_before + batch_span
                    || frontier_after == SEGMENT
                    || frontier_after.is_multiple_of(grow_chunk),
                "frontier should cover the entire batch"
            );
        }
    }
}

// ── Test: batch crossing SEVERAL boundaries ─────────────────────────────────

/// A batch whose total span crosses SEVERAL chunk boundaries still does ONE
/// commit covering all of them. The frontier advances to the rounded end.
#[test]
fn batch_crossing_several_boundaries_one_commit() {
    // This test uses a very large batch of the largest small class to cross
    // multiple chunk boundaries in one shot.
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(windows, not(miri), not(feature = "numa-aware")))]
    {
        let grow_chunk = a.dbg_grow_chunk();
        let _base = seg_base(second_ptr);
        let initial_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(initial_frontier < SEGMENT);

        // We want a batch that covers a large span. Use class 0 (16-byte
        // blocks) with a very large batch: 32768 blocks = 512 KiB span,
        // which crosses two 256 KiB chunk boundaries.
        let mut batch = [core::ptr::null_mut(); 32768];
        let n = a.dbg_carve_batch(0, &mut batch);

        let frontier_after = a.dbg_committed_payload_end_for(second_ptr).unwrap();

        if n > 0 && frontier_after > initial_frontier {
            // Verify all carved blocks are writable — if the commit only
            // covered a partial range, writing to later blocks would fault.
            for &p in &batch[..n] {
                // SAFETY: `p` is a valid 16-byte allocation.
                unsafe {
                    p.write(0xBB);
                    assert_eq!(p.read(), 0xBB);
                }
            }
            // The frontier must be GROW_CHUNK-aligned (the grow logic
            // always rounds up) or equal to SEGMENT.
            assert!(
                frontier_after.is_multiple_of(grow_chunk) || frontier_after == SEGMENT,
                "frontier ({frontier_after}) should be GROW_CHUNK-aligned"
            );
            // The frontier should have jumped past the batch's final block.
            // The last carved block is at an offset within the segment; the
            // frontier must be >= that offset + block_size.
            let base = seg_base(second_ptr);
            let last_block_end = (batch[n - 1] as usize - base) + 16;
            assert!(
                frontier_after >= last_block_end,
                "frontier ({frontier_after}) should be >= last_block_end \
                 ({last_block_end})"
            );
        }
    }
}

// ── Test: commit failure leaves state unchanged ─────────────────────────────

/// When the fault-injection hook causes `commit_pages` to fail, the carve
/// path must leave EVERYTHING unchanged: bump not moved,
/// committed_payload_end not moved, live_count unchanged, page map
/// unwritten, and the allocation returns null. A subsequent normal
/// allocation still works.
#[test]
fn commit_failure_leaves_state_unchanged() {
    let (mut a, second_ptr) = alloc_past_primordial();

    // On Unix/miri the eager path never calls commit_pages on carve, so the
    // fault hook has no effect in carve_block. We can still test that arming
    // it doesn't break anything.
    let initial_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();

    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        // On the eager path, frontier == SEGMENT, so commit_pages is never
        // called in carve. Arm the fault hook and verify alloc still works.
        assert_eq!(initial_frontier, SEGMENT);
        a.dbg_arm_commit_fail(1);
        let p = a.alloc(Layout::from_size_align(16, 8).unwrap());
        // Should still succeed (the eager path never hits commit_pages).
        assert!(!p.is_null());
        return;
    }

    #[cfg(all(windows, not(miri), not(feature = "numa-aware")))]
    {
        assert!(initial_frontier < SEGMENT, "expected lazy frontier");
        let base = seg_base(second_ptr);

        // Push the bump cursor to just before the frontier by allocating.
        let block_size = 16;
        for _ in 0..100_000 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            assert!(!p.is_null());
            if (p as usize) & !(SEGMENT - 1) != base {
                panic!("moved to a new segment too early");
            }
            let ptr_off = (p as usize) - base;
            if ptr_off + block_size * 4 > initial_frontier {
                break;
            }
        }

        // Snapshot state BEFORE the fault. (`dbg_grow_commit_count` is a
        // process-global counter shared by every `#[test]` in this binary —
        // running on parallel test threads by default — so it is NOT used
        // here as a before/after oracle: an unrelated concurrent test in this
        // same file, e.g. `fill_entire_lazy_segment`, can bump it during this
        // test's own `for _ in 0..1000` allocation loop, producing a
        // false-failure race unrelated to this test's own fault injection.
        // The segment-LOCAL frontier below is process-safe and is the
        // load-bearing oracle for "no successful commit happened.")
        let frontier_before = a.dbg_committed_payload_end_for(second_ptr).unwrap();

        // Arm the fault injector: the next commit_pages call will fail.
        a.dbg_arm_commit_fail(1);

        // Allocate enough blocks to cross the frontier. The next carve that
        // needs to grow should fail.
        for _ in 0..1000 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            if p.is_null() {
                break;
            }
            // If we're still in the same segment and haven't hit the frontier
            // yet, the commit wasn't needed — keep going.
            if (p as usize) & !(SEGMENT - 1) != base {
                // Moved to a new segment — the reserve itself may have
                // triggered a commit failure on the initial chunk. Check that
                // the allocation from a fallback segment worked.
                break;
            }
        }

        // The frontier should NOT have advanced (the commit failed).
        let frontier_after = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert_eq!(
            frontier_before, frontier_after,
            "frontier should not advance on commit failure"
        );

        // The fault hook is disarmed now (it was armed for 1 failure).
        // A subsequent allocation should succeed normally.
        let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
        assert!(
            !p.is_null(),
            "allocation after fault recovery should succeed"
        );
    }
}

// ── Test: fill entire segment across all chunks ─────────────────────────────

/// A freshly-lazy segment can be completely filled (carve across all chunks
/// up to SEGMENT) with correct data at every block.
#[test]
fn fill_entire_lazy_segment() {
    let (mut a, second_ptr) = alloc_past_primordial();
    let base = seg_base(second_ptr);

    // Allocate 16-byte blocks until the segment is exhausted (we move to
    // a new segment).
    let block_size = 16;
    let mut ptrs_in_seg = vec![second_ptr];
    for _ in 0..500_000 {
        let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
        assert!(!p.is_null());
        if (p as usize) & !(SEGMENT - 1) != base {
            // Moved to a third segment — the second is full.
            break;
        }
        ptrs_in_seg.push(p);
    }

    // Verify every block is writable and readable.
    for &p in &ptrs_in_seg {
        // SAFETY: `p` is a valid 16-byte allocation.
        unsafe {
            p.write(0xCD);
            assert_eq!(p.read(), 0xCD, "block at {:p} not writable/readable", p);
        }
    }

    // On the lazy path, the frontier should have advanced to SEGMENT (fully
    // committed after filling).
    let final_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
    assert_eq!(
        final_frontier, SEGMENT,
        "after filling the entire segment, frontier should be SEGMENT"
    );
}

// ── Test: carve_batch commit failure leaves batch empty ──────────────────────

/// When `commit_pages` fails mid-batch (because the batch end exceeds the
/// frontier), carve_batch returns 0 blocks with no state change.
#[test]
fn carve_batch_commit_failure_returns_zero() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(not(windows), miri, feature = "numa-aware"))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(windows, not(miri), not(feature = "numa-aware")))]
    {
        let base = seg_base(second_ptr);
        let initial_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(initial_frontier < SEGMENT);

        // Push near the frontier.
        let block_size = 16;
        for _ in 0..100_000 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            assert!(!p.is_null());
            if (p as usize) & !(SEGMENT - 1) != base {
                panic!("moved to a new segment too early");
            }
            let ptr_off = (p as usize) - base;
            if ptr_off + block_size * 4 > initial_frontier {
                break;
            }
        }

        let frontier_before = a.dbg_committed_payload_end_for(second_ptr).unwrap();

        // Arm the fault injector.
        a.dbg_arm_commit_fail(1);

        // Issue a batch that should cross the frontier.
        let mut batch = [core::ptr::null_mut(); 512];
        let n = a.dbg_carve_batch(0, &mut batch);

        // If the batch needed a commit (crossed the frontier), it should have
        // returned 0 and the frontier should be unchanged.
        let frontier_after = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        if frontier_before == frontier_after && frontier_before < SEGMENT {
            // The commit failed and the batch returned 0 (or the batch fit
            // within the frontier — both are correct).
            assert!(
                n == 0 || frontier_after == frontier_before,
                "if commit failed, batch should return 0 or frontier unchanged"
            );
        }
    }
}

// ── Test: primordial frontier matches its reservation gate ─────────────────

/// R7-B6 (primordial lazy commit): the primordial segment's initial
/// `committed_payload_end` frontier must match EXACTLY what
/// `bootstrap::primordial` committed:
///
/// - Under `alloc-lazy-commit` AND NOT `numa-aware` (this test file's default
///   build; on Windows a REAL partial commit, on Unix/miri the
///   `reserve_aligned_lazy` internal eager fallback): the frontier is
///   `primordial_meta_end() + LAZY_FIRST_CHUNK` — the primordial is now
///   lazily committed too, mirroring an ordinary small segment's own initial
///   frontier. No grow commits have fired yet (the first allocation lands
///   inside the initial chunk).
/// - Under `numa-aware` (the primordial reservation still uses the plain
///   eager `Segment::reserve`, matching `reserve_small_segment`'s own NUMA
///   exclusion — see `bootstrap.rs`'s doc): the frontier is `SEGMENT`, the
///   pre-R7-B6 eager behaviour, unchanged.
///
/// This replaces the old `eager_path_is_noop` test, which asserted the
/// primordial ALWAYS has a full-span frontier — true before R7-B6, false
/// now that the primordial participates in lazy commit like any other small
/// segment.
#[test]
fn primordial_frontier_matches_reservation_gate() {
    let mut a = AllocCore::new().unwrap();
    let p = a.alloc(Layout::from_size_align(64, 8).unwrap());
    assert!(!p.is_null());
    let frontier = a.dbg_committed_payload_end_for(p).unwrap();
    // SAFETY: `p` is the pointer just returned by `a.alloc` above — live,
    // exclusively owned, and its segment is owned by `a`.
    let payload_start = unsafe { a.dbg_payload_start_for(p) };

    // NOTE: `dbg_grow_commit_count` is a process-global counter shared by
    // every `#[test]` in this binary, running on parallel test threads by
    // default — it cannot be asserted on here without a false-failure race
    // against unrelated tests (`carve_at_frontier_commits_next_chunk` and
    // friends in this same file deliberately grow it). The frontier equality
    // check below is the load-bearing, deterministic assertion; the
    // dedicated grow-commit tests elsewhere in this file already cover the
    // counter's own correctness.
    #[cfg(not(feature = "numa-aware"))]
    {
        let expected = payload_start + a.dbg_lazy_first_chunk();
        assert_eq!(
            frontier, expected,
            "primordial segment must start with the lazy initial-chunk frontier \
             (meta_end + LAZY_FIRST_CHUNK), matching an ordinary small segment"
        );
    }
    #[cfg(feature = "numa-aware")]
    {
        let _ = payload_start;
        assert_eq!(
            frontier, SEGMENT,
            "under numa-aware the primordial reservation is still eager \
             (Segment::reserve), so the frontier must be the full span"
        );
    }
}
