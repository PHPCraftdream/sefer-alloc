//! Correctness of the `const`-generic builder itself — table shape, the derived
//! O(1) lookup, and the alignment-jump classifier — against sefer's own
//! concrete parameterization (`SEFER_PARAMS` below), via hand-written unit
//! tests (an independent, from-scratch reference builder/classifier, an
//! exhaustive small-size×alignment sweep, and the `Params::extras`
//! precondition `#[should_panic]`s). This file has no proptest of its own —
//! the sibling `tests/proptest_builder.rs` is where `(size, align)` is
//! property-generated, across three additional hand-picked schemes distinct
//! from `SEFER_PARAMS`.

use size_classes::{build_size2class, build_table, size2class_len, Params, SizeClasses};

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
    for &(size, align) in &[(1025usize, 256usize), (2049usize, 1024usize)] {
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
#[should_panic(expected = "table must be strictly increasing (check Params::extras for overlap")]
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
    // "соседний непроверенный случай": `extras = [0]` passed both the
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
    const MIN_BLOCK: usize = 1usize << 63;
    const GEO_COUNT: usize = 2;
    const N: usize = GEO_COUNT;
    let params = Params::new(MIN_BLOCK, (2, 1), GEO_COUNT, &[], 1 << 20);
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
    const MIN_BLOCK: usize = 1usize << 63;
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
// `const`) calls specifically because `const` evaluation already traps on
// overflow regardless of profile -- the bug these tests pin only existed at
// runtime, in `release`, before the fix.

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
    // The report's own contrpример: N = 1, table = [usize::MAX],
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
const EXTREME64_SC: SizeClasses<EXTREME64_N, EXTREME64_L> = SizeClasses::build(EXTREME64_PARAMS);

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
