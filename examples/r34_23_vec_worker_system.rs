//! R34-23 (task #542) real-Vec worker binary — **System arm**.
//!
//! Explicitly installs `std::alloc::System` as this process's
//! `#[global_allocator]` — structurally symmetric with the other two worker
//! binaries (all three have exactly one `#[global_allocator]` static,
//! differing only in its type). The Vec workload body (`include!`d below) is
//! byte-for-byte identical.

use std::alloc::System;

#[global_allocator]
static GLOBAL: System = System;

// Shared Vec workload body — byte-identical across all three worker binaries.
include!("_shared/r34_23_vec_workload.rs");

fn main() {
    let iterations = parse_iterations();
    proc_probe::emit("arm", "system");
    proc_probe::emit_u64("iterations", iterations as u64);
    let mem = proc_probe::snapshot();
    proc_probe::emit_u64("rss_bytes", mem.rss);
    proc_probe::emit_u64("commit_bytes", mem.commit);
    run_all_shapes(iterations);
    // System never reserves SeferAlloc segments.
    proc_probe::emit_u64("segments_reserved_total", 0);
}
