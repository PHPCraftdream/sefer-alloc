//! Shared SEFER_* fixture constants for `tests/builder.rs` and
//! `benches/size_classes_bench.rs` (rush-tests review T4/task #1479):
//! previously duplicated in both files, joined only by a "keep in sync"
//! comment on each side -- a single-sided edit desynced them silently.
//! `tests/common/mod.rs` is the standard Rust convention for a test helper
//! module that is NOT itself compiled as a separate test binary; the bench
//! reaches it via an explicit `#[path = "../tests/common/mod.rs"]`.
//!
//! `pub(crate)`, not `pub` (Sol-run7 P4-1/task #1482): each consumer
//! (`tests/builder.rs`, the bench) compiles this file as a private child
//! module of its OWN crate root, so `pub` here never actually widens the
//! published crate's semver surface -- `pub(crate)` says so explicitly
//! instead of relying on that fact being obvious from context.

use size_classes::{build_table, size2class_len, Params, SizeClasses};

/// Sefer's concrete parameterization (49 classes; the default in-tree
/// scheme) -- the realistic production-like configuration the actual
/// allocator uses.
pub(crate) const SEFER_MIN_BLOCK: usize = 16;
pub(crate) const SEFER_EXTRAS: &[usize] = &[256, 512, 1024, 2048, 4096, 6144, 8192, 12288, 16384];
pub(crate) const SEFER_GEO: usize = 40;
pub(crate) const SEFER_N: usize = SEFER_GEO + SEFER_EXTRAS.len();
pub(crate) const HUGE_THRESHOLD: usize = 4 * 1024 * 1024;
pub(crate) const SEFER_PARAMS: Params = Params::new(
    SEFER_MIN_BLOCK,
    (5, 4),
    SEFER_GEO,
    SEFER_EXTRAS,
    HUGE_THRESHOLD,
);
pub(crate) const SEFER_TABLE: [usize; SEFER_N] = build_table::<SEFER_N>(&SEFER_PARAMS);
pub(crate) const SEFER_MAX: usize = SEFER_TABLE[SEFER_N - 1];
pub(crate) const SEFER_L: usize = size2class_len(SEFER_MAX, SEFER_MIN_BLOCK);
// `static`, not `const`: a `const` this size re-materializes at every use
// site (see the crate's own SizeClasses doc for why).
pub(crate) static SEFER_SC: SizeClasses<SEFER_N, SEFER_L> = SizeClasses::build(SEFER_PARAMS);

/// Slow-path (size, align) pairs, genuinely exercising the divisibility-jump
/// mechanism (pinned by `sefer_bench_jump_rows_genuinely_exercise_the_slow_path`
/// in `tests/builder.rs`).
pub(crate) const JUMP_A: (usize, usize) = (1025, 256);
pub(crate) const JUMP_B: (usize, usize) = (2049, 1024);
