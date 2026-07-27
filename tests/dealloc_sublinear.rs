//! Phase 13.4a regression gate: own-thread free must be **sub-quadratic**.
//!
//! ## What regressed (#41) and what this guards
//!
//! The Phase 8 double-free guard (`free_list_contains`) walked the class free
//! list on EVERY own-thread free — O(free-list length). Freeing N blocks of one
//! class into one segment grows that list 0→N, so the total free work is
//! `0+1+2+…+(N-1) = O(N²)`. On the 16 B churn bench that was ~1.9 ms vs
//! mimalloc's ~11 µs (~115× slower). Phase 13.4a replaces the walk with an O(1)
//! alloc-bitmap bit test, so freeing N blocks is O(N).
//!
//! ## The gate
//!
//! Allocate then free N blocks of one class, measuring the free phase's wall
//! time; repeat for 2N. With O(N) free work, doubling N roughly doubles the
//! time (ratio ≈ 2). With the old O(N²) walk, doubling N **quadruples** it
//! (ratio ≈ 4). We assert the ratio stays well under the quadratic regime.
//!
//! ## Counterfactual (verified by the author)
//!
//! Temporarily restoring the O(N) `free_list_contains` walk in `dealloc_small`
//! makes the free phase O(N²) and the measured ratio jumps toward ~4×, tripping
//! the assertion below — so this test is NON-VACUOUS (it genuinely fails on the
//! regression, not by luck). The bitmap restores it to ~2×.
//!
//! Wall-clock timing is coarse, so we use a generous threshold (ratio < 3.0,
//! halfway between the linear ~2 and quadratic ~4 regimes) and a warm-up pass to
//! damp first-touch noise. The signal between O(N) and O(N²) at these N is huge
//! (the old path was ~100× slower at N≈1024), so the coarse measurement is
//! ample to separate the two regimes without flaking.
//!
//! ## R23-6 (task #375) — `#[ignore]`d: no clean deterministic replacement
//!
//! This test flaked once under `npm run check`'s `--all-features` step
//! (multiple test BINARIES — separate OS processes — competing for CPU; see
//! `docs/CORRECTNESS_OPEN_ITEMS.md` item 3, moved to "Recently resolved", for
//! the investigation). Unlike
//! `tests/regression_segment_table_tombstone_rebuild.rs`'s sibling flaky
//! test (which got a deterministic `alloc-stats` scan-step counter
//! replacement — R23-6), THIS test has **no clean deterministic counter
//! replacement**: the guard it protects
//! (`AllocCore::dealloc_small`'s M2 double-free check, see
//! `src/alloc_core/alloc_core_small.rs`) is, by design, an UNCONDITIONAL
//! O(1) `AllocBitmap::is_free` bit test with no loop — it does exactly one
//! bit-test per free regardless of N. A counter that increments once per
//! `dealloc_small` call would read exactly N after N frees under BOTH the
//! correct O(1) implementation and the regressed O(N²)
//! `free_list_contains`-walk implementation this test guards against (the
//! walk still only gets invoked once per free — its length is what changes,
//! not its call count) — such a counter cannot distinguish the two
//! complexity classes, so it would be a vacuous, not a deterministic,
//! guard. Actually counting the walk's internal steps would require adding
//! a counter to code that no longer contains a loop at all (there would be
//! nothing to instrument without reintroducing the very walk being guarded
//! against), which is not a diagnostic-only addition — it is production hot
//! path.
//!
//! `#[ignore]`d rather than deleted (retains its diagnostic value run in
//! isolation, and its counterfactual documentation above remains accurate);
//! run manually via `cargo test --features alloc-core --test
//! dealloc_sublinear -- --ignored`, or defer to `npm run iai` /
//! `benches/perf_gate_iai.rs`'s `small_churn_16b`-family arms, which are the
//! deterministic Ir-based judges for this same free-path cost.

#![cfg(feature = "alloc-core")]

use core::alloc::Layout;
use std::time::Instant;

use sefer_alloc::alloc_core::AllocCore;

/// Allocate `n` blocks of `layout`, then free them all; return the wall-time of
/// the FREE phase only (the phase whose per-op cost the guard dominates).
fn free_phase_nanos(n: usize, layout: Layout) -> u128 {
    let mut ac = AllocCore::new().expect("primordial");
    let mut ptrs = Vec::with_capacity(n);
    for _ in 0..n {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "alloc returned null");
        ptrs.push(p);
    }
    let start = Instant::now();
    for &p in &ptrs {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
    start.elapsed().as_nanos()
}

#[ignore = "coarse wall-clock; flaky under concurrent test-binary CPU contention \
            (docs/CORRECTNESS_OPEN_ITEMS.md item 3, resolved R23-6/task #375) — \
            no clean deterministic counter replacement exists (the guard it \
            protects is an unconditional O(1) bitmap test with no loop to \
            instrument); run manually with --ignored, or use npm run iai instead"]
#[test]
fn own_thread_free_is_subquadratic() {
    // One small class (16 B). All blocks share the class, so every free's guard
    // sees the same growing free list — the worst case for the old O(N) walk.
    let layout = Layout::from_size_align(16, 16).unwrap();

    const N: usize = 2048;

    // Warm-up: prime the OS reservation / page-in path so the first measured
    // run is not penalised by cold first-touch (which would inflate the small-N
    // baseline and mask a real super-linear trend).
    let _ = free_phase_nanos(N, layout);

    // Measure N and 2N, taking the best of a few runs to damp scheduler noise.
    let best = |n: usize| (0..5).map(|_| free_phase_nanos(n, layout)).min().unwrap();
    let t_n = best(N).max(1);
    let t_2n = best(2 * N).max(1);

    let ratio = t_2n as f64 / t_n as f64;

    // O(N): ratio ≈ 2. O(N²): ratio ≈ 4. Assert we are firmly in the linear
    // regime. The old walk's ratio climbs toward ~4 (and the absolute times
    // explode), tripping this — the counterfactual documented in the module
    // header confirms non-vacuity.
    assert!(
        ratio < 3.0,
        "free phase scaled super-linearly: doubling N ({N}→{}) multiplied free \
         time by {ratio:.2}× (t_N={t_n}ns, t_2N={t_2n}ns). O(N) expects ~2×, \
         O(N²) ~4×. The O(1) bitmap double-free guard regressed back to an \
         O(free-list-length) walk.",
        2 * N
    );
}
