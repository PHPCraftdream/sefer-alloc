//! R17-4 (task #321) regression test for the pad/cache-admission anomaly
//! root-caused in `docs/perf/R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` §2.2.
//!
//! ## The bug this pins
//!
//! The fastbin magazine dealloc dispatch
//! (`HeapCore::dealloc_own_thread_with_base`, `src/registry/heap_core_free.rs`)
//! keys on `SizeClasses::class_for(layout.size())`, NOT on the segment's
//! `kind`. Under `medium-classes` (`SMALL_MAX` == 1 MiB), a Large segment can
//! LEGITIMATELY be freed with a layout whose size classifies small: R14-4's
//! medium→Large promotion (task #289) diverts a medium block to a dedicated
//! 4 MiB Large segment at the 256 KiB threshold, and OPT-G then grows that
//! block IN PLACE up to any size ≤ `SMALL_MAX` while it stays a Large segment
//! — so its dealloc layout (the post-grow size, exactly what the
//! `GlobalAlloc` contract requires) classifies small. Before the R17-4 fix,
//! such a free was misrouted into the small magazine path: the Large segment
//! never reached `AllocCore::dealloc`'s Large branch, was never deposited
//! into `large_cache` nor released, and LEAKED — so every subsequent
//! promotion reserved a fresh 4 MiB segment (the gate's `nopad`/`floor512kib`
//! 0-cache-hit / ~1 GiB-commit anomaly; `fixed2mib`'s 2 MiB dealloc layout
//! classified `None` and so accidentally took the correct substrate path).
//!
//! ## Red/green counterfactual
//!
//! Before the R17-4 fix this test FAILS: the grown-then-freed Large blocks
//! never deposit into `large_cache`, so `large_cache_hits` stays 0 and each
//! round leaks a fresh 4 MiB segment (`segments_reserved_total` grows ~1 per
//! round, far above the cache-reuse bound). After the fix the dealloc routes
//! by `kind` → Large branch → deposit → the next round's promotion reuses the
//! cached segment (`large_cache_hits` > 0, reserved delta bounded).
//!
//! ## Feature gating
//!
//! The assertion is meaningful only when the promotion path is compiled in
//! AND the `large_cache`/hit counter exist: `medium-classes`,
//! `alloc-decommit`, and `alloc-stats` (+ `alloc-global` for `SeferAlloc`).
//! `HAS_PROMOTION` mirrors `tests/r14_4_promotion_free_correctness.rs`'s
//! constant; when the R15-3 zero-headroom exclusion has compiled promotion
//! OUT (e.g. `--all-features`, where `numa-aware` defeats the
//! `large-reserved-capacity` exception), the test early-returns —
//! vacuous-but-passing, never red.
//!
//! Run under the bug's own config:
//!   cargo test --release --features "production medium-classes alloc-stats" --test r17_4_inplace_grown_large_dealloc_routes_by_kind -- --nocapture

#![cfg(all(
    feature = "alloc-global",
    feature = "medium-classes",
    feature = "alloc-decommit",
    feature = "alloc-stats"
))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::SeferAlloc;

const ALIGN: usize = 8;
const PROMOTION_THRESHOLD: usize = 256 * 1024;

/// Mirrors the promotion call site's `#[cfg]`
/// (`src/registry/heap_core_free.rs`): promotion compiles in unless
/// `exact-span-large` is tightening the span to zero headroom (unless
/// `large-reserved-capacity` restores it AND `numa-aware` is not overriding
/// it). When this is `false`, growth stays on the ordinary medium ladder and
/// the Large-misroute this test targets is not reachable — early-return.
#[allow(dead_code)]
const HAS_PROMOTION: bool = !cfg!(feature = "exact-span-large")
    || (cfg!(feature = "large-reserved-capacity") && !cfg!(feature = "numa-aware"));

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, ALIGN).unwrap()
}

/// A promoted-to-Large block, grown IN PLACE to a size that still classifies
/// as a medium class (≤ `SMALL_MAX` under `medium-classes`), must be freed via
/// the kind-keyed Large dealloc path — deposited into `large_cache` so the
/// next promotion reuses it. Before R17-4 the fastbin magazine dispatch
/// misrouted such a free on the *layout* size and the Large segment leaked.
#[test]
fn inplace_grown_large_freed_with_small_layout_is_reused_not_leaked() {
    if !HAS_PROMOTION {
        eprintln!("HAS_PROMOTION == false (R15-3 zero-headroom exclusion) — Large-misroute not reachable in this build; skipping");
        return;
    }

    let a = SeferAlloc::new();
    let stats_before = a.stats();

    const ROUNDS: usize = 12;
    // Final grown size: 512 KiB — comfortably under `SMALL_MAX` (1 MiB under
    // `medium-classes`), so its dealloc layout classifies small via
    // `class_for` (the exact misroute condition). It is also a named medium
    // class (EXTRAS), so the round-up is exact.
    const GROWN_SIZE: usize = 512 * 1024;
    // First grow target crosses the 256 KiB promotion threshold.
    const PROMOTE_SIZE: usize = PROMOTION_THRESHOLD + 68 * 1024; // 324 KiB

    for round in 0..ROUNDS {
        let start_layout = layout(64 * 1024);
        // SAFETY: valid, non-zero-size layout.
        let p = unsafe { a.alloc(start_layout) };
        assert!(!p.is_null(), "round {round}: initial alloc failed");

        // Grow across the promotion threshold -> promote to a 4 MiB Large
        // segment (R14-4). SAFETY: p live, start_layout matches, freed at most
        // once on success.
        let promoted = unsafe { a.realloc(p, start_layout, PROMOTE_SIZE) };
        assert!(
            !promoted.is_null(),
            "round {round}: promoting realloc failed"
        );

        // Grow the promoted Large block IN PLACE (OPT-G) to a size that still
        // classifies small — the block stays a Large segment but its dealloc
        // layout will now be `class_for(GROWN_SIZE) == Some`.
        // SAFETY: promoted live, layout matches, freed at most once on success.
        let grown = unsafe { a.realloc(promoted, layout(PROMOTE_SIZE), GROWN_SIZE) };
        assert!(
            !grown.is_null(),
            "round {round}: in-place grow realloc failed"
        );
        assert_eq!(
            grown, promoted,
            "round {round}: OPT-G in-place grow must return the same pointer"
        );

        // Free with the post-grow layout (`GlobalAlloc` contract). Before the
        // R17-4 fix this was misrouted by the magazine dispatch and the 4 MiB
        // Large segment leaked every round.
        let grown_layout = layout(GROWN_SIZE);
        // SAFETY: grown live, grown_layout matches, freed exactly once.
        unsafe { a.dealloc(grown, grown_layout) };
    }

    let stats_after = a.stats();
    let hits_delta = stats_after.large_cache_hits - stats_before.large_cache_hits;
    let reserved_delta = stats_after.segments_reserved_total - stats_before.segments_reserved_total;

    // PRIMARY signal (direct): the deposited Large segment must have been
    // reused by a later round's promotion. Before R17-4: 0 hits (every
    // promotion reserved fresh because the prior block's dealloc never
    // deposited). After: ROUNDS-1 hits (round 0 reserves+deposits; every
    // later round hits the cached segment).
    assert!(
        hits_delta > 0,
        "expected large_cache reuse after {ROUNDS} promote+in-place-grow+free \
         rounds, but large_cache_hits delta was {hits_delta} — the grown Large \
         block's dealloc is not reaching the kind-keyed Large free path \
         (R17-4 regression: fastbin magazine dispatch misroutes on layout size)"
    );

    // SECONDARY signal (leak bound): with the fix, distinct Large segments
    // reserved across all rounds are bounded by the cache (1 fresh for round
    // 0, reused thereafter) plus at most a couple of medium segments for the
    // initial 64 KiB allocs — well under ROUNDS. Before the fix this grew
    // ~1-per-round (every Large segment leaked). The bound is generous
    // (medium-segment churn from the superseded 64 KiB allocs can add a few)
    // but far below the ROUNDS-shaped runaway the bug produced.
    assert!(
        reserved_delta < ROUNDS as u64,
        "{ROUNDS} rounds reserved {reserved_delta} segments — expected the \
         Large segment to be cached and reused (bounded well under {ROUNDS}); \
         a count near {ROUNDS} indicates each round leaked a fresh 4 MiB \
         Large segment (R17-4 regression)"
    );
}
