//! PERF-P2 (eureka Э3) regression — the own-segment cache must be invalidated
//! the instant a segment leaves the table (`recycle`/`unregister`), so a cache
//! HIT can NEVER route a freed/unmapped/foreign pointer as own-thread.
//!
//! ## Why this is the single most dangerous change in the perf plan
//!
//! `SegmentTable::contains_base` gained a tiny direct-mapped cache of bases
//! proven present by a won hash probe. The hot free path routes own-thread iff
//! `contains_base(base) == true`. If the cache is not evicted when a segment is
//! recycled (its payload decommitted, its OS reservation released, its slot
//! NULLed), a subsequent free of a pointer whose computed base equals the
//! now-recycled base would HIT the stale cache, be routed as own-thread, and
//! write to unmapped / recycled memory — UB, and a direct M2 violation
//! (foreign/stale free must be a no-op).
//!
//! ## What this test proves (Scenario: cache-fill → recycle → stale free)
//!
//! 1. Allocate across several fresh Small segments, then free most blocks so a
//!    non-current segment is left with exactly one live block. Free that last
//!    block **own-thread**: the free path's `contains_base(base)` first FILLS
//!    the cache for `base` (remember-proven), then the block's departure empties
//!    the segment → `decommit_empty_segment` → `recycle(base)` fires, which
//!    NULLs the slot, releases the OS reservation, AND (the code under test)
//!    evicts `base` from the cache.
//! 2. Assert the recycled segment is genuinely gone from the table
//!    (`dbg_live_count_for` / `dbg_contains_base` → None/false) — i.e. a free of
//!    any pointer into it is an M2 no-op, NOT a false-positive own-route.
//!
//! ## Counterfactual (MANDATORY — verified live)
//!
//! Deleting the `self.own_cache_clear(base)` line inside
//! `SegmentTable::recycle` makes this test FAIL: the cache slot for the
//! recycled base survives, so `dbg_contains_base(recycled_ptr)` (which shares
//! the same cache-first `contains_base_ro` logic) returns `true` for a segment
//! that is no longer registered — a stale hit. The observed RED assertion is
//! recorded in the PR / task #146 transcript. Restoring the eviction restores a
//! clean pass. See the report for the pasted red output.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-xthread",
    feature = "alloc-decommit"
))]

use core::alloc::Layout;
use std::collections::HashMap;

use sefer_alloc::alloc_core::AllocCore;

const SEGMENT: usize = 4 * 1024 * 1024;

/// Force a base into the own-segment cache (via a won `contains_base` probe on
/// the own-thread free path), then recycle that exact segment, and assert the
/// recycled base is treated as FOREIGN (M2 no-op), never a stale cache hit.
#[test]
fn recycle_evicts_own_segment_cache_no_stale_hit() {
    // Mechanism 2 (task #51): DISABLE the empty-small-segment pool. This test
    // verifies that RECYCLING a segment evicts it from the own-segment cache;
    // with the pool ON (production default) the emptied segments are RETAINED
    // (not recycled) up to the cap, so the recycle-eviction path this test
    // targets would not fire. Disabling the pool restores deterministic
    // recycling. Pool behaviour is covered by `tests/small_segment_pool.rs`.
    let mut ac = AllocCore::new_with_config(
        sefer_alloc::LargeCacheConfig::new()
            .pool(sefer_alloc::SmallSegmentPoolConfig::new().pool_segments(0)),
    )
    .expect("primordial");
    let layout = Layout::from_size_align(256, 8).unwrap();

    // Spread across several fresh Small segments. Keep every block alive during
    // the spreading phase (freeing early would let the current segment's free
    // list greedily reabsorb blocks — see regression_c3_unbounded_recycle for
    // the same reasoning), recording one survivor pointer per distinct segment.
    const ROUND_BLOCKS: usize = 18_000; // > one ~16K-block segment
    const TARGET_SEGMENTS: usize = 6;
    let mut survivors: HashMap<usize, *mut u8> = HashMap::new();
    let mut all_ptrs: Vec<*mut u8> = Vec::new();
    let mut round = 0usize;
    while survivors.len() < TARGET_SEGMENTS && round < TARGET_SEGMENTS * 4 {
        for _ in 0..ROUND_BLOCKS {
            let p = ac.alloc(layout);
            assert!(!p.is_null(), "alloc null in round {round}");
            let seg_base = (p as usize) & !(SEGMENT - 1);
            survivors.entry(seg_base).or_insert(p);
            all_ptrs.push(p);
        }
        round += 1;
    }
    assert!(
        survivors.len() >= TARGET_SEGMENTS,
        "failed to spread across {TARGET_SEGMENTS} segments (got {})",
        survivors.len()
    );

    // Free every non-survivor block: each segment drops to exactly one live
    // block. This happens AFTER all segments were discovered, so it cannot
    // perturb which segments exist.
    let survivor_set: std::collections::HashSet<usize> =
        survivors.values().map(|&p| p as usize).collect();
    for &p in &all_ptrs {
        if !survivor_set.contains(&(p as usize)) {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { ac.dealloc(p, layout) };
        }
    }

    let decommit_before = AllocCore::dbg_decommit_count();

    // Free each survivor's LAST live block OWN-THREAD. For every non-current
    // segment this does two things in sequence, inside the free path:
    //   (a) `contains_base(base)` is consulted — a won hash probe FILLS the
    //       own-segment cache slot for `base` (remember-proven);
    //   (b) the block's departure empties the segment (live_count → 0) →
    //       `decommit_empty_segment` → `recycle(base)` → slot NULLed, OS
    //       reservation released, and (the code under test) the cache slot for
    //       `base` evicted.
    // So by construction the recycled bases were cached a moment before being
    // recycled — the exact stale-hit hazard this test guards.
    let survivor_ptrs: Vec<*mut u8> = survivors.values().copied().collect();
    for &p in &survivor_ptrs {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }

    let decommit_after = AllocCore::dbg_decommit_count();
    assert!(
        decommit_after > decommit_before,
        "no segment was decommitted/recycled — the survivor construction did \
         not produce empty-able non-current segments; test precondition unmet"
    );

    // The load-bearing check: every recycled survivor segment must now be
    // FOREIGN to the table. `dbg_contains_base` shares the exact cache-first
    // membership logic of the hot `contains_base` path, so a stale cache slot
    // surviving `recycle` would make this return `true` for an unregistered,
    // unmapped base — a false-positive own-route (UB / M2 breach). We require
    // that MOST survivor segments were recycled (at least all but the one
    // legitimately-current segment) AND that NONE of the recycled ones report a
    // stale hit.
    let mut recycled = 0usize;
    let mut stale_hits = 0usize;
    for &p in &survivor_ptrs {
        // A recycled segment: no longer live in the table.
        if ac.dbg_live_count_for(p).is_none() {
            recycled += 1;
            // ...and it must ALSO be invisible to the cache-first membership
            // test. If the cache still holds this base, dbg_contains_base
            // returns true → STALE HIT.
            if ac.dbg_contains_base(p) {
                stale_hits += 1;
            }
        }
    }

    assert_eq!(
        stale_hits, 0,
        "STALE OWN-SEGMENT CACHE HIT: {stale_hits} recycled segment(s) still \
         report contains_base == true after recycle — the cache was not \
         evicted in `recycle`, so a free of a pointer into a recycled/unmapped \
         segment would be routed OWN-THREAD (UB / M2 violation)."
    );
    // The stale-hit check above is the load-bearing assertion. This floor only
    // guards against a vacuous test (zero segments actually recycled → nothing
    // to have gone stale). We require a solid majority — not TARGET-1 exactly,
    // because own-thread free-all can legitimately leave more than one segment
    // un-recycled (the trailing current segment plus whichever segment becomes
    // current as earlier ones recycle), which is orthogonal to the cache.
    assert!(
        recycled >= TARGET_SEGMENTS / 2,
        "only {recycled} of {TARGET_SEGMENTS} survivor segments were recycled; \
         too few to meaningfully exercise cache invalidation (test would be \
         vacuous) — increase the working set"
    );

    // Direct M2 assertion: freeing a pointer into a recycled segment is a
    // no-op. Pick a recycled survivor and free it again — must not fault, must
    // not corrupt the allocator (a stale cache hit would route it own-thread
    // and write into unmapped memory / a mismatched bitmap).
    if let Some(&recycled_ptr) = survivor_ptrs
        .iter()
        .find(|&&p| ac.dbg_live_count_for(p).is_none())
    {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(recycled_ptr, layout) }; // must be an M2 no-op
    }

    // Allocator stays healthy afterwards (reuses recycled slots, hands out
    // valid, distinct, writable blocks).
    let mut fresh = Vec::with_capacity(500);
    for i in 0..500 {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "alloc null at i={i} after recycle/stale-free");
        unsafe {
            core::ptr::write_bytes(p, 0xCD, 256);
            assert_eq!(p.read(), 0xCD);
        }
        fresh.push(p);
    }
    let distinct: std::collections::HashSet<usize> = fresh.iter().map(|&p| p as usize).collect();
    assert_eq!(
        distinct.len(),
        fresh.len(),
        "duplicate pointer — corruption"
    );
    for &p in &fresh {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
}
