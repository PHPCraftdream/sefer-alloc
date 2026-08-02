//! R32-10 (task #501, F2) — coverage for the new
//! `CONTAINS_BASE_TIER1_HITS`/`CONTAINS_BASE_TIER1_MISSES` path-activation
//! oracle: `AllocCore::dbg_contains_base_tier1_hits` /
//! `dbg_contains_base_tier1_misses` (and their `HeapCore` delegations).
//!
//! Two counterfactual-shaped tests: one proves a REPEATED free of the SAME
//! recently-touched segment is observed as an all-hits run (the cache slot
//! stays warm across calls — the intended fast case); the other proves that
//! calling `SegmentTable::dbg_hash_contains_only` (the Tier-2-only bypass
//! hook used by the pre-existing R23-3 isolation gate) does NOT move these
//! new counters at all — confirming the counters are wired specifically to
//! `contains_base`'s own Tier-1 check, not to Tier-2 traffic in general
//! (a wiring bug that swapped which function increments would otherwise be
//! silently invisible: both hooks ultimately touch the same hash table).
//!
//! Both tests require `bench-internals` (the counters read 0 without it —
//! this is itself asserted as the gate's off-state, matching this project's
//! established "reads 0 unless the gating feature is on" convention for
//! every other diagnostic counter, e.g. `dbg_maybe_decay_guard_passed_count`).

#![cfg(all(feature = "alloc-core", feature = "bench-internals"))]

use core::alloc::Layout;
use sefer_alloc::alloc_core::AllocCore;

/// Repeated own-thread frees of blocks from the SAME segment must be
/// observed as Tier-1 HITS after the first touch establishes the cache
/// entry: `contains_base` is called by `AllocCore::dealloc`'s M2 defensive
/// check on every dealloc (see `alloc_core.rs`'s `dealloc` — the
/// single-threaded face still calls `contains_base` for the same ownership
/// guard `HeapCore::dealloc_routing` calls under `alloc-xthread`), so N
/// deallocs of blocks living in ONE segment (well within `OWN_CACHE_SIZE`)
/// must show `hits >= N-1` (the very first call may be a genuine miss if
/// this is the first-ever probe for that base; every call after the base is
/// cached must hit).
#[test]
fn repeated_same_segment_frees_are_observed_as_tier1_hits() {
    let mut ac = AllocCore::new().expect("primordial");
    AllocCore::dbg_reset_contains_base_tier1_counters();

    let layout = Layout::from_size_align(16, 8).unwrap();
    const N: usize = 32;
    let mut ptrs = Vec::with_capacity(N);
    for _ in 0..N {
        let p = ac.alloc(layout);
        assert!(!p.is_null());
        ptrs.push(p);
    }

    let hits_before = AllocCore::dbg_contains_base_tier1_hits();
    let misses_before = AllocCore::dbg_contains_base_tier1_misses();

    for &p in &ptrs {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — each
        // pointer was returned by a prior matching alloc above, is live, and
        // is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }

    let hits_after = AllocCore::dbg_contains_base_tier1_hits();
    let misses_after = AllocCore::dbg_contains_base_tier1_misses();
    let hits_delta = hits_after - hits_before;
    let misses_delta = misses_after - misses_before;

    assert_eq!(
        hits_delta + misses_delta,
        N as u64,
        "expected exactly one contains_base call per dealloc (N={N}); \
         hits_delta={hits_delta} misses_delta={misses_delta}"
    );
    // All N blocks live in the SAME (primordial) segment, well within
    // OWN_CACHE_SIZE. `AllocCore::alloc`'s own hot path does not call
    // `contains_base` at all (it is a dealloc-only ownership guard), so the
    // FIRST dealloc in this loop is this process's first-ever probe for the
    // primordial base and may be a genuine Tier-1 miss (the cache starts
    // all-null — see `SegmentTable::from_primordial`); every call AFTER that
    // first one must hit, since the base was just proven present and cached.
    assert!(
        misses_delta <= 1,
        "expected at most one cold-start Tier-2 fallback for the very first \
         probe of this segment's base, then all-hits thereafter \
         (misses_delta={misses_delta}, hits_delta={hits_delta})"
    );
    assert_eq!(
        hits_delta + misses_delta,
        N as u64,
        "sanity: every dealloc must produce exactly one contains_base call"
    );
    assert!(
        hits_delta >= (N as u64) - 1,
        "same-segment repeated frees must be Tier-1 hits after the first \
         cold-start probe (hits_delta={hits_delta}, N={N})"
    );
}

/// `SegmentTable::dbg_hash_contains_only` (exposed at `AllocCore` as
/// `dbg_hash_contains_only`) is the PRE-EXISTING R23-3 Tier-2-only bypass
/// hook: it calls `hash_contains` directly, deliberately skipping the
/// Tier-1 `own_cache` check `contains_base` performs. The new counters must
/// NOT move when this bypass hook is called — they are wired specifically
/// inside `contains_base`'s own body, not inside `hash_contains` itself (if
/// they had been wired into `hash_contains`, calling the bypass hook would
/// wrongly inflate `misses`, corrupting the hit-rate this oracle exists to
/// report honestly).
#[test]
fn hash_contains_only_bypass_hook_does_not_move_the_new_counters() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(16, 8).unwrap();
    let p = ac.alloc(layout);
    assert!(!p.is_null());

    // `dbg_hash_contains_only` takes a segment BASE (compared by value only,
    // never dereferenced -- see its own doc comment), not an arbitrary
    // in-segment pointer. `p` itself need not be a true segment base for
    // this test's actual claim ("calling this OTHER function never moves
    // the new Tier-1 counters") to hold -- the claim is about which
    // function's body increments the statics, independent of whether the
    // probe itself would find a match. `dbg_contains_base(p)` (which DOES
    // derive the base internally via `os::segment_base_of_ptr`) is used
    // first only to prove `p`'s segment is genuinely registered, i.e. that
    // this is a realistic, non-degenerate probe target.
    assert!(ac.dbg_contains_base(p), "p must be registered");

    AllocCore::dbg_reset_contains_base_tier1_counters();
    let hits_before = AllocCore::dbg_contains_base_tier1_hits();
    let misses_before = AllocCore::dbg_contains_base_tier1_misses();

    // Call the Tier-2-only bypass hook several times. It takes a BASE, not
    // an arbitrary pointer; `p` itself is not necessarily segment-aligned,
    // but `dbg_hash_contains_only` only compares the value against table
    // entries (no dereference), so passing `p` unmodified is a well-defined
    // (if guaranteed-`false`) call that still proves the counters are inert
    // for this function -- the interesting claim is "the new statics never
    // move from calling this OTHER function", which holds regardless of
    // whether the probe itself hits or misses inside hash_contains.
    for _ in 0..8 {
        let _ = ac.dbg_hash_contains_only(p);
    }

    let hits_after = AllocCore::dbg_contains_base_tier1_hits();
    let misses_after = AllocCore::dbg_contains_base_tier1_misses();
    assert_eq!(
        hits_after, hits_before,
        "dbg_hash_contains_only (the Tier-2-only bypass) must not move CONTAINS_BASE_TIER1_HITS"
    );
    assert_eq!(
        misses_after, misses_before,
        "dbg_hash_contains_only (the Tier-2-only bypass) must not move CONTAINS_BASE_TIER1_MISSES"
    );

    // SAFETY (R6-MS-1/2): `p` is a live allocation owned by `ac`, freed
    // exactly once here.
    unsafe { ac.dealloc(p, layout) };
}
