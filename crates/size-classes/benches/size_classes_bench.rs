//! `bench-scale-tool` fixed-iteration benches for `size-classes` (task #761).
//! This crate previously had zero benches of its own — it is a lookup-table
//! crate, so incorrect perf claims would be particularly misleading.
//!
//! Run:
//! ```text
//! cargo bench -p size-classes --bench size_classes_bench -- --calibrate 1
//! cargo bench -p size-classes --bench size_classes_bench
//! ```

use std::hint::black_box;

use bench_scale_tool::Harness;

// Sefer's concrete parameterization (49 classes; the default in-tree
// scheme) and the JUMP_A/JUMP_B slow-path pairs are mechanically shared
// with tests/builder.rs via this module (rush-tests review T4/task
// #1479), replacing the former comment-only "keep in sync" convention --
// a single-sided edit can no longer desync the bench from the test.
#[path = "../tests/common/mod.rs"]
mod common;
use common::{HUGE_THRESHOLD, JUMP_A, JUMP_B, SEFER_MAX, SEFER_SC};

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

    // ── class_for/near_small_max (the boundary class_for actually checks) ────
    // size-classes publication audit run 1 (Sol-codex, P3-1): the three rows
    // this section replaces were all named/commented as measuring
    // `huge_threshold` (4 MiB for SEFER_PARAMS), but `class_for` never reads
    // `huge_threshold` at all -- only `is_huge` does (see the section below).
    // `class_for`'s own early-rejection boundary is `small_max`
    // (`SEFER_MAX`, well under 4 MiB for this scheme), so all three old rows
    // fell through the SAME `need > small_max` branch regardless of the
    // "below/at/above" label and measured nothing distinguishable. These
    // three rows exercise the boundary `class_for` genuinely checks.

    // Just below small_max -- the last size that still resolves to a class.
    h.bench("class_for/near_small_max_below", || {
        let result = black_box(SEFER_SC.class_for(black_box(SEFER_MAX - 1), black_box(1)));
        black_box(result);
    });

    // Exactly at small_max -- the largest resolvable size.
    h.bench("class_for/near_small_max_at", || {
        let result = black_box(SEFER_SC.class_for(black_box(SEFER_MAX), black_box(1)));
        black_box(result);
    });

    // One past small_max -- the early-rejection path (`need > small_max`),
    // returning `None` before ever touching the lookup table.
    h.bench("class_for/above_small_max_rejection", || {
        let result = black_box(SEFER_SC.class_for(black_box(SEFER_MAX + 1), black_box(1)));
        black_box(result);
    });

    // ── is_huge/near_huge_threshold (the policy check that DOES read it) ─────
    // `is_huge` is the one method whose cost the `huge_threshold` boundary
    // actually governs -- a plain `size >= self.huge_threshold` comparison,
    // measured directly here rather than indirectly through `class_for`.

    h.bench("is_huge/near_huge_threshold_below", || {
        let result = black_box(SEFER_SC.is_huge(black_box(HUGE_THRESHOLD - 1)));
        black_box(result);
    });

    h.bench("is_huge/near_huge_threshold_at", || {
        let result = black_box(SEFER_SC.is_huge(black_box(HUGE_THRESHOLD)));
        black_box(result);
    });

    h.bench("is_huge/near_huge_threshold_above", || {
        let result = black_box(SEFER_SC.is_huge(black_box(HUGE_THRESHOLD + 1)));
        black_box(result);
    });

    h.run();
}
