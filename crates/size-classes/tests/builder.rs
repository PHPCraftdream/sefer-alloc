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
