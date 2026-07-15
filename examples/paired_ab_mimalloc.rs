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

// ---------------------------------------------------------------------------
// RSS / commit-charge probes — identical technique to the SeferAlloc arm
// (see that file's module doc for the shared-crate rationale).
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
