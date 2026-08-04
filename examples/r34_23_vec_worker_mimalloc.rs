//! R34-23 (task #542) real-Vec worker binary — **mimalloc arm**.
//!
//! Installs `mimalloc::MiMalloc` as the REAL `#[global_allocator]` for this
//! process. The ONLY difference from `r34_23_vec_worker_sefer.rs` is this
//! file's `#[global_allocator]` static and the `alloc-stats`-gated oracle
//! block (mimalloc never touches `AllocCore`, so the oracle reads 0). The
//! Vec workload body (`include!`d below) is byte-for-byte identical.

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

// Shared Vec workload body — byte-identical across all three worker binaries.
include!("_shared/r34_23_vec_workload.rs");

fn main() {
    let iterations = parse_iterations();
    proc_probe::emit("arm", "mimalloc");
    proc_probe::emit_u64("iterations", iterations as u64);
    let mem = proc_probe::snapshot();
    proc_probe::emit_u64("rss_bytes", mem.rss);
    proc_probe::emit_u64("commit_bytes", mem.commit);
    run_all_shapes(iterations);
    // mimalloc never reserves SeferAlloc segments.
    proc_probe::emit_u64("segments_reserved_total", 0);
}
