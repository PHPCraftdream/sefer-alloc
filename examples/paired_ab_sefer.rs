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

// ---------------------------------------------------------------------------
// RSS / commit-charge probes — same technique as `examples/first_alloc_process.rs`
// (R6-OPT-A1) / `examples/dealloc_only_unbound_thread.rs` (R6-OPT-A5). Kept as
// a self-contained copy per those files' own "no shared examples-support
// crate" rationale (this file's module doc).
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn rss_kib() -> u64 {
    let statm = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
    let resident_pages: u64 = statm
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    resident_pages * 4
}

#[cfg(target_os = "linux")]
fn commit_kib() -> u64 {
    let statm = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
    let total_pages: u64 = statm
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    total_pages * 4
}

#[cfg(windows)]
#[repr(C)]
struct ProcessMemoryCounters {
    cb: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[cfg(windows)]
extern "system" {
    fn GetCurrentProcess() -> isize;
    fn K32GetProcessMemoryInfo(
        process: isize,
        counters: *mut ProcessMemoryCounters,
        cb: u32,
    ) -> i32;
}

#[cfg(windows)]
fn read_counters() -> ProcessMemoryCounters {
    // SAFETY: `counters` is a valid, sufficiently-sized, mutable out-parameter;
    // `GetCurrentProcess` returns a pseudo-handle that needs no close. Same
    // documented usage as the sibling Stage-A harnesses.
    unsafe {
        let mut counters: ProcessMemoryCounters = core::mem::zeroed();
        counters.cb = core::mem::size_of::<ProcessMemoryCounters>() as u32;
        K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb);
        counters
    }
}

#[cfg(windows)]
fn rss_kib() -> u64 {
    (read_counters().working_set_size / 1024) as u64
}

#[cfg(windows)]
fn commit_kib() -> u64 {
    (read_counters().pagefile_usage / 1024) as u64
}

#[cfg(not(any(target_os = "linux", windows)))]
fn rss_kib() -> u64 {
    0
}

#[cfg(not(any(target_os = "linux", windows)))]
fn commit_kib() -> u64 {
    0
}

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
