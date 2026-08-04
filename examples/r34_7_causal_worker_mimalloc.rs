//! R34-7 (task #526) causal-harness worker binary — **mimalloc arm**.
//!
//! Installs `mimalloc::MiMalloc` as the REAL `#[global_allocator]` for this
//! process. The ONLY difference from `r34_7_causal_worker_sefer.rs` is this
//! file's `#[global_allocator]` static and the hardcoded
//! `segments_reserved_total=0` (SeferAlloc is never constructed here). The
//! workload body (`include!`d below) is byte-for-byte identical — see that
//! file's module doc for the full rationale.

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// Shared workload body — byte-identical across all three worker binaries.
// Provides `run_timed`, `parse_size_and_iterations`, `CHURN_OPS`.
include!("_shared/r34_7_causal_workload.rs");

fn main() {
    let (size, iterations) = parse_size_and_iterations();

    let elapsed_ns = run_timed(size, iterations);

    let total_ops = iterations * CHURN_OPS;
    let ns_per_op = elapsed_ns as f64 / total_ops as f64;

    proc_probe::emit("arm", "mimalloc");
    proc_probe::emit_f64("ns_per_op", ns_per_op);
    // SeferAlloc is never constructed in this binary — no counter to move.
    proc_probe::emit_u64("segments_reserved_total", 0);
    proc_probe::emit_u64("size", size as u64);
    proc_probe::emit_u64("iterations", iterations as u64);
}
