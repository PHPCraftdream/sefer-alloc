//! Process-level A/B/B/A judge binary, **mimalloc arm** (task R6-OPT-A6,
//! `radical_optimization_review` §5.5 item 1 / §5.6 / §6 Stage A.1-2).
//!
//! Installs `mimalloc::MiMalloc` as the REAL `#[global_allocator]` for this
//! process (the `mimalloc` dev-dependency already exists in `Cargo.toml`
//! for `benches/global_alloc.rs`'s direct-call comparison — this binary is
//! the first place in the repo that actually installs it globally, rather
//! than calling its `GlobalAlloc` impl directly). Runs the exact same shared
//! workload as `paired_ab_sefer.rs` / `paired_ab_system.rs`
//! (`examples/_shared/paired_ab_workload.rs`, `include!`d verbatim — the
//! ONLY difference between the three binaries is this file's
//! `#[global_allocator]` attribute), times it, and prints:
//!
//! - `RESULT elapsed_ns=<n>` — same quantity the other two arms report.
//! - `RESULT segments_reserved_total=0` — SeferAlloc is never constructed
//!   here, so its diagnostic counter cannot exist/move. Hardcoded `0` (not a
//!   `SeferAlloc::stats()` call — there is no `SeferAlloc` instance in this
//!   binary at all) so `scripts/paired-ab-runner.mjs`'s installed-allocator
//!   sanity check (task's own verification requirement) has a genuine,
//!   structurally-guaranteed zero to compare the SeferAlloc arm's non-zero
//!   reading against.
//! - `RESULT commit_after_kib=<n>` / `RESULT rss_after_kib=<n>` — same probe
//!   technique as the SeferAlloc arm, for the commit-charge/RSS comparison.

use std::time::Instant;

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

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

    println!("RESULT arm=mimalloc");
    println!("RESULT elapsed_ns={elapsed_ns}");
    // SeferAlloc is never constructed in this binary — no counter to move.
    println!("RESULT segments_reserved_total=0");
    println!("RESULT rss_after_kib={}", rss_kib());
    println!("RESULT commit_after_kib={}", commit_kib());
}
