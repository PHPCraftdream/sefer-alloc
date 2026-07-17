//! Process-level A/B/B/A judge binary, **System arm** (task R6-OPT-A6,
//! `radical_optimization_review` §5.5 item 1 / §5.6 / §6 Stage A.1-2).
//!
//! Explicitly installs `std::alloc::System` as this process's
//! `#[global_allocator]` — this is Rust's default allocator anyway (no
//! `#[global_allocator]` attribute at all would behave identically), but an
//! explicit attribute is more honest here: it makes this binary structurally
//! symmetric with `paired_ab_sefer.rs`/`paired_ab_mimalloc.rs` (all three
//! files have exactly one `#[global_allocator]` static, differing only in
//! its type), rather than relying on "the absence of an attribute" as a
//! third, differently-shaped case a future reader could miss.
//!
//! Runs the exact same shared workload as the other two arms
//! (`examples/_shared/paired_ab_workload.rs`, `include!`d verbatim), times
//! it, and prints:
//!
//! - `RESULT elapsed_ns=<n>` — same quantity the other two arms report.
//! - `RESULT segments_reserved_total=0` — SeferAlloc is never constructed
//!   here either; see `paired_ab_mimalloc.rs`'s identical rationale.
//! - `RESULT commit_after_kib=<n>` / `RESULT rss_after_kib=<n>` — same probe
//!   technique as the other two arms.

use std::alloc::System;
use std::time::Instant;

#[global_allocator]
static GLOBAL: System = System;

// The RSS / commit-charge probes (`rss_kib` / `commit_kib`) are now defined
// ONCE in `examples/_shared/paired_ab_workload.rs` (thin wrappers over the
// `proc-memstat` crate's `snapshot()`), `include!`d below — so the OS FFI that
// used to be copy-pasted into each of the three `paired_ab_*` binaries lives
// in one place.

// Shared workload body — see `examples/_shared/paired_ab_workload.rs`'s module
// doc. Byte-identical across all three `paired_ab_*` binaries. Provides
// `run_workload()`.
include!("_shared/paired_ab_workload.rs");

fn main() {
    let t0 = Instant::now();
    run_workload();
    let elapsed_ns = t0.elapsed().as_nanos();

    println!("RESULT arm=system");
    println!("RESULT elapsed_ns={elapsed_ns}");
    // SeferAlloc is never constructed in this binary — no counter to move.
    println!("RESULT segments_reserved_total=0");
    println!("RESULT rss_after_kib={}", rss_kib());
    println!("RESULT commit_after_kib={}", commit_kib());
}
