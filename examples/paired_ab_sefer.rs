//! Process-level A/B/B/A judge binary, **SeferAlloc arm** (task R6-OPT-A6,
//! `radical_optimization_review` §5.5 item 1 / §5.6 / §6 Stage A.1-2).
//!
//! Installs `SeferAlloc` as the REAL `#[global_allocator]` for this process
//! (not a direct/generic `GlobalAlloc` call, unlike `benches/global_alloc.rs`
//! — see that file's module doc and `examples/_shared/paired_ab_workload.rs`'s
//! module doc for why the distinction matters), runs the shared workload
//! (`examples/_shared/paired_ab_workload.rs`, `include!`d verbatim — byte-
//! identical to the code compiled into `paired_ab_mimalloc.rs` /
//! `paired_ab_system.rs`), times the whole run, and prints:
//!
//! - `RESULT elapsed_ns=<n>` — wall-clock time for `run_workload()`, the
//!   quantity `scripts/paired-ab-runner.mjs` pairs across alternating
//!   process launches.
//! - `RESULT segments_reserved_total=<n>` — SeferAlloc's own diagnostic
//!   counter (`SeferAlloc::stats()`), snapshotted AFTER the workload runs.
//!   This is the task's own "genuinely installed, not just named" sanity
//!   check: this counter is always > 0 in THIS binary (the workload
//!   allocates enough to reserve at least the primordial segment) and is
//!   always exactly 0 in `paired_ab_mimalloc.rs`/`paired_ab_system.rs`
//!   (SeferAlloc is never constructed or installed there at all, so the
//!   counter cannot move) — `scripts/paired-ab-runner.mjs` asserts this
//!   asymmetry before trusting any timing comparison.
//! - `RESULT commit_after_kib=<n>` / `RESULT rss_after_kib=<n>` — reused
//!   probe technique from `examples/first_alloc_process.rs` (R6-OPT-A1) /
//!   `examples/dealloc_only_unbound_thread.rs` (R6-OPT-A5), so the runner can
//!   fold in commit-charge/RSS deltas alongside timing (task step 4).

use std::time::Instant;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// The RSS / commit-charge probes (`rss_kib` / `commit_kib`) are now defined
// ONCE in `examples/_shared/paired_ab_workload.rs` (thin wrappers over the
// `proc-memstat` crate's `snapshot()`), `include!`d below — so the OS FFI that
// used to be copy-pasted into each of the three `paired_ab_*` binaries lives
// in one place.

// Shared workload body — see `examples/_shared/paired_ab_workload.rs`'s module
// doc for why `include!` (not a shared crate module) is used. Provides
// `run_workload()`.
include!("_shared/paired_ab_workload.rs");

fn main() {
    let t0 = Instant::now();
    run_workload();
    let elapsed_ns = t0.elapsed().as_nanos();

    let stats = GLOBAL.stats();

    println!("RESULT arm=sefer");
    println!("RESULT elapsed_ns={elapsed_ns}");
    println!(
        "RESULT segments_reserved_total={}",
        stats.segments_reserved_total
    );
    println!("RESULT rss_after_kib={}", rss_kib());
    println!("RESULT commit_after_kib={}", commit_kib());
}
