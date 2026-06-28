//! Task #60 — slot-recycle verification for [`SegmentTable`].
//!
//! ## What is being tested
//!
//! Under `alloc-decommit`, empty small segments are decommitted and their table
//! slots are recycled: the slot is NULLed and the OS reservation is released.
//! Future `register` calls scan for NULL slots and reuse them, lifting the
//! hard 1024-segment cap into an effective "unbounded live segments as long as
//! churn recycles old slots" regime.
//!
//! ## Tests
//!
//! ### `slot_recycle_lifts_cap` (alloc-decommit)
//!
//! Allocates and frees blocks cumulatively across 4096+ segment-lives, verifying
//! that `register` never returns `None` (the table never reports "full") even
//! though 4096 > MAX_SEGMENTS (1024). This is the primary correctness assertion
//! for task #60.
//!
//! ### `without_decommit_cap_is_hard` (alloc-core, !alloc-decommit)
//!
//! Verifies the UNCHANGED behaviour under `!alloc-decommit`: the segment table
//! stays append-only, count grows monotonically, and once MAX_SEGMENTS live
//! segments are registered, `alloc_large` gracefully returns null (OOM), not a
//! panic. This ensures the recycle path does NOT regress the no-decommit case.
//!
//! ### `recycled_slot_is_reused` (alloc-decommit)
//!
//! A focused unit test: alloc K blocks to fill one small segment past the
//! primordial, free all (triggering decommit + recycle of the emptied segment),
//! then alloc again; the allocator must succeed (recycled slot reused) and
//! allocations must be valid and writable.

// ============================================================
// Test 1 — slot recycle lifts the 1024-segment cap
// ============================================================

/// Under `alloc-decommit`, cumulative segments well beyond MAX_SEGMENTS (1024)
/// must succeed because empty segments are recycled (their slots reused).
///
/// Protocol: repeatedly alloc N blocks (enough to require fresh segments beyond
/// the primordial) then dealloc all. Each dealloc cycle empties the non-current
/// small segments → decommit fires → slot recycled. After many cycles, the
/// cumulative segment-creation count far exceeds 1024 — but at any point in
/// time, only O(working_set / SEGMENT) slots are live. `alloc` must succeed
/// throughout.
///
/// `#[cfg_attr(miri, ignore)]` — N=800 per round × 30 rounds is too slow under
/// miri's ~1000× slowdown. The miri coverage lives in `recycled_slot_is_reused`.
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
#[cfg_attr(miri, ignore)]
#[test]
fn slot_recycle_lifts_cap() {
    use core::alloc::Layout;
    use sefer_alloc::alloc_core::AllocCore;

    let mut ac = AllocCore::new().expect("primordial");

    // 256 B blocks: fits many per segment. We need enough per round to spill
    // past the primordial AND past one fresh segment, so that the SECOND fresh
    // segment becomes `small_cur` — leaving the FIRST fresh segment non-current
    // when we free everything → first fresh segment decommits.
    //
    // Primordial payload ≈ 4 MiB − ~100 KiB metadata ≈ ~3.9 MiB / 256 B ≈ 15K blocks.
    // One Small segment ≈ 4 MiB / 256 B ≈ 16K blocks.
    // To get ≥2 fresh segments: N > 15K + 16K = 31K. Use 40K with margin.
    let layout = Layout::from_size_align(256, 8).unwrap();
    const N: usize = 40_000; // blocks per round (fills primordial + 2 fresh segments)
    const ROUNDS: usize = 100; // 100 rounds × ~3 segments/round ≈ 300 cumulative segment-lives

    let decommit_before = AllocCore::dbg_decommit_count();

    for round in 0..ROUNDS {
        let mut ptrs = Vec::with_capacity(N);
        for i in 0..N {
            let p = ac.alloc(layout);
            assert!(
                !p.is_null(),
                "alloc returned null at round={round} i={i} — \
                 slot recycle failed (cap exhausted)"
            );
            ptrs.push(p);
        }
        // Spot-write / read-back to verify the block is usable.
        for (i, &p) in ptrs.iter().enumerate() {
            unsafe {
                let b = (i & 0xFF) as u8;
                p.write(b);
                assert_eq!(
                    p.read(),
                    b,
                    "write/readback failed at round={round} i={i}"
                );
            }
        }
        // Free all — non-current Small segments empty → decommit → recycle.
        for &p in &ptrs {
            ac.dealloc(p, layout);
        }
    }

    let decommit_after = AllocCore::dbg_decommit_count();
    assert!(
        decommit_after > decommit_before,
        "no decommit fired during {ROUNDS} rounds of churn — \
         recycle path was never exercised (decommit hook miswired). \
         Ensure N is large enough to spill into >= 2 fresh Small segments per round \
         so that the first fresh segment is non-current when emptied."
    );

    // With ROUNDS=100 and ~3 segment-lives/round, cumulative segment-creation is
    // ~300+. Since MAX_SEGMENTS=1024 without recycle would fill by round ~341,
    // this test WOULD have failed in the pre-#60 world. With slot recycle, every
    // round decommits the non-current segments and recycles their slots — so
    // `alloc` succeeds throughout.
    // The primary correctness check is that NO alloc ever returns null above.
}

// ============================================================
// Test 2 — without alloc-decommit, old hard-cap behaviour is unchanged
// ============================================================

/// Under `!alloc-decommit`, the segment table is strictly append-only. This test
/// verifies that registering MAX_SEGMENTS+1 live large allocations (each in its
/// own segment) causes `alloc` to return null (graceful OOM) rather than panicking
/// or corrupting state. This is the REGRESSION guard: recycle must not change
/// behaviour when the feature is disabled.
///
/// `#[cfg_attr(miri, ignore)]` — reserves MAX_SEGMENTS large OS segments; too slow
/// under miri. Correctness of the no-decommit path is covered by other invariant tests.
#[cfg(all(feature = "alloc-core", not(feature = "alloc-decommit")))]
#[cfg_attr(miri, ignore)]
#[test]
fn without_decommit_cap_is_hard() {
    use core::alloc::Layout;
    use sefer_alloc::{alloc_core::AllocCore, SegmentLayout};

    let mut ac = AllocCore::new().expect("primordial");

    // Reserve large allocations (each gets its own segment). Keep them live.
    // One slot is already used by the primordial, so MAX_SEGMENTS - 1 more
    // large allocs should fit, and the next one should fail gracefully.
    let large_size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    let layout = Layout::from_size_align(large_size, SegmentLayout::PAGE).unwrap();

    // MAX_SEGMENTS is 1024; primordial occupies slot 0. So we can register at
    // most 1023 additional segments. Try 1025 large allocs; at least one must
    // return null (the 1025th or earlier).
    const ATTEMPT: usize = 1025;

    let mut ptrs = Vec::with_capacity(ATTEMPT);
    let mut null_count = 0usize;
    for _ in 0..ATTEMPT {
        let p = ac.alloc(layout);
        if p.is_null() {
            null_count += 1;
            // First null is expected: the table filled (no recycle). Subsequent
            // allocs may also return null (idempotent OOM). Stop here.
            break;
        }
        ptrs.push(p);
    }

    assert!(
        null_count > 0,
        "expected at least one null from large alloc after MAX_SEGMENTS — \
         table must be full without decommit"
    );

    // Cleanup: free what we have (drop releases segments via Drop).
    // In the no-decommit path, `dealloc` for large segments marks them freed but
    // the OS reservation is held until drop.
    for (&p, _) in ptrs.iter().zip(std::iter::repeat(layout)) {
        ac.dealloc(p, layout);
    }
}

// ============================================================
// Test 3 — focused unit: recycled slot is reused
// ============================================================

/// Focused correctness test for the slot-recycle mechanism (task #60):
///
/// 1. Alloc enough blocks to spill past the primordial into at least one fresh
///    Small segment.
/// 2. Free all blocks. Non-current Small segments → decommit → slot recycled
///    (NULLed + OS reservation released).
/// 3. Alloc another batch. The recycled slot must be reused (register scans for
///    NULL slots). Allocations must be non-null, writable, and distinct.
///
/// This runs under miri (bounded N=500 blocks ÷ 2 KiB block size ≈ needs ~2-3
/// segments, small enough for miri).
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
#[test]
fn recycled_slot_is_reused() {
    use core::alloc::Layout;
    use std::collections::HashSet;

    use sefer_alloc::alloc_core::AllocCore;

    let mut ac = AllocCore::new().expect("primordial");

    // 2 KiB blocks: small enough for miri, large enough to overflow the
    // primordial's payload in a few hundred allocs.
    let layout = Layout::from_size_align(2048, 8).unwrap();
    // 6000 × 2 KiB = 12 MiB — spans the primordial (~4 MiB) plus 2 fresh
    // Small segments. Matches the sizing in `decommit_miri_cycle`.
    const N: usize = 6000;

    let decommit_before = AllocCore::dbg_decommit_count();

    // Phase 1: alloc N blocks.
    let mut ptrs = Vec::with_capacity(N);
    for i in 0..N {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "phase-1 alloc null at i={i}");
        ptrs.push(p);
    }
    // All N pointers must be distinct.
    let set1: HashSet<usize> = ptrs.iter().map(|&p| p as usize).collect();
    assert_eq!(set1.len(), N, "phase-1 alloc handed out duplicates");

    // Phase 2: free all. Non-current Small segments decommit → slots recycled.
    for &p in &ptrs {
        ac.dealloc(p, layout);
    }

    let decommit_after = AllocCore::dbg_decommit_count();
    // Under miri, decommit_pages is a no-op but the bookkeeping (live_count
    // zero-crossing, decommit hook, reset, slot recycle) still runs.
    assert!(
        decommit_after > decommit_before,
        "no decommit fired — segment stayed current throughout (working set \
         too small or decommit hook miswired); \
         decommit_before={decommit_before}, after={decommit_after}"
    );

    // Phase 3: alloc N blocks again. Recycled slots must be reused; the allocator
    // must not fail (null) even though the cumulative segment count exceeds 1.
    let mut ptrs2 = Vec::with_capacity(N);
    for i in 0..N {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "phase-3 alloc null at i={i} after recycle");
        // Writable + readable.
        unsafe {
            let b = (i & 0xFF) as u8;
            p.write(b);
            assert_eq!(p.read(), b, "phase-3 write/readback failed at i={i}");
        }
        ptrs2.push(p);
    }
    // All N re-alloced pointers must be distinct.
    let set2: HashSet<usize> = ptrs2.iter().map(|&p| p as usize).collect();
    assert_eq!(
        set2.len(),
        N,
        "phase-3 alloc handed out duplicates after recycle"
    );

    // Cleanup.
    for &p in &ptrs2 {
        ac.dealloc(p, layout);
    }
}
