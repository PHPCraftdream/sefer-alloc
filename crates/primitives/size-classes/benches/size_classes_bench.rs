//! `bench-scale-tool` fixed-iteration benches for `size-classes`.
//!
//! Run:
//! ```text
//! cargo bench -p size-classes --bench size_classes_bench -- --calibrate 1
//! cargo bench -p size-classes --bench size_classes_bench
//! ```

use std::hint::black_box;

use bench_scale_tool::Harness;

// Sefer's concrete parameterization (49 classes; the default in-tree
// scheme) and the JUMP_* slow-path pairs are mechanically shared
// with tests/builder.rs via this module (rush-tests review T4/task
// #1479), replacing the former comment-only "keep in sync" convention --
// a single-sided edit can no longer desync the bench from the test.
#[path = "../tests/common/mod.rs"]
mod common;
use common::{
    HUGE_THRESHOLD, JUMP_A, JUMP_B, JUMP_DENSE, JUMP_MULTI, JUMP_NONE, SEFER_L, SEFER_MAX,
    SEFER_MIN_BLOCK, SEFER_N, SEFER_SC,
};

/// Step-by-1 walk over `SEFER_SC` built from the SAME primitives as
/// `SizeClasses::class_for`'s slow path, for the `jump_vs_walk` bench pair in
/// `main` (docs/reviews/2026-08-30-094857-size-classes-claude.md P2-2): the
/// prologue mirrors `class_for`'s line for line (same `need`, same
/// `(need - 1) >> SHIFT` seed index, same `seed_idx >= L - 1` guard, same
/// fast-path exit), and the loop body tests divisibility with the same
/// bitmask (`block & (align - 1) == 0` -- NOT `common::walk_class_for`'s
/// runtime-divisor `is_multiple_of`), reads through the same fixed-size
/// arrays (`SEFER_SC.table()`'s `&[usize; N]` / `SEFER_SC.size2class()`'s
/// `&[u8; L]`, NOT a `&[usize]` slice with runtime bounds checks), and uses
/// the shift precomputed as a `const` (NOT `trailing_zeros` re-derived per
/// call). The single structural difference from `class_for` is the loop's
/// advance -- `i += 1` here vs `class_for`'s round-up-and-reseed jump -- so
/// the pair isolates jump-ahead vs step-by-1 rather than primitive
/// differences. (`common::walk_class_for` is deliberately left with its real
/// division: it is the independent proptest oracle. Same precondition as
/// `class_for`: `align` must be a power of two.)
#[inline]
fn step_by_step_walk(size: usize, align: usize) -> Option<usize> {
    // `class_for` reads this from its precomputed `min_block_shift` field;
    // `SEFER_MIN_BLOCK` is a power of two, so `trailing_zeros` const-folds.
    const SHIFT: u32 = SEFER_MIN_BLOCK.trailing_zeros();
    let need = if size > align { size } else { align };
    let seed_idx = (need - 1) >> SHIFT;
    if seed_idx >= SEFER_L - 1 {
        return None;
    }
    let seed = SEFER_SC.size2class()[seed_idx] as usize;
    if align <= (1usize << SHIFT) {
        return Some(seed);
    }
    let table = SEFER_SC.table();
    let mut i = seed;
    while i < SEFER_N {
        let block = table[i];
        if block & (align - 1) == 0 {
            return Some(i);
        }
        i += 1;
    }
    None
}

fn main() {
    let mut h = Harness::new("size_classes_bench", env!("CARGO_MANIFEST_DIR"));

    // ── class_for/small_hit (fast path) ───────────────────────────────────────
    // Fast path: align <= min_block, so the divisibility check is trivially
    // satisfied — one O(1) lookup. This exercises the hot path that most
    // allocation requests take in practice.

    // Small size with align=1 (align <= min_block=16, so fast path)
    h.bench("class_for/small_hit", || {
        let result = black_box(SEFER_SC.class_for(black_box(32), black_box(1)));
        black_box(result);
    });

    // ── try_class_for/small_hit (same input as class_for/small_hit above) ────
    // MS round-3 prepublish review P2-1: the crate's own docs claim
    // `try_class_for` "adds zero cost to `class_for`'s own hot path" --
    // structurally true (it is a separate function, not a wrapper), but no
    // benchmark had ever exercised `try_class_for` at all, so its OWN cost
    // (the added power-of-two validation before delegating) had no measured
    // number next to `class_for`'s. Same (size, align) as
    // `class_for/small_hit` so the only variable between the two rows is
    // the validation branch.
    h.bench("try_class_for/small_hit", || {
        let result = black_box(SEFER_SC.try_class_for(black_box(32), black_box(1)));
        let _ = black_box(result);
    });

    // ── try_class_for/invalid_align_reject (early Err, no arithmetic runs) ───
    // The other half of `try_class_for`'s contract: a non-power-of-two (here
    // zero) `align` is rejected before `need`/the LUT are ever touched -- in
    // EVERY profile, unlike `class_for`, whose own handling of `align == 0`
    // depends on the build profile (see `class_for`'s own `# Preconditions`).
    h.bench("try_class_for/invalid_align_reject", || {
        let result = black_box(SEFER_SC.try_class_for(black_box(32), black_box(0)));
        let _ = black_box(result);
    });

    // ── class_for/fast_slow_boundary (align == 16 vs align == 32) ────────────
    // size-classes round-6 prepublish review P4-7: every other row sits far
    // from the fast/slow split (align=1 for fast, align>=128 for slow) --
    // nothing brackets the branch the split exists for. `align = 16 =
    // MIN_BLOCK` is the last align the fast path serves; `align = 32` is the
    // first that takes the slow path, and (since class index 1, block 32,
    // is itself 32-divisible) the cheapest possible slow-path case: one
    // divisibility check, zero jump iterations.
    h.bench("class_for/at_min_block_align_fast", || {
        let result = black_box(SEFER_SC.class_for(black_box(32), black_box(16)));
        black_box(result);
    });

    h.bench("class_for/one_past_min_block_align_slow", || {
        let result = black_box(SEFER_SC.class_for(black_box(32), black_box(32)));
        black_box(result);
    });

    // ── class_for/large_align_slow_path (divisibility-jump) ──────────────────
    // Slow path: align > min_block, so we need to check divisibility and
    // potentially jump over non-divisible classes. This is a different
    // asymptotic/cost from the fast path and must be measured separately.
    //
    // size-classes publication audit run 2 (Claude, review-2 F2): these two
    // rows used to call `class_for(256, 256)` / `class_for(1024, 1024)`.
    // Since 256 and 1024 are themselves table entries (`common::SEFER_EXTRAS`),
    // `need = max(size, align) = align` seeds the lookup EXACTLY on an
    // align-divisible class, so the jump loop's round-up-and-reseek body
    // (the actual "slow path" cost) never ran once -- both rows silently
    // measured the same one-check-then-return cost the fast path already
    // takes. Replaced with (size, align) pairs whose seed class is NOT
    // align-divisible, so at least one real jump executes; pinned by
    // `sefer_bench_jump_rows_genuinely_exercise_the_slow_path` in
    // tests/builder.rs (a path-activation oracle, so a future table change
    // can't silently make these rows inert again without a test failing).

    // size=1025, align=256: seed is the 1200 B class (not 256-divisible);
    // the jump lands on the 2048 B class.
    h.bench("class_for/large_align_slow_path", || {
        let result = black_box(SEFER_SC.class_for(black_box(JUMP_A.0), black_box(JUMP_A.1)));
        black_box(result);
    });

    // size=2049, align=1024: seed is the 2368 B class (not 1024-divisible);
    // the jump lands on the 4096 B class.
    h.bench("class_for/large_align_slow_path_1024", || {
        let result = black_box(SEFER_SC.class_for(black_box(JUMP_B.0), black_box(JUMP_B.1)));
        black_box(result);
    });

    // ── class_for/jump_vs_walk (wall-clock: divisibility-jump vs step-by-1) ─
    // The crate claims the slow path's jump is a real optimization over
    // stepping one class at a time, but SEFER's own table is only 49 classes
    // / 392 bytes -- small enough to always be cache-hot, so the win was
    // proven by ITERATION COUNT (fewer loop passes) but never measured by
    // WALL-CLOCK against the linear walk it replaces.
    //
    // Fixture: JUMP_NONE, the deepest slow-path case (seed class 36, block
    // 17760): the jump visits 10 of the 13 remaining classes -- indices 37,
    // 38 and 40 are skipped by the round-up, and that skipping IS the
    // mechanism under test -- while a step-by-1 walk from the same seed must
    // visit all 13 (none of the 13 table entries from 17760 up to 258752 is
    // 16384-divisible); both arms return None. claude publication review
    // P2-2: this pair originally used JUMP_A, on which the jump and the walk
    // visit the IDENTICAL set 18 -> 21 (the jump skips nothing there), so
    // the measured delta reflected only the two functions' incidental
    // primitive differences -- a runtime-divisor `is_multiple_of`,
    // `&[usize]` slice bounds checks, per-call `trailing_zeros` -- not the
    // jump-ahead itself.
    //
    // Both halves are fixed here: the fixture moved to JUMP_NONE (where the
    // jump genuinely skips), and the walk arm moved off
    // `common::walk_class_for` onto the local `step_by_step_walk` above,
    // which is built from `class_for`'s own primitives so primitive
    // differences cannot account for any of the remaining delta.
    // `common::walk_class_for` itself is untouched -- its real division is
    // deliberate independence for the proptest oracle. (The `_jump` arm
    // measures the same call as `slow_path_none` below; it is repeated here
    // so the pair keeps two arms differing in exactly one thing.)

    h.bench("class_for/jump_vs_walk_none_jump", || {
        let result = black_box(SEFER_SC.class_for(black_box(JUMP_NONE.0), black_box(JUMP_NONE.1)));
        black_box(result);
    });

    h.bench("class_for/jump_vs_walk_none_walk", || {
        let result = black_box(step_by_step_walk(
            black_box(JUMP_NONE.0),
            black_box(JUMP_NONE.1),
        ));
        black_box(result);
    });

    // ── class_for/multi_jump (a different table region than JUMP_A/JUMP_B) ───
    // claude publication review P2-1: this row's comment used to claim
    // JUMP_A/JUMP_B "each take exactly ONE jump-loop iteration" and that this
    // row adds a second -- false (JUMP_A takes 4, JUMP_B takes 3; this row's
    // JUMP_MULTI takes only 2, making it the SHALLOWEST of the three, not the
    // deepest). Kept as a distinct row because it seeds from a different table
    // region/density, not because of iteration depth. Exact per-fixture
    // iteration counts (JUMP_A=4, JUMP_B=3, JUMP_MULTI=2, JUMP_DENSE=2,
    // JUMP_NONE=10) are pinned by
    // `sefer_bench_jump_rows_genuinely_exercise_the_slow_path` in
    // tests/builder.rs.
    h.bench("class_for/multi_jump", || {
        let result =
            black_box(SEFER_SC.class_for(black_box(JUMP_MULTI.0), black_box(JUMP_MULTI.1)));
        black_box(result);
    });

    // ── class_for/slow_path_none (jump walks the table, still returns None) ─
    // `above_small_max_rejection` below returns `None` via the EARLY guard,
    // never touching the jump loop at all. This row instead genuinely enters
    // the slow path (need=16385 <= small_max=258752) and takes 10 real
    // iterations -- visiting 10 of the 13 remaining classes (indices 37, 38,
    // 40 skipped by the round-up), none of the visited ones divisible by
    // 16384 -- before the `next_idx >= L - 1` index-space guard fires from
    // inside the loop body on the 10th iteration (at the last class, index
    // 48, still `< N`) and returns `None`; the table is never actually run
    // off the end of. Pinned alongside `multi_jump` above.
    h.bench("class_for/slow_path_none", || {
        let result = black_box(SEFER_SC.class_for(black_box(JUMP_NONE.0), black_box(JUMP_NONE.1)));
        black_box(result);
    });

    // ── class_for/dense_align_slow_path (a different table "density") ────────
    // JUMP_A/JUMP_B/JUMP_MULTI/JUMP_NONE use aligns 256/1024/512/16384; this
    // row uses align=128, under which ~31% of SEFER_TABLE's classes are
    // already divisible (vs ~20% for align=256) -- a denser slow-path point,
    // still genuinely exercising the jump (seed class 6, block 144, is NOT
    // 128-divisible; 2 iterations to `Some(9)` -- see common::JUMP_DENSE's
    // own doc for the derivation).
    h.bench("class_for/dense_align_slow_path", || {
        let result =
            black_box(SEFER_SC.class_for(black_box(JUMP_DENSE.0), black_box(JUMP_DENSE.1)));
        black_box(result);
    });

    // ── class_for/near_small_max (the boundary class_for actually checks) ────
    // size-classes publication audit run 1 (Sol-codex, P3-1): the three rows
    // this section replaces were all named/commented as measuring
    // `huge_threshold` (4 MiB for SEFER_PARAMS), but `class_for` never reads
    // `huge_threshold` at all -- only `is_huge` does (see the section below).
    // `class_for`'s own early-rejection boundary is `small_max`
    // (`SEFER_MAX`, well under 4 MiB for this scheme), so all three old rows
    // fell through the same early-rejection branch (then spelled
    // `need > small_max`; today the `seed_idx >= L - 1` guard) regardless of
    // the "below/at/above" label and measured nothing distinguishable. These
    // three rows exercise the boundary `class_for` genuinely checks.

    // A size one MIN_BLOCK bucket below small_max -- `SEFER_MAX - 1` would
    // seed the SAME bucket (index 16171) as `_at` below, since `SEFER_MAX`
    // is itself a MIN_BLOCK multiple; `- SEFER_MIN_BLOCK` lands one bucket
    // over (16170), making this a genuinely distinct measurement (size-classes
    // round-5 prepublish review P3-2).
    h.bench("class_for/near_small_max_below", || {
        let result =
            black_box(SEFER_SC.class_for(black_box(SEFER_MAX - SEFER_MIN_BLOCK), black_box(1)));
        black_box(result);
    });

    // Exactly at small_max -- the largest resolvable size.
    h.bench("class_for/near_small_max_at", || {
        let result = black_box(SEFER_SC.class_for(black_box(SEFER_MAX), black_box(1)));
        black_box(result);
    });

    // One past small_max -- the early-rejection path (`need` past
    // `small_max`, i.e. the `seed_idx >= L - 1` guard), returning `None`
    // before ever touching the lookup table.
    h.bench("class_for/above_small_max_rejection", || {
        let result = black_box(SEFER_SC.class_for(black_box(SEFER_MAX + 1), black_box(1)));
        black_box(result);
    });

    // ── is_huge/near_huge_threshold (the policy check that DOES read it) ─────
    // `is_huge` is the one method whose cost the `huge_threshold` boundary
    // actually governs -- a plain `size >= self.huge_threshold` comparison,
    // measured directly here rather than indirectly through `class_for`. One
    // row, not a below/at/above triple: unlike `class_for`'s LUT lookup (whose
    // cost can vary by which bucket/class a size lands in), this is a single
    // branch-free compare whose cost cannot differ by operand value -- three
    // rows would measure the same thing three times (size-classes round-5
    // prepublish review P3-2).
    h.bench("is_huge/near_huge_threshold", || {
        let result = black_box(SEFER_SC.is_huge(black_box(HUGE_THRESHOLD)));
        black_box(result);
    });

    h.run();
}
