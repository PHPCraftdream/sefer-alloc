//! `heap_lifecycle_teardown` — Criterion bench isolating round4's N1 change's
//! teardown cost (task R5-R4, follow-up to the round5 performance review,
//! `docs/agent_reviews_round5/performance_review.md` §3 N1, §8.2 item 1, §10
//! Stage A.3/C).
//!
//! ## Why this bench exists
//!
//! Round4's N1 (task #95) wired `AbandonGuard::drop`
//! (`src/global/tls_heap.rs`) to call `HeapCore::trim_for_recycle`
//! (`src/registry/heap_core.rs`) on thread exit/recycle: flush the entire
//! tcache, drain the small-segment hysteresis pool, and evict the whole large
//! cache — before the slot is handed back to the registry for reuse. The
//! canonical churn table (`benches/global_alloc.rs`) never tears a thread
//! down, so this cost has never shown up there. The round5 review flagged
//! that no existing bench separately measures:
//!
//!   (a) pure per-op allocation throughput (no teardown at all),
//!   (b) the cost of a final drain in isolation, as a function of how much
//!       state accumulated before it ran, and
//!   (c) the FULL `trim_for_recycle` cost as it actually happens in
//!       production — via real thread exit, `AbandonGuard::drop`, INCLUDING
//!       the OS thread-spawn/join overhead that comes bundled with it in any
//!       real "spin up a worker, do some work, let it die" workload.
//!
//! These three were previously blended together in ad-hoc, inconsistent ways
//! across `examples/malloc_macro.rs` / `crates/malloc-bench/src/lib.rs` (per
//! the review, those run teardown before/around the stop-timer boundary
//! inconsistently). This bench reports all three as **separate,
//! non-summed** `bench_function`s / groups, so each can be read off
//! independently.
//!
//! ## The three measurement points (one group each)
//!
//! 1. **`heap_lifecycle_ops_only`** — pure alloc/dealloc/churn throughput.
//!    `SeferAlloc::dbg_trim_current_thread` is NEVER called in this group's
//!    timed region (nor at all, for this group) — the timer stops the moment
//!    the N allocate+free operations are done. This is the control/baseline:
//!    "what does an op cost with no teardown in the picture at all".
//!
//! 2. **`heap_lifecycle_drain_only`** — the cost of
//!    [`SeferAlloc::dbg_trim_current_thread`] alone, called ONCE after N
//!    operations have already built up tcache/pool/cache state, timed in a
//!    SEPARATE `bench_function` from the N operations that produced that
//!    state (the N operations run in `iter_batched`'s untimed setup closure;
//!    only the trim call itself is inside the timed routine). This isolates
//!    `trim_for_recycle`'s own cost as a function of accumulated state,
//!    without folding it into the ops number. `dbg_trim_current_thread`
//!    (`src/global/sefer_alloc.rs`, `#[doc(hidden)]`, `alloc-global`-gated)
//!    calls the exact production `HeapCore::trim_for_recycle` primitive the
//!    real `AbandonGuard::drop` teardown path runs, but WITHOUT tearing down
//!    TLS or recycling the registry slot — the calling (bench) thread keeps
//!    its own (now-emptied) heap afterward, so this group can run entirely
//!    on criterion's single bench thread with no thread spawn involved.
//!
//! 3. **`heap_lifecycle_thread_exit`** — the FULL production lifecycle: a
//!    real `std::thread::spawn` does N allocations then returns normally,
//!    letting the REAL `AbandonGuard::drop` run via native TLS teardown (not
//!    the `dbg_trim_current_thread` shortcut) as the thread unwinds, and the
//!    whole `spawn(...).join()` round trip is timed. A companion
//!    `heap_lifecycle_thread_spawn_empty` group spawns and joins a thread
//!    that does ZERO allocations (so its `AbandonGuard` never bound a heap
//!    and its `Drop` is a no-op — see `AbandonGuard::drop`'s null-check early
//!    return), giving a reference point for "OS thread spawn/join overhead
//!    alone" to subtract from group 3's numbers by eye. **This bench does
//!    not do the subtraction itself** (criterion doesn't support cross-group
//!    arithmetic) — read `heap_lifecycle_thread_exit`'s time minus
//!    `heap_lifecycle_thread_spawn_empty`'s time (at the SAME N) as an
//!    estimate of "the trim/teardown work specifically", keeping in mind
//!    that estimate is noisy (OS scheduler jitter dominates at low N).
//!
//! ## N (operations per thread lifetime) sweep
//!
//! Swept across the three points the round5 review names: **1, 1_000,
//! 100_000** operations before the lifetime ends. Every block is
//! `BLOCK_SIZE = 64` bytes (matches `benches/heap_xthread.rs` /
//! `benches/heap_fanin_production.rs`), well under `SMALL_MAX`, so every
//! block routes through the small/tcache path — the path `trim_for_recycle`'s
//! `flush_all_tcache` call actually drains.
//!
//! ## Bench profile — scaled down from the review's ideal, like
//! `heap_fanin_production` was
//!
//! Fast-bench-profile discipline (CLAUDE.md: short warm-up/measurement,
//! `sample_size(10)`, whole suite in a few seconds to a couple of minutes).
//! `heap_lifecycle_thread_exit` at N = 100_000 is the one point in this
//! matrix that risked blowing the budget (100,000 allocations on a freshly
//! spawned thread, `sample_size(10)` times, PLUS OS spawn/join overhead per
//! sample): reduced to `sample_size(10)` with a 2s measurement window
//! (up from the 500ms/1.5s `heap_fanin_production` uses for its
//! smaller-N groups) — matching that bench's own precedent of scaling N down
//! (2,000 → 400) rather than letting a single point blow the whole matrix's
//! budget. Measured total run time for the full 3-group x 3-N matrix here is
//! well under a minute (see the task's final summary for real numbers).
//!
//! ## Process-global state
//!
//! `HeapRegistry` / TLS state are process-global. `heap_lifecycle_ops_only`
//! and `heap_lifecycle_drain_only` reuse ONE `SeferAlloc` on criterion's
//! single bench thread across all N (and across the whole file, since
//! `criterion_main!` runs every registered group sequentially on one
//! thread by default) — each group calls `dbg_trim_current_thread` before it
//! starts sweeping N, to reset tcache/pool/cache state left over from a
//! PRIOR group, mirroring `heap_fanin_production`'s / `global_alloc.rs`'s
//! established pattern for isolating cross-group TLS-heap state (R5-R3).
//! `heap_lifecycle_thread_exit` and `heap_lifecycle_thread_spawn_empty` each
//! spawn brand-new OS threads per iteration by construction, so they do not
//! share this concern — every iteration gets a fresh registry slot claim (or
//! a recycled one, exercising the real recycle path).

#![cfg(feature = "alloc-global")]
#![allow(clippy::cast_possible_truncation, clippy::needless_pass_by_value)]

use std::alloc::{GlobalAlloc, Layout};
use std::hint::black_box;
use std::thread;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};

use sefer_alloc::SeferAlloc;

/// Block size for every allocation in this bench. 64 B is well under
/// `SMALL_MAX`, so every block routes through the tcache/small-segment path
/// that `trim_for_recycle` (`flush_all_tcache` + `drain_small_pool`) drains —
/// matching `heap_xthread`'s / `heap_fanin_production`'s `BLOCK_SIZE`.
const BLOCK_SIZE: usize = 64;

/// The three "thread lifetime" shapes named by the round5 performance
/// review (§3 N1 / §8.2 item 1): a thread that does essentially nothing, one
/// that does a modest amount of work, and one that does a lot of work,
/// before its lifetime ends and teardown happens.
const N_VALUES: &[usize] = &[1, 1_000, 100_000];

fn layout() -> Layout {
    Layout::from_size_align(BLOCK_SIZE, 8).unwrap()
}

/// Timed routine shared by `heap_lifecycle_ops_only` and the setup phase of
/// `heap_lifecycle_drain_only`: allocate `n` blocks, then free every one of
/// them (own-thread free, off the ring path), leaving the tcache/pool
/// populated by the free half but no live blocks outstanding — the same
/// "did some work, now idle" shape a real short-lived worker thread has right
/// before it exits.
fn run_ops(alloc: &SeferAlloc, n: usize) {
    let layout = layout();
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(n);
    for _ in 0..n {
        // SAFETY: `layout` has non-zero size and valid alignment.
        let p = unsafe { alloc.alloc(layout) };
        assert!(!p.is_null(), "pre-alloc OOM at n={n}");
        ptrs.push(p);
    }
    for p in ptrs {
        // SAFETY: `p` was returned by `alloc.alloc(layout)` above, still
        // live (never freed until now).
        unsafe { alloc.dealloc(p, layout) };
    }
    black_box(alloc);
}

/// **Group 1 — operations-only throughput.** No trim, no teardown, anywhere
/// in the timed region: this is the pure per-op cost, the control/baseline
/// the other two groups are compared against.
fn bench_ops_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap_lifecycle_ops_only");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_millis(800));

    let alloc = SeferAlloc::new();
    // Reset any tcache/pool/cache state a prior group in this same process
    // left behind (R5-R3's cross-group TLS-heap-state isolation pattern).
    alloc.dbg_trim_current_thread();

    for &n in N_VALUES {
        group.bench_function(format!("n={n}"), |b| {
            b.iter(|| run_ops(&alloc, n));
        });
        // Reset between N points too, so a larger N's leftover tcache/pool
        // state cannot inflate the NEXT (possibly smaller) N's numbers.
        alloc.dbg_trim_current_thread();
    }

    group.finish();
}

/// **Group 2 — explicit final drain cost, isolated from the operations that
/// produced the state it drains.** `iter_batched`'s untimed setup closure
/// runs `run_ops` (N allocate+free operations, populating tcache/pool/cache
/// state); the TIMED routine is `dbg_trim_current_thread()` alone. This is
/// the direct measurement of `trim_for_recycle`'s own cost as a function of
/// how much state N operations built up.
fn bench_drain_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap_lifecycle_drain_only");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_millis(800));

    let alloc = SeferAlloc::new();
    alloc.dbg_trim_current_thread();

    for &n in N_VALUES {
        group.bench_function(format!("n={n}"), |b| {
            b.iter_batched(
                || {
                    // Untimed: build up tcache/pool/cache state via N
                    // allocate+free operations.
                    run_ops(&alloc, n);
                },
                |()| {
                    // Timed: ONLY the drain/trim call.
                    alloc.dbg_trim_current_thread();
                },
                BatchSize::SmallInput,
            );
        });
    }

    group.finish();
}

/// Allocate `n` blocks and free them all on a freshly spawned thread, then
/// let the thread return normally — this is what lets the REAL
/// `AbandonGuard::drop` run via native TLS teardown when the thread's stack
/// unwinds after `join`, exercising the exact production path (not the
/// `dbg_trim_current_thread` shortcut group 2 uses).
fn thread_exit_with_ops(n: usize) {
    let handle = thread::spawn(move || {
        let alloc = SeferAlloc::new();
        let layout = layout();
        let mut ptrs: Vec<*mut u8> = Vec::with_capacity(n);
        for _ in 0..n {
            // SAFETY: `layout` has non-zero size and valid alignment.
            let p = unsafe { alloc.alloc(layout) };
            assert!(!p.is_null(), "pre-alloc OOM at n={n}");
            ptrs.push(p);
        }
        for p in ptrs {
            // SAFETY: `p` was returned by `alloc.alloc(layout)` above.
            unsafe { alloc.dealloc(p, layout) };
        }
        // Falling off the end here drops this thread's `AbandonGuard` during
        // native TLS teardown — the REAL production trim/recycle path runs
        // here, not via `dbg_trim_current_thread`.
    });
    handle.join().expect("worker thread must not panic");
}

/// **Group 3 — full thread-exit lifecycle cost.** Times the whole
/// `thread::spawn(...).join()` round trip for a thread that does N
/// allocate+free operations then exits normally. Compare against
/// `heap_lifecycle_thread_spawn_empty` (same N sweep structure, but the
/// spawned thread does ZERO allocations) to separate "OS thread spawn/join
/// overhead" from "the trim/teardown work specifically" — see this file's
/// module doc for why that subtraction is done by eye, not in-bench.
fn bench_thread_exit(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap_lifecycle_thread_exit");
    group.sample_size(10);
    // N=100_000 spawns a thread that does 100k allocations THEN tears down a
    // populated heap for real, sample_size(10) times — the one point in this
    // matrix that risks the fast-bench-profile budget the way
    // `heap_fanin_production`'s `starved` N=2_000 case once did. A wider
    // measurement window (2s, vs 800ms for the smaller groups above) keeps
    // criterion from complaining about insufficient samples at this N without
    // materially lengthening the smaller-N points (they finish long before
    // the window elapses; criterion stops early once `sample_size` iterations
    // complete).
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(2));

    for &n in N_VALUES {
        group.bench_function(format!("n={n}"), |b| {
            b.iter(|| thread_exit_with_ops(n));
        });
    }

    group.finish();
}

/// Reference group: spawn and join a thread that allocates NOTHING. Its
/// `AbandonGuard` never binds a heap (`heap.get()` stays null), so its
/// `Drop` hits the early-return no-op branch — this isolates pure OS
/// thread-spawn/join overhead, with no allocator teardown work in the
/// picture at all. Not swept over `N_VALUES` (there is no "N operations" for
/// an empty thread) — a single `bench_function`, used as the subtrahend
/// reference for every N point in `heap_lifecycle_thread_exit` by eye.
fn bench_thread_spawn_empty(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap_lifecycle_thread_spawn_empty");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_secs(1));

    group.bench_function("empty", |b| {
        b.iter(|| {
            let handle = thread::spawn(|| {});
            handle.join().expect("empty worker thread must not panic");
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_ops_only,
    bench_drain_only,
    bench_thread_exit,
    bench_thread_spawn_empty
);
criterion_main!(benches);
