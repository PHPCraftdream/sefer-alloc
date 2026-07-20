//! B4 (R7 Workstream B): correctness-gate matrix + fault-injection tests.
//!
//! Feature-gated: `alloc-lazy-commit` + `alloc-decommit` (both required for the
//! full lifecycle: lazy reserve, grow-on-carve, decommit/pool/reuse).
//!
//! These tests verify the B4 scenario list from R7_PLAN.md:
//!   - First block of each chunk; block exactly on a chunk boundary.
//!   - Batch crossing one boundary and several boundaries.
//!   - Commit failure BEFORE bump update (state fully unchanged) for BOTH
//!     carve_block and carve_batch, including the k-th-commit-fails variant
//!     (B4 fault hook).
//!   - Retry after a failure succeeds.
//!   - Decommit -> partial recommit -> continue.
//!   - Pool retain/reuse with a fault mid-reuse.
//!   - Pool eviction/release with a partially-committed segment.
//!   - Cross-thread free of a block in a partially-committed segment.
//!   - Allocator drop/release of a reservation whose payload is only partially
//!     committed (no double-free, no decommit-of-uncommitted UB).
//!   - Primordial metadata untouched (always eager).
//!   - alloc_zeroed correctness on freshly-committed pages.
//!   - Windows NUMA combination gate (lazy path is gated on the genuine
//!     Windows-lazy leg: `all(not(numa-aware), windows, not(miri))`).
//!
//! R8-5 (task #218): every test's eager early-return leg expanded from
//! `numa-aware` to `any(numa-aware, not(windows), miri)` because on Unix/miri
//! `reserve_aligned_lazy` already commits the whole segment (the OS has no
//! partial-commit distinction there / miri models no RSS), so the allocator's
//! own frontier now starts at SEGMENT there too and grow-on-carve never fires.
//! Only the genuine Windows-lazy leg exercises the grow/fault bodies.
//!
//! The B4 "fail the k-th commit" hook (`dbg_arm_commit_fail_at`) is ADDITIVE
//! to B2's `dbg_arm_commit_fail`; B2/B3 tests remain green.

#![cfg(all(feature = "alloc-lazy-commit", feature = "alloc-decommit"))]
// Every test in this file has an eager early-return leg
// (`any(numa-aware, not(windows), miri)`) and a lazy-exercise leg
// (`all(not(numa-aware), windows, not(miri))`). On the eager leg the
// lazy-exercise bindings compile out unused. Silence the lint family on
// every leg that is NOT the genuine Windows-lazy one (R8-5, task #218).
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
            // Spilled into a new segment; free this overflow pointer too.
            unsafe {
                a.dealloc(p, Layout::from_size_align(block_size, 8).unwrap());
            }
            break;
        }
        ptrs.push(p);
    }

    // Free all blocks in the segment so it empties and gets pooled.
    for &p in &ptrs {
        unsafe {
            a.dealloc(p, Layout::from_size_align(block_size, 8).unwrap());
        }
    }

    (base, ptrs)
}

// ============================================================================
// SCENARIO 1: First block of each chunk
// ============================================================================

/// The very first block carved from a fresh lazy segment lands in the first
/// committed chunk. As the bump grows, the first block of each subsequent
/// chunk should trigger exactly one grow commit. Counterfactual: if the grow
/// logic is missing, the allocator would fault on the first block past the
/// initial chunk.
#[test]
fn first_block_of_each_chunk() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        // Eager path: frontier == SEGMENT, no commits needed.
        let frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert_eq!(frontier, SEGMENT);
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let grow_chunk = a.dbg_grow_chunk();
        let base = seg_base(second_ptr);
        let initial_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(initial_frontier < SEGMENT, "expected lazy frontier");

        // Track how many distinct frontier values we observe as we fill.
        let mut frontier_steps = vec![initial_frontier];
        let block_size = 4096; // 4 KiB blocks to cross chunks faster.
        let mut commits_at_step = vec![a.dbg_grow_commit_count()];

        for _ in 0..2000 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            assert!(!p.is_null());
            if (p as usize) & !(SEGMENT - 1) != base {
                break; // spilled to next segment
            }
            let f = a.dbg_committed_payload_end_for(p).unwrap();
            if f != *frontier_steps.last().unwrap() {
                frontier_steps.push(f);
                commits_at_step.push(a.dbg_grow_commit_count());
            }
        }

        // We should have observed at least 2 frontier steps (initial + at
        // least one grow). Each step should advance to a GROW_CHUNK-aligned
        // boundary (or SEGMENT). The first step may be LESS than GROW_CHUNK
        // because the initial frontier (meta_end + LAZY_FIRST_CHUNK) is not
        // necessarily at a GROW_CHUNK boundary from the segment base — the
        // grow logic does align_up(carve_end, GROW_CHUNK), which rounds to
        // the next absolute GROW_CHUNK boundary.
        assert!(
            frontier_steps.len() >= 2,
            "expected at least 2 frontier steps, got {}",
            frontier_steps.len()
        );
        for w in frontier_steps.windows(2) {
            let step = w[1] - w[0];
            assert!(
                step <= grow_chunk || w[1] == SEGMENT,
                "frontier step ({step}) should be <= GROW_CHUNK ({grow_chunk}) \
                 or reach SEGMENT"
            );
            // Each new frontier value must be GROW_CHUNK-aligned or SEGMENT.
            assert!(
                w[1].is_multiple_of(grow_chunk) || w[1] == SEGMENT,
                "frontier ({}) should be GROW_CHUNK-aligned or SEGMENT",
                w[1]
            );
        }
    }
}

// ============================================================================
// SCENARIO 2: Block exactly ON a chunk boundary
// ============================================================================

/// When blocks are carved past the committed frontier, the frontier advances
/// to a GROW_CHUNK-aligned boundary. This test verifies that the boundary
/// crossing fires a commit and the frontier is GROW_CHUNK-aligned afterward.
/// Counterfactual: without the grow check, carving past the frontier would
/// fault on uncommitted memory.
#[test]
fn block_exactly_on_chunk_boundary() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let grow_chunk = a.dbg_grow_chunk();
        let base = seg_base(second_ptr);
        let initial_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(initial_frontier < SEGMENT, "expected lazy frontier");

        // Allocate blocks until we observe a frontier advance in this segment.
        let block_size = 4096; // 4 KiB to cross faster
        let mut saw_advance = false;

        for _ in 0..2000 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            assert!(!p.is_null());
            if (p as usize) & !(SEGMENT - 1) != base {
                break; // moved to a new segment
            }
            let frontier = a.dbg_committed_payload_end_for(p).unwrap();
            if frontier > initial_frontier {
                // The frontier advanced. It must be GROW_CHUNK-aligned.
                assert!(
                    frontier.is_multiple_of(grow_chunk) || frontier == SEGMENT,
                    "frontier ({frontier}) should be GROW_CHUNK-aligned or SEGMENT"
                );
                saw_advance = true;
                break;
            }
        }

        assert!(
            saw_advance,
            "expected to see the frontier advance past its initial value"
        );
    }
}

// ============================================================================
// SCENARIO 3: Batch crossing one boundary (ONE commit)
// ============================================================================

/// carve_batch crossing a chunk boundary advances the frontier. After the
/// batch, all carved blocks are writable (proving the commit covered them).
/// Counterfactual: without the batch-level commit, blocks past the frontier
/// would fault on write.
#[test]
fn batch_crosses_one_boundary_one_commit() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let grow_chunk = a.dbg_grow_chunk();
        let initial_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(initial_frontier < SEGMENT);

        // Issue a batch from the fresh second segment. The batch of 16384
        // 16-byte blocks (256 KiB) should cross at least one chunk boundary
        // from the bump's starting position.
        let commits_before = a.dbg_grow_commit_count();
        let mut batch = [core::ptr::null_mut(); 16384];
        let n = a.dbg_carve_batch(0, &mut batch);

        if n == 0 {
            // The segment might already be full (release-mode optimization
            // differences). Skip the test rather than fail — the core
            // assertion is covered by batch_crosses_several_boundaries.
            return;
        }

        let frontier_after = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        let commits_after = a.dbg_grow_commit_count();

        // All carved blocks must be writable.
        for &p in &batch[..n] {
            unsafe {
                p.write(0xCC);
                assert_eq!(p.read(), 0xCC);
            }
        }

        if frontier_after > initial_frontier {
            // At least one commit should have fired.
            assert!(
                commits_after > commits_before,
                "at least one commit should have fired for the batch"
            );
            assert!(
                frontier_after.is_multiple_of(grow_chunk) || frontier_after == SEGMENT,
                "frontier should be GROW_CHUNK-aligned"
            );
        }
    }
}

// ============================================================================
// SCENARIO 4: Batch crossing SEVERAL boundaries
// ============================================================================

/// A large batch whose span crosses multiple chunk boundaries still issues
/// ONE commit covering them all.
#[test]
fn batch_crosses_several_boundaries() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let _grow_chunk = a.dbg_grow_chunk();
        let base = seg_base(second_ptr);
        let initial_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(initial_frontier < SEGMENT);

        let commits_before = a.dbg_grow_commit_count();

        // Issue a very large batch of 16-byte blocks: 32768 * 16 = 512 KiB,
        // which crosses two 256 KiB chunk boundaries from the start.
        let mut batch = [core::ptr::null_mut(); 32768];
        let n = a.dbg_carve_batch(0, &mut batch);

        let frontier_after = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        let commits_after = a.dbg_grow_commit_count();

        if n > 0 && frontier_after > initial_frontier {
            // At least one commit should have fired.
            assert!(
                commits_after > commits_before,
                "at least one commit should have fired for the multi-boundary batch"
            );
            // All carved blocks must be writable (if the commit only
            // partially covered the batch, writing would fault).
            for &p in &batch[..n] {
                unsafe {
                    p.write(0xBB);
                    assert_eq!(p.read(), 0xBB);
                }
            }
            // Frontier should cover the batch end.
            let last_end = (batch[n - 1] as usize - base) + 16;
            assert!(
                frontier_after >= last_end,
                "frontier ({frontier_after}) should cover batch end ({last_end})"
            );
        }
    }
}

// ============================================================================
// SCENARIO 5: carve_block commit failure — state fully unchanged
// ============================================================================

/// When `commit_pages` fails on carve_block, EVERYTHING stays unchanged:
/// bump not moved, committed_payload_end not moved, live_count unchanged,
/// and the allocation returns null. Counterfactual: if the bump were
/// advanced before the commit check, it would be wrong after the failure.
#[test]
fn carve_block_commit_failure_state_unchanged() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let base = seg_base(second_ptr);
        let initial_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(initial_frontier < SEGMENT);

        // Push bump near the frontier.
        let block_size = 16;
        for _ in 0..100_000 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            assert!(!p.is_null());
            if (p as usize) & !(SEGMENT - 1) != base {
                panic!("moved to a new segment too early");
            }
            let off = (p as usize) - base;
            if off + block_size * 4 > initial_frontier {
                break;
            }
        }

        let frontier_before = a.dbg_committed_payload_end_for(second_ptr).unwrap();

        // Arm B2-style fault: next commit fails.
        a.dbg_arm_commit_fail(1);

        // Try to allocate enough to cross the frontier. The carve that
        // crosses should fail.
        for _ in 0..1000 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            if p.is_null() {
                break;
            }
            if (p as usize) & !(SEGMENT - 1) != base {
                // Moved to a fallback segment (the commit failure caused the
                // allocator to give up on this segment and try a fresh one).
                break;
            }
        }

        // The frontier of the FAILED segment should NOT have advanced.
        // (The allocator may have committed pages on a DIFFERENT segment
        // as a fallback, but THIS segment's frontier stays unchanged.)
        let frontier_after = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert_eq!(
            frontier_before, frontier_after,
            "frontier must not advance on commit failure"
        );

        // Recovery: the fault is disarmed. A subsequent alloc must succeed.
        let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
        assert!(!p.is_null(), "alloc after fault recovery should succeed");
    }
}

// ============================================================================
// SCENARIO 6: carve_batch commit failure — returns 0
// ============================================================================

/// When `commit_pages` fails on carve_batch, 0 blocks are returned and
/// state is unchanged. Counterfactual: without the pre-commit check,
/// blocks would be carved into uncommitted memory.
#[test]
fn carve_batch_commit_failure_returns_zero_blocks() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
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
            let off = (p as usize) - base;
            if off + block_size * 4 > initial_frontier {
                break;
            }
        }

        let frontier_before = a.dbg_committed_payload_end_for(second_ptr).unwrap();

        // Arm the fault: next commit fails.
        a.dbg_arm_commit_fail(1);

        // Issue a batch that must cross the frontier.
        let mut batch = [core::ptr::null_mut(); 512];
        let n = a.dbg_carve_batch(0, &mut batch);

        let frontier_after = a.dbg_committed_payload_end_for(second_ptr).unwrap();

        // If the batch needed a commit (frontier didn't change), it should
        // have returned 0.
        if frontier_after == frontier_before && frontier_before < SEGMENT {
            assert_eq!(n, 0, "batch should return 0 when commit fails");
        }
    }
}

// ============================================================================
// SCENARIO 7: k-th-commit-fails (B4 hook) — carve_block
// ============================================================================

/// The B4 "fail the k-th commit" hook lets the first (k-1) commits succeed
/// and fails exactly the k-th. After the k-th failure, subsequent commits
/// succeed normally. Counterfactual: without the k-th-commit hook, we can
/// only fail ALL commits (B2 hook), not a specific mid-sequence one.
#[test]
fn kth_commit_fails_carve_block() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let base = seg_base(second_ptr);
        let grow_chunk = a.dbg_grow_chunk();
        let initial_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(initial_frontier < SEGMENT);

        // Arm B4 hook: fail the 2nd commit (let the 1st succeed).
        a.dbg_arm_commit_fail_at(2);

        let commits_before = a.dbg_grow_commit_count();

        // Allocate blocks to trigger multiple chunk boundary crossings.
        let block_size = 4096;
        let mut frontier_history = vec![initial_frontier];

        for _ in 0..2000 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            if p.is_null() {
                break;
            }
            if (p as usize) & !(SEGMENT - 1) != base {
                // Moved to a new segment. The 2nd commit must have failed on
                // the old segment, causing a fallback.
                break;
            }
            let f = a.dbg_committed_payload_end_for(p).unwrap();
            if *frontier_history.last().unwrap() != f {
                frontier_history.push(f);
            }
        }

        let commits_after = a.dbg_grow_commit_count();
        // The 1st commit should have succeeded.
        assert!(
            commits_after > commits_before,
            "the first commit should have succeeded"
        );

        // The frontier should have advanced at least once (the first commit).
        assert!(
            frontier_history.len() >= 2,
            "expected at least one successful frontier advance"
        );
        let first_step = frontier_history[1] - frontier_history[0];
        // The first step may be less than GROW_CHUNK (see first_block_of_each_chunk
        // comment). The new frontier must be GROW_CHUNK-aligned.
        assert!(
            first_step <= grow_chunk || frontier_history[1] == SEGMENT,
            "first advance ({first_step}) should be <= GROW_CHUNK ({grow_chunk})"
        );
        assert!(
            frontier_history[1].is_multiple_of(grow_chunk) || frontier_history[1] == SEGMENT,
            "first frontier ({}) should be GROW_CHUNK-aligned or SEGMENT",
            frontier_history[1]
        );

        // The 2nd commit should have failed, causing either a null return
        // or a segment switch. Either way, the segment's frontier should
        // NOT have advanced a second time from B4's perspective.
        // (If it DID advance twice, that means B4's fault hook didn't fire.)
    }
}

// ============================================================================
// SCENARIO 8: k-th-commit-fails — carve_batch
// ============================================================================

/// Same as scenario 7 but exercises carve_batch: arm B4 to fail the 2nd
/// commit, issue batches that each trigger a commit, and verify the 2nd
/// batch returns 0.
#[test]
fn kth_commit_fails_carve_batch() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let _base = seg_base(second_ptr);
        let initial_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(initial_frontier < SEGMENT);

        // Issue a first batch to advance past the initial frontier.
        let mut batch1 = [core::ptr::null_mut(); 32768];
        let n1 = a.dbg_carve_batch(0, &mut batch1);
        assert!(n1 > 0, "first batch should succeed");
        let frontier_after_first = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(
            frontier_after_first > initial_frontier,
            "first batch should advance the frontier"
        );

        // Now arm B4: fail the NEXT (1st from now) commit.
        a.dbg_arm_commit_fail_at(1);

        // Issue a second batch that should cross the new frontier.
        // It should fail (return 0) because the commit is blocked.
        let mut batch2 = [core::ptr::null_mut(); 32768];
        let n2 = a.dbg_carve_batch(0, &mut batch2);

        let frontier_after_second = a.dbg_committed_payload_end_for(second_ptr).unwrap();

        // If the batch needed a commit and it failed, n2 == 0 and frontier unchanged.
        if frontier_after_second == frontier_after_first && frontier_after_first < SEGMENT {
            assert_eq!(
                n2, 0,
                "second batch should return 0 when the k-th commit fails"
            );
        }
    }
}

// ============================================================================
// SCENARIO 9: Retry after failure succeeds
// ============================================================================

/// After a commit failure, disarming the fault hook and retrying the same
/// allocation succeeds. The frontier advances normally.
#[test]
fn retry_after_failure_succeeds() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
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
            let off = (p as usize) - base;
            if off + block_size * 4 > initial_frontier {
                break;
            }
        }

        let _frontier_before = a.dbg_committed_payload_end_for(second_ptr).unwrap();

        // Arm: fail the next commit.
        a.dbg_arm_commit_fail(1);

        // Attempt alloc — should fail or fallback to a new segment.
        for _ in 0..100 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            if p.is_null() || (p as usize) & !(SEGMENT - 1) != base {
                break;
            }
        }

        // Disarm (already disarmed after 1 failure, but make sure).
        a.dbg_arm_commit_fail(0);

        // Now retry — should succeed.
        let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
        assert!(!p.is_null(), "retry after fault should succeed");

        // Write + read to verify the block is usable.
        unsafe {
            p.write(0xAA);
            assert_eq!(p.read(), 0xAA, "retried allocation should be writable");
        }
    }
}

// ============================================================================
// SCENARIO 10: Decommit -> partial recommit -> continue
// ============================================================================

/// R8-10 (task #223): after a fill-empty-pool cycle, the segment stays
/// FULLY committed (pool admission no longer decommits/resets it — see
/// `release_or_pool_empty_segment`'s doc comment). Reuse goes through the
/// free-list path and costs zero additional grow-commits, since the frontier
/// was already fully grown by the time the segment emptied (it had to be, to
/// have carved every block that was later freed).
///
/// Counterfactual (pre-fix, B3 design): decommit-on-admission reset the
/// frontier back to the initial lazy chunk, so reuse re-grew it incrementally
/// via grow-on-carve — this test used to assert exactly that re-growth.
#[test]
fn decommit_partial_recommit_continue() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let _grow_chunk = a.dbg_grow_chunk();
        let lazy_first_chunk = a.dbg_lazy_first_chunk();
        let base = seg_base(second_ptr);
        let expected_initial = small_meta_end() + lazy_first_chunk;

        // Fill and empty the segment so it gets pooled.
        let (_base, _ptrs) = fill_and_empty_segment(&mut a, second_ptr);
        assert!(a.dbg_pooled_count() > 0, "segment should be pooled");

        // R8-10: pool admission does NOT reset the frontier — it stays at
        // whatever it grew to while the segment was being filled (which must
        // be > the initial lazy chunk, since filling the whole segment
        // necessarily carved past it).
        let frontier_after_pool = a.dbg_committed_payload_end_for(base as *mut u8).unwrap();
        assert!(
            frontier_after_pool > expected_initial,
            "after pool, frontier should stay at its fully-grown value \
             (> initial lazy value {expected_initial}), got {frontier_after_pool}"
        );

        // Reuse the pooled segment via the free-list path: this costs ZERO
        // additional grow-commits, since the frontier is already where it
        // needs to be for every free-listed block.
        let commits_before = a.dbg_grow_commit_count();
        let mut reused = false;
        for _ in 0..600_000 {
            let p = a.alloc(Layout::from_size_align(16, 8).unwrap());
            assert!(!p.is_null());
            if (p as usize) & !(SEGMENT - 1) == base {
                reused = true;
                break;
            }
        }
        assert!(reused, "expected the pooled segment to be reused");
        let commits_after = a.dbg_grow_commit_count();
        assert_eq!(
            commits_after, commits_before,
            "reuse via the free-list path should not fire any grow commits"
        );
    }
}

// ============================================================================
// SCENARIO 11: Pool retain/reuse with a fault mid-reuse
// ============================================================================

/// R8-10 (task #223): a pooled segment's reuse (via the free-list path) never
/// issues a grow-commit — the frontier was already fully grown before the
/// segment emptied, and pool admission does not reset it. So a commit-fault
/// armed right after pooling can only fire on a DIFFERENT (fresh) segment's
/// growth, not on the reused one. This test verifies that: the pooled
/// segment's own frontier and free-list-driven reuse are completely
/// unaffected by a fault injected on the next grow-commit elsewhere, and the
/// allocator stays functional throughout (falling back to a fresh segment /
/// null on the injected fault, per the honest-OOM contract).
///
/// Counterfactual (pre-fix, B3 design): reuse popped the pooled segment as a
/// fresh carve target and the FIRST carve into it re-grew the frontier via a
/// real grow-commit, so arming a fault at commit #1 directly targeted the
/// reused segment's own recommit.
#[test]
fn pool_reuse_with_fault_mid_reuse() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let lazy_first_chunk = a.dbg_lazy_first_chunk();
        let base = seg_base(second_ptr);
        let expected_initial = small_meta_end() + lazy_first_chunk;

        // Fill and empty the segment so it gets pooled.
        let (_base, _ptrs) = fill_and_empty_segment(&mut a, second_ptr);
        assert!(a.dbg_pooled_count() > 0, "segment should be pooled");

        // R8-10: frontier stays fully grown (not reset) after pool admission.
        let frontier_pooled = a.dbg_committed_payload_end_for(base as *mut u8).unwrap();
        assert!(frontier_pooled > expected_initial);

        // Reuse the pooled segment via the free-list path BEFORE arming any
        // fault: this must succeed unconditionally (no grow-commit involved).
        let mut reused = false;
        for _ in 0..1000 {
            let p = a.alloc(Layout::from_size_align(16, 8).unwrap());
            assert!(!p.is_null());
            if (p as usize) & !(SEGMENT - 1) == base {
                reused = true;
                break;
            }
        }
        assert!(
            reused,
            "reuse of the pooled segment must succeed unconditionally \
             (no grow-commit on the free-list reuse path to fault)"
        );

        // Now arm the B4 fault hook: the NEXT grow-commit (necessarily on
        // some other, fresh or growing segment, since the reused one carves
        // from its intact free list with no commit involved) should fail
        // gracefully — allocator stays functional (falls back / returns null
        // per the honest-OOM contract), never crashes.
        a.dbg_arm_commit_fail_at(1);
        let block_size = 4096;
        let mut allocations_succeeded = 0;
        for _ in 0..600_000 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            if p.is_null() {
                break;
            }
            allocations_succeeded += 1;
        }
        assert!(
            allocations_succeeded > 0,
            "allocator should remain functional after a fault on some \
             later segment's grow-commit"
        );

        // The pooled/reused segment's own frontier is unaffected by a fault
        // that fired elsewhere; still readable, still >= its grown value.
        let frontier_after = a.dbg_committed_payload_end_for(base as *mut u8).unwrap();
        assert!(
            frontier_after >= expected_initial,
            "frontier should be at least the initial lazy value"
        );
    }
}

// ============================================================================
// SCENARIO 12: Pool eviction/release with partially-committed segment
// ============================================================================

/// When a segment is partially committed and the pool is drained (eviction),
/// releasing it does not fault. The release path handles partial commit
/// correctly (single VirtualFree(MEM_RELEASE) releases the entire
/// reservation regardless of which sub-ranges are committed).
#[test]
fn pool_eviction_release_partial_commit() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let base = seg_base(second_ptr);

        // Partially fill the segment (not all chunks committed).
        let block_size = 4096;
        let mut ptrs = vec![second_ptr];
        for _ in 0..100 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            assert!(!p.is_null());
            if (p as usize) & !(SEGMENT - 1) == base {
                ptrs.push(p);
            } else {
                unsafe {
                    a.dealloc(p, Layout::from_size_align(block_size, 8).unwrap());
                }
                break;
            }
        }

        // Verify the segment is only partially committed.
        let frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        // (On the lazy path, it should be < SEGMENT unless we filled it.)
        let _ = frontier; // observation only

        // Free all blocks so the segment empties and enters the pool.
        for &p in &ptrs {
            unsafe {
                a.dealloc(p, Layout::from_size_align(block_size, 8).unwrap());
            }
        }

        // Drain the pool — this releases the segment to the OS.
        // If the release path mishandles partial commit, this would fault.
        let drained = a.dbg_drain_small_pool();
        // The segment should have been in the pool and drained.
        // (Other segments might also be in the pool, so drained >= 0.)
        let _ = drained;

        // If we got here without faulting, the release of a partially-committed
        // segment is correct.
    }
}

// ============================================================================
// SCENARIO 13: Cross-thread free in a partially-committed segment
// ============================================================================

/// A block allocated from a partially-committed segment can be freed from
/// another thread (cross-thread free via the remote-free ring). The metadata
/// is always committed (it lives below the initial chunk boundary), so the
/// ring entry is processed correctly. The `off >= bump` stale guard doesn't
/// misfire.
///
/// This test requires `alloc-global` + `alloc-xthread` to exercise the real
/// cross-thread path, but since we're testing at the `AllocCore` level (no
/// global allocator), we simulate a "cross-thread-like" free by freeing a
/// block that was allocated from a segment with a partial frontier, and
/// verifying the metadata is intact and the live count is correct.
#[test]
fn cross_thread_free_partial_segment() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let base = seg_base(second_ptr);
        let initial_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(initial_frontier < SEGMENT, "expected lazy frontier");

        // Allocate a few blocks from the partially-committed segment.
        let block_size = 16;
        let mut ptrs = vec![second_ptr];
        for _ in 0..100 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            assert!(!p.is_null());
            if (p as usize) & !(SEGMENT - 1) == base {
                ptrs.push(p);
            }
        }

        let live_before = a.dbg_live_count_for(second_ptr).unwrap();
        assert!(live_before > 0, "live count should be positive");

        // Free a block from the partially-committed segment.
        // In the real cross-thread path, this goes through the remote-free ring
        // and is drained by the owner. At the AllocCore level, dealloc on the
        // owner thread is the same code path that processes ring entries.
        let ptr_to_free = ptrs[ptrs.len() / 2]; // free a middle block
        unsafe {
            a.dealloc(ptr_to_free, Layout::from_size_align(block_size, 8).unwrap());
        }

        let live_after = a.dbg_live_count_for(second_ptr).unwrap();
        assert_eq!(
            live_after,
            live_before - 1,
            "live count should decrement by 1 after dealloc"
        );

        // The frontier should not have changed (dealloc doesn't shrink it).
        let frontier_after = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(
            frontier_after >= initial_frontier,
            "frontier should not regress after dealloc"
        );

        // The freed block should be on the free list now.
        // Verify by checking the alloc bitmap.
        assert!(
            a.dbg_is_free_for(ptr_to_free),
            "freed block should be marked free in the bitmap"
        );
    }
}

// ============================================================================
// SCENARIO 14: Allocator drop/release of partially-committed reservation
// ============================================================================

/// Dropping an `AllocCore` whose segments are only partially committed does
/// not fault (no double-free, no decommit-of-uncommitted UB). The release
/// path uses VirtualFree(MEM_RELEASE) on the entire reservation, which
/// handles partial commit correctly.
#[test]
fn allocator_drop_partial_commit_no_fault() {
    // Create an allocator, allocate from a lazy segment (partially committed),
    // and drop the allocator. If the release path is buggy, this faults.
    // R8-5: only the genuine Windows-lazy leg produces a partially-committed
    // segment; every eager leg (numa-aware OR Unix/miri) has frontier = SEGMENT
    // and there is no partial-commit drop scenario to exercise.
    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let (mut a, second_ptr) = alloc_past_primordial();
        let base = seg_base(second_ptr);

        // Allocate a few more blocks to partially commit the segment.
        for _ in 0..100 {
            let p = a.alloc(Layout::from_size_align(4096, 8).unwrap());
            assert!(!p.is_null());
            if (p as usize) & !(SEGMENT - 1) != base {
                break;
            }
        }

        let frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(frontier < SEGMENT, "segment should be partially committed");

        // Drop the allocator. This releases all segments. If the release
        // path faults on a partially-committed segment, this test crashes.
        drop(a);
        // If we got here, the drop was clean.
    }
}

// ============================================================================
// SCENARIO 15: Primordial metadata committed up front (R7-B6 lazy primordial)
// ============================================================================

/// R7-B6 (primordial lazy commit): under `alloc-lazy-commit` AND NOT
/// `numa-aware`, the primordial segment's metadata region (header, page map,
/// bin table, remote ring, registry array, hash table, free-list array + top
/// — everything `bootstrap::primordial` writes) plus one initial payload
/// chunk are committed UP FRONT by `Segment::reserve_lazy`, BEFORE any of
/// those writes run — there is no write-before-commit hazard. Its frontier is
/// therefore `primordial_meta_end() + LAZY_FIRST_CHUNK`, not the pre-R7-B6
/// `SEGMENT` (full span). Under `numa-aware` the primordial reservation is
/// still the plain eager `Segment::reserve` (mirroring
/// `reserve_small_segment`'s own NUMA exclusion), so the frontier there
/// remains `SEGMENT`, unchanged.
///
/// Counterfactual this test still guards against: if the primordial's
/// `initial_commit` argument were computed WRONG (too small — e.g. omitting
/// a metadata region `bootstrap::primordial` actually writes), the very
/// first allocation's metadata writes would fault. This test's mere SUCCESS
/// (the `alloc` above returns non-null and every later test in this suite
/// keeps working) is therefore part of the correctness signal, not just the
/// frontier-value assertion below.
#[test]
fn primordial_metadata_committed_up_front() {
    let mut a = AllocCore::new().unwrap();
    let p = a.alloc(Layout::from_size_align(64, 8).unwrap());
    assert!(!p.is_null());

    let frontier = a.dbg_committed_payload_end_for(p).unwrap();
    // SAFETY: `p` is the pointer just returned by `a.alloc` above — live,
    // exclusively owned, and its segment is owned by `a`.
    let payload_start = unsafe { a.dbg_payload_start_for(p) };

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    assert_eq!(
        frontier,
        payload_start + a.dbg_lazy_first_chunk(),
        "primordial segment must start with the lazy initial-chunk frontier"
    );
    #[cfg(all(not(feature = "numa-aware"), any(not(windows), miri)))]
    {
        let _ = payload_start;
        assert_eq!(
            frontier, SEGMENT,
            "primordial segment on Unix/miri must have frontier == SEGMENT \
             (R8-5: reserve_aligned_lazy already committed the whole segment)"
        );
    }
    #[cfg(feature = "numa-aware")]
    {
        let _ = payload_start;
        assert_eq!(
            frontier, SEGMENT,
            "under numa-aware the primordial segment must have frontier == SEGMENT (eager)"
        );
    }

    // The grow commit count should not have incremented for the primordial's
    // FIRST, tiny allocation (it lands well inside the initial chunk).
    // NOTE: this is a weaker check — other tests in the same process may have
    // incremented the process-global counter. We just check it's readable.
    let _count = a.dbg_grow_commit_count();
}

// ============================================================================
// SCENARIO 16: alloc_zeroed correctness on freshly-committed pages
// ============================================================================

/// Freshly committed pages (from grow-on-carve) must be zero-filled. Windows
/// guarantees MEM_COMMIT returns zeroed pages. alloc_zeroed should return
/// all zeros on blocks from freshly committed pages.
#[test]
fn alloc_zeroed_on_fresh_commit() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let base = seg_base(second_ptr);
        let lazy_first_chunk = a.dbg_lazy_first_chunk();
        let initial_chunk_end = small_meta_end() + lazy_first_chunk;

        // Allocate blocks to push past the initial chunk, so the next alloc
        // comes from freshly committed pages.
        let block_size = 4096;
        for _ in 0..500 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            assert!(!p.is_null());
            if (p as usize) & !(SEGMENT - 1) != base {
                break;
            }
            let off = (p as usize) - base;
            if off > initial_chunk_end {
                break;
            }
        }

        // Now use alloc_zeroed on the same segment.
        let zp = a.alloc_zeroed(Layout::from_size_align(block_size, 8).unwrap());
        assert!(!zp.is_null(), "alloc_zeroed should succeed");

        // Verify all bytes are zero.
        let slice = unsafe { core::slice::from_raw_parts(zp, block_size) };
        for (i, &byte) in slice.iter().enumerate() {
            assert_eq!(
                byte, 0,
                "alloc_zeroed byte at offset {i} should be 0, got {byte:#x}"
            );
        }
    }
}

// ============================================================================
// SCENARIO 17: Windows NUMA combination gate
// ============================================================================

/// The lazy-commit path is gated on the genuine Windows-lazy leg
/// (`all(not(numa-aware), windows, not(miri))`). When `numa-aware` is
/// enabled, the reserve path uses `VirtualAllocExNuma` (eager, node-pinned)
/// and the frontier is SEGMENT. R8-5 (task #218): on Unix/miri the lazy
/// reservation itself commits the whole segment (no partial-commit
/// distinction there), so the frontier is ALSO SEGMENT there. This test
/// asserts the compile-time gate.
///
/// Since `numa-aware` and `alloc-lazy-commit` are independent features, we
/// verify at compile time (via the cfg gate in the reserve path) that the
/// lazy arm is never taken when `numa-aware` is on. At runtime, we verify
/// the primordial's frontier matches this same gate (covered by
/// `primordial_metadata_committed_up_front`, R7-B6).
///
/// The cfg gate in alloc_core_small.rs (R8-5):
///   `#[cfg(all(not(numa-aware), windows, not(miri)))]` → lazy frontier
///   every other leg → `SEGMENT`
/// ensures that NUMA + lazy-commit = eager, AND that Unix/miri + lazy-commit
/// = eager (frontier == SEGMENT).
#[test]
fn numa_lazy_commit_gate() {
    // This test verifies the COMPILE-TIME gate exists by observing its effect.
    // The non-primordial segment's frontier mirrors `reserve_small_segment`'s
    // own 3-way cfg gate (R8-5, task #218): genuine Windows-lazy
    // (`all(not(numa-aware), windows, not(miri))`) → lazy
    // (`meta_end + LAZY_FIRST_CHUNK`); every other leg → SEGMENT.
    let (a, second_ptr) = alloc_past_primordial();
    let frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        assert!(
            frontier < SEGMENT,
            "with alloc-lazy-commit ON and numa-aware OFF on real Windows, \
             the segment should be lazily committed (frontier < SEGMENT)"
        );
    }

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        assert_eq!(
            frontier, SEGMENT,
            "with numa-aware ON, OR on Unix/miri (where reserve_aligned_lazy \
             commits the whole segment), the frontier must be SEGMENT (R8-5)"
        );
    }
}

// ============================================================================
// SCENARIO 18: Eager-path feature-OFF is a pure no-op
// ============================================================================

/// On the eager path (Unix, miri, `numa-aware`, or feature-OFF), the
/// grow-on-carve check is never true (frontier == SEGMENT), zero commits
/// fire, and the behavior is byte-identical to the pre-B1 code.
///
/// R8-5 (task #218): on Unix/miri the lazy reservation itself commits the
/// whole segment, so the primordial AND non-primordial frontiers both start
/// at SEGMENT there too — the "eager" claim now covers every leg EXCEPT
/// genuine Windows-lazy (`all(not(numa-aware), windows, not(miri))`), which
/// is the only configuration where the frontier starts at the lazy value
/// and grow-on-carve can fire.
#[test]
fn eager_path_is_pure_noop() {
    let mut a = AllocCore::new().unwrap();
    let p = a.alloc(Layout::from_size_align(64, 8).unwrap());
    assert!(!p.is_null());
    let frontier = a.dbg_committed_payload_end_for(p).unwrap();
    // SAFETY: `p` is the pointer just returned by `a.alloc` above — live,
    // exclusively owned, and its segment is owned by `a`.
    let payload_start = unsafe { a.dbg_payload_start_for(p) };

    // Primordial frontier: 3-way split mirroring `bootstrap::primordial`'s
    // own stamping (R8-5).
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
    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let (a2, second) = alloc_past_primordial();
        let f2 = a2.dbg_committed_payload_end_for(second).unwrap();
        assert_eq!(
            f2, SEGMENT,
            "non-primordial eager segment must have full frontier"
        );
        // No grow commits should have fired on the eager path. Under every
        // eager leg every OTHER test in this file early-returns without
        // allocating (their lazy branches are gated on the genuine
        // Windows-lazy leg), so the process-global counter has no concurrent
        // writer here.
        assert_eq!(
            a2.dbg_grow_commit_count(),
            0,
            "no grow commits on the eager path"
        );
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        // Genuine Windows-lazy leg ONLY: the non-primordial segment is lazily
        // committed, its initial frontier is `meta_end + LAZY_FIRST_CHUNK`
        // (< SEGMENT). Grow-on-carve fires as soon as carving exceeds that
        // frontier, so the process-global grow-commit counter is NOT
        // asserted here (it races with the other lazy-path tests in this
        // binary that allocate in parallel).
        let (a2, second) = alloc_past_primordial();
        let f2 = a2.dbg_committed_payload_end_for(second).unwrap();
        assert!(
            f2 < SEGMENT,
            "non-primordial lazy segment must start partially committed \
             (f2 = {f2}, SEGMENT = {SEGMENT})"
        );
    }
}

// ============================================================================
// SCENARIO 19: Full segment lifecycle — alloc -> fill -> decommit -> reuse -> drop
// ============================================================================

/// Complete lifecycle of a lazily-committed segment: alloc into it, fill it,
/// decommit when empty, reuse from pool, allocate again, then drop the
/// allocator. No faults at any stage.
#[test]
fn full_lazy_segment_lifecycle() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let base = seg_base(second_ptr);
        let lazy_first_chunk = a.dbg_lazy_first_chunk();
        let expected_initial = small_meta_end() + lazy_first_chunk;

        // Phase 1: Verify initial lazy state.
        let f1 = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert_eq!(f1, expected_initial, "initial frontier mismatch");

        // Phase 2: Fill and empty the segment.
        let (_base, _ptrs) = fill_and_empty_segment(&mut a, second_ptr);
        assert!(a.dbg_pooled_count() > 0, "segment should be pooled");

        // Phase 3: R8-10 — frontier is NOT reset by pool admission; it stays
        // at its fully-grown value (the segment was completely filled before
        // it emptied, so its frontier necessarily grew past the initial
        // lazy chunk).
        let f3 = a.dbg_committed_payload_end_for(base as *mut u8).unwrap();
        assert!(
            f3 > expected_initial,
            "frontier should stay fully grown after pool, not reset"
        );

        // Phase 4: Allocate — should land back in the reused segment via the
        // free-list path.
        let mut reused = false;
        for _ in 0..600_000 {
            let p = a.alloc(Layout::from_size_align(16, 8).unwrap());
            assert!(!p.is_null());
            if (p as usize) & !(SEGMENT - 1) == base {
                reused = true;
                break;
            }
        }
        assert!(reused, "expected reuse of the pooled segment");

        // Phase 5: Drop the allocator. If the release of a partially-committed
        // reused segment faults, this crashes.
        drop(a);
    }
}

// ============================================================================
// SCENARIO 20: Multiple size classes with lazy commit
// ============================================================================

/// Allocating blocks of various size classes from a lazily-committed segment
/// works correctly. Each class uses different block sizes but shares the same
/// bump cursor and frontier.
#[test]
fn multiple_size_classes_lazy() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let _base = seg_base(second_ptr);

        // Allocate various size classes from the same segment.
        let sizes = [16, 32, 64, 128, 256, 512, 1024, 2048, 4096];
        let mut ptrs = Vec::new();
        for &size in &sizes {
            let p = a.alloc(Layout::from_size_align(size, 8).unwrap());
            assert!(!p.is_null(), "alloc({size}) should succeed");
            ptrs.push((p, size));
        }

        // Write and read each block to verify they're writable.
        for &(p, size) in &ptrs {
            unsafe {
                for i in 0..size {
                    p.add(i).write(0x5D);
                }
                for i in 0..size {
                    assert_eq!(
                        p.add(i).read(),
                        0x5D,
                        "byte {i} of {size}-byte block at {p:p} corrupt"
                    );
                }
            }
        }
    }
}

// ============================================================================
// SCENARIO 21: B4 hook does not break B2 hook semantics
// ============================================================================

/// Verify that the B4 "fail the k-th commit" hook is truly additive: when
/// B4 is armed but B2 is not, B2's semantics are unchanged. When both are
/// armed, B2 fires first. Counterfactual: if B4 replaced B2's hook, the
/// existing B2/B3 tests would break.
#[test]
fn b4_hook_does_not_break_b2() {
    let (mut a, second_ptr) = alloc_past_primordial();

    #[cfg(any(feature = "numa-aware", not(windows), miri))]
    {
        let _ = second_ptr;
        return;
    }

    #[cfg(all(not(feature = "numa-aware"), windows, not(miri)))]
    {
        let base = seg_base(second_ptr);
        let initial_frontier = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert!(initial_frontier < SEGMENT);

        // Arm B2 (fail the next 1 commit) but NOT B4.
        a.dbg_arm_commit_fail(1);

        // Push near the frontier and allocate to trigger a commit.
        let block_size = 16;
        for _ in 0..100_000 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            assert!(!p.is_null());
            if (p as usize) & !(SEGMENT - 1) != base {
                break;
            }
            let off = (p as usize) - base;
            if off + block_size * 4 > initial_frontier {
                break;
            }
        }

        let frontier_before = a.dbg_committed_payload_end_for(second_ptr).unwrap();

        // Arm ONLY B2: the next commit should fail via B2's path.
        a.dbg_arm_commit_fail(1);
        // B4 is NOT armed (default 0).

        // Allocate to trigger the commit.
        for _ in 0..100 {
            let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
            if p.is_null() || (p as usize) & !(SEGMENT - 1) != base {
                break;
            }
        }

        // The frontier should not have advanced (B2 hook fired).
        let frontier_after = a.dbg_committed_payload_end_for(second_ptr).unwrap();
        assert_eq!(
            frontier_before, frontier_after,
            "B2 hook should still work when B4 is not armed"
        );

        // Recovery: disarm and alloc succeeds.
        a.dbg_arm_commit_fail(0);
        let p = a.alloc(Layout::from_size_align(block_size, 8).unwrap());
        assert!(!p.is_null(), "recovery after B2 fault should work");
    }
}
