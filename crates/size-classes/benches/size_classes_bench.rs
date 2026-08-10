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
use size_classes::{build_table, size2class_len, Params, SizeClasses};

// ---------------------------------------------------------------------------
// Sefer's concrete parameterization (49 classes; the default in-tree scheme).
// Copied from tests/builder.rs — this is the realistic production-like
// configuration the actual allocator uses.
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

    // Size with align=256 (> min_block=16, so divisibility-jump slow path)
    h.bench("class_for/large_align_slow_path", || {
        let result = black_box(SEFER_SC.class_for(black_box(256), black_box(256)));
        black_box(result);
    });

    // Size with align=1024 (> min_block=16, so divisibility-jump slow path)
    h.bench("class_for/large_align_slow_path_1024", || {
        let result = black_box(SEFER_SC.class_for(black_box(1024), black_box(1024)));
        black_box(result);
    });

    // ── class_for/near_huge_threshold ────────────────────────────────────────
    // Test cost near the huge_threshold boundary (4 MiB for SEFER_PARAMS).
    // This exercises the large-size rejection path near the policy threshold.

    // Just below the threshold (4 MiB - 1)
    h.bench("class_for/near_huge_threshold_below", || {
        let result = black_box(SEFER_SC.class_for(black_box(4 * 1024 * 1024 - 1), black_box(1)));
        black_box(result);
    });

    // Exactly at the threshold (4 MiB)
    h.bench("class_for/near_huge_threshold_at", || {
        let result = black_box(SEFER_SC.class_for(black_box(4 * 1024 * 1024), black_box(1)));
        black_box(result);
    });

    // Just above the threshold (4 MiB + 1)
    h.bench("class_for/near_huge_threshold_above", || {
        let result = black_box(SEFER_SC.class_for(black_box(4 * 1024 * 1024 + 1), black_box(1)));
        black_box(result);
    });

    h.run();
}
