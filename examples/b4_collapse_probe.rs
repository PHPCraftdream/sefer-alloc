//! Collapse probe for `SeferAlloc` (bimodal larson collapse): runs ONLY
//! `SeferAlloc` with `malloc-bench-rs` workload semantics copied verbatim,
//! sampling per-window throughput + perf counters + Windows process metrics.
//! Output: `PASS`/`WIN`/`CNT` lines + one `RESULT_JSON {...}` line.
//!
//! Run: `cargo run --release --example b4_collapse_probe`
//!   `--features "alloc-global alloc-xthread [internals bench-internals]"`.
//! Ops accounting: `ops` = step ops + drained-mailbox ops, matching
//! `malloc_bench_rs::larson_worker`'s returned count.

#![cfg(feature = "alloc-global")]
// Win32 FFI structs are declared locally (cfg(windows) only); no FFI dep.
#![allow(unsafe_code)]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::SeferAlloc;

/// A live allocation: ptr + exact `Layout` (needed for `dealloc`).
struct Block {
    ptr: *mut u8,
    layout: Layout,
}

// SAFETY: ownership transfer only; the consumer is the unique owner.
unsafe impl Send for Block {}

struct XorShift64(u64);

impl XorShift64 {
    #[inline]
    fn new(seed: u64) -> Self {
        Self(seed | 1) // avoid the all-zero state (xorshift fixed point)
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

    /// Uniform-ish in `[0, n)` (n > 0).
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// Small-skewed size: mostly 16..512 B, rarely up to ~8 KiB.
#[inline]
fn pick_size(rng: &mut XorShift64) -> usize {
    let r = rng.next_u64();
    if r.is_multiple_of(32) {
        512 + (r >> 8) as usize % (8 * 1024 - 512) // ~3% large
    } else {
        16 + (r >> 8) as usize % (512 - 16) // ~97% small
    }
}

#[inline]
fn layout_for(size: usize) -> Layout {
    Layout::from_size_align(size.max(1), 8).unwrap()
}

/// Allocate a small-skewed block, touch its first byte (0xA5); null from
/// `SeferAlloc` = allocator bug → panic. Safety: freed exactly once.
unsafe fn alloc_block(a: &SeferAlloc, rng: &mut XorShift64) -> Block {
    let layout = layout_for(pick_size(rng));
    // SAFETY: layout has non-zero size and valid alignment.
    let ptr = unsafe { a.alloc(layout) };
    if ptr.is_null() {
        panic!(
            "SeferAlloc returned null for layout {{size={}, align={}}} — allocator bug",
            layout.size(),
            layout.align()
        );
    }
    // SAFETY: ptr is valid for `layout.size() >= 1` bytes.
    unsafe { ptr.write(0xA5) };
    Block { ptr, layout }
}

/// Free a block produced by `alloc_block`. Safety: allocated by `a`, live.
unsafe fn free_block(a: &SeferAlloc, block: Block) {
    // SAFETY: block.ptr came from `a.alloc(block.layout)`; freed once.
    unsafe { a.dealloc(block.ptr, block.layout) };
}

/// Drain the mailbox, freeing each block once (counted into `local_ops`).
/// Safety: blocks are uniquely owned here; freed exactly once.
unsafe fn drain_mailbox(a: &SeferAlloc, rx: &Receiver<Block>, local_ops: &mut u64) {
    while let Ok(block) = rx.try_recv() {
        // SAFETY: unique ownership, freed once.
        unsafe { free_block(a, block) };
        *local_ops += 1;
    }
}

/// Flush a worker's local counter into its atomic slot (the only cross-thread
/// RMW, amortised over the flush interval).
fn flush_ops(local_ops: &mut u64, slot: &AtomicU64) {
    slot.fetch_add(*local_ops, Ordering::Relaxed);
    *local_ops = 0;
}

const HANDOFF_EVERY: usize = 16;
/// Flush cadence (larson): every 1024 steps.
const FLUSH_EVERY: usize = 1024;

/// Hand `block` to another thread; on send failure (receiver gone, so never
/// observed elsewhere) free it locally — freed exactly once.
unsafe fn cross_thread_free(
    a: &SeferAlloc,
    rng: &mut XorShift64,
    senders: &[Sender<Block>],
    self_idx: usize,
    block: Block,
) {
    let n = senders.len();
    let mut target = rng.below(n);
    if target == self_idx {
        target = (target + 1) % n;
    }
    if let Err(e) = senders[target].send(block) {
        // SAFETY: send failed → block never observed elsewhere.
        unsafe { free_block(a, e.0) };
    }
}

/// One larson worker (copied from `malloc_bench_rs`). Safety: free-exactly-
/// once; `a` is a shared stateless facade.
#[allow(clippy::too_many_arguments)]
unsafe fn larson_worker(
    a: &SeferAlloc,
    seed: u64,
    steps: usize,
    working_set: usize,
    senders: &[Sender<Block>],
    rx: &Receiver<Block>,
    self_idx: usize,
    local_ops: &mut u64,
    slot: &AtomicU64,
) {
    let mut rng = XorShift64::new(seed);
    let n_threads = senders.len();

    let mut slots: Vec<Option<Block>> = Vec::with_capacity(working_set);
    for _ in 0..working_set {
        // SAFETY: freed once below via `slots`.
        slots.push(Some(unsafe { alloc_block(a, &mut rng) }));
    }

    for step in 0..steps {
        // SAFETY: received blocks are uniquely owned here.
        unsafe { drain_mailbox(a, rx, local_ops) };

        let idx = rng.below(working_set);

        if n_threads > 1 && step % HANDOFF_EVERY == 0 {
            // Hand the slot's block to another thread (ownership moves out).
            if let Some(block) = slots[idx].take() {
                // SAFETY: ownership moves out; helper frees-on-failure.
                unsafe { cross_thread_free(a, &mut rng, senders, self_idx, block) };
            }
        } else if let Some(block) = slots[idx].take() {
            // SAFETY: allocated by `a`, owned here, freed once.
            unsafe { free_block(a, block) };
        }

        // SAFETY: tracked in `slots`.
        slots[idx] = Some(unsafe { alloc_block(a, &mut rng) });
        *local_ops += 1;
        if step % FLUSH_EVERY == FLUSH_EVERY - 1 {
            flush_ops(local_ops, slot);
        }
    }

    // SAFETY: teardown — every remaining block is owned here, freed once.
    for block in slots.drain(..).flatten() {
        unsafe { free_block(a, block) };
    }
    flush_ops(local_ops, slot);
    black_box(());
}

/// One mstress worker (copied from `malloc_bench_rs`). Safety: as larson.
#[allow(clippy::too_many_arguments)]
unsafe fn mstress_worker(
    a: &SeferAlloc,
    seed: u64,
    rounds: usize,
    block_count: usize,
    senders: &[Sender<Block>],
    rx: &Receiver<Block>,
    self_idx: usize,
    local_ops: &mut u64,
    slot: &AtomicU64,
) {
    let mut rng = XorShift64::new(seed);
    let n_threads = senders.len();

    for _ in 0..rounds {
        // SAFETY: inbound blocks uniquely owned here.
        unsafe { drain_mailbox(a, rx, local_ops) };

        let mut blocks: Vec<Option<Block>> = Vec::with_capacity(block_count);
        for _ in 0..block_count {
            // SAFETY: tracked in `blocks`.
            blocks.push(Some(unsafe { alloc_block(a, &mut rng) }));
        }

        // Free half in random order; ~1 in 8 of those go cross-thread.
        let half = block_count / 2; // free half; ~1/8 cross-thread
        for _ in 0..half {
            let idx = rng.below(block_count);
            if let Some(block) = blocks[idx].take() {
                if n_threads > 1 && rng.below(8) == 0 {
                    // SAFETY: ownership moves out; helper frees-on-failure.
                    unsafe { cross_thread_free(a, &mut rng, senders, self_idx, block) };
                } else {
                    // SAFETY: owned here, freed once.
                    unsafe { free_block(a, block) };
                }
                *local_ops += 1;
            }
        }

        for slot_ in blocks.iter_mut() {
            if slot_.is_none() {
                // SAFETY: tracked in `blocks`.
                *slot_ = Some(unsafe { alloc_block(a, &mut rng) });
            }
        }

        // SAFETY: everything remaining is owned here, freed once.
        for block in blocks.drain(..).flatten() {
            unsafe { free_block(a, block) };
            *local_ops += 1;
        }
        flush_ops(local_ops, slot); // once per round
    }
    flush_ops(local_ops, slot);
    black_box(());
}
/// Process metrics: (page_faults, working_set, user_ms, kernel_ms, cycles).
#[cfg(windows)]
fn proc_metrics_snapshot() -> (u64, u64, u64, u64, u64) {
    #[rustfmt::skip]
    mod ffi { // locally-declared Win32 FFI (no external crate)
        use std::os::raw::c_void;
        pub type Handle = *mut c_void;
        #[repr(C)] pub struct FileTime { pub low: u32, pub high: u32 }
        // Full PROCESS_MEMORY_COUNTERS: `cb` must be the full size or the call
        // fails (hence the unused quota/pagefile fields).
        #[repr(C)] pub struct ProcessMemoryCounters {
            pub cb: u32, pub page_fault_count: u32,
            pub peak_working_set_size: usize, pub working_set_size: usize,
            pub quota_peak_paged_pool_usage: usize, pub quota_paged_pool_usage: usize,
            pub quota_peak_non_paged_pool_usage: usize, pub quota_non_paged_pool_usage: usize,
            pub pagefile_usage: usize, pub peak_pagefile_usage: usize,
        }
        extern "system" {
            pub fn GetCurrentProcess() -> Handle;
            pub fn GetProcessTimes(process: Handle, creation: *mut FileTime, exit: *mut FileTime, kernel: *mut FileTime, user: *mut FileTime) -> i32;
            pub fn K32GetProcessMemoryInfo(process: Handle, counters: *mut ProcessMemoryCounters, cb: u32) -> i32;
            pub fn QueryProcessCycleTime(process: Handle, cycles: *mut u64) -> i32;
        }
    }

    // SAFETY: every call passes the (always-valid) current-process
    // pseudo-handle and correctly-sized, correctly-aligned out-structures
    // (zero-init is valid for these plain-old-data types); nothing retains
    // the pointers.
    unsafe {
        let process = ffi::GetCurrentProcess();

        // SAFETY: zero-init is valid for this POD struct; `cb` set below.
        let mut pmc: ffi::ProcessMemoryCounters = std::mem::zeroed();
        pmc.cb = std::mem::size_of::<ffi::ProcessMemoryCounters>() as u32;
        let ok_mem = ffi::K32GetProcessMemoryInfo(process, &mut pmc, pmc.cb) != 0;
        let page_faults = u64::from(if ok_mem { pmc.page_fault_count } else { 0 });
        let working_set = if ok_mem {
            pmc.working_set_size as u64
        } else {
            0
        };

        // SAFETY: zero-init is valid for these POD out-params.
        let mut times: [ffi::FileTime; 4] = std::mem::zeroed();
        let ok_times = ffi::GetProcessTimes(
            process,
            &mut times[0],
            &mut times[1],
            &mut times[2],
            &mut times[3],
        ) != 0;
        let ft_ms = |ft: &ffi::FileTime| (u64::from(ft.high) << 32 | u64::from(ft.low)) / 10_000;
        let (user_ms, kernel_ms) = if ok_times {
            (ft_ms(&times[3]), ft_ms(&times[2]))
        } else {
            (0, 0)
        };

        let mut cycles: u64 = 0;
        if ffi::QueryProcessCycleTime(process, &mut cycles) == 0 {
            cycles = 0;
        }

        (page_faults, working_set, user_ms, kernel_ms, cycles)
    }
}

/// Non-Windows stub: no process metrics available.
#[cfg(not(windows))]
fn proc_metrics_snapshot() -> (u64, u64, u64, u64, u64) {
    (0, 0, 0, 0, 0)
}
/// Snapshot all counters in fixed sorted order; groups gated like the lib.
fn counters_snapshot(alloc: &SeferAlloc) -> Vec<(&'static str, u64)> {
    let mut out: Vec<(&'static str, u64)> = Vec::new();

    let stats = alloc.stats();
    out.push((
        "foreign_or_unroutable_frees",
        stats.foreign_or_unroutable_frees,
    ));
    out.extend_from_slice(&[
        ("config_conflicts", stats.config_conflicts),
        ("decommit_calls", stats.decommit_calls),
        ("heaps_claimed_high_water", stats.heaps_claimed_high_water),
        ("large_xthread_reclaimed", stats.large_xthread_reclaimed),
        ("segments_released_total", stats.segments_released_total),
        ("segments_reserved_total", stats.segments_reserved_total),
    ]);

    // internals-gated re-export (src/global/mod.rs).
    #[cfg(feature = "internals")]
    out.push((
        "fallback_lock_acquisitions",
        sefer_alloc::global::dbg_fallback_lock_acquisitions(),
    ));
    #[cfg(feature = "bench-internals")]
    out.push((
        "exit_lock_giveups",
        sefer_alloc::global::dbg_exit_lock_giveups(),
    ));

    #[cfg(feature = "bench-internals")]
    {
        let src = sefer_alloc::registry::HeapRegistry::dbg_registry_claim_sources();
        out.extend_from_slice(&[
            ("claims_bump", src[1]),
            ("claims_cap_scan", src[2]),
            (
                "claims_free_set",
                sefer_alloc::registry::HeapRegistry::dbg_registry_free_set_claims(),
            ),
            ("claims_hint", src[0]),
            ("claims_recovery", src[3]),
        ]);
    }

    #[cfg(all(
        feature = "internals",
        feature = "bench-internals",
        feature = "alloc-global",
        feature = "alloc-xthread"
    ))]
    {
        let (drain_calls, records) = alloc.dbg_sidecar_ingress_stats();
        let b4 = SeferAlloc::dbg_b4_collapse_counters();
        out.extend_from_slice(&[
            ("sidecar_ingress_drain_calls", drain_calls),
            ("sidecar_ingress_records_consumed", records),
            (
                "background_ingress_step_calls",
                alloc.dbg_background_ingress_step_calls(),
            ),
            ("b4_find_segment_calls", b4[0]),
            ("b4_scan_calls", b4[1]),
            ("b4_scan_words", b4[2]),
            ("b4_foreign_publish_ok", b4[3]),
            ("b4_directory_lookups", b4[4]),
            ("b4_shard_lock_spins", b4[5]),
            ("b4_shard_lock_yields", b4[6]),
        ]);
    }

    #[cfg(windows)]
    {
        let (pf, ws, ums, kms, cyc) = proc_metrics_snapshot();
        out.extend_from_slice(&[
            ("proc_cycles", cyc),
            ("proc_kernel_ms", kms),
            ("proc_page_faults", pf),
            ("proc_user_ms", ums),
            ("proc_working_set_bytes", ws),
        ]);
    }

    out
}
/// Per-key deltas (end − start), nonzero only, sorted.
fn counters_delta(
    start: &[(&'static str, u64)],
    end: &[(&'static str, u64)],
) -> Vec<(&'static str, u64)> {
    let mut deltas: Vec<(&'static str, u64)> = Vec::new();
    for ((k0, v0), (k1, v1)) in start.iter().zip(end.iter()) {
        debug_assert_eq!(k0, k1);
        let d = v1.saturating_sub(*v0);
        if d != 0 {
            deltas.push((k0, d));
        }
    }
    deltas.sort_unstable_by_key(|&(k, _)| k);
    deltas
}

/// Fixed-point ops/sec × 100 from `ops` over `elapsed_us` (rounded).
fn ops_per_sec_x100(ops: u64, elapsed_us: u128) -> u64 {
    if elapsed_us == 0 {
        return 0;
    }
    // ops * 1e6 us/s * 100 / elapsed_us.
    ((u128::from(ops) * 100_000_000 + elapsed_us / 2) / elapsed_us) as u64
}

/// `ops_per_sec` with 2 decimals (human-readable lines).
fn ops_per_sec_f(ops: u64, elapsed_us: u128) -> f64 {
    ops_per_sec_x100(ops, elapsed_us) as f64 / 1e8 // → millions of ops/sec
}

/// Per-pass knobs (steps/sizes/window/seed).
#[derive(Clone)]
struct BenchCfg {
    steps: usize,
    ws: usize,
    mb: usize,
    window_ms: u64,
    seed: u64,
}

/// One pass at thread count `t`: spawn `t` workers (Barrier start, one mpsc
/// mailbox each), sample windows, join, drain leftovers, print PASS/WIN lines.
fn run_pass(
    workload: &str,
    alloc: &SeferAlloc,
    t: usize,
    cfg: &BenchCfg,
) -> (u64, u128, Vec<(u64, u64)>) {
    let finished = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(Barrier::new(t));
    // Per-thread op slots (workers flush LOCAL counters in, amortising the
    // cross-thread RMW to once per flush interval instead of once per op).
    let ops_slots: Arc<Vec<AtomicU64>> = Arc::new((0..t).map(|_| AtomicU64::new(0)).collect());
    let total_ops = || {
        ops_slots
            .iter()
            .map(|s| s.load(Ordering::Relaxed))
            .sum::<u64>()
    };

    // One mailbox per worker; each holds the full sender-set clone (as in
    // `malloc_bench_rs::run`) and returns `rx` + finish timestamp post-run.
    let mut senders: Vec<Sender<Block>> = Vec::with_capacity(t);
    let mut rxs: Vec<Receiver<Block>> = Vec::with_capacity(t);
    for _ in 0..t {
        let (tx, rx) = channel::<Block>();
        senders.push(tx);
        rxs.push(rx);
    }
    let senders = Arc::new(senders);
    // WORK-time: shared `start` read before spawn; each worker reports its
    // OWN finish elapsed as its last statement; PASS uses max(finish_us) —
    // wall time incl. the sampler's trailing sleep would inflate T=1 ~10x.
    let start = Arc::new(Instant::now());
    let (done_tx, done_rx) = channel::<(Receiver<Block>, u128)>();

    let mut handles = Vec::with_capacity(t);
    for (i, rx) in rxs.into_iter().enumerate() {
        let (finished, ops_slots) = (Arc::clone(&finished), Arc::clone(&ops_slots));
        let (barrier, senders) = (Arc::clone(&barrier), Arc::clone(&senders));
        let (done_tx, start) = (done_tx.clone(), Arc::clone(&start));
        let workload = workload.to_string();
        let cfg = cfg.clone();
        let worker_seed = cfg.seed.wrapping_add(0xA076_1D64_78BD_642Fu64)
            ^ (i as u64).wrapping_mul(0xE703_7ED1_A0B0_28DB);
        handles.push(thread::spawn(move || unsafe {
            barrier.wait();
            // Own `SeferAlloc` (stateless facade → 'static closure).
            // SAFETY: free-exactly-once upheld here + post-join sweep.
            let alloc = SeferAlloc::new();
            let sd = worker_seed;
            let mut local_ops: u64 = 0;
            let slot = &ops_slots[i];
            if workload == "mstress" {
                mstress_worker(
                    &alloc,
                    sd,
                    cfg.steps,
                    cfg.mb,
                    &senders,
                    &rx,
                    i,
                    &mut local_ops,
                    slot,
                );
            } else {
                larson_worker(
                    &alloc,
                    sd,
                    cfg.steps,
                    cfg.ws,
                    &senders,
                    &rx,
                    i,
                    &mut local_ops,
                    slot,
                );
            }
            // Last drain before handing `rx` back (malloc_bench_rs style).
            drain_mailbox(&alloc, &rx, &mut local_ops);
            flush_ops(&mut local_ops, slot);
            drop(senders);
            let finish_us = start.elapsed().as_micros(); // OWN finish timestamp
            let _ = done_tx.send((rx, finish_us));
            finished.fetch_add(1, Ordering::Relaxed);
        }));
    }
    drop(done_tx);

    // Sampler: windows are cumulative WALL-time samples; the last window may
    // extend past work end — intentional, PASS uses work-time finish stamps.
    let (sample, window_dur) = (
        cfg.window_ms != 0,
        Duration::from_millis(cfg.window_ms.max(1)),
    );
    let start_wall = Instant::now();
    let mut windows: Vec<(u64, u64)> = Vec::new(); // (ms, cumulative ops)
    let mut next_at = start_wall + window_dur;
    while finished.load(Ordering::Relaxed) < t {
        if !sample {
            thread::sleep(Duration::from_millis(5));
            continue;
        }
        if let Some(wait) = next_at.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }
        let ms = start_wall.elapsed().as_millis() as u64;
        let ops = total_ops();
        windows.push((ms, ops));
        println!(
            "WIN t={} k={} ms={} ops_per_sec={:.2}M",
            t,
            windows.len(),
            ms,
            ops_per_sec_f(ops, start_wall.elapsed().as_micros())
        );
        next_at += window_dur;
    }

    for handle in handles {
        handle.join().expect("worker thread panicked");
    }

    // Post-join sweep: all senders dropped → drain each returned receiver
    // once (leftovers are housekeeping, freed but not counted as ops).
    drop(senders);
    let mut leftover_ops: u64 = 0;
    let mut work_us: u128 = 0;
    while let Ok((rx, finish_us)) = done_rx.try_recv() {
        // SAFETY: uniquely-owned blocks; no sender can race anymore.
        unsafe { drain_mailbox(alloc, &rx, &mut leftover_ops) };
        work_us = work_us.max(finish_us); // PASS time = latest worker finish
    }

    let ops = total_ops();
    println!(
        "PASS t={} ops={} secs={:.3} ops_per_sec={:.2}M",
        t,
        ops,
        work_us as f64 / 1e6,
        ops_per_sec_f(ops, work_us)
    );

    let rate = |(ms, ops): (u64, u64)| {
        (
            ms,
            if ms == 0 {
                0
            } else {
                ops_per_sec_x100(ops, u128::from(ms) * 1_000)
            },
        )
    };
    let win_pairs: Vec<(u64, u64)> = windows.iter().copied().map(rate).collect();

    (ops, work_us, win_pairs)
}
#[derive(Debug)]
struct Args {
    sweep: Vec<usize>,
    steps: usize,
    workload: String,
    working_set: usize,
    mstress_blocks: usize,
    window_ms: u64,
    seed: u64,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            sweep: vec![1, 2, 4],
            steps: 400_000,
            workload: "larson".to_string(),
            working_set: 768,
            mstress_blocks: 512,
            window_ms: 250,
            seed: 0x9E37_79B9_7F4A_7C15,
        }
    }
}

const USAGE: &str = "usage: b4_collapse_probe [--threads N] [--steps N] [--sweep 1,2,4] [--workload larson|mstress] [--working-set N] [--mstress-blocks N] [--window-ms N] [--seed N]";

/// Parse a numeric option value from `value`.
fn num<T: std::str::FromStr>(value: &Option<String>) -> Option<T> {
    value.as_ref().and_then(|s| s.trim().parse::<T>().ok())
}

fn parse_seed(s: &str) -> Option<u64> {
    s.strip_prefix("0x")
        .or_else(|| s.strip_prefix("0X"))
        .and_then(|hex| u64::from_str_radix(hex, 16).ok())
        .or_else(|| s.parse::<u64>().ok())
}

/// Bad args print usage, exit 2.
fn parse_args() -> Args {
    let mut args = Args::default();
    let argv: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < argv.len() {
        let value = argv.get(i + 1).cloned();
        let ok = match argv[i].as_str() {
            // --threads N == a one-entry sweep.
            "--threads" => num::<usize>(&value).map(|n| args.sweep = vec![n]).is_some(),
            "--steps" => num::<usize>(&value).map(|n| args.steps = n).is_some(),
            "--sweep" => value
                .and_then(|v| {
                    let list: Option<Vec<usize>> = v
                        .split(',')
                        .map(|p| p.trim().parse::<usize>().ok())
                        .collect();
                    list.filter(|l| !l.is_empty() && l.iter().all(|&n| n > 0))
                })
                .map(|l| args.sweep = l)
                .is_some(),
            "--workload" => value
                .filter(|v| v == "larson" || v == "mstress")
                .map(|v| args.workload = v)
                .is_some(),
            "--working-set" => num::<usize>(&value).map(|n| args.working_set = n).is_some(),
            "--mstress-blocks" => num::<usize>(&value)
                .map(|n| args.mstress_blocks = n)
                .is_some(),
            "--window-ms" => num::<u64>(&value).map(|n| args.window_ms = n).is_some(),
            "--seed" => value
                .and_then(|v| parse_seed(&v))
                .map(|n| args.seed = n)
                .is_some(),
            _ => false,
        };
        if !ok {
            eprintln!(
                "unknown or malformed argument: {}
{USAGE}",
                argv[i]
            );
            std::process::exit(2);
        }
        i += 2;
    }
    args
}
fn main() {
    let args = parse_args();
    let cfg = BenchCfg {
        steps: args.steps,
        ws: args.working_set,
        mb: args.mstress_blocks,
        window_ms: args.window_ms,
        seed: args.seed,
    };
    println!(
        "== b4_collapse_probe args={args:?} seed={:#x} ==",
        args.seed
    );

    let alloc = SeferAlloc::new();
    let mut pass_json: Vec<String> = Vec::new();

    for &t in &args.sweep {
        let pass_start = counters_snapshot(&alloc);
        let (ops, work_us, windows_pairs) = run_pass(&args.workload, &alloc, t, &cfg);
        let deltas = counters_delta(&pass_start, &counters_snapshot(&alloc));
        for (key, delta) in &deltas {
            println!("CNT t={} {key}=+{delta}", t);
        }
        let windows: Vec<String> = windows_pairs
            .iter()
            .map(|&(ms, x100)| format!("[{ms},{x100}]"))
            .collect();
        let counters: Vec<String> = deltas
            .iter()
            .map(|&(k, d)| format!("\"{k}\":{d}"))
            .collect();
        pass_json.push(format!(
            "{{\"threads\":{},\"ops\":{},\"secs\":{},\"ops_per_sec\":{},\
             \"windows\":[{}],\"counters_delta\":{{{}}}}}",
            t,
            ops,
            (work_us / 1_000) as u64,
            ops_per_sec_x100(ops, work_us),
            windows.join(","),
            counters.join(",")
        ));
    }

    let counters_end: Vec<String> = counters_snapshot(&alloc)
        .iter()
        .map(|&(key, value)| format!("\"{key}\":{value}"))
        .collect();

    println!(
        "RESULT_JSON {{\"workload\":\"{}\",\"seed\":{},\"passes\":[{}],\
         \"counters_end\":{{{}}}}}",
        args.workload,
        args.seed,
        pass_json.join(","),
        counters_end.join(",")
    );
}
