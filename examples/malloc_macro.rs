//! Multi-threaded macro-benchmark for `SeferAlloc` vs `mimalloc` vs `System`.
//!
//! # Driven by `malloc-bench-rs` (no duplicate workload)
//!
//! The larson/mstress workload lives ONCE, in the publishable `malloc-bench-rs`
//! crate (`crates/malloc-bench/src/lib.rs`, `Workload::{Larson, Mstress}` +
//! `run`/`run_with`/`sweep`/`sweep_with`). This example is a thin driver over
//! that crate: it selects the workloads, the thread sweep, and the allocators,
//! and — under the `pinning` feature — passes an `on_thread_start` hook
//! (`sweep_with`) that pins worker *i* to core *i*. It carries NO copy of the
//! workload primitives, so there is nothing to keep in sync (the previous
//! "deliberate duplication" / task #28 drift liability is retired).
//!
//! Run with:
//!   `cargo run --release --example malloc_macro --features "alloc-global alloc-xthread"`
//!
//! Unlike `benches/global_alloc.rs` (a single-threaded micro-churn of one fixed
//! layout), this harness exercises the dimensions a real allocator must serve:
//!   1. **multi-thread scaling** — a sweep over T = 1, 2, 4 worker threads,
//!   2. **cross-thread free** — a fraction of blocks are handed to another
//!      thread and freed there (under `alloc-xthread` this routes through the
//!      per-segment remote-free path),
//!   3. **mixed sizes** — small-skewed distribution (16..512 B, rare larger).
//!
//! Two workloads, both reporting **aggregate ops/sec** (an op = one alloc+free
//! pair) over a fixed operation budget measured with `Instant::elapsed`:
//!   - **larson**  — server-churn: each thread keeps a working set of live
//!     slots; each step frees a random slot and allocates a new random-size
//!     block into it. Periodically a block is handed off cross-thread.
//!   - **mstress** — rounds of "fill a vector of mixed blocks → free half in
//!     random order → refill → free all"; a fraction freed cross-thread.
//!
//! Under `--features pinning`, worker *i* is pinned to core *i* via the
//! Phase-7c `core_affinity` organ (reused through
//! `PinnedRunner::pin_current_thread_to_core`). Because a heap is bound to its
//! thread through TLS (`current_for_alloc`), pinning the thread keeps the heap's
//! segments warm in one core's cache. Best-effort: if the OS refuses the
//! affinity the worker still runs (just unpinned).

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
#![allow(clippy::cast_precision_loss)]

use std::alloc::System;

#[cfg(feature = "pinning")]
use malloc_bench_rs::sweep_with;
use malloc_bench_rs::{sweep, Config, Workload};
use sefer_alloc::SeferAlloc;

/// Build the shared `Config` for the sweep. `threads` is overwritten per
/// sweep entry by `sweep`/`sweep_with`, so its value here is a placeholder.
fn base_config(steps_per_thread: usize) -> Config {
    Config {
        threads: 1,
        steps_per_thread,
        working_set: 768,    // ~512..1024 live blocks per thread (larson)
        mstress_blocks: 512, // blocks per mstress round
    }
}

/// Run one (workload × allocator) sweep and return `Vec<(threads, ops/sec)>`.
///
/// `pinned`: when `true` (only reachable under the `pinning` feature), worker
/// *i* is pinned to core *i* via an `on_thread_start` hook. When `false` (or on
/// a non-`pinning` build), no affinity calls are made.
fn sweep_alloc<A>(
    workload: Workload,
    cfg: &Config,
    thread_sweep: &[usize],
    make_alloc: fn() -> A,
    pinned: bool,
) -> Vec<(usize, f64)>
where
    A: std::alloc::GlobalAlloc + Send + 'static,
{
    let _ = pinned;
    #[cfg(feature = "pinning")]
    if pinned {
        // Resolve the host core ids once; the hook pins worker `i` to
        // `cores[i % cores.len()]`. Best-effort: ignored if the OS refuses.
        if let Some(cores) = sefer_alloc::PinnedRunner::available_cores() {
            if !cores.is_empty() {
                let cores = std::sync::Arc::new(cores);
                return sweep_with(workload, cfg, thread_sweep, make_alloc, move |i| {
                    let core = cores[i % cores.len()];
                    let _ = sefer_alloc::PinnedRunner::pin_current_thread_to_core(core);
                });
            }
        }
        // Host refused core enumeration: fall through to the unpinned path.
    }
    sweep(workload, cfg, thread_sweep, make_alloc)
}

/// Run the full workload × T sweep for one pinning mode and print a table.
fn run_sweep(steps_per_thread: usize, thread_sweep: &[usize], pinned: bool) {
    let cfg = base_config(steps_per_thread);
    for &workload in &[Workload::Larson, Workload::Mstress] {
        let name = match workload {
            Workload::Larson => "larson",
            Workload::Mstress => "mstress",
        };
        let mode = if pinned { "pinned" } else { "unpinned" };
        println!("--- workload: {name}  (mode: {mode}) ---");
        println!(
            "{:>3}  {:>16}  {:>16}  {:>16}",
            "T", "SeferAlloc", "mimalloc", "System"
        );
        let sefer = sweep_alloc(workload, &cfg, thread_sweep, || SeferAlloc::new(), pinned);
        let mi = sweep_alloc(workload, &cfg, thread_sweep, || mimalloc::MiMalloc, pinned);
        let sys = sweep_alloc(workload, &cfg, thread_sweep, || System, pinned);
        for (((t, sefer), (_, mi)), (_, sys)) in sefer.iter().zip(mi.iter()).zip(sys.iter()) {
            println!(
                "{:>3}  {:>14.2} M  {:>14.2} M  {:>14.2} M",
                t,
                sefer / 1e6,
                mi / 1e6,
                sys / 1e6
            );
        }
        println!();
    }
}

fn main() {
    println!("== sefer-alloc MT macro-benchmark ==");
    println!("Deterministic xorshift PRNG (fixed seeds); aggregate ops/sec.");
    println!("op = one alloc+free pair. Higher is better.\n");

    // Op budget per thread tuned so the whole suite runs in a few seconds.
    // (Total across the sweep = budget × sum(threads) × workloads × allocators.)
    let steps_per_thread = 400_000usize;
    let thread_sweep = [1usize, 2, 4];

    #[cfg(feature = "pinning")]
    {
        // Phase 13.6: run TWO modes so pinned vs unpinned is directly comparable
        // in one process (same warm caches, same machine state).
        let cores = sefer_alloc::PinnedRunner::available_cores();
        match &cores {
            Some(cs) => println!(
                "[pinning] host reports {} core id(s); worker i pinned to core i (round-robin).\n",
                cs.len()
            ),
            None => println!(
                "[pinning] host refused core enumeration; pinned mode falls back to unpinned.\n"
            ),
        }
        println!("===== BASELINE (unpinned) =====\n");
        run_sweep(steps_per_thread, &thread_sweep, false);
        println!("===== PINNED (heap == core) =====\n");
        run_sweep(steps_per_thread, &thread_sweep, true);
    }

    #[cfg(not(feature = "pinning"))]
    {
        // Default build: single (unpinned) sweep, byte-for-byte the pre-13.6
        // behaviour. Build with `--features pinning` for the pinned comparison.
        run_sweep(steps_per_thread, &thread_sweep, false);
    }

    println!("(M = million ops/sec. RSS is not measured here — no portable,");
    println!(" dependency-free peak-RSS probe across Win/Linux/macOS; would");
    println!(" require platform syscalls. Reported honestly as N/A.)");
}
