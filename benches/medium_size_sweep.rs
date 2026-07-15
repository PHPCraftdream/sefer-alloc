//! `medium_size_sweep` — independent-alloc/free judge for the SMALL_MAX
//! architectural cliff (task R6-OPT-A3, `radical_optimization_review` §4
//! P0-3 measurement plan / §5.5 item 8 / §6 Stage A.5).
//!
//! ## Why this harness exists — the cliff it makes visible
//!
//! `SMALL_MAX` (`src/alloc_core/size_classes.rs`, currently **258,752 B**,
//! confirmed at harness-authoring time by binary-searching
//! `AllocCore::dbg_layout_class_for`) is the boundary between two completely
//! different allocation strategies:
//!
//! - **`<= SMALL_MAX`:** the small free-list path. Many objects share one
//!   4 MiB segment (`SEGMENT`); a 258,752 B block is large enough that only
//!   ~15 fit per segment (16 fit arithmetically, but the primordial segment
//!   reserves one block's worth for its self-hosted registry, and every
//!   fresh small segment loses some to per-segment metadata — see
//!   `benches/perf_gate_iai.rs`'s `multiseg_cold_256k` comment, which this
//!   harness's control points deliberately mirror).
//! - **`> SMALL_MAX`:** the dedicated-segment Large path. **Every** object,
//!   however small the excess over `SMALL_MAX`, gets its OWN 4 MiB span plus
//!   one `SegmentTable` slot. 262,144 B (literal 256 KiB) is only 3,392 B
//!   over `SMALL_MAX` but pays the FULL dedicated-span cost.
//!
//! No existing bench measures this directly:
//! - `benches/perf_gate_iai.rs`'s `multiseg_cold_256k` DELIBERATELY stays at
//!   `SMALL_MAX` exactly, with an explicit comment warning that literal
//!   256 KiB would cross into the Large path and break its geometry — i.e.
//!   it exists specifically to AVOID the cliff this harness targets.
//! - `benches/large_realloc.rs` only exercises a SINGLE growing span via
//!   `realloc` (4/16/64 MiB) — nowhere near the cliff, and realloc-of-one,
//!   not independent-object-population behavior.
//!
//! This is Stage A — a MEASUREMENT harness, not a source change. It exists
//! so the separate, blocked task R6-OPT-P0-3 (a prototype "medium
//! allocator" for 256 KiB–2 MiB) can honestly prove whatever win it claims
//! against a real, numeric baseline instead of an assertion.
//!
//! ## Harness shape: custom timing loop, not Criterion
//!
//! Mirrors R6-OPT-A2's (`benches/heap_fanin_persistent.rs`) reasoning:
//! Criterion's `b.iter()` model reports only mean/median per
//! `benchmark_group`/`BenchmarkId` pair and cannot cleanly express "alloc N
//! objects, hold them all live, THEN time a batched free" as a single
//! percentile-bearing sample, nor read this crate's own diagnostic counters
//! (`dbg_table_count`, `dbg_segments_reserved_total`,
//! `dbg_segments_released_total`) at precise checkpoints around the timed
//! region. A custom loop also makes the OS-reservation-COUNT metric (this
//! harness's headline number) a direct before/after diagnostic delta
//! instead of something inferred from timing noise.
//!
//! ## Metrics obtained, and how
//!
//! - **ns/op (alloc, free), p50/p99/mean:** `std::time::Instant` around each
//!   individual alloc/free call in the timed phase; sorted for percentiles.
//! - **OS reservation COUNT:** `AllocCore::dbg_segments_reserved_total()` /
//!   `dbg_segments_released_total()` — process-wide monotonic counters (task
//!   E1), read as a delta across the measured phase. This is the number
//!   that shows "1,024 objects -> 1,024 dedicated spans" directly.
//! - **`SegmentTable` occupancy:** `AllocCore::dbg_table_count()` — the
//!   table's high-water slot count on the SAME `AllocCore` instance driving
//!   the same-thread variants (task #135's regression-guard seam, same one
//!   `tests/segment_table_o1.rs` uses).
//! - **committed/private bytes, RSS-after-touch:** Windows
//!   `K32GetProcessMemoryInfo` (`WorkingSetSize` for RSS,
//!   `PagefileUsage` for commit charge), mirroring R6-OPT-A1's
//!   `examples/first_alloc_process.rs` probe verbatim (same struct layout,
//!   same extern decls). Taken as PROCESS-WIDE snapshots at sweep
//!   checkpoints (before/after each cardinality's alloc phase) — this
//!   harness runs as a `benches/` binary, not a process-per-sample example
//!   like A1's; process-wide snapshots are the honest thing to report here
//!   (per-object attribution is not obtainable without spawning a fresh
//!   process per data point, which would defeat the "sweep in one run"
//!   design — flagged, not faked). Linux gets `/proc/self/statm` mirrors of
//!   the same two figures for parity; other platforms report `0`
//!   (unavailable), matching the established convention in
//!   `first_alloc_process.rs` / `rss_probe.rs`.
//! - **internal fragmentation:** payload bytes requested (`size * live_n`)
//!   vs bytes actually reserved (`segments delta * SEGMENT`) — a computed
//!   ratio, not a new OS probe.
//! - **p99 latency:** included alongside mean/p50 in every reported line.
//!
//! ## Access patterns implemented
//!
//! `cold` (fresh segments every cardinality/size point — the sweep never
//! frees between cardinality steps within one size), `repeated-reuse`
//! (alloc/free/alloc the same size+cardinality N times so segments/slots
//! recycle), `random-lifetime` (objects freed in a `Xorshift64`-shuffled
//! order — not LIFO/FIFO, so the freelist cannot rely on an artificially
//! favorable reuse pattern), `same-thread` (default; drives `AllocCore`
//! directly, giving instance-scoped `dbg_table_count`), and `cross-thread`
//! (gated on `alloc-xthread`; allocates on a producer thread and frees on a
//! different consumer thread via the real `SeferAlloc` `GlobalAlloc` face —
//! the production cross-thread-free path, not the `dbg_push_to_ring` test
//! seam `benches/heap_xthread.rs` uses).
//!
//! ## R6-OPT-A2 lesson applied: repeated-measurement consistency
//!
//! The sibling task R6-OPT-A2 shipped a harness whose first version leaked
//! state across repeated measurements of the SAME configuration (LIFO
//! heap-slot reuse carrying forward an undrained ring). This harness has an
//! analogous risk: the `repeated-reuse` access pattern measures the SAME
//! (size, cardinality) point multiple times back-to-back WITHIN one
//! `AllocCore`/heap instance by construction. [`verify_repeated_measurement_
//! consistency`] runs one (size, cardinality) cell's cold-alloc + free
//! timing THREE times, interleaved with an unrelated size/cardinality point
//! in between (not back-to-back — the exact shape that exposed the sibling
//! bug), and asserts the three p50s stay within a generous ratio of each
//! other. Wired into `main()` unconditionally, not opt-in, so this class of
//! bug cannot silently regress here either.
//!
//! ## Run
//!
//! ```text
//! # Quick default (fast, per this project's benchmark-speed convention):
//! cargo run --release --bench medium_size_sweep --features alloc-core -- --bench
//!
//! # Reduced representative tier (adds 384 KiB/1.5 MiB/random-lifetime/cardinality 1024):
//! cargo run --release --bench medium_size_sweep --features alloc-core -- --bench --reduced
//!
//! # Full matrix (every size x every cardinality x every access pattern):
//! cargo run --release --bench medium_size_sweep --features alloc-core -- --bench --full-matrix
//!
//! # Cross-thread variant (requires alloc-xthread):
//! cargo run --release --bench medium_size_sweep --features "alloc-core alloc-global alloc-xthread" -- --bench --reduced
//! ```
//!
//! (`--bench` is threaded through by `cargo bench`/`cargo run --bench`'s own
//! harness shim; this binary's OWN flags — `--reduced` / `--full-matrix` —
//! are read from `std::env::args()` directly, the same pattern
//! `heap_fanin_persistent.rs` uses, since `harness = false` means no
//! criterion/libtest arg parsing happens for us.)

#![cfg(feature = "alloc-core")]
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::needless_pass_by_value
)]

use std::alloc::Layout;
use std::time::{Duration, Instant};

use sefer_alloc::AllocCore;

// ===========================================================================
// §0 — process-wide memory probes (mirrors examples/first_alloc_process.rs)
// ===========================================================================

/// The 4 MiB segment size this crate reserves per Large allocation (and per
/// small-class segment). Sourced from the SAME public constant the small
/// classifier is built against (`SegmentLayout::SEGMENT`), so this harness
/// cannot silently drift from the real geometry if the segment size is ever
/// retuned.
const SEGMENT: usize = sefer_alloc::SegmentLayout::SEGMENT;

/// `SMALL_MAX` control point — the largest small size class. Confirmed
/// 258,752 B via a binary-search probe over `AllocCore::dbg_layout_class_for`
/// at harness-authoring time (see module doc). NOT re-derived at runtime from
/// a private constant (`size_classes::SMALL_MAX` is `pub(crate)`, invisible
/// from `benches/`) — instead [`assert_small_max_control_points`] cross-checks
/// this literal against the live classifier on every run, so a future retune
/// of the size-class table fails loudly here instead of silently measuring
/// the wrong geometry.
const SMALL_MAX_CONTROL: usize = 258_752;

/// `SMALL_MAX + 1 byte` rounded up to the literal 256 KiB control point the
/// task mandates (262,144 = 256 x 1024 exactly). This is 3,392 B past
/// `SMALL_MAX_CONTROL` — deliberately NOT `SMALL_MAX_CONTROL + 1` (that would
/// also cross the cliff, but the task's literal 256 KiB value is the one
/// worth reporting since it is what a caller would naturally request).
const POST_CLIFF_CONTROL: usize = 262_144;

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

/// Resident-set-size snapshot, in KiB. Windows: `WorkingSetSize` via
/// `K32GetProcessMemoryInfo` (identical calling convention to
/// `examples/first_alloc_process.rs::rss_kib`). Linux: `/proc/self/statm`
/// field 1 (resident pages) x 4 KiB. Other platforms: `0` (unavailable,
/// documented — not fabricated).
#[cfg(windows)]
fn rss_kib() -> u64 {
    // SAFETY: `counters` is a valid, sufficiently-sized, mutable out-parameter;
    // `GetCurrentProcess` returns a pseudo-handle that needs no close. This is
    // the documented calling convention for `GetProcessMemoryInfo`, identical
    // to the one already vetted in `examples/first_alloc_process.rs`.
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

/// Commit-charge snapshot, in KiB. Windows: `PagefileUsage` from the same
/// `GetProcessMemoryInfo` call (R6-OPT-A1's addition, mirrored here
/// verbatim). Linux: `/proc/self/statm` field 0 (total virtual size) x 4 KiB
/// — the nearest Linux analogue, though its overcommit model differs.
#[cfg(windows)]
fn commit_kib() -> u64 {
    // SAFETY: see `rss_kib` above — identical documented calling convention.
    unsafe {
        let mut counters: ProcessMemoryCounters = core::mem::zeroed();
        counters.cb = core::mem::size_of::<ProcessMemoryCounters>() as u32;
        let ok = K32GetProcessMemoryInfo(GetCurrentProcess(), &mut counters, counters.cb);
        if ok == 0 {
            0
        } else {
            (counters.pagefile_usage / 1024) as u64
        }
    }
}

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

#[cfg(not(any(windows, target_os = "linux")))]
fn rss_kib() -> u64 {
    0
}

#[cfg(not(any(windows, target_os = "linux")))]
fn commit_kib() -> u64 {
    0
}

// ===========================================================================
// §1 — PRNG for random-lifetime free ordering (deterministic, no dep)
// ===========================================================================

struct Xorshift64(u64);

impl Xorshift64 {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Fisher-Yates shuffle of `v` in place.
    fn shuffle<T>(&mut self, v: &mut [T]) {
        let n = v.len();
        for i in (1..n).rev() {
            let j = (self.next_u64() as usize) % (i + 1);
            v.swap(i, j);
        }
    }
}

// ===========================================================================
// §2 — percentile helpers
// ===========================================================================

/// Compute (p50, p99, mean) in nanoseconds from a slice of per-op durations.
/// Empty input reports all-zero (never panics).
fn percentiles(samples: &mut [Duration]) -> (f64, f64, f64) {
    if samples.is_empty() {
        return (0.0, 0.0, 0.0);
    }
    samples.sort_unstable();
    let n = samples.len();
    let p50 = samples[n / 2].as_secs_f64() * 1e9;
    let p99_idx = ((n as f64) * 0.99) as usize;
    let p99 = samples[p99_idx.min(n - 1)].as_secs_f64() * 1e9;
    let sum: f64 = samples.iter().map(|d| d.as_secs_f64() * 1e9).sum();
    let mean = sum / n as f64;
    (p50, p99, mean)
}

// ===========================================================================
// §3 — same-thread measurement cell (drives AllocCore directly)
// ===========================================================================

/// Result of one (size, cardinality, access-pattern) measurement cell.
#[derive(Clone, Copy)]
struct CellResult {
    alloc_p50_ns: f64,
    alloc_p99_ns: f64,
    alloc_mean_ns: f64,
    free_p50_ns: f64,
    free_p99_ns: f64,
    free_mean_ns: f64,
    segments_reserved_delta: u64,
    segments_released_delta: u64,
    table_count_after: u32,
    payload_bytes: u64,
    reserved_bytes_est: u64,
    frag_ratio: f64,
    /// `Some(k)` if `alloc` returned null on the k-th object (0-indexed) —
    /// i.e. this cell hit real OOM / address-space exhaustion before
    /// reaching its full requested cardinality. This is NOT a harness bug:
    /// at `n=1024` with a post-cliff (dedicated-4-MiB-span) size, the cell
    /// needs `n * 4 MiB` of contiguous-enough virtual address space (up to
    /// 4 GiB), which can genuinely exceed what a given host/process can
    /// reserve — itself part of the cliff's cost story, not noise to hide.
    /// All objects successfully allocated before the null are freed before
    /// this cell returns (no leak carried into the next cell), and the
    /// timing/segment-delta fields reflect only the SUCCESSFUL prefix.
    oom_at: Option<usize>,
}

impl CellResult {
    fn report(&self, label: &str) {
        let oom_note = match self.oom_at {
            Some(k) => format!("  OOM@{k}"),
            None => String::new(),
        };
        eprintln!(
            "  {label:<60} alloc(p50/p99/mean ns)={:>9.1}/{:>9.1}/{:>9.1}  \
             free(p50/p99/mean ns)={:>9.1}/{:>9.1}/{:>9.1}  \
             segs(+{}/-{})  table={}  payload={}B  frag={:.4}{oom_note}",
            self.alloc_p50_ns,
            self.alloc_p99_ns,
            self.alloc_mean_ns,
            self.free_p50_ns,
            self.free_p99_ns,
            self.free_mean_ns,
            self.segments_reserved_delta,
            self.segments_released_delta,
            self.table_count_after,
            self.payload_bytes,
            self.frag_ratio,
        );
    }
}

/// Cold same-thread cell: alloc `n` objects of `size` (all held live), time
/// each alloc, then free them in `order`, timing each free. `core` is reused
/// across cells in a sweep (so `repeated-reuse` access patterns actually
/// recycle segments/slots) — callers control freshness by constructing a new
/// `AllocCore` when a genuinely cold segment is required.
fn run_same_thread_cell(
    core: &mut AllocCore,
    size: usize,
    n: usize,
    order: FreeOrder,
) -> CellResult {
    let layout = Layout::from_size_align(size, 8).unwrap();

    let segs_reserved_before = AllocCore::dbg_segments_reserved_total();
    let segs_released_before = AllocCore::dbg_segments_released_total();

    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(n);
    let mut alloc_times: Vec<Duration> = Vec::with_capacity(n);
    let mut oom_at: Option<usize> = None;
    for i in 0..n {
        let t0 = Instant::now();
        let p = core.alloc(layout);
        alloc_times.push(t0.elapsed());
        if p.is_null() {
            eprintln!(
                "  [OOM] alloc returned null for size={size} at object {i}/{n} — \
                 real address-space/memory exhaustion, not a harness bug. Freeing the \
                 {i} objects already allocated and reporting this cell's SUCCESSFUL prefix."
            );
            oom_at = Some(i);
            break;
        }
        ptrs.push(p);
    }
    let n_live = ptrs.len();

    let segs_reserved_after = AllocCore::dbg_segments_reserved_total();

    // Determine free order over the SUCCESSFULLY allocated prefix only.
    let mut free_indices: Vec<usize> = (0..n_live).collect();
    match order {
        FreeOrder::Fifo => {}
        FreeOrder::Lifo => free_indices.reverse(),
        FreeOrder::Random(seed) => {
            let mut rng = Xorshift64::new(seed);
            rng.shuffle(&mut free_indices);
        }
    }

    let mut free_times: Vec<Duration> = Vec::with_capacity(n_live);
    for &idx in &free_indices {
        let p = ptrs[idx];
        let t0 = Instant::now();
        // SAFETY (R6-MS-1/2): `p` was returned by a prior matching `alloc`
        // call on this same `core` above, is live, and is freed exactly
        // once (each index visited exactly once via `free_indices`, a
        // permutation of `0..n_live`). On the OOM path this frees exactly
        // the successfully-allocated prefix, leaving no leak carried into
        // the next cell.
        unsafe { core.dealloc(p, layout) };
        free_times.push(t0.elapsed());
    }

    let segs_released_after = AllocCore::dbg_segments_released_total();

    let (alloc_p50, alloc_p99, alloc_mean) = percentiles(&mut alloc_times);
    let (free_p50, free_p99, free_mean) = percentiles(&mut free_times);

    let payload_bytes = (size as u64) * (n_live as u64);
    let segments_reserved_delta = segs_reserved_after - segs_reserved_before;
    let reserved_bytes_est = segments_reserved_delta * (SEGMENT as u64);
    let frag_ratio = if reserved_bytes_est > 0 {
        payload_bytes as f64 / reserved_bytes_est as f64
    } else {
        1.0
    };

    CellResult {
        alloc_p50_ns: alloc_p50,
        alloc_p99_ns: alloc_p99,
        alloc_mean_ns: alloc_mean,
        free_p50_ns: free_p50,
        free_p99_ns: free_p99,
        free_mean_ns: free_mean,
        segments_reserved_delta,
        segments_released_delta: segs_released_after - segs_released_before,
        table_count_after: core.dbg_table_count(),
        payload_bytes,
        reserved_bytes_est,
        frag_ratio,
        oom_at,
    }
}

#[derive(Clone, Copy)]
enum FreeOrder {
    Fifo,
    Lifo,
    Random(u64),
}

// ===========================================================================
// §4 — cross-thread measurement cell (gated on alloc-global + alloc-xthread)
// ===========================================================================

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
mod xthread {
    use super::{percentiles, CellResult, FreeOrder, Xorshift64, SEGMENT};
    use sefer_alloc::{AllocCore, SeferAlloc};
    use std::alloc::{GlobalAlloc, Layout};
    use std::sync::{Arc, Barrier};
    use std::thread;
    use std::time::{Duration, Instant};

    struct SendPtr(*mut u8);
    // SAFETY: transferred exactly once, producer -> consumer, via a Vec moved
    // across the thread boundary before either side touches it concurrently;
    // no aliasing (mirrors the `Block`/`unsafe impl Send` pattern in
    // `examples/rss_probe.rs`).
    unsafe impl Send for SendPtr {}

    /// Cross-thread cell: a producer thread allocates `n` objects of `size`
    /// (from ITS OWN claimed heap, the real production per-thread-heap
    /// model), hands them to a consumer thread via a channel, and the
    /// consumer frees them (in `order`) on a DIFFERENT thread than the one
    /// that allocated them — exercising the real `dealloc_foreign_slow` /
    /// `RemoteFreeRing` cross-thread-free path (`alloc-xthread`), not the
    /// `dbg_push_to_ring` test seam. Uses `SeferAlloc` (the real
    /// `GlobalAlloc` face) on both threads, matching production usage.
    ///
    /// Segment-count deltas use the SAME process-wide
    /// `AllocCore::dbg_segments_reserved_total`/`_released_total` statics the
    /// same-thread cell reads — they are process-wide by construction, so
    /// reading them from the orchestrating (main) thread around the whole
    /// producer+consumer exchange is valid regardless of which thread
    /// actually calls `alloc`/`dealloc`.
    pub fn run_cross_thread_cell(size: usize, n: usize, order: FreeOrder) -> CellResult {
        let layout = Layout::from_size_align(size, 8).unwrap();
        let start_barrier = Arc::new(Barrier::new(2));
        let (tx, rx) = std::sync::mpsc::channel::<(Vec<SendPtr>, Vec<Duration>, Option<usize>)>();

        let segs_reserved_before = AllocCore::dbg_segments_reserved_total();
        let segs_released_before = AllocCore::dbg_segments_released_total();

        let producer_barrier = Arc::clone(&start_barrier);
        let producer = thread::spawn(move || {
            let a = SeferAlloc::new();
            producer_barrier.wait();
            let mut ptrs = Vec::with_capacity(n);
            let mut alloc_times = Vec::with_capacity(n);
            let mut oom_at = None;
            for i in 0..n {
                let t0 = Instant::now();
                // SAFETY: `layout` has non-zero size and valid (power-of-two)
                // alignment.
                let p = unsafe { a.alloc(layout) };
                alloc_times.push(t0.elapsed());
                if p.is_null() {
                    // Real OOM / address-space exhaustion (e.g. n=1024
                    // post-cliff objects each needing a dedicated 4 MiB
                    // span can exceed what the host can reserve) — not a
                    // harness bug. Stop early; the consumer frees only the
                    // successfully allocated prefix.
                    oom_at = Some(i);
                    break;
                }
                ptrs.push(SendPtr(p));
            }
            tx.send((ptrs, alloc_times, oom_at))
                .expect("consumer disconnected");
        });

        start_barrier.wait();
        let (ptrs, mut alloc_times, oom_at) =
            rx.recv().expect("producer disconnected before sending");
        producer.join().expect("producer thread panicked");
        let n_live = ptrs.len();
        if let Some(k) = oom_at {
            eprintln!(
                "  [OOM] cross-thread producer alloc returned null for size={size} at \
                 object {k}/{n} — freeing the {k} objects already allocated."
            );
        }

        let segs_reserved_after = AllocCore::dbg_segments_reserved_total();

        // Determine free order on the CONSUMER thread (different from the
        // producer that allocated) — the whole point of this variant.
        let mut free_indices: Vec<usize> = (0..n_live).collect();
        match order {
            FreeOrder::Fifo => {}
            FreeOrder::Lifo => free_indices.reverse(),
            FreeOrder::Random(seed) => {
                let mut rng = Xorshift64::new(seed);
                rng.shuffle(&mut free_indices);
            }
        }

        let consumer = thread::spawn(move || {
            let a = SeferAlloc::new();
            let mut free_times = Vec::with_capacity(n_live);
            for &idx in &free_indices {
                let p = ptrs[idx].0;
                let t0 = Instant::now();
                // SAFETY: `p` was returned by the producer thread's `alloc`
                // call above with this exact `layout`, is live, and is freed
                // exactly once (each index visited exactly once). This is
                // the documented cross-thread-free contract `alloc-xthread`
                // exists to make sound.
                unsafe { a.dealloc(p, layout) };
                free_times.push(t0.elapsed());
            }
            free_times
        });
        let mut free_times = consumer.join().expect("consumer thread panicked");

        let segs_released_after = AllocCore::dbg_segments_released_total();

        let (alloc_p50, alloc_p99, alloc_mean) = percentiles(&mut alloc_times);
        let (free_p50, free_p99, free_mean) = percentiles(&mut free_times);

        let payload_bytes = (size as u64) * (n_live as u64);
        let segments_reserved_delta = segs_reserved_after - segs_reserved_before;
        let reserved_bytes_est = segments_reserved_delta * (SEGMENT as u64);
        let frag_ratio = if reserved_bytes_est > 0 {
            payload_bytes as f64 / reserved_bytes_est as f64
        } else {
            1.0
        };

        CellResult {
            alloc_p50_ns: alloc_p50,
            alloc_p99_ns: alloc_p99,
            alloc_mean_ns: alloc_mean,
            free_p50_ns: free_p50,
            free_p99_ns: free_p99,
            free_mean_ns: free_mean,
            segments_reserved_delta,
            segments_released_delta: segs_released_after - segs_released_before,
            table_count_after: 0, // process-wide table not scoped to one AllocCore here.
            payload_bytes,
            reserved_bytes_est,
            frag_ratio,
            oom_at,
        }
    }
}

// ===========================================================================
// §5 — sweep configuration
// ===========================================================================

/// Byte sizes swept, per the task's mandatory list. `SMALL_MAX_CONTROL`
/// (258,752 B) and `POST_CLIFF_CONTROL` (262,144 B) are the mandatory
/// control points either side of the cliff.
const SIZES: &[(usize, &str)] = &[
    (240 * 1024, "240KiB"),
    (252 * 1024, "252KiB"),
    (253 * 1024, "253KiB"),
    (255 * 1024, "255KiB"),
    (256 * 1024, "256KiB(=262144B)"),
    (257 * 1024, "257KiB"),
    (SMALL_MAX_CONTROL, "SMALL_MAX(258752B)"),
    (POST_CLIFF_CONTROL, "POST_CLIFF(262144B)"),
    (320 * 1024, "320KiB"),
    (384 * 1024, "384KiB"),
    (512 * 1024, "512KiB"),
    (768 * 1024, "768KiB"),
    (1024 * 1024, "1MiB"),
    (1536 * 1024, "1.5MiB"),
    (2048 * 1024, "2MiB"),
    (4096 * 1024, "4MiB"),
];

/// Sizes touched by the quick (default) profile — per this project's
/// "benchmarks run fast by default" convention. Always includes both control
/// points (the harness's core deliverable) plus a small representative
/// slice either side of the cliff.
const QUICK_SIZES: &[usize] = &[
    240 * 1024,
    SMALL_MAX_CONTROL,
    POST_CLIFF_CONTROL,
    512 * 1024,
    2048 * 1024,
];

/// Cardinalities swept, per the task's mandatory list.
const CARDINALITIES: &[usize] = &[1, 8, 64, 1024];

/// Cardinalities touched by the quick profile (1024 live 4 MiB objects would
/// need 4 GiB+ of address space for large sizes — deferred to `--reduced`/
/// `--full-matrix`).
const QUICK_CARDINALITIES: &[usize] = &[1, 8, 64];

/// Confirms the compile-time literal `SMALL_MAX_CONTROL` still matches the
/// live classifier — so a future retune of the size-class table fails loudly
/// here instead of silently measuring the wrong geometry (this harness has
/// no access to the `pub(crate)` `size_classes::SMALL_MAX` from `benches/`,
/// so the cross-check is via the public `dbg_layout_class_for` seam instead).
fn assert_small_max_control_points(core: &AllocCore) {
    let at_control = Layout::from_size_align(SMALL_MAX_CONTROL, 8).unwrap();
    let one_past = Layout::from_size_align(SMALL_MAX_CONTROL + 1, 8).unwrap();
    assert!(
        core.dbg_layout_class_for(at_control).is_some(),
        "SMALL_MAX_CONTROL ({SMALL_MAX_CONTROL}) no longer classifies as a small class — \
         the size-class table has been retuned; update this harness's control-point literal."
    );
    assert!(
        core.dbg_layout_class_for(one_past).is_none(),
        "SMALL_MAX_CONTROL + 1 ({}) still classifies as a small class — \
         SMALL_MAX has grown past this harness's assumed control point; update the literal.",
        SMALL_MAX_CONTROL + 1
    );
    assert!(
        core.dbg_layout_class_for(Layout::from_size_align(POST_CLIFF_CONTROL, 8).unwrap())
            .is_none(),
        "POST_CLIFF_CONTROL (262144) unexpectedly classifies as a SMALL class — \
         the cliff has moved past literal 256 KiB; this harness's premise no longer holds."
    );
    eprintln!(
        "control-point check: SMALL_MAX_CONTROL={SMALL_MAX_CONTROL}B classifies small, \
         SMALL_MAX_CONTROL+1B and POST_CLIFF_CONTROL={POST_CLIFF_CONTROL}B classify large. OK."
    );
}

// ===========================================================================
// §6 — repeated-measurement consistency check (the R6-OPT-A2 lesson applied)
// ===========================================================================

/// Runs the SAME (size, cardinality) cell three times, interleaved with an
/// UNRELATED cell in between (not back-to-back — the exact shape that
/// exposed the R6-OPT-A2 sibling bug: a later occurrence of the same
/// configuration degrading due to state carried forward by an earlier,
/// different cell). Uses a FRESH `AllocCore` per repeat (this harness's
/// same-thread cells are driven by an explicit, caller-owned `AllocCore`
/// rather than a shared registry slot pool, so there is no LIFO-reuse
/// vector analogous to R6-OPT-A2's `HeapRegistry::recycle` — but the
/// consistency check is run anyway, unconditionally, as the task requires:
/// a similar leak could exist independently here via a different
/// mechanism, e.g. `dbg_table_count`'s free-list not recycling cleanly
/// across `AllocCore` instances sharing the same process-wide segment
/// registry statics).
fn verify_repeated_measurement_consistency() {
    const REPEATS: usize = 3;
    const CELL_SIZE: usize = SMALL_MAX_CONTROL;
    const CELL_N: usize = 64;

    eprintln!(
        "\nrepeated-measurement consistency check (size={CELL_SIZE}B, n={CELL_N}, \
         {REPEATS} repeats interleaved with an unrelated cell):"
    );

    let mut p50s: Vec<f64> = Vec::with_capacity(REPEATS);
    for i in 0..REPEATS {
        let mut core = AllocCore::new().expect("AllocCore::new failed");
        let result = run_same_thread_cell(&mut core, CELL_SIZE, CELL_N, FreeOrder::Fifo);
        eprintln!(
            "  repeat[{i}]: alloc_p50={:.1}ns free_p50={:.1}ns segs(+{}/-{}) table={}",
            result.alloc_p50_ns,
            result.free_p50_ns,
            result.segments_reserved_delta,
            result.segments_released_delta,
            result.table_count_after,
        );
        p50s.push(result.alloc_p50_ns + result.free_p50_ns);

        if i + 1 < REPEATS {
            // Interleave an unrelated cell (different size, different
            // cardinality, fresh AllocCore) between repeats.
            let mut unrelated = AllocCore::new().expect("AllocCore::new failed");
            let _ = run_same_thread_cell(&mut unrelated, 512 * 1024, 8, FreeOrder::Random(0xABCD));
        }
    }

    let max_p50 = p50s.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let min_p50 = p50s.iter().copied().fold(f64::INFINITY, f64::min);
    let ratio = if min_p50 > 0.0 {
        max_p50 / min_p50
    } else {
        1.0
    };
    eprintln!(
        "  spread across {REPEATS} repeats: min={min_p50:.1}ns max={max_p50:.1}ns ratio={ratio:.2}x"
    );

    // Generous ceiling (10x): this check's purpose is to catch a GROSS
    // state-leak degradation (the R6-OPT-A2 bug showed a ~10-20x blowup,
    // ns -> ms), not to police normal timer/scheduler noise between runs.
    assert!(
        ratio < 10.0,
        "REGRESSION-SHAPED RESULT: repeated measurement of the SAME (size={CELL_SIZE}, \
         n={CELL_N}) cell varied by {ratio:.2}x across {REPEATS} repeats interleaved with an \
         unrelated cell — this is the signature the R6-OPT-A2 sibling task's cross-cell \
         state-leak bug produced (a later occurrence of the same configuration silently \
         degrading due to state left behind by an earlier, different measurement). \
         Investigate before trusting any repeated-reuse numbers from this harness."
    );
    eprintln!("  PASS: repeated-measurement consistency held (ratio < 10x).\n");
}

// ===========================================================================
// §7 — sweep drivers
// ===========================================================================

fn run_cold_sweep(sizes: &[usize], cardinalities: &[usize]) {
    eprintln!("\n=== cold (same-thread, FIFO free order) ===");
    for &size in sizes {
        for &n in cardinalities {
            let mut core = AllocCore::new().expect("AllocCore::new failed");
            let label = format!("cold size={size}B n={n}");
            let result = run_same_thread_cell(&mut core, size, n, FreeOrder::Fifo);
            result.report(&label);
        }
    }
}

/// Repeated-reuse access pattern: the same `AllocCore` allocs/frees the same
/// (size, cardinality) point across several rounds so segments/slots get
/// recycled — the access pattern the task specifically calls out as needing
/// the R6-OPT-A2-style repeated-measurement care (this harness's own
/// [`verify_repeated_measurement_consistency`] covers that separately).
/// Frees in **LIFO** order deliberately: LIFO is the maximally
/// freelist/segment-favorable order (mirrors a stack-discipline workload)
/// and is the natural counterpart to `run_random_lifetime_sweep`'s
/// deliberately UNfavorable shuffled order — together the two sweeps
/// bracket the best-case and adversarial-case reuse behavior either side of
/// the cliff.
fn run_repeated_reuse_sweep(sizes: &[usize], cardinalities: &[usize]) {
    const REUSE_ROUNDS: usize = 4;
    eprintln!(
        "\n=== repeated-reuse (same AllocCore across {REUSE_ROUNDS} rounds, LIFO free order) ==="
    );
    for &size in sizes {
        for &n in cardinalities {
            let mut core = AllocCore::new().expect("AllocCore::new failed");
            for round in 0..REUSE_ROUNDS {
                let label = format!("reuse size={size}B n={n} round={round}");
                let result = run_same_thread_cell(&mut core, size, n, FreeOrder::Lifo);
                result.report(&label);
            }
        }
    }
}

fn run_random_lifetime_sweep(sizes: &[usize], cardinalities: &[usize]) {
    eprintln!("\n=== random-lifetime (same-thread, shuffled free order) ===");
    for &size in sizes {
        for &n in cardinalities {
            let mut core = AllocCore::new().expect("AllocCore::new failed");
            let label = format!("random size={size}B n={n}");
            let result = run_same_thread_cell(&mut core, size, n, FreeOrder::Random(0x9E37_79B9));
            result.report(&label);
        }
    }
}

#[cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
fn run_cross_thread_sweep(sizes: &[usize], cardinalities: &[usize]) {
    eprintln!("\n=== cross-thread (producer allocs, consumer frees, FIFO order) ===");
    for &size in sizes {
        for &n in cardinalities {
            let label = format!("xthread size={size}B n={n}");
            let result = xthread::run_cross_thread_cell(size, n, FreeOrder::Fifo);
            result.report(&label);
        }
    }
}

#[cfg(not(all(feature = "alloc-global", feature = "alloc-xthread")))]
fn run_cross_thread_sweep(_sizes: &[usize], _cardinalities: &[usize]) {
    eprintln!(
        "\n=== cross-thread variant SKIPPED (requires --features \"alloc-global alloc-xthread\") ==="
    );
}

/// The core deliverable: report the 258,752 B vs 262,144 B discontinuity
/// explicitly, at cardinality 1024 (the cardinality where the cliff's cost
/// is starkest — 1024 dedicated 4 MiB spans vs a shared handful of
/// segments).
fn report_cliff_discontinuity(full_cardinality: bool) {
    eprintln!("\n=== CORE DELIVERABLE: SMALL_MAX cliff discontinuity ===");
    let cardinalities: &[usize] = if full_cardinality {
        &[1, 8, 64, 1024]
    } else {
        &[1, 8, 64]
    };
    for &n in cardinalities {
        let mut core_pre = AllocCore::new().expect("AllocCore::new failed");
        let pre = run_same_thread_cell(&mut core_pre, SMALL_MAX_CONTROL, n, FreeOrder::Fifo);
        let mut core_post = AllocCore::new().expect("AllocCore::new failed");
        let post = run_same_thread_cell(&mut core_post, POST_CLIFF_CONTROL, n, FreeOrder::Fifo);

        let pre_oom = pre.oom_at.map_or(String::new(), |k| format!("  [OOM@{k}]"));
        let post_oom = post
            .oom_at
            .map_or(String::new(), |k| format!("  [OOM@{k}]"));
        eprintln!(
            "  n={n:>5}:  PRE-CLIFF  (258752B) segs=+{:<4} table={:<4} frag={:.4}  reserved={}KiB{pre_oom}",
            pre.segments_reserved_delta,
            pre.table_count_after,
            pre.frag_ratio,
            pre.reserved_bytes_est / 1024,
        );
        eprintln!(
            "            POST-CLIFF (262144B) segs=+{:<4} table={:<4} frag={:.4}  reserved={}KiB{post_oom}",
            post.segments_reserved_delta,
            post.table_count_after,
            post.frag_ratio,
            post.reserved_bytes_est / 1024,
        );
        eprintln!(
            "            -> segment-count ratio (post/pre) = {:.2}x   reserved-bytes ratio = {:.2}x",
            post.segments_reserved_delta as f64 / pre.segments_reserved_delta.max(1) as f64,
            post.reserved_bytes_est as f64 / pre.reserved_bytes_est.max(1) as f64,
        );
        if pre.oom_at.is_some() || post.oom_at.is_some() {
            eprintln!(
                "            NOTE: one or both sides hit real OOM at this cardinality — \
                 ratios above are computed over the OOM'd side's SUCCESSFUL prefix, not the \
                 full requested n; still informative (the OOM itself, on the post-cliff side \
                 at high cardinality, IS part of the cliff's cost story) but not apples-to-apples \
                 with a non-OOM row."
            );
        }
    }
}

// ===========================================================================
// §8 — main
// ===========================================================================

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let full_matrix = args.iter().any(|a| a == "--full-matrix");
    let reduced = args.iter().any(|a| a == "--reduced");

    eprintln!("medium_size_sweep — SMALL_MAX cliff judge (R6-OPT-A3)");
    eprintln!("SEGMENT={SEGMENT}B  SMALL_MAX_CONTROL={SMALL_MAX_CONTROL}B  POST_CLIFF_CONTROL={POST_CLIFF_CONTROL}B");
    let rss0 = rss_kib();
    let commit0 = commit_kib();
    eprintln!("process baseline: rss={rss0}KiB commit={commit0}KiB");

    {
        let probe = AllocCore::new().expect("AllocCore::new failed");
        assert_small_max_control_points(&probe);
    }

    // Always run the repeated-measurement consistency check — the R6-OPT-A2
    // lesson applied unconditionally, not opt-in.
    verify_repeated_measurement_consistency();

    // Always report the core deliverable.
    report_cliff_discontinuity(full_matrix);

    let (sizes, cardinalities): (Vec<usize>, &[usize]) = if full_matrix {
        (SIZES.iter().map(|&(s, _)| s).collect(), CARDINALITIES)
    } else if reduced {
        (SIZES.iter().map(|&(s, _)| s).collect(), &[1, 8, 64, 1024])
    } else {
        (QUICK_SIZES.to_vec(), QUICK_CARDINALITIES)
    };

    run_cold_sweep(&sizes, cardinalities);
    run_random_lifetime_sweep(&sizes, cardinalities);

    // repeated-reuse and cross-thread are more expensive (4x rounds, thread
    // spawn per cell) — keep them to the control points + a small
    // cardinality slice in the quick profile.
    let (reuse_sizes, reuse_card): (&[usize], &[usize]) = if full_matrix || reduced {
        (&sizes, cardinalities)
    } else {
        (&[SMALL_MAX_CONTROL, POST_CLIFF_CONTROL], &[1, 8, 64])
    };
    run_repeated_reuse_sweep(reuse_sizes, reuse_card);
    run_cross_thread_sweep(reuse_sizes, reuse_card);

    let rss1 = rss_kib();
    let commit1 = commit_kib();
    eprintln!(
        "\nprocess final: rss={rss1}KiB (delta={}KiB) commit={commit1}KiB (delta={}KiB)",
        rss1 as i64 - rss0 as i64,
        commit1 as i64 - commit0 as i64,
    );

    eprintln!(
        "\nmode: {}",
        if full_matrix {
            "--full-matrix"
        } else if reduced {
            "--reduced"
        } else {
            "quick (default) — pass --reduced or --full-matrix for deeper coverage"
        }
    );
}
