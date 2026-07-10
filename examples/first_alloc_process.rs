//! Process-per-sample first-alloc RSS + latency probe for `SeferAlloc`
//! (RAD-1 / Phase 0(a), the E1 registry-bootstrap judge).
//!
//! ## Why a fresh process — and why Criterion cannot do this
//!
//! The defect this harness judges (RAD-1, since FIXED) was a *first-touch* one:
//! the registry bootstrap (`src/registry/bootstrap.rs`) used to write `next_free`
//! into all `MAX_HEAPS = 4096` slots at a ~7488 B stride (under `production`;
//! `HeapSlot` is `#[repr(align(64))]` with the inline `HeapCore` carrying the
//! magazine + large-cache state), dirtying ~4096 distinct pages ≈ 16 MiB of
//! demand-zero RSS on the FIRST allocation of the process. That cost was paid
//! EXACTLY ONCE per process, at the first `registry::ensure()`. Criterion (and
//! the iai bench) run many iterations inside ONE long-lived process, so after
//! the first iteration every page is already resident — the first-touch cost is
//! invisible to them. The only way to measure it is to sample a FRESH process
//! each time, which is what `scripts/first-alloc-bench.mjs` does (it runs THIS
//! binary N times as separate processes and aggregates). The RAD-1 lazy-init fix
//! (bootstrap no longer pre-populates `next_free`) drops this harness's headline
//! RSS Δ from ~16.1 MiB to ~0.1 MiB and first-alloc latency from ~8.6 ms to
//! ~0.17 ms; this harness stays as the regression guard.
//!
//! ## What it prints
//!
//! One machine-parseable line per metric, prefixed `RESULT `, so the runner can
//! `grep`/parse it robustly regardless of surrounding log noise:
//!
//! ```text
//! RESULT rss_before_kib=<n>
//! RESULT rss_after_1_heap_kib=<n>
//! RESULT rss_after_8_heaps_kib=<n>
//! RESULT rss_after_64_heaps_kib=<n>
//! RESULT peak_rss_kib=<n>
//! RESULT first_alloc_latency_ns=<n>
//! RESULT heaps_claimed_high_water=<n>
//! ```
//!
//! `rss_after_1_heap_kib − rss_before_kib` is the headline: BEFORE the RAD-1 fix
//! it included the ~16 MiB registry-materialisation first-touch; AFTER the
//! lazy-`next_free` fix it collapses to the primordial-segment cost only
//! (~0.1 MiB). `peak_rss_kib` is a Windows-only cross-check (peak working set,
//! not trimmed by the OS) confirming the first-touch pages were made resident.
//!
//! ## Platform honesty
//!
//! RSS is read directly from the OS:
//! - **Linux:** `/proc/self/statm` field 2 (resident pages) × page size.
//! - **Windows:** `K32GetProcessMemoryInfo` (`WorkingSetSize`), via a tiny
//!   `extern "system"` FFI in THIS example only (examples are separate crates
//!   and may use `unsafe`; the library stays `#![forbid(unsafe_code)]`).
//! - **Other (macOS/BSD):** RSS is reported as `0` (unavailable) — the latency
//!   figure is still valid. This limitation is documented, not faked.
//!
//! Latency is a coarse single-shot `Instant` measurement of the FIRST `alloc`
//! call (which triggers the whole bootstrap). Because it is one sample in one
//! process, the runner aggregates across many processes for a stable figure.
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example first_alloc_process --features production
//! # or via the aggregating runner:
//! node scripts/first-alloc-bench.mjs
//! ```
//!
//! ## Why `production` features, specifically
//!
//! The registry footprint is dominated by the INLINE `HeapCore` in each slot,
//! and `HeapCore`'s size depends heavily on the feature set: with only
//! `alloc-global`+`alloc-xthread` it is ~104 B (the magazine/large-cache state
//! is compiled out), giving a ~768 KiB registry whose `next_free` first-touch is
//! only ~192 pages. Under the full `production` set (`alloc-decommit` +
//! `fastbin`) `HeapCore` inflates to ~7.3 KiB (the `Tcache` magazine + large-
//! cache config are inlined), so `HeapSlot` is ~7488 B, the registry is ~29 MiB,
//! and the eager `next_free` loop dirties all 4096 slots on distinct pages
//! (stride > 4 KiB) = ~16 MiB first-touch. `production` is therefore the ONLY
//! feature set that exhibits the E1 defect — and the set CI and the iai gate
//! use — so this harness is `production`-gated (via `fastbin`/`alloc-decommit`).

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "fastbin"
))]
#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use std::alloc::{GlobalAlloc, Layout};
use std::time::Instant;

use sefer_alloc::SeferAlloc;

// NOTE: we deliberately do NOT install SeferAlloc as the `#[global_allocator]`
// here. We want to control EXACTLY when the sefer registry bootstrap first
// touches memory (the first `alloc` call below), so the RSS delta we attribute
// to it is not polluted by the process's own startup allocations going through
// sefer. The process's incidental allocations (argv, the `Vec`s we spawn) use
// the system allocator; only the explicit `unsafe { sefer.alloc(..) }` calls
// exercise sefer, so `rss_after_1_heap − rss_before` isolates the sefer
// bootstrap's first-touch cost.

// ---------------------------------------------------------------------------
// RSS probe (platform-specific).
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
fn rss_kib() -> u64 {
    // /proc/self/statm: "size resident shared text lib data dt" (in pages).
    // Field 1 (0-indexed) is resident pages.
    let statm = std::fs::read_to_string("/proc/self/statm").unwrap_or_default();
    let resident_pages: u64 = statm
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    // Page size is 4 KiB on every Linux target we run (getpagesize would need
    // libc; 4 KiB is correct for the CI/dev hosts and the harness is a rough
    // judge, not a precise accountant).
    resident_pages * 4
}

#[cfg(windows)]
fn rss_kib() -> u64 {
    // GetProcessMemoryInfo via the K32-prefixed export (available on every
    // supported Windows without linking psapi.lib explicitly). `WorkingSetSize`
    // is the resident-set-size analogue.
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

    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(
            process: isize,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }

    // SAFETY: `counters` is a valid, sufficiently-sized, mutable out-parameter;
    // `GetCurrentProcess` returns a pseudo-handle that needs no close. This is
    // the documented calling convention for `GetProcessMemoryInfo`.
    unsafe {
        let mut counters: ProcessMemoryCounters = core::mem::zeroed();
        counters.cb = core::mem::size_of::<ProcessMemoryCounters>() as u32;
        let ok = K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb);
        if ok == 0 {
            0
        } else {
            (counters.working_set_size / 1024) as u64
        }
    }
}

/// Peak working-set (Windows only) — NOT affected by the OS trimming the live
/// working set between the write and the measurement. Used as a cross-check that
/// the first-touch pages were actually made resident at some point, even if the
/// live `WorkingSetSize` was later trimmed. Returns 0 where unavailable.
#[cfg(windows)]
fn peak_rss_kib() -> u64 {
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
    extern "system" {
        fn GetCurrentProcess() -> isize;
        fn K32GetProcessMemoryInfo(
            process: isize,
            counters: *mut ProcessMemoryCounters,
            cb: u32,
        ) -> i32;
    }
    // SAFETY: see `rss_kib` above — identical documented calling convention.
    unsafe {
        let mut counters: ProcessMemoryCounters = core::mem::zeroed();
        counters.cb = core::mem::size_of::<ProcessMemoryCounters>() as u32;
        let ok = K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb);
        if ok == 0 {
            0
        } else {
            (counters.peak_working_set_size / 1024) as u64
        }
    }
}

#[cfg(not(windows))]
fn peak_rss_kib() -> u64 {
    // Non-Windows: peak-RSS cross-check not wired (Linux /proc/self/status
    // VmHWM could serve, but the live statm figure is already prompt on Linux —
    // Linux does not trim like Windows). Report 0 (not applicable).
    0
}

#[cfg(not(any(target_os = "linux", windows)))]
fn rss_kib() -> u64 {
    // macOS/BSD: no cheap, dependency-free RSS read. Report 0 (unavailable) —
    // the latency figure below is still valid on these platforms.
    0
}

// ---------------------------------------------------------------------------
// N concurrently-live heap claims.
// ---------------------------------------------------------------------------

/// Claim `n` registry slots SIMULTANEOUSLY (n fresh threads, all held open at
/// once) and return the RSS while all `n` are concurrently live.
///
/// Why not spawn-then-join one thread at a time: on thread exit the registry
/// slot is immediately recycled (`AbandonGuard::drop`, "thread death =
/// RELEASE THE SLOT ONLY" — `src/global/tls_heap.rs`), so a naive
/// spawn-join-spawn-join loop never has more than ~1-2 slots concurrently
/// claimed no matter how many threads it churns through — it would measure
/// repeated single-slot reuse, not RSS growth with live heap count. Two
/// barriers make this honest: `claimed` gates the RSS snapshot until every
/// worker has claimed (via its first `alloc`) and is holding its block open;
/// `release` then lets all workers free and exit together.
fn rss_with_n_concurrent_heaps(sefer: &'static SeferAlloc, n: usize) -> u64 {
    use std::sync::{Arc, Barrier};

    let claimed = Arc::new(Barrier::new(n + 1));
    let release = Arc::new(Barrier::new(n + 1));
    let mut handles = Vec::with_capacity(n);

    for _ in 0..n {
        let claimed = Arc::clone(&claimed);
        let release = Arc::clone(&release);
        handles.push(std::thread::spawn(move || {
            let layout = Layout::from_size_align(16, 8).unwrap();
            // SAFETY: `layout` is non-zero; the pointer is freed below with
            // the same layout (or never touched if alloc failed). This is
            // the documented `GlobalAlloc` contract.
            let p = unsafe { sefer.alloc(layout) };
            if !p.is_null() {
                // Touch the block so the claim is not optimised away.
                unsafe { p.write_bytes(0xA5, 1) };
            }
            claimed.wait(); // signal "I have claimed my heap slot"
            release.wait(); // hold open until the RSS snapshot is taken
            if !p.is_null() {
                // SAFETY: same layout as the alloc above.
                unsafe { sefer.dealloc(p, layout) };
            }
        }));
    }

    claimed.wait(); // blocks until all `n` workers have claimed
    let rss = rss_kib(); // all `n` heaps are concurrently live here
    release.wait(); // let workers free + exit

    for h in handles {
        h.join().expect("heap-claim thread panicked");
    }
    rss
}

fn main() {
    // Leak a `SeferAlloc` so it is `'static` (the spawn closures capture it).
    // `SeferAlloc::new()` itself does NOT bootstrap the registry — the first
    // `alloc` does.
    let sefer: &'static SeferAlloc = Box::leak(Box::new(SeferAlloc::new()));

    let rss_before = rss_kib();

    // ── First allocation on the MAIN thread ──────────────────────────────
    // This is THE bootstrap trigger: `registry::ensure()` materialises the
    // whole slot array (on current `main`, writing `next_free` into all 4096
    // slots — the ~16 MiB first-touch this harness judges), plus the primordial
    // segment reserve. We time exactly this call.
    let layout = Layout::from_size_align(16, 8).unwrap();
    let t0 = Instant::now();
    // SAFETY: non-zero layout; freed below with the same layout.
    let first = unsafe { sefer.alloc(layout) };
    let first_alloc_latency_ns = t0.elapsed().as_nanos();
    assert!(!first.is_null(), "first alloc returned null");
    // SAFETY: same layout as the alloc above.
    unsafe {
        first.write_bytes(0xA5, 1);
        sefer.dealloc(first, layout);
    }

    let rss_after_1_heap = rss_kib();

    // ── 8 CONCURRENTLY-live heaps ─────────────────────────────────────────
    let rss_after_8_heaps = rss_with_n_concurrent_heaps(sefer, 8);

    // ── 64 CONCURRENTLY-live heaps ────────────────────────────────────────
    let rss_after_64_heaps = rss_with_n_concurrent_heaps(sefer, 64);

    let high_water = sefer.stats().heaps_claimed_high_water;
    let peak_rss = peak_rss_kib();

    // Machine-parseable results (prefixed so the runner can grep them out of
    // any surrounding noise). One metric per line.
    println!("RESULT rss_before_kib={rss_before}");
    println!("RESULT rss_after_1_heap_kib={rss_after_1_heap}");
    println!("RESULT rss_after_8_heaps_kib={rss_after_8_heaps}");
    println!("RESULT rss_after_64_heaps_kib={rss_after_64_heaps}");
    println!("RESULT peak_rss_kib={peak_rss}");
    println!("RESULT first_alloc_latency_ns={first_alloc_latency_ns}");
    println!("RESULT heaps_claimed_high_water={high_water}");
}
