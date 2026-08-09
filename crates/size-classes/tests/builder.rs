//! Correctness of the `const`-generic builder itself — table shape, the derived
//! O(1) lookup, and the alignment-jump classifier — against sefer's own
//! concrete parameterization (`SEFER_PARAMS` below), via hand-written unit
//! tests (an independent, from-scratch reference builder/classifier, an
//! exhaustive small-size×alignment sweep, and the `Params::extras`
//! precondition `#[should_panic]`s). This file has no proptest of its own —
//! the sibling `tests/proptest_builder.rs` is where `(size, align)` is
//! property-generated, across three additional hand-picked schemes distinct
//! from `SEFER_PARAMS`.

use size_classes::{build_table, size2class_len, Params, SizeClasses};

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
    for _ in 0..geo_count {
        geo.push(cur);
        let mut next = (cur * num).div_ceil(den);
        next = (next + mask) & !mask;
        if next <= cur {
            next = cur + min_block;
        }
        cur = next;
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

// ---------------------------------------------------------------------------
// Sefer's concrete parameterization (49 classes; the default in-tree scheme).
// ---------------------------------------------------------------------------

const SEFER_MIN_BLOCK: usize = 16;
const SEFER_EXTRAS: &[usize] = &[256, 512, 1024, 2048, 4096, 6144, 8192, 12288, 16384];
const SEFER_GEO: usize = 40;
const SEFER_N: usize = SEFER_GEO + SEFER_EXTRAS.len();
const SEFER_PARAMS: Params = Params::new(
    SEFER_MIN_BLOCK,
    (5, 4),
    SEFER_GEO,
    SEFER_EXTRAS,
    4 * 1024 * 1024,
);
const SEFER_TABLE: [usize; SEFER_N] = build_table::<SEFER_N>(&SEFER_PARAMS);
const SEFER_MAX: usize = SEFER_TABLE[SEFER_N - 1];
const SEFER_L: usize = size2class_len(SEFER_MAX, SEFER_MIN_BLOCK);
const SEFER_SC: SizeClasses<SEFER_N, SEFER_L> = SizeClasses::build(SEFER_PARAMS);

#[test]
fn sefer_table_matches_reference_and_is_strictly_increasing() {
    let want = reference_table(SEFER_MIN_BLOCK, (5, 4), SEFER_GEO, SEFER_EXTRAS);
    assert_eq!(&SEFER_TABLE[..], &want[..]);
    // Derive-not-hardcode: the count is whatever the params produce.
    assert_eq!(SEFER_SC.count(), SEFER_N);
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
    for &align in &aligns {
        for size in 1..=(SEFER_MAX + 1) {
            let got = SEFER_SC.class_for(size, align);
            let want = reference_class_for(&SEFER_TABLE, size, align);
            assert_eq!(got, want, "drift at size={size} align={align}");
            if let Some(idx) = got {
                let block = SEFER_TABLE[idx];
                assert!(block >= size.max(align));
                assert!(block.is_multiple_of(align));
            }
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

#[test]
#[should_panic(expected = "strictly increasing")]
fn extras_overlapping_geometric_run_panics() {
    // min_block=16, extras=[16, 32], geo_count=8 — the exact §5.1(b)
    // reproduction: both extras already appear in the geometric run
    // (pre-fix: table = [16, 16, 32, 32, 48, 64, 80, 112, 144, 192], indices
    // 1 and 3 permanently unreachable, no diagnostic). The overlap collapses
    // adjacent slots to equal values, so it surfaces as a strict-increase
    // violation in `build_size2class` (the monotonicity chokepoint), which
    // is what `SizeClasses::build` reaches on a non-const call.
    const MIN_BLOCK: usize = 16;
    const EXTRAS: &[usize] = &[16, 32];
    const GEO_COUNT: usize = 8;
    const N: usize = GEO_COUNT + EXTRAS.len();
    let params = Params::new(MIN_BLOCK, (5, 4), GEO_COUNT, EXTRAS, 1 << 20);
    // `extras` here IS strictly increasing and IS min_block-aligned, so
    // `build_table` alone accepts it (case (b) is purely an overlap with the
    // geometric run, not an `extras`-internal-shape defect) — the violation
    // only becomes visible once merged into the full table.
    let table = build_table::<N>(&params);
    let max_class = table[N - 1];
    let l = size2class_len(max_class, MIN_BLOCK);
    // The table's real max is 192 (the geometric run 16,32,48,64,80,112,144,192
    // merged with extras=[16,32], per the comment above). `size2class_len(200,
    // MIN_BLOCK)` is used here only because `size2class_len` floor-divides
    // identically for both inputs (192/16+1 == 200/16+1 == 13) — this asserts
    // the equivalence, not that 200 is the table's actual maximum.
    assert_eq!(
        l,
        size2class_len(192, MIN_BLOCK),
        "sanity: expected max 192 (floor-division-equivalent to size2class_len(200, ..))"
    );
    // Route through `SizeClasses::build` (the real chokepoint,
    // `build_size2class` internally) rather than calling `build_size2class`
    // directly, since `L` must be a const generic matching `l` above.
    const L: usize = size2class_len(200, MIN_BLOCK);
    let _ = SizeClasses::<N, L>::build(params);
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
    const MIN_BLOCK: usize = 1usize << 63;
    const GEO_COUNT: usize = 2;
    const N: usize = GEO_COUNT;
    let params = Params::new(MIN_BLOCK, (2, 1), GEO_COUNT, &[], 1 << 20);
    let _ = build_table::<N>(&params);
}

#[test]
fn is_huge_uses_the_policy_threshold_not_an_os_constant() {
    // huge_threshold is a pure Params policy value; the crate never references
    // an OS segment size. Two different thresholds → two different verdicts for
    // the same size, proving it is parameterized.
    const P_SMALL: Params = Params::new(16, (5, 4), 4, &[], 1024);
    const N: usize = 4;
    const T: [usize; N] = build_table::<N>(&P_SMALL);
    const L: usize = size2class_len(T[N - 1], 16);
    const SC: SizeClasses<N, L> = SizeClasses::build(P_SMALL);
    assert!(SC.is_huge(1024));
    assert!(SC.is_huge(4096));
    assert!(!SC.is_huge(1023));
}
