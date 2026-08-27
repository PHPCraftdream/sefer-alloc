//! Correctness of the `const`-generic builder itself — table shape, the derived
//! O(1) lookup, and the alignment-jump classifier — against sefer's own
//! concrete parameterization (`SEFER_PARAMS` in `common/mod.rs`), via hand-written unit
//! tests (an independent, from-scratch reference builder/classifier, an
//! exhaustive small-size×alignment sweep, and the `Params::extras`
//! precondition `#[should_panic]`s). This file has no proptest of its own —
//! the sibling `tests/proptest_builder.rs` is where `(size, align)` is
//! property-generated, across three additional hand-picked schemes distinct
//! from `SEFER_PARAMS`.

use size_classes::{
    build_size2class, build_table, size2class_len, InvalidAlign, Params, SizeClasses,
};

mod common;
use common::{
    HUGE_THRESHOLD, JUMP_A, JUMP_B, SEFER_EXTRAS, SEFER_GEO, SEFER_MAX, SEFER_MIN_BLOCK, SEFER_N,
    SEFER_SC, SEFER_TABLE,
};

/// A faithful, from-scratch reference table builder (a plain `Vec` version of
/// the crate's `const fn build_table`) so tests do not trust the crate's own
/// output. Geometric run merged with sorted `extras`.
fn reference_table(
    min_block: usize,
    growth: (usize, usize),
    geo_count: usize,
    extras: &[usize],
) -> Vec<usize> {
    let (num, den) = growth;
    let mask = min_block - 1;
    let mut geo = Vec::with_capacity(geo_count);
    let mut cur = min_block;
    for i in 0..geo_count {
        geo.push(cur);
        // MS prepublish review, task #1503 (P2-2): two independent fixes to
        // this loop, both needed -- verified by fault injection (each one
        // alone still panicked at the geo_count=182 boundary production
        // itself accepts):
        // 1. Only advance when a next class is actually needed (mirrors
        //    production's `if gi < geo_count` guard, lib.rs) -- computing an
        //    extra, unused advance past the last requested class overflows
        //    at exactly the geo_count where production stops one step
        //    earlier and never attempts it.
        // 2. Widen the multiply to `u128` before rounding (mirrors
        //    production's own `cur as u128` widening) -- a plain `usize`
        //    multiply overflows at a SMALLER `cur` than production tolerates
        //    even for a legitimately-needed step, since production's
        //    widening lets the quotient fit `usize` while the intermediate
        //    product does not (e.g. `min_block = 2^62, growth = (3, 3)` at
        //    `cur = 2^63`, documented on `build_table`'s own doc comment).
        if i + 1 < geo_count {
            let scaled = (cur as u128 * num as u128).div_ceil(den as u128);
            let rounded = (scaled + mask as u128) & !(mask as u128);
            let rounded: usize = rounded
                .try_into()
                .expect("reference_table: geometric progression overflows usize");
            cur = if rounded > cur {
                rounded
            } else {
                cur.checked_add(min_block)
                    .expect("reference_table: geometric progression overflows usize")
            };
        }
    }
    // Sorted merge of geo and extras.
    let mut out = Vec::with_capacity(geo_count + extras.len());
    let (mut gi, mut ei) = (0, 0);
    while gi < geo.len() || ei < extras.len() {
        let take_geo = if gi >= geo.len() {
            false
        } else if ei >= extras.len() {
            true
        } else {
            geo[gi] < extras[ei]
        };
        if take_geo {
            out.push(geo[gi]);
            gi += 1;
        } else {
            out.push(extras[ei]);
            ei += 1;
        }
    }
    out
}

/// Reference classifier (independent of the crate's `class_for`): smallest
/// class with `block >= max(size, align)` AND `block % align == 0`.
fn reference_class_for(table: &[usize], size: usize, align: usize) -> Option<usize> {
    let need = size.max(align);
    table
        .iter()
        .position(|&b| b >= need && b.is_multiple_of(align))
}

// MS prepublish review, task #1503 (P2-2): the exact geo_count where
// production succeeds (182 on 64-bit, per sefer_growth_geo_count_182_is_
// the_last_that_fits_on_64_bit below) but where `reference_table`'s
// PRE-fix unconditional last-iteration advance would additionally compute
// one more, unused step -- the same step that overflows at geo_count=183.
// Pins that `reference_table` agrees with production at the boundary
// instead of spuriously panicking on work production never does.
#[cfg(target_pointer_width = "64")]
#[test]
fn reference_table_does_not_overcompute_at_the_geo_count_182_boundary() {
    const GEO_COUNT: usize = 182;
    const N: usize = GEO_COUNT;
    let params = Params::new(16, (5, 4), GEO_COUNT, &[], 1 << 20);
    let want = reference_table(16, (5, 4), GEO_COUNT, &[]);
    let got = build_table::<N>(&params);
    assert_eq!(&got[..], &want[..]);
}

#[test]
fn sefer_table_matches_reference_and_is_strictly_increasing() {
    let want = reference_table(SEFER_MIN_BLOCK, (5, 4), SEFER_GEO, SEFER_EXTRAS);
    assert_eq!(&SEFER_TABLE[..], &want[..]);
    // Derive-not-hardcode: the count is whatever the params produce.
    assert_eq!(SEFER_SC.count(), SEFER_N);
    // rush-tests review T3/task #1478: these accessors had zero call sites
    // anywhere in the suite -- an accessor returning the wrong FIELD (e.g.
    // min_block() returning huge_threshold) would pass everything else.
    // huge_threshold() (oxx prepublish review P1-3) added to the same pin.
    assert_eq!(SEFER_SC.min_block(), SEFER_MIN_BLOCK);
    assert_eq!(SEFER_SC.small_align_max(), SEFER_MIN_BLOCK); // documented == min_block
    assert_eq!(SEFER_SC.huge_threshold(), HUGE_THRESHOLD);
    for w in SEFER_TABLE.windows(2) {
        assert!(w[0] < w[1], "table must be strictly increasing: {w:?}");
    }
    for &b in &SEFER_TABLE {
        assert!(
            b.is_multiple_of(SEFER_MIN_BLOCK),
            "class {b} not a multiple of min_block"
        );
    }
    // The exact-256 and page-aligned extras really landed in the table.
    for &e in SEFER_EXTRAS {
        assert!(SEFER_TABLE.contains(&e), "extra {e} missing from table");
    }
}

#[test]
fn sefer_class_for_matches_reference_over_full_small_sweep() {
    // Every alignment the slow path can carry (powers of two up to SMALL_MAX),
    // and every size 1..=SMALL_MAX+1, against the independent reference.
    let mut aligns = vec![1usize, 2, 4, 8, 16];
    let mut a = 32;
    while a <= SEFER_MAX {
        aligns.push(a);
        a <<= 1;
    }
    // rush-tests review T5/task #1480: `a` here is the first power of two
    // STRICTLY GREATER than SEFER_MAX -- every align tested above stays
    // `<= SEFER_MAX`, so `need = max(size, align)` was never pushed past
    // `small_max` by `align` alone (only ever by `size`). Pushing it makes
    // the early-rejection branch's ALIGN-driven trigger reachable too.
    aligns.push(a);

    // oxx review P3-4 (task #1519): a full `1..=SEFER_MAX+1` step-by-1 sweep
    // is ~258753 sizes x 19 aligns = ~4.9M `class_for` calls (plus as many
    // independent-reference calls), run 3x in CI (debug/release/i686-debug).
    // Below SMALL_STEP_CEIL, sizes are cheap and this is the zone real
    // allocations hit most, so keep the full step-by-1 walk there. Above it,
    // step by SEFER_MIN_BLOCK, but ALWAYS explicitly include every table
    // entry and every align value (both are exact `need` boundaries, since
    // `need = max(size, align)`) together with their immediate neighbors --
    // a large step alone could straddle a boundary and never observe the
    // off-by-one it guards.
    const SMALL_STEP_CEIL: usize = 8192;
    let mut boundary_points: Vec<usize> = Vec::new();
    for &b in SEFER_TABLE.iter() {
        boundary_points.push(b.saturating_sub(1));
        boundary_points.push(b);
        boundary_points.push(b.saturating_add(1));
    }
    for &al in &aligns {
        boundary_points.push(al.saturating_sub(1));
        boundary_points.push(al);
        boundary_points.push(al.saturating_add(1));
    }
    boundary_points.retain(|&s| (1..=SEFER_MAX + 1).contains(&s) && s > SMALL_STEP_CEIL);
    boundary_points.sort_unstable();
    boundary_points.dedup();

    let sizes: Vec<usize> = (1..=SMALL_STEP_CEIL)
        .chain(boundary_points)
        .chain((SMALL_STEP_CEIL + 1..=SEFER_MAX + 1).step_by(SEFER_MIN_BLOCK))
        .collect();

    for &align in &aligns {
        for &size in &sizes {
            let got = SEFER_SC.class_for(size, align);
            let want = reference_class_for(&SEFER_TABLE, size, align);
            assert_eq!(got, want, "drift at size={size} align={align}");
            if let Some(idx) = got {
                let block = SEFER_TABLE[idx];
                assert!(block >= size.max(align));
                assert!(block.is_multiple_of(align));
            }
            // oxx prepublish review P2-4: every `align` in this sweep is
            // already a valid power of two, so `try_class_for` must agree
            // with `class_for` (never `Err`) on every one of these pairs --
            // broader evidence for the "never panics" doc claim than the 4
            // hand-picked pairs in
            // try_class_for_matches_class_for_on_every_valid_input alone.
            assert_eq!(SEFER_SC.try_class_for(size, align), Ok(got));
        }
    }
}

#[test]
fn sefer_size2class_matches_scan_for_every_bucket() {
    let s2c = SEFER_SC.size2class();
    for (k, &class_idx) in s2c.iter().enumerate() {
        let need = ((k + 1) * SEFER_MIN_BLOCK).min(SEFER_MAX);
        let want = SEFER_TABLE.iter().position(|&b| b >= need).unwrap();
        assert_eq!(
            class_idx as usize, want,
            "SIZE2CLASS[{k}] drift (need={need})"
        );
    }
}

// A minimal scheme pinning `size2class()`'s raw-indexing-domain contract
// (size2class() doc, Sol-run5 P2-1): min_block=16, doubling growth gives
// table=[16,32,64], small_max=64, L = 64/16 + 1 = 5 -- small enough to name
// every one of the three zones explicitly.
const DOMAIN_MB: usize = 16;
const DOMAIN_N: usize = 3;
const DOMAIN_P: Params = Params::new(DOMAIN_MB, (2, 1), DOMAIN_N, &[], 1 << 20);
const DOMAIN_T: [usize; DOMAIN_N] = build_table::<DOMAIN_N>(&DOMAIN_P);
const DOMAIN_L: usize = size2class_len(DOMAIN_T[DOMAIN_N - 1], DOMAIN_MB);
static DOMAIN_SC: SizeClasses<DOMAIN_N, DOMAIN_L> = SizeClasses::build(DOMAIN_P);

#[test]
fn size2class_raw_domain_valid_and_false_sentinel_zones() {
    assert_eq!(DOMAIN_T, [16, 32, 64]);
    assert_eq!(DOMAIN_L, 5);
    let s2c = DOMAIN_SC.size2class();

    // Valid domain: size <= small_max resolves to the true smallest fitting
    // class, matching class_for.
    for size in 1..=64usize {
        let idx = (size - 1) >> DOMAIN_SC.min_block_shift();
        let raw = s2c[idx] as usize;
        assert_eq!(
            Some(raw),
            DOMAIN_SC.class_for(size, 1),
            "size={size} raw lookup must agree with class_for in the valid domain"
        );
    }

    // False-sentinel window: small_max < size <= L * min_block (65..=80)
    // lands in-bounds on bucket L-1 and returns the LAST class index -- a
    // false "fits" the raw array does not itself reject, unlike class_for.
    for size in 65..=80usize {
        let idx = (size - 1) >> DOMAIN_SC.min_block_shift();
        assert_eq!(idx, DOMAIN_L - 1, "size={size} must land on the top bucket");
        let raw = s2c[idx] as usize;
        assert_eq!(
            raw,
            DOMAIN_N - 1,
            "size={size} sentinel must be the last class"
        );
        assert_eq!(
            DOMAIN_SC.class_for(size, 1),
            None,
            "class_for must correctly reject size={size}, unlike the raw sentinel"
        );
    }
}

#[test]
fn debug_impl_prints_a_summary_not_the_raw_tables() {
    // rush2-holistic F5/task #1489: Debug is hand-written specifically to
    // avoid dumping both raw tables (~16 KiB total for a realistic scheme,
    // almost all of it the size2class LUT).
    // Pins that on a regression back to `#[derive(Debug)]`: DOMAIN_T has
    // 3 entries and DOMAIN_L is 5, so a derive would print every element of
    // `table: [16, 32, 64]` and `size2class: [0, 1, 2, 2, 2]` -- this
    // asserts the summary fields are present and the raw field names/values
    // are not.
    let s = format!("{DOMAIN_SC:?}");
    assert!(s.contains("SizeClasses"), "got: {s}");
    assert!(s.contains(&format!("min_block: {DOMAIN_MB}")), "got: {s}");
    assert!(
        s.contains(&format!("small_max: {}", DOMAIN_T[DOMAIN_N - 1])),
        "got: {s}"
    );
    assert!(
        !s.contains("table:"),
        "must not print the raw table field: {s}"
    );
    assert!(
        !s.contains("size2class:"),
        "must not print the raw LUT field: {s}"
    );
}

#[test]
#[should_panic(expected = "index out of bounds")]
fn size2class_raw_domain_first_out_of_bounds_size_panics() {
    // size=81 is the first size whose raw index (80 >> 4 == 5) reaches L
    // itself -- genuinely out-of-bounds, not a clamped sentinel.
    let size = 81usize;
    let idx = (size - 1) >> DOMAIN_SC.min_block_shift();
    assert_eq!(
        idx, DOMAIN_L,
        "precondition: this size must compute idx == L"
    );
    let _ = DOMAIN_SC.size2class()[idx];
}

#[test]
fn sefer_jump_skips_non_divisible_run_for_align_128() {
    // (128,128): seed is the ~144 B class (not 128-divisible); the jump must
    // land on the 256 B exact class in one hop.
    let got = SEFER_SC.class_for(128, 128).expect("(128,128) resolves");
    let block = SEFER_TABLE[got];
    assert!(block.is_multiple_of(128));
    assert!(block >= 128);
    // The seed itself is NOT 128-divisible (else the jump would be a no-op).
    let seed = SEFER_SC.size2class()[(128 - 1) >> SEFER_MIN_BLOCK.trailing_zeros()] as usize;
    assert!(!SEFER_TABLE[seed].is_multiple_of(128));
}

#[test]
fn sefer_bench_jump_rows_genuinely_exercise_the_slow_path() {
    // task #1424 (review-2 F2): `benches/size_classes_bench.rs`'s
    // `large_align_slow_path`/`large_align_slow_path_1024` rows used to call
    // `class_for(256, 256)`/`class_for(1024, 1024)` -- since 256 and 1024 are
    // themselves table entries, `need == align` lands the seed EXACTLY on an
    // align-divisible class, so `class_for`'s jump-loop body (round up,
    // re-seek) never ran; both rows silently measured the same fast-path-ish
    // single check the `small_hit` row already covers, not the slow path
    // their names claimed. This is a path-activation oracle (this repo's own
    // R30-8 convention) for the replacement (size, align) pairs the fixed
    // benchmark now uses -- pinning that the seed is genuinely NOT
    // align-divisible for both, so a future table change can't silently make
    // the bench inert again without this test catching it.
    // JUMP_A/JUMP_B now come from `common` -- mechanically shared with
    // benches/size_classes_bench.rs (rush-tests review T4/task #1479),
    // replacing the former comment-only "keep in sync" convention.
    for &(size, align) in &[JUMP_A, JUMP_B] {
        let need = size.max(align);
        let seed = SEFER_SC.size2class()[(need - 1) >> SEFER_MIN_BLOCK.trailing_zeros()] as usize;
        assert!(
            !SEFER_TABLE[seed].is_multiple_of(align),
            "size={size} align={align}: seed class {} (block {}) is already \
             align-divisible -- the jump loop's round-up body would never run",
            seed,
            SEFER_TABLE[seed]
        );
        let got = SEFER_SC
            .class_for(size, align)
            .unwrap_or_else(|| panic!("({size}, {align}) must resolve"));
        assert!(SEFER_TABLE[got].is_multiple_of(align));
        assert!(SEFER_TABLE[got] >= need);
    }
}

// size-classes publication audit run 7 (oxx, P3-2): bench-local twins of
// JUMP_MULTI/JUMP_NONE/JUMP_DENSE from benches/size_classes_bench.rs. These
// are NOT shared via `common` (the bench-coverage task this test belongs to
// is scoped to editing only the bench file + this one path-activation-oracle
// exception), so the constants are duplicated here deliberately -- this test
// is the guard against the two copies silently drifting apart, exactly as
// `sefer_bench_jump_rows_genuinely_exercise_the_slow_path` above guards
// JUMP_A/JUMP_B.
const JUMP_MULTI: (usize, usize) = (513, 512);
const JUMP_NONE: (usize, usize) = (16385, 16384);
const JUMP_DENSE: (usize, usize) = (129, 128);

#[test]
fn sefer_bench_new_jump_rows_genuinely_exercise_the_slow_path() {
    // JUMP_MULTI and JUMP_DENSE must each take a genuine multi-iteration
    // slow path (>= 2 jump-loop passes) before resolving `Some`; JUMP_NONE
    // must take a genuine multi-iteration slow path (>= 2 passes) that walks
    // the table via the jump loop (not the early `need > small_max`
    // rejection) before exhausting it and returning `None`. A future table
    // change (params, extras, growth) could silently collapse any of these
    // back to a 0- or 1-iteration case -- this test fails loudly if so,
    // instead of the bench row quietly measuring the wrong thing again.
    let shift = SEFER_MIN_BLOCK.trailing_zeros();
    let small_max = *SEFER_TABLE.last().unwrap();

    for &(size, align, want) in &[
        (JUMP_MULTI.0, JUMP_MULTI.1, Some(17usize)),
        (JUMP_DENSE.0, JUMP_DENSE.1, Some(9usize)),
    ] {
        let need = size.max(align);
        assert!(
            need <= small_max,
            "size={size} align={align}: must not be early-rejected"
        );
        let seed = SEFER_SC.size2class()[(need - 1) >> shift] as usize;
        assert!(
            !SEFER_TABLE[seed].is_multiple_of(align),
            "size={size} align={align}: seed class {seed} (block {}) is already \
             align-divisible -- the jump loop would take 0 iterations",
            SEFER_TABLE[seed]
        );
        // Simulate the jump loop independently to count iterations (an
        // independent re-derivation, not a call into `class_for` itself,
        // matching this file's existing reference-implementation style).
        let mut i = seed;
        let mut iters = 0usize;
        let mut result = None;
        while i < SEFER_TABLE.len() {
            iters += 1;
            let block = SEFER_TABLE[i];
            if block.is_multiple_of(align) {
                result = Some(i);
                break;
            }
            let next_mult = (block | (align - 1)) + 1;
            if next_mult > small_max {
                break;
            }
            i = SEFER_SC.size2class()[(next_mult - 1) >> shift] as usize;
        }
        assert!(
            iters >= 2,
            "size={size} align={align}: only {iters} jump-loop iteration(s) -- \
             not a genuine multi-jump case"
        );
        assert_eq!(
            result, want,
            "size={size} align={align}: simulated result drift"
        );
        assert_eq!(
            SEFER_SC.class_for(size, align),
            want,
            "size={size} align={align}: class_for disagrees with the simulation"
        );
    }

    // JUMP_NONE: same shape, but must end in `None` after >= 2 iterations,
    // not via the early `need > small_max` rejection.
    let (size, align) = JUMP_NONE;
    let need = size.max(align);
    assert!(need <= small_max, "JUMP_NONE must not be early-rejected");
    let seed = SEFER_SC.size2class()[(need - 1) >> shift] as usize;
    assert!(
        !SEFER_TABLE[seed].is_multiple_of(align),
        "JUMP_NONE seed class {seed} (block {}) is already align-divisible",
        SEFER_TABLE[seed]
    );
    let mut i = seed;
    let mut iters = 0usize;
    let mut result = None;
    while i < SEFER_TABLE.len() {
        iters += 1;
        let block = SEFER_TABLE[i];
        if block.is_multiple_of(align) {
            result = Some(i);
            break;
        }
        let next_mult = (block | (align - 1)) + 1;
        if next_mult > small_max {
            break;
        }
        i = SEFER_SC.size2class()[(next_mult - 1) >> shift] as usize;
    }
    assert!(
        iters >= 2,
        "JUMP_NONE: only {iters} jump-loop iteration(s) -- not a genuine multi-jump case"
    );
    assert_eq!(result, None, "JUMP_NONE: simulation must end in None");
    assert_eq!(
        SEFER_SC.class_for(size, align),
        None,
        "JUMP_NONE: class_for disagrees with the simulation"
    );
}

// ---------------------------------------------------------------------------
// `Params::extras` precondition violations — both must now be `const`-eval
// panics (compile errors when the params are truly `const`), not silent
// wrong answers. Exercised here via runtime (non-const) invocations of the
// same `const fn`s so the panic can be caught by `#[should_panic]`, matching
// this crate's existing precondition-testing shape (`min_block` power-of-two,
// `geo_count > 0`, `N == geo_count + extras.len()` are likewise asserted in
// `build_table`/`build_size2class` with no separate compile-fail harness).
//
// docs/reviews/2026-08-06-size-classes-publish-readiness-review.md §5.1 (S1)
// reproduced both as silent-corruption bugs before this fix.

#[test]
#[should_panic(expected = "multiple of min_block")]
fn extras_not_multiple_of_min_block_panics() {
    // min_block=16, extras=[100, 200], geo_count=8 — the exact §5.1(a)
    // reproduction: 100 is not a multiple of 16, and (pre-fix) silently
    // merged into the table at index 5, breaking `class_for`'s fast-path
    // alignment invariant with no diagnostic.
    const MIN_BLOCK: usize = 16;
    const EXTRAS: &[usize] = &[100, 200];
    const GEO_COUNT: usize = 8;
    const N: usize = GEO_COUNT + EXTRAS.len();
    let params = Params::new(MIN_BLOCK, (5, 4), GEO_COUNT, EXTRAS, 1 << 20);
    let _ = build_table::<N>(&params);
}

// size-classes publication audit run 1 (Sol-codex, P2-2): this used to be a
// SINGLE test, `extras_overlapping_geometric_run_panics`, whose comments
// explicitly documented that `build_table` ALONE accepted this input and the
// violation only became visible once merged into `build_size2class` via
// `SizeClasses::build` -- i.e. the test's own stated purpose was to pin a
// standalone-`build_table` contract violation, exactly the P2-2 finding.
// Once `build_table` grew its OWN merged-table monotonicity check (this same
// commit), the input below panics one line into that old test's setup
// (`build_table::<N>(&params)`) instead of at the `SizeClasses::build` call
// the test's assertions and comments were actually about -- `#[should_panic]`
// still matched (both panic messages happen to contain "must be strictly
// increasing"), but for a different reason than the test claimed to check,
// silently defeating its own stated chokepoint. Split into two tests with
// distinct SUTs and distinct, non-overlapping expected substrings, so a
// panic from the wrong site cannot coincidentally satisfy either one (the
// same class of bug task #730 already fixed once for the single-test
// version).

#[test]
#[should_panic(expected = "build_table: merged table must be strictly increasing")]
fn extras_overlapping_geometric_run_panics_in_build_table() {
    // min_block=16, extras=[16, 32], geo_count=8 — the exact §5.1(b)
    // reproduction: both extras already appear in the geometric run
    // (pre-fix: table = [16, 16, 32, 32, 48, 64, 80, 112, 144, 192], indices
    // 1 and 3 permanently unreachable, no diagnostic). `extras` here IS
    // strictly increasing and IS min_block-aligned among itself, so only the
    // MERGED-table check (not the per-`extras`-entry checks) can catch this.
    const MIN_BLOCK: usize = 16;
    const EXTRAS: &[usize] = &[16, 32];
    const GEO_COUNT: usize = 8;
    const N: usize = GEO_COUNT + EXTRAS.len();
    let params = Params::new(MIN_BLOCK, (5, 4), GEO_COUNT, EXTRAS, 1 << 20);
    let _ = build_table::<N>(&params);
}

#[test]
#[should_panic(expected = "table must be strictly increasing (hand-built tables must satisfy")]
fn hand_built_overlapping_table_panics_in_build_size2class() {
    // Defense-in-depth: `build_size2class`'s own monotonicity check (kept
    // deliberately, not removed, once `build_table` grew its own) is the
    // ONLY guard for a table a caller assembles BY HAND rather than through
    // `build_table` -- e.g. a `const` array literal, or a table built by
    // some other means entirely. Bypasses `build_table` on purpose: this
    // duplicate `[16, 16, 32, ...]` shape is never produced by
    // `build_table` (it now rejects the same shape at its own chokepoint,
    // per the test above), so this is the one remaining path that reaches
    // `build_size2class`'s check.
    const MIN_BLOCK: usize = 16;
    const N: usize = 3;
    const TABLE: [usize; N] = [16, 16, 32];
    const L: usize = size2class_len(32, MIN_BLOCK);
    let _ = build_size2class::<N, L>(&TABLE, MIN_BLOCK);
}

#[test]
#[should_panic(expected = "every entry must be >= min_block")]
fn extras_zero_class_panics() {
    // size-classes publication audit run 1 (Sol-codex, P2-2), the
    // adjacent unverified case: `extras = [0]` passed both the
    // multiple-of-min_block check (0 is a multiple of everything) and the
    // strictly-increasing-among-itself check (only one entry), so it
    // reached the table as a zero-sized class before `min_block`'s own
    // documented minimum-block-size meaning -- unreachable for any real
    // `Layout` (`align >= 1`) yet present in `count`/`table`/`block_size(0)`.
    // Rejected outright rather than given a documented meaning.
    const MIN_BLOCK: usize = 16;
    const EXTRAS: &[usize] = &[0];
    const GEO_COUNT: usize = 8;
    const N: usize = GEO_COUNT + EXTRAS.len();
    let params = Params::new(MIN_BLOCK, (5, 4), GEO_COUNT, EXTRAS, 1 << 20);
    let _ = build_table::<N>(&params);
}

// rush-tests review (T2/task #1477): nine documented `# Panics`
// conditions (plus `block_size`'s out-of-range panic, ten tests total)
// had zero test coverage anywhere in the crate -- grep-verified against
// every production assert's exact message string before writing these.
// Each pins one precondition at its own call site, so a deleted/reworded
// guard fails here instead of only in a future audit's static reading.

#[test]
#[should_panic(expected = "size2class_len: min_block must be a power of two")]
fn size2class_len_rejects_non_pow2_min_block() {
    let _ = size2class_len(64, 12);
}

#[test]
#[should_panic(expected = "min_block must be a power of two")]
fn build_table_rejects_non_pow2_min_block() {
    let params = Params::new(12, (5, 4), 1, &[], 1 << 20);
    let _ = build_table::<1>(&params);
}

#[test]
#[should_panic(expected = "min_block must be a power of two")]
fn build_size2class_rejects_non_pow2_min_block() {
    const TABLE: [usize; 3] = [16, 32, 48];
    let _ = build_size2class::<3, 4>(&TABLE, 12);
}

#[test]
#[should_panic(expected = "geo_count must be > 0")]
fn build_table_rejects_zero_geo_count() {
    let params = Params::new(16, (5, 4), 0, &[16, 32], 1 << 20);
    let _ = build_table::<2>(&params);
}

#[test]
#[should_panic(expected = "growth denominator must be > 0")]
fn build_table_rejects_zero_growth_denominator() {
    // The guard exists specifically to replace a bare "attempt to divide
    // by zero" with a named diagnostic -- pin the named message, not just
    // that SOME panic occurs.
    let params = Params::new(16, (1, 0), 1, &[], 1 << 20);
    let _ = build_table::<1>(&params);
}

#[test]
#[should_panic(expected = "N must equal geo_count + extras.len()")]
fn build_table_rejects_n_mismatch() {
    // geo_count=3 + extras.len()=1 == 4, but N is declared as 5 -- the
    // single likeliest real-user error when hand-deriving the const
    // generic.
    let params = Params::new(16, (5, 4), 3, &[64], 1 << 20);
    let _ = build_table::<5>(&params);
}

#[test]
#[should_panic(expected = "Params::extras: must be strictly increasing")]
fn build_table_rejects_non_increasing_extras_among_themselves() {
    // Distinct from `extras_overlapping_geometric_run_panics_in_build_table`
    // above: [64, 32] is not increasing WITHIN `extras` itself, never mind
    // the geometric run -- the per-entry check this test pins fires before
    // the merged-table check ever runs.
    let params = Params::new(16, (5, 4), 4, &[64, 32], 1 << 20);
    let _ = build_table::<6>(&params);
}

#[test]
#[should_panic(expected = "table must be non-empty")]
fn build_size2class_rejects_empty_table() {
    const TABLE: [usize; 0] = [];
    let _ = build_size2class::<0, 1>(&TABLE, 16);
}

#[test]
#[should_panic(expected = "L must equal size2class_len(max_class, min_block)")]
fn build_size2class_rejects_wrong_l() {
    // Correct L for table=[16,32,48], min_block=16 is size2class_len(48,16)
    // == 4; 5 is deliberately wrong. Distinct from the OVERFLOW variant
    // (`build_size2class_l_check_overflow_panics_instead_of_accepting_a_wrong_l`
    // below) -- this is the plain non-overflowing mismatch.
    const TABLE: [usize; 3] = [16, 32, 48];
    let _ = build_size2class::<3, 5>(&TABLE, 16);
}

#[test]
#[should_panic(expected = "index out of bounds")]
fn block_size_rejects_out_of_range_index() {
    // Documented `# Panics` on `SizeClasses::block_size`: "if idx >= N".
    // Every production call site only ever passes an index `class_for`
    // returned, so this path is otherwise never exercised.
    let _ = DOMAIN_SC.block_size(DOMAIN_N);
}

// task #730 (rust-intel audit §D1a/§F1, INFO): `reference_table`'s
// rounding/spacing core (`let mut next = (cur * num).div_ceil(den); next =
// (next + mask) & !mask;`, near the top of this file) is BYTE-IDENTICAL to
// `build_table`'s own formula -- a circular-oracle shape: it proves
// const-eval and runtime-eval agree on ONE expression tree, not that the
// expression tree itself computes the right spacing. A shared misconception
// in the rounding/spacing formula would be structurally unobservable to
// `sefer_table_matches_reference_and_is_strictly_increasing` above. This
// test is a genuinely independent check: `GOLDEN` was computed BY HAND (see
// the arithmetic in the comment below), not derived from either
// `build_table` or `reference_table`.
#[test]
fn geometric_run_matches_hand_derived_golden_values() {
    // min_block=16, growth=(5,4) (1.25x spacing), 8 classes starting at
    // min_block. Each step: multiply by 5/4 (round up to an integer, i.e.
    // ceiling division), then round up to the next multiple of min_block
    // (16) -- computed independently of this crate's own code, by hand:
    //   c0 = 16
    //   c1 = round_up(ceil(16  * 5 / 4), 16) = round_up(20,  16) = 32
    //   c2 = round_up(ceil(32  * 5 / 4), 16) = round_up(40,  16) = 48
    //   c3 = round_up(ceil(48  * 5 / 4), 16) = round_up(60,  16) = 64
    //   c4 = round_up(ceil(64  * 5 / 4), 16) = round_up(80,  16) = 80
    //   c5 = round_up(ceil(80  * 5 / 4), 16) = round_up(100, 16) = 112
    //   c6 = round_up(ceil(112 * 5 / 4), 16) = round_up(140, 16) = 144
    //   c7 = round_up(ceil(144 * 5 / 4), 16) = round_up(180, 16) = 192
    const GOLDEN: [usize; 8] = [16, 32, 48, 64, 80, 112, 144, 192];
    const MIN_BLOCK: usize = 16;
    const GEO_COUNT: usize = 8;
    const P: Params = Params::new(MIN_BLOCK, (5, 4), GEO_COUNT, &[], 1 << 20);
    const T: [usize; GEO_COUNT] = build_table::<GEO_COUNT>(&P);
    assert_eq!(
        T, GOLDEN,
        "geometric run drifted from the hand-derived golden values"
    );
}

// ---------------------------------------------------------------------------
// task #701 (rust-intel audit §B26, MEDIUM): the geometric-advance multiply
// (`cur * num`) was a bare, unchecked `usize` multiply on a value that grows
// on every geometric step -- in a release profile (this crate does not
// control the consumer's `overflow-checks` setting) it would silently WRAP,
// and the subsequent `next <= cur` min-step fallback masked the wrap into a
// valid-looking, strictly-increasing table (min_block-sized steps instead of
// the requested geometry) rather than surfacing an error anywhere.
// `build_size2class`'s own monotonicity check cannot catch this, since the
// masked table genuinely IS strictly increasing.
//
// docs/reviews/2026-08-07-size-classes-rust-intel-audit.md §B26.

#[test]
#[should_panic(expected = "geometric progression overflows usize")]
fn geometric_advance_overflow_panics_instead_of_silently_wrapping() {
    // min_block = 2^63 (the largest representable power-of-two usize on a
    // 64-bit target), growth = doubling (2/1), geo_count = 2 -- the very
    // first advance step computes `cur.checked_mul(num)` = `(1 << 63) * 2` =
    // `1 << 64`, which overflows `usize::MAX` (`(1 << 64) - 1`) by exactly
    // one bit. This is the smallest possible reproduction: geo_count = 1
    // never advances at all (the loop body only advances when
    // `gi < geo_count` after the FIRST push), so 2 is the minimum count that
    // actually reaches the checked multiply.
    // size-classes publication audit run 2 (Claude, review-2 F6): a literal
    // `1usize << 63` is a hard `const`-eval compile error on a 32-bit
    // target (shift amount >= the type's bit width). `usize::BITS - 1` is
    // the portable, semantically identical form -- "the largest
    // representable power-of-two usize" scales with the target, and every
    // overflow computed below scales proportionally with it.
    const MIN_BLOCK: usize = 1usize << (usize::BITS - 1);
    const GEO_COUNT: usize = 2;
    const N: usize = GEO_COUNT;
    let params = Params::new(MIN_BLOCK, (2, 1), GEO_COUNT, &[], 1 << 20);
    let _ = build_table::<N>(&params);
}

// fh publication audit P4-2: `build_table`'s own `# Panics` doc (lib.rs)
// cites `geo_count = 183` (64-bit) / `84` (32-bit) as the first-overflowing
// `geo_count` for `min_block = 16, growth = (5, 4)` -- this crate's own
// tests' example scheme -- but until now no test on either width actually
// pinned it (independently re-derived by exact-integer replay before writing
// these: 182/83 succeed, 183/84 overflow). One `cfg`-gated pair per width,
// mirroring the `extreme64_overflow` module's width-specific-fixture
// pattern; the 32-bit pair only ever runs under a genuine 32-bit `usize`
// target (e.g. `--target i686-unknown-linux-gnu`), never under the default
// 64-bit host build.
#[cfg(target_pointer_width = "64")]
#[test]
fn sefer_growth_geo_count_182_is_the_last_that_fits_on_64_bit() {
    const GEO_COUNT: usize = 182;
    const N: usize = GEO_COUNT;
    let params = Params::new(16, (5, 4), GEO_COUNT, &[], 1 << 20);
    let _ = build_table::<N>(&params);
}

#[cfg(target_pointer_width = "64")]
#[test]
#[should_panic(expected = "geometric progression overflows usize")]
fn sefer_growth_geo_count_183_overflows_on_64_bit() {
    const GEO_COUNT: usize = 183;
    const N: usize = GEO_COUNT;
    let params = Params::new(16, (5, 4), GEO_COUNT, &[], 1 << 20);
    let _ = build_table::<N>(&params);
}

#[cfg(target_pointer_width = "32")]
#[test]
fn sefer_growth_geo_count_83_is_the_last_that_fits_on_32_bit() {
    const GEO_COUNT: usize = 83;
    const N: usize = GEO_COUNT;
    let params = Params::new(16, (5, 4), GEO_COUNT, &[], 1 << 20);
    let _ = build_table::<N>(&params);
}

#[cfg(target_pointer_width = "32")]
#[test]
#[should_panic(expected = "geometric progression overflows usize")]
fn sefer_growth_geo_count_84_overflows_on_32_bit() {
    const GEO_COUNT: usize = 84;
    const N: usize = GEO_COUNT;
    let params = Params::new(16, (5, 4), GEO_COUNT, &[], 1 << 20);
    let _ = build_table::<N>(&params);
}

// task #755's closing review (F4, MEDIUM): the min-step fallback (`next =
// cur + min_block`, taken when the geometric advance does not exceed `cur`)
// had a bare `+` sharing the exact overflow hazard #701 fixed on its two
// neighbouring `checked_*` calls -- named in #701's own commit message as
// a known-but-unfixed sibling. `growth.0 == 0` is a docs-blessed valid
// scheme (see `build_table`'s own doc comment) whose EVERY advance step
// goes through this exact fallback, since a zero numerator always computes
// `next == 0 <= cur`. `min_block = 1 << 63` doubling via repeated
// min-block steps overflows on the second step's fallback add.
#[test]
#[should_panic(expected = "geometric progression overflows usize")]
fn min_step_fallback_overflow_panics_instead_of_silently_wrapping() {
    // size-classes publication audit run 2 (Claude, review-2 F6): a literal
    // `1usize << 63` is a hard `const`-eval compile error on a 32-bit
    // target (shift amount >= the type's bit width). `usize::BITS - 1` is
    // the portable, semantically identical form -- "the largest
    // representable power-of-two usize" scales with the target, and every
    // overflow computed below scales proportionally with it.
    const MIN_BLOCK: usize = 1usize << (usize::BITS - 1);
    const GEO_COUNT: usize = 2;
    const N: usize = GEO_COUNT;
    // growth.0 == 0 forces every advance through the min-step fallback
    // (the geometric term is always 0, which never exceeds `cur`).
    let params = Params::new(MIN_BLOCK, (0, 1), GEO_COUNT, &[], 1 << 20);
    let _ = build_table::<N>(&params);
}

// ---------------------------------------------------------------------------
// size-classes publication audit run 1 (Sol-codex, P2-1): `size2class_len`'s
// trailing `+ 1` was a bare add on a value that can legitimately be
// `usize::MAX` (`max_class / min_block`), so a release build silently
// wrapped to `0` instead of panicking -- the exact class of release-silent,
// profile-dependent bug the `checked_mul`/`checked_add` fixes above already
// exist to prevent for the geometric advance. These are runtime (not
// `const`) calls to keep the reproduction simple (no extra `const`
// binding); the bug these tests pin is release-profile-specific either
// way, `const` or runtime -- const-eval overflow checks follow the
// `overflow-checks` profile for a `const fn`'s body (task #1423/#1431,
// empirically verified), not a blanket "always traps" rule.

#[test]
#[should_panic(expected = "size2class_len: max_class / min_block + 1 overflows usize")]
fn size2class_len_overflow_panics_instead_of_silently_wrapping() {
    // usize::MAX / 1 == usize::MAX; the `+ 1` then overflows. Pre-fix this
    // silently returned 0 in release instead of panicking in every profile.
    let _ = size2class_len(usize::MAX, 1);
}

#[test]
#[should_panic(expected = "size2class_len: max_class / min_block + 1 overflows usize")]
fn build_size2class_l_check_overflow_panics_instead_of_accepting_a_wrong_l() {
    // The report's own counterexample: N = 1, table = [usize::MAX],
    // min_block = 1 -> the mathematically correct L is `size2class_len`'s
    // own overflow panic, since `usize::MAX / 1 + 1` does not fit in
    // `usize`. Pre-fix, `build_size2class`'s inline `small_max / min_block +
    // 1` wrapped to 0 in release, so `L == 0` (a genuinely malformed,
    // too-short lookup for this table) satisfied the check instead of being
    // rejected. Since the fix makes the `L` check delegate to
    // `size2class_len` itself, this must now panic with that function's own
    // message in every profile, not silently build an empty `[u8; 0]`
    // lookup.
    let table: [usize; 1] = [usize::MAX];
    let _ = build_size2class::<1, 0>(&table, 1);
}

// ---------------------------------------------------------------------------
// task #1417, P2-1 items 3 and 4: the two checked_* sites cc94a46 fixed but
// its own regression tests did not pin -- `build_size2class`'s per-bucket
// `need` clamp and `class_for`'s slow-path round-up. Both are exercised
// through one shared, entirely `Params`-assemblable 64-bit scheme:
//
//   min_block = 1 << 62, geo_count = 1, extras = [2 << 62, 3 << 62]
//     -> table = [1 << 62, 2 << 62, 3 << 62], small_max = 3 << 62,
//        L = size2class_len(3 << 62, 1 << 62) = 4.
//
// geo_count = 1 never reaches the geometric advance (that only runs after
// the FIRST push, guarded by `gi < geo_count`), so the scheme is legal for
// any growth pair; (5, 4) matches the rest of this file.
//
// docs/reviews/2026-08-26-102907-size-classes-publication-audit-run-1-Sol-codex.md
// (P2-1), fixed in cc94a46.

// size-classes publication audit run 2 (Claude, review-2 F6, extended past
// the report's own two named sites): every const in this module uses a
// shift of 62 or 63, each a hard `const`-eval compile error on a 32-bit
// target -- the same class of portability landmine F6 flagged elsewhere in
// this file, just introduced afterward by the tests below (task #1423).
// `#[cfg(target_pointer_width = "64")]` on the whole module is the fix the
// F1 finding itself already recommended for exactly this shape of test.
#[cfg(target_pointer_width = "64")]
mod extreme64_overflow {
    use super::*;

    const EXTREME64_MIN_BLOCK: usize = 1usize << 62;
    const EXTREME64_GEO_COUNT: usize = 1;
    const EXTREME64_EXTRAS: &[usize] = &[2 << 62, 3 << 62];
    const EXTREME64_N: usize = EXTREME64_GEO_COUNT + EXTREME64_EXTRAS.len();
    const EXTREME64_SMALL_MAX: usize = 3 << 62;
    const EXTREME64_L: usize = size2class_len(EXTREME64_SMALL_MAX, EXTREME64_MIN_BLOCK);
    const EXTREME64_PARAMS: Params = Params::new(
        EXTREME64_MIN_BLOCK,
        (5, 4),
        EXTREME64_GEO_COUNT,
        EXTREME64_EXTRAS,
        1 << 20,
    );
    // The const-built scheme. With the clamp fix reverted, THIS const is a
    // hard E0080 compile error in every CHECKED context (a debug-profile build
    // of this test target) -- one of the two failure modes the first test below
    // uses to detect a reverted fix; see that test's comment for why NOTHING
    // can detect it under `--release`.
    const EXTREME64_SC: SizeClasses<EXTREME64_N, EXTREME64_L> =
        SizeClasses::build(EXTREME64_PARAMS);

    /// The same scheme built at runtime -- a plain call of the `const fn`, so
    /// overflow follows the RUNTIME profile (release wraps, debug traps), unlike
    /// `EXTREME64_SC`, which const-evaluates once at compile time.
    fn extreme64_scheme_runtime() -> SizeClasses<EXTREME64_N, EXTREME64_L> {
        SizeClasses::build(EXTREME64_PARAMS)
    }

    #[test]
    fn build_size2class_bucket_need_overflow_clamps_to_last_class() {
        // The top bucket k = L-1 = 3 computes (k + 1) * min_block = 4 * (1<<62)
        // = 2^64, which does not fit in `usize`. cc94a46's `checked_mul` folds
        // that into the existing `_ => small_max` clamp; the bare multiply it
        // replaced wrapped to 0 in release.
        //
        // What the fix observably changed, and what this test therefore pins:
        // pre-fix the scheme above was UNBUILDABLE in every CHECKED context --
        // const evaluation was a hard E0080 compile error, and a runtime call
        // panicked ("attempt to multiply with overflow"). Both halves are
        // exercised here (`EXTREME64_SC` for the const path,
        // `extreme64_scheme_runtime()` for the runtime path), so a reverted fix
        // fails this test twice over in a debug build.
        //
        // Under `--release` the reverted fix is, by contrast, observationally
        // EQUIVALENT -- no test can fail against it there, which is worth
        // stating because the wrap itself is real: the top bucket is the only
        // one that can overflow, and it wraps to exactly 0 (for every scheme
        // whose top bucket overflows, L * min_block = small_max + min_block =
        // 2^64); a wrapped `need = 0` can only make the monotone pointer
        // advance LESS, and by bucket L-2 (whose need is exactly `small_max`)
        // the pointer already sits on the last class -- so the wrapped and
        // clamped tables coincide. Release const-eval wraps too (const-eval
        // overflow checks follow the profile for const-fn bodies, unlike
        // literal expressions such as `const X: u8 = 255 + 1`, which error in
        // every profile). What remains release-pinned here is the table itself:
        // the correct `[0, 1, 2, 2]`, verified against an overflow-safe
        // reference below.
        let rt = extreme64_scheme_runtime();

        // Both evaluation contexts agree, and the scheme has the promised shape.
        assert_eq!(EXTREME64_SC.table(), &[1usize << 62, 2 << 62, 3 << 62]);
        assert_eq!(rt.table(), EXTREME64_SC.table());
        assert_eq!(rt.size2class(), EXTREME64_SC.size2class());

        // The clamp itself: the overflowing top bucket must resolve to the LAST
        // class (the `small_max` sentinel), in both evaluation contexts.
        assert_eq!(
            rt.size2class()[EXTREME64_L - 1],
            (EXTREME64_N - 1) as u8,
            "top bucket must clamp to the last class"
        );
        assert_eq!(
            EXTREME64_SC.size2class()[EXTREME64_L - 1],
            (EXTREME64_N - 1) as u8
        );

        // Full-bucket scan, in the style of sefer_size2class_matches_scan_for_every_bucket.
        // The reference `need` clamps the MULTIPLIER first: the true mathematical
        // (k+1)*min_block exceeds small_max exactly when k+1 exceeds
        // small_max/min_block, so the reference itself cannot overflow -- unlike
        // the crate formula under test, which must survive (k+1)*min_block
        // overflowing at k = L-1 and still land on the same answer.
        let max_multiplier = EXTREME64_SMALL_MAX / EXTREME64_MIN_BLOCK; // 3
        for (k, &class_idx) in rt.size2class().iter().enumerate() {
            let need = (k + 1).min(max_multiplier) * EXTREME64_MIN_BLOCK;
            let want = rt.table().iter().position(|&b| b >= need).unwrap();
            assert_eq!(
                class_idx as usize, want,
                "SIZE2CLASS[{k}] drift (need={need})"
            );
        }
    }

    #[test]
    fn raw_index_via_shift_avoids_the_overflowing_l_times_min_block_bound() {
        // The size2class() doc (Sol-run6 P2-1/task #1467) warns against
        // deriving the false-sentinel window as `size <= L * min_block()` --
        // for THIS scheme `L * min_block == 4 * (1<<62) == 2^64`, which does
        // not fit `usize`. Pins the doc's actual overflow-free alternative:
        // derive the raw index via `checked_sub` + shift and compare it to
        // `L - 1` directly, never via that overflowing byte-size bound.
        let shift = EXTREME64_SC.min_block_shift();

        for size in [EXTREME64_SMALL_MAX + 1, usize::MAX] {
            let idx = size.checked_sub(1).expect("size > 0 here") >> shift;
            assert_eq!(
                idx,
                EXTREME64_L - 1,
                "size={size} must land on the sentinel bucket without computing L * min_block"
            );
            assert_eq!(
                EXTREME64_SC.size2class()[idx] as usize,
                EXTREME64_N - 1,
                "size={size} sentinel must resolve to the last class"
            );
        }
    }

    #[test]
    fn class_for_next_multiple_overflow_returns_none() {
        let sc = extreme64_scheme_runtime();
        // Companion sanity: the slow path resolves normally on this scheme when
        // a representable multiple EXISTS -- 2<<62 is itself a multiple of
        // 1<<63, so the very first probe returns before any round-up.
        assert_eq!(sc.class_for(2 << 62, 1 << 63), Some(1));
        // The pinned case: 3<<62 seeds the last class, which is NOT a multiple
        // of 1<<63; the next multiple of 1<<63 above it is 2^64, unrepresentable
        // in usize. `(3<<62) | ((1<<63)-1)` is usize::MAX, so `checked_add(1)`
        // yields None and `class_for` must return None -- the same outcome the
        // `next_mult > small_max` clamp already produces for every other
        // out-of-range case on this path. Pre-fix (bare `+ 1`), release wrapped
        // usize::MAX + 1 to 0, and the subsequent `(next_mult - 1)` index then
        // wrapped back around to the same seed class -- an infinite loop that
        // never returns; debug trapped on the add itself instead.
        assert_eq!(sc.class_for(3 << 62, 1 << 63), None);
    }

    #[test]
    fn build_size2class_bucket_need_overflow_flips_the_release_answer_for_a_hand_built_table() {
        // Follow-up to `build_size2class_bucket_need_overflow_clamps_to_last_class`
        // above: that test's own comment explains why the clamp fix is
        // observationally inert under `--release` for ANY `Params`-assembled
        // scheme -- `build_table` forces every table entry (geometric AND
        // extras) to be a multiple of `min_block`, which forces an overflowing
        // top bucket to satisfy `L * min_block == small_max + min_block ==
        // 2^64`, wrapping to exactly 0; a `need = 0` cannot move the pointer
        // backward, and the pointer is already parked on the last class by the
        // PRECEDING bucket in that specific case.
        //
        // `build_size2class` is a standalone public entry point though (its own
        // doc: "Build the O(1) size→class lookup FROM A TABLE"), and review-2's
        // F9 finding is exactly that its defense-in-depth path never checks
        // `small_max % min_block == 0` for a hand-built table -- so a caller
        // bypassing `build_table` can supply one where that identity does NOT
        // hold. This table does that: `small_max = (3 << 62) + 5` is 5 past a
        // multiple of `min_block`, which makes the PRE-overflow bucket's need
        // (`3 << 62`) strictly less than `small_max`, so the pointer stops at
        // class_idx = 2 (table[2] = (3 << 62) + 2) one slot short of the last
        // class -- BEFORE the overflowing final bucket ever runs. A wrapped
        // `need = 0` there then leaves the pointer parked at 2 instead of
        // advancing to 3: a WRONG top-bucket answer under `--release`, with no
        // panic at all -- the exact release-silent, profile-dependent
        // misclassification P2-1 originally flagged. Verified independently via
        // exact-width (u128) arithmetic before writing this test: fixed table =
        // `[0, 1, 2, 3]`, pre-fix (bare multiply) table = `[0, 1, 2, 2]`.
        const MIN_BLOCK: usize = 1usize << 62;
        const N: usize = 4;
        const TABLE: [usize; N] = [1 << 62, 2 << 62, (3 << 62) + 2, (3 << 62) + 5];
        const SMALL_MAX: usize = TABLE[N - 1];
        const L: usize = size2class_len(SMALL_MAX, MIN_BLOCK);

        let s2c = build_size2class::<N, L>(&TABLE, MIN_BLOCK);

        assert_eq!(
            s2c[L - 1],
            (N - 1) as u8,
            "top bucket must resolve to the last class, not the pointer's pre-overflow position"
        );

        // Full-bucket scan against a u128 reference, which cannot overflow at
        // any k (unlike a usize replay of the crate's own `(k + 1) * min_block`,
        // which would hit exactly the same overflow this test exists to catch).
        for (k, &class_idx) in s2c.iter().enumerate() {
            let need_u128 = (k as u128 + 1) * MIN_BLOCK as u128;
            let want_need = if need_u128 < SMALL_MAX as u128 {
                need_u128 as usize
            } else {
                SMALL_MAX
            };
            let want = TABLE.iter().position(|&b| b >= want_need).unwrap();
            assert_eq!(class_idx as usize, want, "size2class[{k}] drift");
        }
    }
}

// ---------------------------------------------------------------------------
// task #729 (rust-intel audit §F2/§B26, MEDIUM): `class_for`'s documented fit
// predicate ("`block_size % align == 0`") was silently violated by BOTH
// paths for a non-power-of-two `align` -- the fast path never checked
// divisibility at all, and the slow path's bitmask round-up is only correct
// for a power-of-two `align`. Neither path panicked; both could return a
// non-conforming class or a wrong `None`. `align` must be a power of two is
// now a documented precondition, enforced by a `debug_assert!` (deliberately
// NOT a release-active `assert!` -- see the doc comment on `class_for` for
// why this differs from task #701's promotion).
//
// docs/reviews/2026-08-07-size-classes-rust-intel-audit.md §F2 (lib.rs:408)
// and the companion §B26 (lib.rs:432).

// task #755's closing review (F3, HIGH): the guard under test is a
// `debug_assert!`, which compiles away entirely in `--release` (debug
// assertions off). `#[should_panic]` against a guard that cannot fire in
// release makes this test itself fail under `cargo test --release` --
// reproduced verbatim: "test did not panic as expected". Gated to the only
// profile that can satisfy it; release has nothing to assert here by
// design (see `class_for`'s own doc comment on why this is deliberately
// debug-only, not promoted to a release-active `assert!`).
#[cfg(debug_assertions)]
#[test]
#[should_panic(expected = "align must be a power of two")]
fn class_for_non_pow2_align_violates_debug_assert() {
    // A tiny scheme is enough: the debug_assert fires before either the
    // fast-path or slow-path arithmetic even runs, so the scheme's actual
    // shape does not matter for this test.
    const MIN_BLOCK: usize = 16;
    const N: usize = 4;
    const P: Params = Params::new(MIN_BLOCK, (5, 4), N, &[], 1 << 20);
    const T: [usize; N] = build_table::<N>(&P);
    const L: usize = size2class_len(T[N - 1], MIN_BLOCK);
    const SC: SizeClasses<N, L> = SizeClasses::build(P);
    // align = 6 is not a power of two -- exactly the out-of-contract shape
    // §F2/§B26 describe (neither 1, 2, 4, 8, ... nor a Layout-derived value).
    let _ = SC.class_for(32, 6);
}

// MS prepublish review, task #1500 (P1-1, fh's recommended fix B): a checked
// twin of `class_for` for callers whose `align` is not already known-valid
// by construction. Runs in BOTH profiles (no `#[cfg(debug_assertions)]`
// gate) -- the validation is the function's own explicit job, not a
// debug-only guard.
#[test]
fn try_class_for_matches_class_for_on_every_valid_input() {
    for &(size, align) in &[(1usize, 1usize), (200, 16), (1025, 256), (2049, 1024)] {
        assert_eq!(
            SEFER_SC.try_class_for(size, align),
            Ok(SEFER_SC.class_for(size, align)),
            "size={size}, align={align}"
        );
    }
}

#[test]
fn try_class_for_rejects_non_pow2_align() {
    assert_eq!(SEFER_SC.try_class_for(32, 6), Err(InvalidAlign(6)));
}

// The exact corner `class_for` cannot handle safely even in release: `align
// == 0, size == 0` underflows `need - 1` to `usize::MAX`, which then panics
// on the out-of-bounds `size2class` index (an unconditional bounds check,
// not a compiled-away debug_assert -- see class_for_non_pow2_align_violates
// _debug_assert's own doc comment on why THAT guard is debug-only; indexing
// panics are not). `try_class_for` must reject align=0 before any of that
// arithmetic runs, in every profile.
#[test]
fn try_class_for_rejects_zero_align_without_panicking() {
    assert_eq!(SEFER_SC.try_class_for(0, 0), Err(InvalidAlign(0)));
}

#[test]
fn invalid_align_display_names_the_offending_value() {
    let msg = InvalidAlign(6).to_string();
    assert!(msg.contains('6'), "got: {msg}");
}

#[test]
fn is_huge_uses_the_policy_threshold_not_an_os_constant() {
    // huge_threshold is a pure Params policy value; the crate never references
    // an OS segment size. Two different thresholds → two different verdicts for
    // the same size, proving it is parameterized.
    //
    // task #730 (rust-intel audit §D1, INFO): this comment's claim was
    // previously asserted only in PROSE -- the test built a single scheme
    // (huge_threshold: 1024), so an `is_huge` HARDCODED to compare against
    // the literal 1024 would have passed. The `>=` boundary pin (1023 vs
    // 1024) was real, so the test was not vacuous, only under-delivering on
    // its own stated claim. Now builds a SECOND scheme with a DIFFERENT
    // threshold (4096) and asserts the SAME size (2048) gets OPPOSITE
    // verdicts across the two schemes -- the actual parameterization proof
    // the comment above promises.
    const P_SMALL: Params = Params::new(16, (5, 4), 4, &[], 1024);
    const N: usize = 4;
    const T: [usize; N] = build_table::<N>(&P_SMALL);
    const L: usize = size2class_len(T[N - 1], 16);
    const SC: SizeClasses<N, L> = SizeClasses::build(P_SMALL);
    assert!(SC.is_huge(1024));
    assert!(SC.is_huge(4096));
    assert!(!SC.is_huge(1023));

    const P_LARGE_THRESHOLD: Params = Params::new(16, (5, 4), 4, &[], 4096);
    const T2: [usize; N] = build_table::<N>(&P_LARGE_THRESHOLD);
    const L2: usize = size2class_len(T2[N - 1], 16);
    const SC2: SizeClasses<N, L2> = SizeClasses::build(P_LARGE_THRESHOLD);
    const PROBE: usize = 2048;
    assert!(
        SC.is_huge(PROBE),
        "SC (threshold 1024) must call {PROBE} huge"
    );
    assert!(
        !SC2.is_huge(PROBE),
        "SC2 (threshold 4096) must NOT call {PROBE} huge -- same size, opposite \
         verdict across the two schemes proves is_huge is genuinely parameterized \
         by Params::huge_threshold, not hardcoded"
    );
}

// ---------------------------------------------------------------------------
// size-classes publication audit run 2 (Sol-codex, P2-1): the `u8` capacity
// bound was `N < 256`, off by one -- entries are class INDICES (`0..=N-1`),
// not the count, so 256 classes fit exactly. These three pin the corrected
// boundary from both sides.

/// A 256-class linear scheme: `min_block = 1`, `growth = (0, 1)` (the
/// docs-blessed min-step degradation), so the table is exactly `1..=256`.
const MAX_N: usize = 256;
const MAX_PARAMS: Params = Params::new(1, (0, 1), MAX_N, &[], 1 << 20);
const MAX_TABLE: [usize; MAX_N] = build_table::<MAX_N>(&MAX_PARAMS);
const MAX_L: usize = size2class_len(MAX_TABLE[MAX_N - 1], 1);

#[test]
fn exactly_256_classes_build_and_index_up_to_255() {
    // 256 classes -> indices 0..=255, all representable in u8.
    assert_eq!(MAX_TABLE[0], 1);
    assert_eq!(MAX_TABLE[MAX_N - 1], 256);
    assert_eq!(MAX_L, 257);

    let s2c = build_size2class::<MAX_N, MAX_L>(&MAX_TABLE, 1);

    // The largest index actually produced is 255 -- exactly u8::MAX, not a
    // truncation of 256.
    assert_eq!(
        s2c[MAX_L - 2],
        u8::MAX,
        "bucket for the largest in-range size must resolve to class 255"
    );
    assert_eq!(
        s2c[MAX_L - 1],
        u8::MAX,
        "top bucket clamps to the last class"
    );

    // Full scan against an independent reference: every bucket's answer is
    // the smallest class >= (k+1)*min_block, clamped at small_max.
    for (k, &class_idx) in s2c.iter().enumerate() {
        let need = (k + 1).min(MAX_TABLE[MAX_N - 1]);
        let want = MAX_TABLE.iter().position(|&b| b >= need).unwrap();
        assert_eq!(class_idx as usize, want, "size2class[{k}] drift");
    }
}

#[test]
#[should_panic(expected = "the class count must not exceed 256")]
fn exactly_257_classes_are_rejected() {
    // One past the boundary: index 256 is NOT representable in u8.
    const N: usize = 257;
    const PARAMS: Params = Params::new(1, (0, 1), N, &[], 1 << 20);
    let table = build_table::<N>(&PARAMS);
    let _ = build_size2class::<N, 258>(&table, 1);
}

// ---------------------------------------------------------------------------
// size-classes publication audit run 2 (Sol-codex, P3-1): the advance step
// used to reject a scheme whose intermediate `cur * num` overflows even when
// the actual next class is representable. These two pin both directions of
// the corrected domain.

#[test]
#[cfg(target_pointer_width = "64")]
fn representable_next_class_survives_an_unrepresentable_intermediate_product() {
    // min_block = 2^62, growth = (3, 3), geo_count = 3.
    // Step 2 computes cur * num = 2^63 * 3 = 27670116110564327424, past
    // usize::MAX -- but ceil(cur * 3 / 3) == cur, so the min-step fallback
    // yields 3 * 2^62 = 13835058055282163712, which fits. Pre-fix the
    // checked_mul on the intermediate rejected the whole scheme.
    const MIN_BLOCK: usize = 1usize << 62;
    const N: usize = 3;
    const PARAMS: Params = Params::new(MIN_BLOCK, (3, 3), N, &[], 1 << 20);
    const TABLE: [usize; N] = build_table::<N>(&PARAMS);

    assert_eq!(TABLE, [1usize << 62, 1usize << 63, 3usize << 62]);
    // Every class still fits and the table is still strictly increasing.
    for w in TABLE.windows(2) {
        assert!(w[0] < w[1], "table must be strictly increasing: {w:?}");
    }
}

#[test]
#[cfg(target_pointer_width = "64")]
#[should_panic(expected = "geometric progression overflows usize")]
fn a_genuinely_unrepresentable_next_class_still_panics() {
    // Counterpart to the test above: here the RESULT itself, not just the
    // intermediate, exceeds usize -- min_block = 2^63 doubled is 2^64. The
    // u128 widening must not weaken this into a silent wrap.
    const MIN_BLOCK: usize = 1usize << 63;
    const N: usize = 2;
    const PARAMS: Params = Params::new(MIN_BLOCK, (2, 1), N, &[], 1 << 20);
    let _ = build_table::<N>(&PARAMS);
}

#[test]
fn extras_interleaving_the_geometric_run_is_accepted_and_preserved() {
    // size-classes publication audit run 2 (Sol-codex, P3-3): the docs used
    // to say an extra that "interleaves" with the geometric run is rejected.
    // It is not, and must not be -- placing an exact class the progression
    // skips is the main reason `extras` exists. Only a DUPLICATE is rejected
    // (see extras_overlapping_geometric_run_panics_in_build_table above).
    //
    // The production scheme itself depends on this: all nine SEFER extras
    // land strictly between two geometric neighbours (256 between 240 and
    // 304, and so on). Pinned here on a small scheme so the property is
    // stated directly rather than implied by the big table.
    const MIN_BLOCK: usize = 16;
    const GEO_COUNT: usize = 6;
    // The bare geometric run for these params is 16, 32, 48, 64, 80, 112.
    // 96 is 16-aligned and falls strictly between 80 and 112 -- a gap the
    // progression skips entirely.
    const EXTRAS: &[usize] = &[96];
    const N: usize = GEO_COUNT + EXTRAS.len();
    const PARAMS: Params = Params::new(MIN_BLOCK, (5, 4), GEO_COUNT, EXTRAS, 1 << 20);
    const TABLE: [usize; N] = build_table::<N>(&PARAMS);

    assert_eq!(TABLE, [16, 32, 48, 64, 80, 96, 112]);
    assert!(
        TABLE.contains(&96),
        "an interleaving extra must survive the merge, not be dropped"
    );
    for w in TABLE.windows(2) {
        assert!(w[0] < w[1], "merged table must stay strictly increasing");
    }

    // And it is genuinely reachable through the derived lookup -- not a
    // monotonicity-valid but unindexable slot.
    const L: usize = size2class_len(TABLE[N - 1], MIN_BLOCK);
    const SC: SizeClasses<N, L> = SizeClasses::build(PARAMS);
    let idx = SC.class_for(81, 1).expect("81 B resolves");
    assert_eq!(
        SC.block_size(idx),
        96,
        "a size in the gap the extra fills must resolve TO that extra"
    );
}

#[test]
fn readme_example_compiles_and_derives_its_generics() {
    // README.md is never pulled into rustdoc via `#[doc = include_str!(..)]`,
    // so its example fence (```rust, for GitHub/crates.io highlighting) is
    // never compiled by `cargo test --doc` either. Mirrored here verbatim so
    // it cannot silently rot -- in particular the L-derivation, which used to
    // be a hand-pinned magic 258_752 (Sol-run2 P4-1).
    const MIN_BLOCK: usize = 16;
    const GEO_COUNT: usize = 40;
    const EXTRAS: &[usize] = &[256, 512, 1024, 2048, 4096];
    const PARAMS: Params = Params::new(MIN_BLOCK, (5, 4), GEO_COUNT, EXTRAS, 4 << 20);

    const N: usize = GEO_COUNT + EXTRAS.len();
    const TABLE: [usize; N] = build_table::<N>(&PARAMS);
    const L: usize = size2class_len(TABLE[N - 1], MIN_BLOCK);

    static SC: SizeClasses<N, L> = SizeClasses::build(PARAMS);

    assert_eq!(SC.count(), N);
    assert_eq!(SC.small_max(), TABLE[N - 1]);
    let idx = SC.class_for(100, 8).expect("100 B resolves");
    assert!(SC.block_size(idx) >= 100);
}
