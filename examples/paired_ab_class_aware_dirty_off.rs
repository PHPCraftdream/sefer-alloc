//! R14-3 (task #288) process-level A/B/B/A judge binary, **baseline arm**
//! (`class-aware-dirty` OFF).
//!
//! Companion to `paired_ab_class_aware_dirty_on.rs`. Both binaries
//! `include!` the identical fixed-work round
//! (`examples/_shared/paired_ab_class_aware_dirty_workload.rs`) — the ONLY
//! difference between them is whether this binary is built WITH or WITHOUT
//! the `class-aware-dirty` feature. This is the fixed-work, process-level
//! counterpart to `benches/r12_7_class_aware_dirty_wallclock.rs`, built to
//! answer the exact methodology gap three independent Round-13 reviews
//! raised against that bench's headline "21.71x at N=8" figure: the bench's
//! own `ns/owner_alloc` number is a SUB-WINDOW timer, while criterion's own
//! full-round "time:" output (which this binary's `elapsed_ns` reproduces
//! directly, one process launch at a time, no criterion machinery involved)
//! moved far less. See `docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB.md`
//! for the full report and
//! `examples/_shared/paired_ab_class_aware_dirty_workload.rs`'s module doc
//! for the workload's exact shape.
//!
//! **RESULT lines:**
//! - `RESULT elapsed_ns=<n>` — the FULL round wall-clock (pre-alloc + timed
//!   window + recycle) — the metric `scripts/paired-ab-runner.mjs` pairs by
//!   default.
//! - `RESULT window_ns=<n>` — the SAME sub-window the criterion bench's
//!   `run_round` times, reported alongside for the companion comparison.
//! - `RESULT owner_allocs=<n>` — the fixed owner-alloc count actually
//!   completed (sanity: must be `>= 800`, the `MIN_OWNER_ITERS` floor).
//! - `RESULT segments_reserved_total=<n>` — the installed-allocator sanity
//!   counter `paired-ab-runner.mjs`'s config-mode sanity gate can key off of
//!   (nonzero in both arms here, since both genuinely exercise SeferAlloc via
//!   `HeapRegistry`/`AllocCore` directly — no `--sanity` gate is configured
//!   for this pair's `--config` JSON because BOTH arms are SeferAlloc, only
//!   the feature flag differs).
//!
//! **Build:** `cargo build --release --example paired_ab_class_aware_dirty_off --features "production alloc-stats"`

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-segment-directory",
    feature = "alloc-stats"
))]

// Shared fixed-work round body — see
// `examples/_shared/paired_ab_class_aware_dirty_workload.rs`'s module doc for
// why `include!` (not a shared crate module) is used. Provides
// `run_fixed_work_round() -> (full_round_ns, window_ns, owner_allocs)`.
include!("_shared/paired_ab_class_aware_dirty_workload.rs");

fn main() {
    let _ = bootstrap::ensure();

    let (full_round_ns, window_ns, owner_allocs) = run_fixed_work_round();

    let segments_reserved_total = AllocCore::dbg_segments_reserved_total();

    proc_probe::emit("arm", "class_aware_dirty_off");
    proc_probe::emit_ns("elapsed_ns", full_round_ns);
    proc_probe::emit_ns("window_ns", window_ns);
    proc_probe::emit_u64("owner_allocs", owner_allocs as u64);
    proc_probe::emit_u64("segments_reserved_total", segments_reserved_total);
}

#[cfg(not(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-segment-directory",
    feature = "alloc-stats"
)))]
fn main() {
    eprintln!(
        "paired_ab_class_aware_dirty_off: requires --features \
         \"alloc-global alloc-xthread alloc-segment-directory alloc-stats\" \
         (e.g. \"production alloc-stats\")"
    );
    std::process::exit(1);
}
