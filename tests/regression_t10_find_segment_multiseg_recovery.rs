//! T10 (perf#1) — multi-segment free-recovery regression for
//! `find_segment_with_free`.
//!
//! `find_segment_with_free_impl` walks every owned segment `[0, count)` on a
//! free-list miss, lazily draining each segment's remote-free ring and
//! returning the first whose `BinTable` head for the requested class is
//! non-null. T10 explored replacing (or short-circuiting) this O(n) scan with a
//! per-class membership hint; that experiment was **NO-GO** (see
//! `docs/perf/IAI_BASELINE.md`'s T10 entry) and was reverted, leaving the scan
//! as-is. These tests were written to pin the invariant the experiment had to
//! preserve — and they remain valuable as guards on the EXISTING scan: any
//! future optimisation to this path must not violate them.
//!
//! Two invariants, both driven by `AllocCore` directly (single-threaded, no
//! magazine layer — `alloc_small`'s `find_segment_with_free` is the path
//! exercised):
//!
//! 1. **No missed segment** — a segment that HAS free space must always be
//!    found (never a false OOM or an unnecessary fresh-segment carve). Driven
//!    by freeing blocks across several segments and re-allocating the same set:
//!    no NEW segment may be carved (all segments are full after the fill, so a
//!    carve is only reachable via `find_segment_with_free` returning None — a
//!    missed segment).
//! 2. **No stranded free across segment-drain transitions** — as the scan
//!    drains one segment's class free list and moves to the next, every freed
//!    block must still be recoverable. Same no-new-segment assertion, under a
//!    different geometry (many more blocks per segment → many more drain
//!    transitions).
//!
//! Both use the largest small class (`SMALL_MAX`) so a modest allocation count
//! spans several segments. Segment boundaries are detected dynamically via
//! `segment_base_of` (the primordial segment hosts the registry, so it holds
//! fewer blocks than a normal segment — a fixed blocks-per-segment divisor
//! would be wrong).
//!
//! ## R6-OPT-P0-3a — pool-cap interaction with the medium-classes feature
//!
//! `find_segment_recovers_frees_across_segments_without_missed_segment`'s
//! "no NEW segment may be carved" assertion is only a scan-correctness
//! invariant when NO segment is genuinely RELEASED (to the OS) during the
//! free-all pass — a release is a legitimate, DIFFERENT reason for the base
//! set to change, unrelated to whether the scan found a segment's free
//! blocks. Under `alloc-decommit` (part of `production`, which this file's
//! CI-mandated feature matrix includes), an emptied segment is pooled up to
//! `SmallSegmentPoolConfig::DEFAULT_POOL_SEGMENTS` (4) and released beyond
//! that cap. `SMALL_MAX` classes ~15+ blocks per 4 MiB segment (pre-existing,
//! ~253 KiB), so a 48-allocation sweep spans only ~3-4 segments — always
//! within the pool cap, so no release ever fires and the test's invariant
//! holds by construction. `medium-classes` (R6-OPT-P0-3a) raises `SMALL_MAX`
//! to 1 MiB, which fits only ~3-4 blocks per segment — the SAME 48-allocation
//! sweep now spans ~13-16 segments, comfortably EXCEEDING the pool cap of 4,
//! so several segments are legitimately released during the free-all pass
//! and the base set genuinely changes on re-alloc — not a missed segment.
//! Confirmed empirically (a debug instrumentation pass, not kept in this
//! file, showed `pooled_count=4` (cap reached) and `released_delta=10` for a
//! 48×1-MiB sweep) and by the observed ~50% flaky-failure rate this
//! produced under `cargo test --all-features` before this fix.
//!
//! Fixed by explicitly configuring a pool cap large enough that NO segment
//! this test can produce is ever released, restoring the test's original
//! scan-only invariant regardless of which small-class geometry is in play.
//! `alloc-decommit`-gated (the config API only exists under that feature);
//! without `alloc-decommit` there is no pool/release at all, so the
//! plain `AllocCore::new()` path already carries no release hazard.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

/// The largest small class — chosen so a modest allocation count spans several
/// 4 MiB segments (the primordial segment holds fewer than a normal one due to
/// its self-hosted registry metadata).
const BLOCK: usize = SegmentLayout::SMALL_MAX;
/// Enough allocations to span at least 3 segments (primordial + 2+ fresh).
const SPAN_COUNT: usize = 48;
const LAYOUT: Layout = match Layout::from_size_align(BLOCK, 8) {
    Ok(l) => l,
    Err(_) => panic!("SMALL_MAX layout is valid"),
};

fn base_of(p: *mut u8) -> usize {
    SegmentLayout::segment_base_of(p as usize)
}

/// Build an `AllocCore` with a pool cap generously larger than any segment
/// count this test's sweeps can produce (`SPAN_COUNT` blocks of `SMALL_MAX`
/// size, worst case ~1 segment per block under `medium-classes`, plus the
/// `drain_transitions` test's 3072-block/4096-byte sweep spanning far more
/// segments than that but at ~1000+ blocks/segment so it never approaches
/// the default cap either way) — see the module doc's "pool-cap interaction"
/// section for why this is necessary specifically under `medium-classes`.
/// Under `alloc-decommit` this uses an explicit config; without it, plain
/// `AllocCore::new()` has no pool/release mechanism at all, so no
/// configuration is needed.
fn new_core_with_generous_pool() -> AllocCore {
    #[cfg(feature = "alloc-decommit")]
    {
        use sefer_alloc::{LargeCacheConfig, SmallSegmentPoolConfig};
        let cfg = LargeCacheConfig::new().pool(
            SmallSegmentPoolConfig::new()
                .pool_segments(4096)
                .pool_byte_cap(4096 * SegmentLayout::SEGMENT),
        );
        AllocCore::new_with_config(cfg).expect("AllocCore::new_with_config")
    }
    #[cfg(not(feature = "alloc-decommit"))]
    {
        AllocCore::new().expect("AllocCore::new")
    }
}

#[test]
fn find_segment_recovers_frees_across_segments_without_missed_segment() {
    // The "missed segment" case: if `find_segment_with_free` ever skipped a
    // segment that held freed blocks, those blocks would be stranded and the
    // re-allocations would fall through to `reserve_small_segment` (a brand-new
    // segment with a brand-new base). All segments are full after the fill, so
    // a carve is ONLY reachable via the scan returning None — a missed segment.
    // The segment-base set after re-alloc must therefore EQUAL the set before.
    let mut core = new_core_with_generous_pool();
    let mut alloced: Vec<*mut u8> = Vec::with_capacity(SPAN_COUNT);
    for _ in 0..SPAN_COUNT {
        let p = core.alloc(LAYOUT);
        assert!(!p.is_null());
        alloced.push(p);
    }
    // Sanity: the allocations really did span several segments.
    let mut freed_bases: Vec<usize> = alloced.iter().map(|&p| base_of(p)).collect();
    freed_bases.sort_unstable();
    freed_bases.dedup();
    assert!(
        freed_bases.len() >= 3,
        "expected >= 3 segments to exercise the multi-segment scan, got {}",
        freed_bases.len()
    );

    // Free everything — each block returns to its OWN segment's BinTable.
    for &p in &alloced {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, LAYOUT) };
    }
    // Re-allocate the same count. Every freed block must be recoverable.
    let mut realloced: Vec<*mut u8> = Vec::with_capacity(SPAN_COUNT);
    for _ in 0..SPAN_COUNT {
        let p = core.alloc(LAYOUT);
        assert!(
            !p.is_null(),
            "re-alloc returned null — a freed block was stranded (missed segment)"
        );
        assert_eq!((p as usize) % 8, 0, "re-alloc not aligned");
        realloced.push(p);
    }
    // The segment-base set must not grow: a new base means a fresh segment was
    // carved, which only happens when `find_segment_with_free` returned None
    // despite frees being available (a missed segment). (Pointer-set equality
    // is NOT a valid assertion here: `carve_block_with_refill` pre-populates a
    // freshly-carved segment's BinTable with up to 31 refill blocks the caller
    // never saw, so a re-alloc legitimately returns blocks never in the freed
    // set — but they still come from an EXISTING segment, so the base-set stays
    // unchanged.)
    let mut realloc_bases: Vec<usize> = realloced.iter().map(|&p| base_of(p)).collect();
    realloc_bases.sort_unstable();
    realloc_bases.dedup();
    assert_eq!(
        freed_bases, realloc_bases,
        "re-alloc carved a NEW segment (base set grew) — a segment's frees were \
         missed by find_segment_with_free"
    );
}

#[test]
fn find_segment_recovers_frees_through_segment_drain_transitions() {
    // The segment-drain-transition case. The free-all / realloc-all cycle
    // drains one segment's class free list, then the scan moves to the next,
    // and so on. If the scan ever failed to advance correctly (or a future
    // optimisation cached a stale "next segment" pointer), a freed block would
    // be stranded and a fresh segment carved. Same no-new-segment assertion,
    // under a geometry with many more blocks per segment (4096 B → 1024
    // blocks/segment) → many more drain transitions per segment.
    let mut core = new_core_with_generous_pool();
    let layout = Layout::from_size_align(4096, 8).unwrap();
    let total = 3072;
    let mut alloced: Vec<*mut u8> = Vec::with_capacity(total);
    for _ in 0..total {
        let p = core.alloc(layout);
        assert!(!p.is_null());
        alloced.push(p);
    }
    let mut bases: Vec<usize> = alloced.iter().map(|&p| base_of(p)).collect();
    bases.sort_unstable();
    bases.dedup();
    assert!(
        bases.len() >= 3,
        "expected >= 3 segments, got {}",
        bases.len()
    );

    for &p in &alloced {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
    }
    let mut realloced: Vec<*mut u8> = Vec::with_capacity(total);
    for _ in 0..total {
        let p = core.alloc(layout);
        assert!(!p.is_null(), "re-alloc returned null (stranded free)");
        realloced.push(p);
    }
    let mut realloc_bases: Vec<usize> = realloced.iter().map(|&p| base_of(p)).collect();
    realloc_bases.sort_unstable();
    realloc_bases.dedup();
    assert_eq!(
        bases, realloc_bases,
        "re-alloc carved a NEW segment — a freed block was stranded when the \
         scan drained a segment and advanced to the next"
    );
}
