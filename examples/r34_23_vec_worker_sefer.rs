//! R34-23 (task #542) real-Vec worker binary — **SeferAlloc arm**.
//!
//! Installs `SeferAlloc` as the REAL `#[global_allocator]` for this process.
//! The shared Vec workload (`examples/_shared/r34_23_vec_workload.rs`,
//! `include!`d verbatim — byte-identical across all three worker binaries)
//! drives a genuine `Vec<u8>` through `.push()`/`.shrink_to_fit()`/`.reserve_exact()`,
//! exercising std's own growth-factor realloc logic through the installed
//! allocator. Driven by `scripts/r34_23_vec_harness.mjs` as a fresh subprocess
//! per (allocator, rep) — see that script's module doc for the causal-
//! isolation rationale.
//!
//! Emits `RESULT` lines: per-shape per-rep `elapsed_ns`/`realloc_count`, plus
//! the sefer-only realloc path-activation oracle deltas (`oracle_*_delta`).

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Shared Vec workload body — see `examples/_shared/r34_23_vec_workload.rs`'s
// module doc for why `include!` (not a shared crate module) is used. Provides
// `run_all_shapes`, `parse_iterations`.
include!("_shared/r34_23_vec_workload.rs");

fn main() {
    let iterations = parse_iterations();
    proc_probe::emit("arm", "sefer");
    proc_probe::emit_u64("iterations", iterations as u64);
    let mem = proc_probe::snapshot();
    proc_probe::emit_u64("rss_bytes", mem.rss);
    proc_probe::emit_u64("commit_bytes", mem.commit);
    run_all_shapes(iterations);
    // SeferAlloc sanity: segments must have been reserved.
    let stats = GLOBAL.stats();
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
}
