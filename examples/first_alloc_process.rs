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
//! RESULT commit_before_kib=<n>
//! RESULT commit_after_1_heap_kib=<n>
//! RESULT commit_after_8_heaps_kib=<n>
//! RESULT commit_after_64_heaps_kib=<n>
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
//! `commit_*_kib` is a SEPARATE axis from RSS (R6-OPT-A1, radical_optimization_
//! review §4 P0-2 / §5.5 item 9 / §6 Stage A.3): on Windows, `crates/vmem`
//! commits the FULL exact size of the Registry + inline `HeapOverflow` array in
//! one `VirtualAlloc(MEM_COMMIT)` call, which shows up as Windows commit charge
//! (`PagefileUsage`) even though it is largely demand-zero and therefore
//! invisible to `WorkingSetSize`/RSS. Expect `commit_after_1_heap_kib −
//! commit_before_kib` to be dramatically larger than the corresponding RSS
//! delta (on the order of ~125 MiB: ≈29 MiB registry + ≈96 MiB inline
//! `HeapOverflow` across 4096 slots) — that gap is the whole point of this
//! metric; it is a real cost the RSS-only judge could never see.
//!
//! ## Platform honesty
//!
//! RSS, commit charge, and peak RSS are read from the `proc-probe` crate's
//! re-export of `proc-memstat`'s `snapshot()` (`crates/proc-memstat`), which
//! holds the OS FFI in ONE audited place (the readers used to be triplicated in
//! this file); the `RESULT` lines are emitted via `proc_probe::emit_*`:
//! - **Linux:** `/proc/self/statm` resident/size pages × page size, plus
//!   `/proc/self/status` `VmHWM` for peak RSS.
//! - **Windows:** `K32GetProcessMemoryInfo` — `WorkingSetSize` (rss),
//!   `PagefileUsage` (commit — the standard Windows commit-charge figure),
//!   `PeakWorkingSetSize` (peak rss).
//! - **macOS:** `task_info(MACH_TASK_BASIC_INFO)` — `resident_size` (rss),
//!   `virtual_size` (commit), `resident_size_max` (peak rss).
//! - **Other:** reported as `0` (unavailable) — the latency figure is still
//!   valid. This limitation is documented, not faked.
//!
//! `snapshot()` returns BYTES; this harness prints KiB, so each reader below
//! converts at the boundary (`/ 1024`), keeping the historic `*_kib` line
//! numbers unchanged.
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

// ---------------------------------------------------------------------------
// RSS / commit-charge / peak-RSS probes — thin KiB wrappers over the
// `proc-probe` crate's re-export of `proc-memstat`'s same-instant `snapshot()`
// (bytes). The OS FFI that used to be triplicated in this file now lives in ONE
// place (`crates/proc-memstat`, reached via `proc-probe`'s "measure + report"
// re-export). The `RESULT` lines are emitted via `proc_probe::emit_*` (see
// `main`). Printed line names/units are unchanged.
// ---------------------------------------------------------------------------

fn rss_kib() -> u64 {
    proc_probe::snapshot().rss / 1024
}

fn commit_kib() -> u64 {
    proc_probe::snapshot().commit / 1024
}

fn peak_rss_kib() -> u64 {
    // `None` (platform without a peak counter) prints as 0, matching this
    // harness's prior "not applicable = 0" convention.
    proc_probe::snapshot().peak_rss.unwrap_or(0) / 1024
}

// NOTE: we deliberately do NOT install SeferAlloc as the `#[global_allocator]`
// here. We want to control EXACTLY when the sefer registry bootstrap first
// touches memory (the first `alloc` call below), so the RSS delta we attribute
// to it is not polluted by the process's own startup allocations going through
// sefer. The process's incidental allocations (argv, the `Vec`s we spawn) use
// the system allocator; only the explicit `unsafe { sefer.alloc(..) }` calls
// exercise sefer, so `rss_after_1_heap − rss_before` isolates the sefer
// bootstrap's first-touch cost.

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
///
/// Returns `(rss_kib, commit_kib)` — both snapshotted at the SAME instant
/// (all `n` heaps concurrently live), so a `commit − rss` comparison at this
/// point is apples-to-apples (R6-OPT-A1: commit-charge is a separate axis
/// from RSS, added alongside it here rather than replacing it).
fn rss_with_n_concurrent_heaps(sefer: &'static SeferAlloc, n: usize) -> (u64, u64) {
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
    let commit = commit_kib();
    release.wait(); // let workers free + exit

    for h in handles {
        h.join().expect("heap-claim thread panicked");
    }
    (rss, commit)
}

fn main() {
    // Leak a `SeferAlloc` so it is `'static` (the spawn closures capture it).
    // `SeferAlloc::new()` itself does NOT bootstrap the registry — the first
    // `alloc` does.
    let sefer: &'static SeferAlloc = Box::leak(Box::new(SeferAlloc::new()));

    let rss_before = rss_kib();
    let commit_before = commit_kib();

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
    let commit_after_1_heap = commit_kib();

    // ── 8 CONCURRENTLY-live heaps ─────────────────────────────────────────
    let (rss_after_8_heaps, commit_after_8_heaps) = rss_with_n_concurrent_heaps(sefer, 8);

    // ── 64 CONCURRENTLY-live heaps ────────────────────────────────────────
    let (rss_after_64_heaps, commit_after_64_heaps) = rss_with_n_concurrent_heaps(sefer, 64);

    let high_water = sefer.stats().heaps_claimed_high_water;
    let peak_rss = peak_rss_kib();

    // Machine-parseable results (emitted via `proc_probe::emit_*` so the runner
    // can grep them out of any surrounding noise). One metric per line.
    proc_probe::emit_u64("rss_before_kib", rss_before);
    proc_probe::emit_u64("rss_after_1_heap_kib", rss_after_1_heap);
    proc_probe::emit_u64("rss_after_8_heaps_kib", rss_after_8_heaps);
    proc_probe::emit_u64("rss_after_64_heaps_kib", rss_after_64_heaps);
    proc_probe::emit_u64("peak_rss_kib", peak_rss);
    proc_probe::emit_u64("commit_before_kib", commit_before);
    proc_probe::emit_u64("commit_after_1_heap_kib", commit_after_1_heap);
    proc_probe::emit_u64("commit_after_8_heaps_kib", commit_after_8_heaps);
    proc_probe::emit_u64("commit_after_64_heaps_kib", commit_after_64_heaps);
    proc_probe::emit_ns("first_alloc_latency_ns", first_alloc_latency_ns);
    proc_probe::emit_u64("heaps_claimed_high_water", high_water);
}
