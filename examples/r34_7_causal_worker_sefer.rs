//! R34-7 (task #526) causal-harness worker binary — **SeferAlloc arm**.
//!
//! Installs `SeferAlloc` as the REAL `#[global_allocator]` for this process
//! (not a direct `GlobalAlloc` call — see the R32/R33 review's §"P1" for why
//! that distinction matters: `benches/global_alloc.rs`'s direct-call,
//! single-process comparison is non-causal). Parses `--size` / `--iterations`
//! from the command line, runs the shared churn-write workload
//! (`examples/_shared/r34_7_causal_workload.rs`, `include!`d verbatim —
//! byte-identical across all three worker binaries), and prints:
//!
//! - `RESULT ns_per_op=<f64>` — nanoseconds per free+alloc pair, the primary
//!   metric `scripts/r34_7_causal_harness.mjs` collects per subprocess.
//! - `RESULT segments_reserved_total=<n>` — SeferAlloc's own diagnostic counter
//!   (`SeferAlloc::stats()`), the installed-allocator sanity check: > 0 here,
//!   always 0 in the mimalloc/system binaries.
//! - `RESULT size=<n>` / `RESULT iterations=<n>` — echo-back of the CLI args
//!   the orchestrator passed, so a mismatch (wrong size, etc.) is visible in
//!   the raw output without cross-referencing the launch command.

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// Shared workload body — see `examples/_shared/r34_7_causal_workload.rs`'s
// module doc for why `include!` (not a shared crate module) is used. Provides
// `run_timed`, `parse_size_and_iterations`, `CHURN_OPS`.
include!("_shared/r34_7_causal_workload.rs");

fn main() {
    let (size, iterations) = parse_size_and_iterations();

    let elapsed_ns = run_timed(size, iterations);

    // ns per free+alloc pair: total timed nanoseconds / (rounds × ops/round).
    let total_ops = iterations * CHURN_OPS;
    let ns_per_op = elapsed_ns as f64 / total_ops as f64;

    let stats = GLOBAL.stats();

    proc_probe::emit("arm", "sefer");
    proc_probe::emit_f64("ns_per_op", ns_per_op);
    proc_probe::emit_u64("segments_reserved_total", stats.segments_reserved_total);
    proc_probe::emit_u64("size", size as u64);
    proc_probe::emit_u64("iterations", iterations as u64);
}
