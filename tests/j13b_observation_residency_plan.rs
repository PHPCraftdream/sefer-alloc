//! J13b: integration tests for the large-THP observation residency
//! sampling planner (`LargeThpSpanGeometry`, `LargeThpResidencyPlan`,
//! `LargeThpResidencyWindow`).
//!
//! The module under test is intentionally NOT wired into the crate tree
//! (see its own mod.rs); these tests include it through a `#[path]`
//! attribute instead of a crate path — no production integration is
//! pretended.
//!
//! Coverage: geometry normalization at boundaries and its explicit
//! rejection set including the VALIDATED base-alignment premise
//! (unaligned base rejected, absolute range end overflow-checked) and
//! the checked absolute-address accounting; every planner rejection
//! variant; the 16-window and 2-MiB caps as ENDPOINTS (16 ok / 17
//! rejected; exactly 2 MiB ok / above rejected) and on huge-page
//! geometries; page alignment — span-relative AND absolute — plus span
//! bounds, strict monotonicity, pairwise page-disjointness (no
//! duplicates); the DIVISIBLE maximal-packing EXACT tiling oracle
//! (seed-independent grid whose last window ends exactly at the span
//! end) and the NON-DIVISIBLE negative oracles (coverage stays at
//! `placed * window_pages` pages for every seed — whole-span tiling is
//! impossible; remainder pages are distributed across the strata and
//! the unsampled gaps are NOT tail-pinned, pinned by hand for 5/2 and
//! for an 8-pages / 2×3-pages worked example); explicit shrink-to-fit;
//! the trailing partial page
//! never being sampled; tiny spans and huge geometries (`u64::MAX`
//! span, 2 MiB base pages) exercising the `u128` narrowing paths with
//! VALID windows; seed determinism and honest cross-seed placement
//! variance; and the single-window bounds property.
//!
//! Oracle discipline: every exact expectation is justified by an inline
//! arithmetic comment that does NOT re-derive the planner's formula —
//! maximal packing over an EXACT page multiple forces the grid (every
//! stratum is exactly one window long, so the within-stratum draw is
//! `% 1 == 0` and window i must start at page i), the non-divisible
//! case counts covered pages structurally (4 of 5 for any seed),
//! alignment/bounds/disjointness/monotonicity are structural
//! properties, and cross-seed variance uses an explicitly-commented
//! statistical bound whose failure would itself be the
//! broken-decorrelator defect the test exists to catch.

#[allow(dead_code)]
#[path = "../src/alloc_core/config/policy/large_thp_observation/mod.rs"]
mod large_thp_observation;

use large_thp_observation::bounded_mix::bounded_mix;
use large_thp_observation::large_thp_observation_error::LargeThpObservationError;
use large_thp_observation::large_thp_residency_plan::LargeThpResidencyPlan;
use large_thp_observation::large_thp_span_geometry::LargeThpSpanGeometry;

/// Asserts the structural invariants that hold for EVERY well-formed
/// plan over `geometry`: page alignment (span-relative AND absolute),
/// in-span bounds, strict monotonicity, pairwise page-disjointness (no
/// duplicate or overlapping windows), and the placed-count/slice-length
/// agreement.
fn assert_structural_invariants(plan: &LargeThpResidencyPlan, geometry: &LargeThpSpanGeometry) {
    let page = geometry.base_page_size();
    let base = geometry.base_address();
    let windows = plan.windows();
    assert_eq!(windows.len(), usize::from(plan.placed()));
    for (i, w) in windows.iter().enumerate() {
        assert!(w.page_count >= 1, "window {i}: page_count >= 1");
        assert_eq!(
            w.byte_offset,
            w.start_page_index * page,
            "window {i}: byte_offset must equal start_page_index * page"
        );
        assert_eq!(
            w.byte_offset % page,
            0,
            "window {i}: byte_offset must be page-aligned"
        );
        assert!(
            w.byte_offset + u64::from(w.page_count) * page <= geometry.span_size_bytes(),
            "window {i}: window must end within the span"
        );
        assert!(
            w.start_page_index + u64::from(w.page_count) <= geometry.full_page_count(),
            "window {i}: window pages must lie within the full-page range"
        );
        // Absolute alignment is guaranteed by construction: the geometry
        // validated a page-aligned base, and window offsets are page
        // multiples. base + offset cannot overflow — the whole absolute
        // range end was validated at construction.
        let absolute = base + w.byte_offset;
        assert_eq!(
            absolute % page,
            0,
            "window {i}: absolute address must be page-aligned"
        );
        assert!(
            absolute + u64::from(w.page_count) * page <= base + geometry.span_size_bytes(),
            "window {i}: absolute window must end within the absolute range"
        );
        if i > 0 {
            let prev = &windows[i - 1];
            assert!(
                w.byte_offset > prev.byte_offset,
                "window {i}: offsets must be strictly increasing"
            );
            assert!(
                w.start_page_index >= prev.start_page_index + u64::from(prev.page_count),
                "window {i}: windows must be page-disjoint"
            );
        }
    }
}

#[test]
fn geometry_rejects_invalid_inputs_explicitly() {
    // Each rejection is the documented variant for that input — nothing
    // is clamped or silently truncated.
    // Zero SPAN is the SECOND argument under the (base, span, page)
    // signature; the same triple with a one-page span is VALID (the
    // positive control below pins that the rejection is selective, and
    // `geometry_normalizes_whole_page_accounting` re-checks it).
    assert_eq!(
        LargeThpSpanGeometry::new(0, 0, 4096),
        Err(LargeThpObservationError::ZeroSpanSize)
    );
    let one_page = LargeThpSpanGeometry::new(0, 4096, 4096).unwrap();
    assert_eq!(one_page.full_page_count(), 1);
    assert_eq!(one_page.trailing_partial_bytes(), 0);
    assert_eq!(
        LargeThpSpanGeometry::new(4096, 4096, 0),
        Err(LargeThpObservationError::ZeroPageSize)
    );
    assert_eq!(
        LargeThpSpanGeometry::new(4096, 4096, 12),
        Err(LargeThpObservationError::PageSizeNotPowerOfTwo)
    );
    assert_eq!(
        LargeThpSpanGeometry::new(4096, 100, 4096),
        Err(LargeThpObservationError::NoFullPages)
    );
    // Validated base-alignment premise: base 1 with a 4096-byte span and
    // 4096-byte pages hosts NO whole aligned page at all — rejected
    // here, not deferred to the caller.
    assert_eq!(
        LargeThpSpanGeometry::new(1, 4096, 4096),
        Err(LargeThpObservationError::BaseNotPageAligned)
    );
    assert_eq!(
        LargeThpSpanGeometry::new(4097, 4096, 4096),
        Err(LargeThpObservationError::BaseNotPageAligned)
    );
    // The absolute range end must be representable: an ALIGNED base at
    // the top of the address space plus any span overflows.
    let aligned_top = u64::MAX - 4095;
    assert_eq!(
        LargeThpSpanGeometry::new(aligned_top, 8192, 4096),
        Err(LargeThpObservationError::AddressOverflow)
    );
}

#[test]
fn geometry_normalizes_whole_page_accounting() {
    let partial = LargeThpSpanGeometry::new(4096, 2 * 4096 + 100, 4096).unwrap();
    assert_eq!(partial.base_address(), 4096);
    assert_eq!(partial.span_size_bytes(), 2 * 4096 + 100);
    assert_eq!(partial.base_page_size(), 4096);
    assert_eq!(partial.full_page_count(), 2);
    assert_eq!(partial.trailing_partial_bytes(), 100);
    // Absolute accounting: offset 0 is the base itself; the whole-range
    // offset is the absolute end (4096 + 8292 = 12388).
    assert_eq!(partial.checked_absolute_address(0), Ok(4096));
    assert_eq!(
        partial.checked_absolute_address(2 * 4096 + 100),
        Ok(4096 + 2 * 4096 + 100)
    );
    let exact = LargeThpSpanGeometry::new(0, 4096, 4096).unwrap();
    assert_eq!(exact.full_page_count(), 1);
    assert_eq!(exact.trailing_partial_bytes(), 0);
    let huge = LargeThpSpanGeometry::new(0, u64::MAX, 4096).unwrap();
    // 2^64 - 1 = 4095 (mod 4096) trailing bytes; full pages are
    // (2^64 - 1 - 4095) / 4096 = 2^52 - 1 = 4_503_599_627_370_495.
    assert_eq!(huge.trailing_partial_bytes(), 4095);
    assert_eq!(huge.full_page_count(), 4_503_599_627_370_495);
}

#[test]
fn plan_rejects_invalid_requests_explicitly() {
    let g = LargeThpSpanGeometry::new(0, 8 * 1024 * 1024, 4096).unwrap();
    assert_eq!(
        LargeThpResidencyPlan::plan(&g, 0, 4096, 1),
        Err(LargeThpObservationError::EmptySample)
    );
    assert_eq!(
        LargeThpResidencyPlan::plan(&g, 17, 4096, 1),
        Err(LargeThpObservationError::WindowCapExceeded)
    );
    assert_eq!(
        LargeThpResidencyPlan::plan(&g, 1, 0, 1),
        Err(LargeThpObservationError::ZeroWindow)
    );
    // 4 MiB + 4 KiB is page-aligned but above the 2 MiB cap.
    assert_eq!(
        LargeThpResidencyPlan::plan(&g, 1, 4 * 1024 * 1024 + 4096, 1),
        Err(LargeThpObservationError::WindowTooLarge)
    );
    // 4097 bytes is within the cap but not a page multiple.
    assert_eq!(
        LargeThpResidencyPlan::plan(&g, 1, 4097, 1),
        Err(LargeThpObservationError::WindowNotPageAligned)
    );
    // A 2-page window does not fit a 1-page span at all.
    let tiny = LargeThpSpanGeometry::new(0, 4096, 4096).unwrap();
    assert_eq!(
        LargeThpResidencyPlan::plan(&tiny, 1, 2 * 4096, 1),
        Err(LargeThpObservationError::WindowExceedsSpan)
    );
}

#[test]
fn caps_are_hard_endpoints() {
    let g = LargeThpSpanGeometry::new(0, 64 * 1024 * 1024, 4096).unwrap();
    // 16 windows is INSIDE the cap (16 placed); 17 is rejected, never
    // clamped.
    let ok16 = LargeThpResidencyPlan::plan(&g, 16, 4096, 3).unwrap();
    assert_eq!(ok16.placed(), 16);
    assert_eq!(
        LargeThpResidencyPlan::plan(&g, 17, 4096, 3),
        Err(LargeThpObservationError::WindowCapExceeded)
    );
    // Exactly 2 MiB per window is INSIDE the cap (512 base pages);
    // one page more is rejected.
    let ok_2mib =
        LargeThpResidencyPlan::plan(&g, 1, LargeThpResidencyPlan::MAX_WINDOW_BYTES, 3).unwrap();
    assert_eq!(ok_2mib.windows()[0].page_count, 512);
    assert_eq!(
        LargeThpResidencyPlan::plan(&g, 1, LargeThpResidencyPlan::MAX_WINDOW_BYTES + 4096, 3),
        Err(LargeThpObservationError::WindowTooLarge)
    );
    // The cap constant matches the preregistered K = 16.
    assert_eq!(LargeThpResidencyPlan::MAX_WINDOWS, 16);
}

#[test]
fn placement_is_aligned_bounded_monotonic_and_disjoint() {
    let g = LargeThpSpanGeometry::new(0, 8 * 1024 * 1024, 4096).unwrap();
    let plan = LargeThpResidencyPlan::plan(&g, 4, 512 * 1024, 123).unwrap();
    assert_eq!(plan.placed(), 4);
    assert_structural_invariants(&plan, &g);
    // 512 KiB / 4 KiB = 128 base pages per window (exact arithmetic).
    for w in plan.windows() {
        assert_eq!(w.page_count, 128);
    }
}

#[test]
fn maximal_packing_tiles_the_span_exactly_seed_independently() {
    // DIVISIBLE maximal packing: 16 full pages, 1-page windows, 16
    // requested — k == 16 == full_page_count / window_pages makes every
    // stratum exactly one window page long, so the within-stratum draw
    // is `% 1 == 0` and window i MUST start at page i — a
    // seed-independent grid whose union covers the span with no gaps and
    // no overlaps.
    let g = LargeThpSpanGeometry::new(0, 16 * 4096, 4096).unwrap();
    for seed in [0u64, 7, u64::MAX] {
        let plan = LargeThpResidencyPlan::plan(&g, 16, 4096, seed).unwrap();
        assert_eq!(plan.placed(), 16);
        for (i, w) in plan.windows().iter().enumerate() {
            assert_eq!(w.start_page_index, i as u64, "seed {seed} window {i}");
            assert_eq!(w.byte_offset, i as u64 * 4096, "seed {seed} window {i}");
            assert_eq!(w.page_count, 1);
        }
        assert_structural_invariants(&plan, &g);
    }
    let plan = LargeThpResidencyPlan::plan(&g, 16, 4096, 7).unwrap();
    let last = plan.windows().last().unwrap();
    assert_eq!(
        last.byte_offset + u64::from(last.page_count) * 4096,
        16 * 4096,
        "tiling endpoint: the last window ends exactly at the span end"
    );
}

#[test]
fn non_divisible_packing_covers_four_of_five_pages_without_tiling() {
    // NON-DIVISIBLE negative oracle: 5 full pages with 2 windows of 2
    // pages → placed 2 (floor(5/2)); the strata are [0,2) and [2,5).
    // The FIRST stratum is exactly 2 pages, so its window is FORCED to
    // page 0 (slack 1, seed-independent); the second stratum has slack
    // 3-2+1 = 2, so its window starts at page 2 or page 3 — the single
    // unsampled page is the tail page 4 ONLY when the draw picks 2, and
    // is the INTERIOR page 2 when the draw picks 3: gaps are wherever
    // the draw puts them, never promised to sit at the tail. Coverage
    // is exactly 4 of the 5 pages for EVERY seed — whole-span tiling is
    // impossible. The expected second start is located by the PUBLIC
    // primitive (the same fixture pattern as the selector tests) and
    // checked against the real plan.
    const PAGE: u64 = 4096;
    let g = LargeThpSpanGeometry::new(0, 5 * PAGE, PAGE).unwrap();
    for seed in [0u64, 7, u64::MAX] {
        let plan = LargeThpResidencyPlan::plan(&g, 2, 2 * PAGE, seed).unwrap();
        assert_eq!(plan.placed(), 2);
        assert_structural_invariants(&plan, &g);
        // Manual stratification: stratum 0 = [0,2) (slack 1 → forced
        // page 0); stratum 1 = [2,5) (slack 2 → page 2 or 3).
        let expected_starts = [0, 2 + bounded_mix(seed, 1) % 2];
        let starts: Vec<u64> = plan.windows().iter().map(|w| w.start_page_index).collect();
        assert_eq!(
            starts, expected_starts,
            "seed {seed}: starts must match the manual strata + public primitive"
        );
        let mut covered = [false; 5];
        for w in plan.windows() {
            for page in w.start_page_index..w.start_page_index + u64::from(w.page_count) {
                covered[page as usize] = true;
            }
        }
        assert_eq!(
            covered.iter().filter(|&&c| c).count(),
            4,
            "seed {seed}: exactly 4 of the 5 pages are sampled — whole-span tiling is impossible"
        );
    }
}

#[test]
fn eight_pages_two_three_page_windows_manual_strata_example() {
    // Manual worked example: 8 full pages, 2 windows of 3 pages.
    // Maximal packing: k = floor(8/3) = 2 → placed 2. The floor-division
    // strata are floor(8*i/2)..floor(8*(i+1)/2) — [0,4) AND [4,8): BOTH
    // strata are 4 pages long, because the 2 remainder pages are
    // distributed across the strata, not appended to the last one. Each
    // stratum has slack 4-3+1 = 2, so the window starts come from
    // {0, 1} and {4, 5}: a gap can sit INSIDE the span (start 1 leaves
    // page 0 unsampled — a leading gap), and no gap is promised to be a
    // tail gap. Exactly 6 of the 8 pages are covered for every seed;
    // each expected start is located by the PUBLIC primitive and
    // checked against the real plan.
    const PAGE: u64 = 4096;
    let g = LargeThpSpanGeometry::new(0, 8 * PAGE, PAGE).unwrap();
    for seed in [0u64, 7, 0xABCD, u64::MAX] {
        let plan = LargeThpResidencyPlan::plan(&g, 2, 3 * PAGE, seed).unwrap();
        assert_eq!(plan.placed(), 2);
        assert_structural_invariants(&plan, &g);
        // Manual stratification: both strata are 4 pages; slack 2 each.
        let expected_starts = [bounded_mix(seed, 0) % 2, 4 + bounded_mix(seed, 1) % 2];
        let starts: Vec<u64> = plan.windows().iter().map(|w| w.start_page_index).collect();
        assert_eq!(
            starts, expected_starts,
            "seed {seed}: starts must match the manual strata + public primitive"
        );
        let mut covered = [false; 8];
        for w in plan.windows() {
            for page in w.start_page_index..w.start_page_index + u64::from(w.page_count) {
                covered[page as usize] = true;
            }
        }
        assert_eq!(
            covered.iter().filter(|&&c| c).count(),
            6,
            "seed {seed}: exactly 6 of the 8 pages are sampled"
        );
    }
}

#[test]
fn shrink_to_fit_is_explicit_not_silent() {
    // A 2-page span with 1-page windows can hold only 2 disjoint
    // windows; requesting 4 reports placed == 2 explicitly (never
    // out-of-range, never silently 4). Maximal packing at k == 2 (exact
    // multiple) forces the exact page grid {0, 1}.
    let g = LargeThpSpanGeometry::new(0, 2 * 4096, 4096).unwrap();
    let plan = LargeThpResidencyPlan::plan(&g, 4, 4096, 9).unwrap();
    assert_eq!(plan.placed(), 2);
    assert_structural_invariants(&plan, &g);
    let starts: Vec<u64> = plan.windows().iter().map(|w| w.start_page_index).collect();
    assert_eq!(starts, vec![0, 1]);
}

#[test]
fn trailing_partial_page_is_never_sampled() {
    // A span of 2 full pages plus a 100-byte tail: sampling is bounded
    // by full_page_count, and maximal packing (k == 2, exact multiple)
    // forces the exact page grid {0, 1} — so every planned byte lies
    // below the full-page boundary and the 100-byte tail is never inside
    // any window.
    let g = LargeThpSpanGeometry::new(0, 2 * 4096 + 100, 4096).unwrap();
    assert_eq!(g.trailing_partial_bytes(), 100);
    let plan = LargeThpResidencyPlan::plan(&g, 2, 4096, 11).unwrap();
    assert_eq!(plan.placed(), 2);
    for w in plan.windows() {
        assert!(w.byte_offset + u64::from(w.page_count) * 4096 <= 2 * 4096);
    }
}

#[test]
fn tiny_and_huge_geometries_stay_bounded() {
    // Tiny: a 1-page span with a 1-page window forced to page 0
    // (k == 1, stratum == window: the draw is `% 1 == 0`).
    let tiny = LargeThpSpanGeometry::new(0, 4096, 4096).unwrap();
    let plan = LargeThpResidencyPlan::plan(&tiny, 1, 4096, 5).unwrap();
    assert_eq!(plan.placed(), 1);
    assert_eq!(plan.windows()[0].byte_offset, 0);
    assert_structural_invariants(&plan, &tiny);

    // Huge: a u64::MAX span — strata boundaries are computed in u128 and
    // narrowed through checked conversions; 16 one-page windows stay
    // in-bounds, aligned, disjoint and monotonic.
    let huge = LargeThpSpanGeometry::new(0, u64::MAX, 4096).unwrap();
    let plan = LargeThpResidencyPlan::plan(&huge, 16, 4096, 13).unwrap();
    assert_eq!(plan.placed(), 16);
    assert_structural_invariants(&plan, &huge);

    // Huge-page endpoint with a VALID window: a 4 MiB span of 2 MiB
    // base pages, windows of exactly one 2 MiB page each — maximal
    // packing (k == 2, exact multiple) forces the seed-independent byte
    // grid {0, 2^21}; the 2^21 offset exercises the u128 narrowing.
    let big_pages = LargeThpSpanGeometry::new(0, 1u64 << 22, 1u64 << 21).unwrap();
    let plan = LargeThpResidencyPlan::plan(&big_pages, 2, 1u64 << 21, 17).unwrap();
    assert_eq!(plan.placed(), 2);
    let offsets: Vec<u64> = plan.windows().iter().map(|w| w.byte_offset).collect();
    assert_eq!(offsets, vec![0, 1u64 << 21]);
    assert_structural_invariants(&plan, &big_pages);
    for seed in [0u64, u64::MAX] {
        let other = LargeThpResidencyPlan::plan(&big_pages, 2, 1u64 << 21, seed).unwrap();
        assert_eq!(
            other, plan,
            "seed {seed}: maximal packing is seed-independent"
        );
    }
}

#[test]
fn the_window_cap_holds_on_huge_page_geometries() {
    // The 2 MiB per-window cap is NOT weakened for huge base pages: a
    // 2^62-byte window on a 2^62-page geometry is explicitly rejected
    // (the previous test wrongly unwrapped this exact call as a plan —
    // the guard fires first).
    let huge_pages = LargeThpSpanGeometry::new(0, 1u64 << 63, 1u64 << 62).unwrap();
    assert_eq!(
        LargeThpResidencyPlan::plan(&huge_pages, 2, 1u64 << 62, 17),
        Err(LargeThpObservationError::WindowTooLarge)
    );
    // A base page LARGER than the window cap cannot host any whole
    // window: a 2 MiB window is not a multiple of a 4 MiB page.
    let pages_4mib = LargeThpSpanGeometry::new(0, 1u64 << 23, 1u64 << 22).unwrap();
    assert_eq!(
        LargeThpResidencyPlan::plan(&pages_4mib, 1, 1u64 << 21, 17),
        Err(LargeThpObservationError::WindowNotPageAligned)
    );
}

#[test]
fn plans_are_deterministic_by_seed_and_differ_across_seeds() {
    let g = LargeThpSpanGeometry::new(0, 8 * 1024 * 1024, 4096).unwrap();
    // Determinism: identical inputs give identical plans (the plan is a
    // pure function of (geometry, requested, window_bytes, seed)).
    let a = LargeThpResidencyPlan::plan(&g, 4, 512 * 1024, 42).unwrap();
    let b = LargeThpResidencyPlan::plan(&g, 4, 512 * 1024, 42).unwrap();
    assert_eq!(a, b);
    // Variance across seeds (statistical, honest): each of 16 seeds
    // draws 3 within-stratum offsets from ~385-page slacks (2048 full
    // pages / 4 strata = 512-page strata, minus the 128-page window,
    // plus one = 385 draws); all 16 plans byte-identical would require
    // dozens of coincident folds — its failure would itself be the
    // broken-decorrelator defect this test exists to catch.
    let mut plans = Vec::new();
    for seed in 1..=16u64 {
        let plan = LargeThpResidencyPlan::plan(&g, 4, 512 * 1024, seed).unwrap();
        assert_eq!(plan.placed(), 4);
        assert_structural_invariants(&plan, &g);
        plans.push(plan);
    }
    plans.dedup();
    assert!(
        plans.len() > 1,
        "16 seeds producing identical placements would be a broken decorrelator"
    );
}

#[test]
fn single_window_lies_anywhere_within_bounds() {
    // k == 1: one stratum spanning the whole page range; the window must
    // lie within [0, span - window_bytes] — a bounds property (its exact
    // position is the seed's draw, not asserted).
    let g = LargeThpSpanGeometry::new(0, 8 * 1024 * 1024, 4096).unwrap();
    let plan = LargeThpResidencyPlan::plan(&g, 1, 512 * 1024, 77).unwrap();
    assert_eq!(plan.placed(), 1);
    assert_structural_invariants(&plan, &g);
    let w = plan.windows()[0];
    assert!(w.byte_offset <= 8 * 1024 * 1024 - 512 * 1024);
    assert_eq!(w.page_count, 128);
}
