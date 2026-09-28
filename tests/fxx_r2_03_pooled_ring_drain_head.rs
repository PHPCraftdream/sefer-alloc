//! fxx R2-03 (P4, performance): regression test for
//! `AllocCore::drain_segment_ring`
//! (`src/alloc_core/small/alloc_core_small/find_segment.rs`).
//!
//! ## The bug
//!
//! On a `Decommitted { pooled: true }` outcome, `drain_segment_ring` used to
//! call `release_or_pool_empty_segment(base)` — which, for `pooled == true`,
//! leaves the segment live/registered/committed — and `return` BEFORE
//! `meta_for_ring.set_ring_drain_head(new_head)` ran. `ring.head` was already
//! advanced by the drain, but the owner-cached `ring_drain_head` in the
//! segment header stayed at its PRE-drain value. The next time this (still
//! live, possibly pooled-then-reused) segment is visited, the guard's
//! `ring.tail_relaxed() == cached_head` check sees a stale `cached_head` and
//! performs one full, wasted `ring.drain()` (Acquire pair + unconditional
//! `head.store(Release)`) before finally refreshing the cache — a redundant
//! drain, not a correctness bug (P4).
//!
//! ## What this test drives
//!
//! `dbg_drain_all_rings` (the crate's usual full-drain test hook) is
//! deliberately NOT used here: it is a separate, unconditional-drain
//! implementation that never touches `ring_drain_head` at all, so it cannot
//! exercise this cache-staleness regression by construction. Instead this
//! test drives the REAL production path — the O(S) linear-scan fallback
//! inside `find_segment_with_free_impl`, which calls `drain_segment_ring`
//! per segment — via the new `dbg_find_segment_with_free_for_test` forwarder
//! (`bench-internals`-gated, added alongside the fix).
//!
//! Scenario: spread allocations of the (feature-set-)largest small class
//! across 3 distinct segments (so the middle one, `target`, is guaranteed
//! non-current). Cross-thread-free every live block of `target` via
//! `dbg_push_to_ring` (never `dealloc`, which recycles immediately without
//! ever touching `drain_segment_ring`/the ring-head cache). Then run the
//! real scan via the forwarder: `target` empties, decommits, and — with the
//! default pool cap (4) never yet exhausted — is POOLED (`pooled: true`),
//! the exact outcome the bug leaves the cache stale on. Assert the cache
//! (`dbg_ring_drain_head_for_test`) equals the ring's real head.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;

use sefer_alloc::AllocCore;

const SEGMENT: usize = 4 * 1024 * 1024;

fn segment_base_of(p: *mut u8) -> usize {
    (p as usize) & !(SEGMENT - 1)
}

#[test]
fn pooled_decommit_refreshes_ring_drain_head_cache() {
    let mut ac = AllocCore::new().expect("primordial");

    // The largest small class in this build: keeps blocks-per-segment low
    // (a few dozen, well under the ring's 256-entry capacity) so spreading
    // across several segments needs only a modest number of allocations.
    let class_idx = AllocCore::dbg_small_class_count() - 1;
    let block_size = AllocCore::dbg_block_size(class_idx);
    let layout = Layout::from_size_align(block_size, 8).unwrap();

    // Every pointer this test allocates, grouped by segment base, in
    // first-seen order. `live_count` tracks exactly the blocks HANDED OUT
    // (not merely carved — a refill batch's un-popped extras net zero, see
    // `carve_block`'s doc), so this vector, per segment, is exactly that
    // segment's live set.
    let mut seg_order: Vec<usize> = Vec::new();
    let mut by_seg: std::collections::HashMap<usize, Vec<*mut u8>> =
        std::collections::HashMap::new();

    let mut guard = 0usize;
    while seg_order.len() < 3 {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "alloc failed while spreading segments");
        let base = segment_base_of(p);
        if !by_seg.contains_key(&base) {
            seg_order.push(base);
        }
        by_seg.entry(base).or_default().push(p);
        guard += 1;
        assert!(guard < 1_000_000, "failed to spread across 3 segments");
    }

    // `seg_order[0]` is the primordial (current at the very start);
    // `seg_order[2]` is current NOW (we just allocated into it); `seg_order[1]`
    // is therefore guaranteed non-current — the precondition
    // `dec_live_and_maybe_decommit` requires (`base == small_cur` is
    // rejected) alongside "must be `Small`, never `Primordial`" (also
    // satisfied: `seg_order[1]` is a plain small segment).
    let target_base = seg_order[1];
    let target_ptrs = by_seg
        .remove(&target_base)
        .expect("target segment recorded");
    assert!(!target_ptrs.is_empty(), "target segment has no live blocks");

    // Cross-thread-free EVERY live block of `target` via the ring —
    // deliberately NOT `dealloc` (own-thread free recycles the segment
    // immediately, never going through `drain_segment_ring`/the cache at
    // all, so it cannot exercise this regression).
    for &p in &target_ptrs {
        // SAFETY (R6-MS-4): `p` is a live allocation owned by `ac`, one
        // logical remote free per pointer; `p` is not re-issued or
        // separately dealloc'd before the drain this test triggers next.
        // `class_idx` is the block's actual class.
        assert!(
            unsafe { ac.dbg_push_to_ring(p, class_idx) },
            "ring push failed (ring overflow) — target segment has more live \
             blocks than the ring's capacity; reduce the block count or grow \
             the ring for this test"
        );
    }

    // Drive the REAL scan/drain path directly (deterministic — no need to
    // wait for `alloc()`'s own incidental free-list-miss scans to reach
    // `target`).
    let hit = ac.dbg_find_segment_with_free_for_test(class_idx);
    assert_eq!(
        hit.map(|p| p as usize),
        Some(target_base),
        "the linear scan must find `target` (every block just freed) as the hit"
    );

    // `target` must still be registered: this is the `pooled: true` leg of
    // `Decommitted` (default pool cap is 4, unexhausted here), which is
    // exactly the outcome the bug leaves the `ring_drain_head` cache stale
    // on.
    let (cached_head, real_head) = ac
        .dbg_ring_drain_head_for_test(target_ptrs[0])
        .expect("target segment must still be registered after a pooled decommit");

    assert_eq!(
        u64::from(cached_head),
        real_head,
        "fxx R2-03 regression: ring_drain_head cache ({cached_head}) does not \
         match the ring's real head ({real_head}) after a pooled decommit — \
         the cache was not refreshed before release_or_pool_empty_segment"
    );
}
