//! `heap_fanin_production` — Criterion bench for the REAL production
//! cross-thread free path (task R5-R1, follow-up to the round5 performance
//! review, `docs/agent_reviews_round5/performance_review.md` §4.3 / §10
//! Stage A.2).
//!
//! ## Why this bench exists
//!
//! `benches/heap_xthread.rs` measures the `RemoteFreeRing` push→drain cycle
//! DIRECTLY via `AllocCore::dbg_push_to_ring` / `dbg_drain_all_rings` — a
//! `#[doc(hidden)]` test-only seam. That bypasses the production
//! cross-thread free path entirely: no real producer threads, no real
//! contention, no `HeapCore::dealloc_foreign_slow` /
//! `push_with_overflow_retry` call, no owner-identity check
//! (`owner_thread_free`). The round5 review flagged that round4's R2
//! calibration (`RING_PUSH_RETRY_SPINS` cut 32× from 262,144 to 8,192 =
//! 32 * `RING_CAP`) has never been measured, as a WALL-CLOCK number, against
//! a realistic multi-thread producer workload — only the correctness
//! counterfactual (`tests/remote_fanin.rs`,
//! `remote_fanin_high_contention_budget_is_sufficient`) exists, and that is
//! a `cargo test` pass/fail judge, not a timing measurement.
//!
//! This bench closes that gap: it drives the SAME production path
//! `tests/remote_fanin.rs` exercises —
//! `HeapRegistry::claim` → `HeapCore::alloc` / `HeapCore::dealloc` →
//! (cross-thread) `dealloc_foreign_slow` → `push_with_overflow_retry` →
//! `HeapRegistry::recycle` — with REAL `std::thread::spawn` producer
//! threads, each freeing blocks it did not allocate (a genuine cross-thread
//! free, exactly how this ring is used in production), and reports
//! wall-clock ns/op via Criterion PLUS the same diagnostic counters
//! (`DBG_RING_OVERFLOW`, `DBG_RING_PUSH_RETRIED`,
//! `DBG_RING_PUSH_RETRY_EXHAUSTED`) `tests/remote_fanin.rs` uses as its
//! correctness oracle, reported here as `eprintln!` diagnostics (matching
//! `benches/global_alloc.rs`'s `working_set_cycle` bench, which reports
//! `AllocStats` deltas alongside the timing).
//!
//! ## Harness shape
//!
//! Two owner-behavior variants (mirroring `tests/remote_fanin.rs`'s two
//! native harnesses), each swept across a producer-count matrix
//! (1/2/4/8/16/32 concurrent producer threads):
//!
//!   - **`active`** — the owner keeps allocating (and therefore, per
//!     `find_segment_with_free`'s lazy per-segment ring drain, keeps
//!     draining) CONCURRENTLY with the producers' frees, matching
//!     `remote_fanin_concurrent_overflow_is_recovered` /
//!     `remote_fanin_high_contention_budget_is_sufficient`'s realistic
//!     shape — sustained pressure with a live, cycling consumer.
//!   - **`starved`** — the owner does ABSOLUTELY NOTHING while the
//!     producers free every block, then performs a single reclaim pass
//!     once every producer thread has joined, matching
//!     `remote_fanin_owner_starved_residual_is_bounded`'s pathological
//!     shape — this is the shape that exercises
//!     `push_with_overflow_retry`'s retry loop hardest (nothing drains the
//!     ring until the owner wakes up) and therefore the shape most
//!     sensitive to the `RING_PUSH_RETRY_SPINS` calibration.
//!
//! A third "paused then resumes mid-burst" variant was considered (the task
//! brief allowed scoping down from an "active / paused / exiting" ideal to
//! this "active / starved" minimum) but was left out: it does not add a
//! distinct THIRD point on the retry-pressure axis — it interpolates
//! between `active` (owner always draining) and `starved` (owner never
//! draining until the end) without exercising any additional branch of
//! `push_with_overflow_retry` or `HeapOverflow` that those two do not
//! already cover between them. `active` and `starved` are the two
//! endpoints of "how long does the ring go undrained", which is the axis
//! that actually stresses the retry/overflow mechanism; a bench matrix that
//! already sweeps 1..32 producers on both endpoints gives a much bigger
//! actionable signal per minute of bench time than adding a third
//! intermediate owner state would.
//!
//! ## Bench profile
//!
//! Short profile per this project's "fast bench profile" discipline
//! (CLAUDE.md): `sample_size(10)`, short warm-up/measurement (500ms warm-up
//! / 1.5s measurement per `bench_function`, slightly more generous than
//! `heap_xthread`'s because this contention shape is noisier — OS scheduler
//! jitter across up to 33 threads per sample). `N` (blocks per iteration) is
//! deliberately small (400 — see its own doc comment) specifically to keep
//! the full 2-owner-state x 6-producer-count matrix inside a couple of
//! minutes: an earlier `N = 2_000` version measured ~4 minutes for the full
//! matrix (`starved` samples alone ran up to ~2.5s each, well past this
//! group's 1.5s measurement-time target, because of genuine
//! `RING_PUSH_RETRY_SPINS` spin-retry CPU cost at that overflow volume — see
//! `N`'s doc comment).
//!
//! Every block is `BLOCK_SIZE = 64` bytes (matching `tests/remote_fanin.rs`'s
//! `BLOCK_SIZE`), well under `SMALL_MAX`, so every block routes through the
//! ring path (never Large/A1).
//!
//! ## Process-global state
//!
//! `HeapRegistry` and the `DBG_RING_*` counters are process-global statics.
//! Unlike `cargo test`'s default multi-threaded runner (which is why
//! `tests/remote_fanin.rs` needs its own `SerialGuard`), a single `cargo
//! bench` binary built with `harness = false` + `criterion_main!` runs its
//! registered `bench_function`s sequentially on one thread by default — no
//! serialization guard is needed here (verified: this file's own two
//! `bench_function` groups never overlap in the run log).

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
#![allow(clippy::cast_possible_truncation, clippy::needless_pass_by_value)]

use std::alloc::Layout;
use std::hint::black_box;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};

use sefer_alloc::alloc_core::remote_free_ring::DBG_RING_OVERFLOW;
use sefer_alloc::registry::{
    bootstrap, HeapCore, HeapRegistry, DBG_RING_PUSH_RETRIED, DBG_RING_PUSH_RETRY_EXHAUSTED,
};

/// A small-class size well under `SMALL_MAX`, so every block is routed
/// through the ring (never the Large/A1 path). Matches
/// `tests/remote_fanin.rs`'s `BLOCK_SIZE`.
const BLOCK_SIZE: usize = 64;

/// Producer-thread counts swept for both owner-state variants.
const PRODUCER_COUNTS: &[usize] = &[1, 2, 4, 8, 16, 32];

/// Blocks allocated (and then cross-thread-freed) per bench iteration.
/// Large enough to force ring overflow (`RING_CAP = 256` per segment) so the
/// retry/overflow path is genuinely exercised, small enough to keep the
/// whole producer-count matrix inside this project's fast-bench-profile
/// budget. An early version of this bench used `N = 2_000`, which (measured)
/// pushed each `starved` sample to ~2s — `RING_PUSH_RETRY_SPINS = 8,192`
/// means every block that overflows the ring burns up to 8,192 spin+CAS
/// attempts before either landing or falling through to `HeapOverflow`, so
/// thousands of overflowing blocks under `starved` (owner never drains
/// during the burst) is genuinely CPU-bound retry work, not bench overhead —
/// but at criterion's `sample_size(10)` that blew the "couple of minutes for
/// the whole matrix" budget several times over (~4 minutes measured for the
/// full 2-owner-state x 6-producer-count matrix). `N = 400` still reliably
/// forces overflow (> `RING_CAP`, confirmed via the `overflow_delta`
/// diagnostic) while keeping every `bench_function` inside its allotted
/// warm-up/measurement window.
const N: usize = 400;

/// Diagnostic counter snapshot, used to report per-`bench_function` deltas
/// (matching `benches/global_alloc.rs::bench_working_set_cycle`'s
/// `AllocStats`-delta reporting style).
#[derive(Clone, Copy)]
struct RingCounters {
    overflow: u64,
    retried: u64,
    exhausted: u64,
}

fn snapshot_counters() -> RingCounters {
    RingCounters {
        overflow: DBG_RING_OVERFLOW.load(Ordering::Relaxed),
        retried: DBG_RING_PUSH_RETRIED.load(Ordering::Relaxed),
        exhausted: DBG_RING_PUSH_RETRY_EXHAUSTED.load(Ordering::Relaxed),
    }
}

fn report_delta(label: &str, before: RingCounters, after: RingCounters) {
    eprintln!(
        "{label}: overflow_delta={} retried_delta={} exhausted_delta={}",
        after.overflow.saturating_sub(before.overflow),
        after.retried.saturating_sub(before.retried),
        after.exhausted.saturating_sub(before.exhausted),
    );
}

/// **`active`** owner-state iteration: the owner allocates `N` blocks, hands
/// them to `producers` remote threads to free, and — WHILE those producers
/// race — keeps allocating a growing batch of its OWN blocks (never
/// self-freeing mid-batch, so `alloc_small`'s own free-list fast path never
/// short-circuits before reaching `find_segment_with_free`'s ring drain —
/// see `tests/remote_fanin.rs`'s harness-1 doc comment, lines 217-230, for
/// the full explanation of why this shape is necessary to force genuine
/// draining). Frees its own batch in one shot at the end (own-thread free,
/// off the ring path) once every producer has joined.
fn run_active(producers: usize) {
    let layout = Layout::from_size_align(BLOCK_SIZE, 8).unwrap();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let heap_addr = heap as usize;

    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "owner pre-alloc returned null");
        ptrs.push(p);
    }

    let addrs: Vec<usize> = ptrs.iter().map(|&p| p as usize).collect();
    let chunk = N.div_ceil(producers);
    let mut handles = Vec::with_capacity(producers);
    for slice in addrs.chunks(chunk) {
        let slice = slice.to_vec();
        handles.push(thread::spawn(move || {
            let _ = bootstrap::ensure();
            let remote_heap = HeapRegistry::claim();
            assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
            for addr in slice {
                let p = addr as *mut u8;
                unsafe { (*remote_heap).dealloc(p, layout) };
            }
            unsafe { HeapRegistry::recycle(remote_heap) };
        }));
    }

    // The owner concurrently allocates a growing batch WITHOUT self-freeing,
    // forcing every alloc() to fall through to find_segment_with_free — the
    // call that actually drains every owned segment's RemoteFreeRing —
    // exactly as tests/remote_fanin.rs's harness 1 does.
    let owner_rounds: thread::JoinHandle<()> = thread::spawn(move || {
        let heap = heap_addr as *mut HeapCore;
        let mut batch: Vec<*mut u8> = Vec::with_capacity(N);
        for _ in 0..N {
            let p = unsafe { (*heap).alloc(layout) };
            if p.is_null() {
                continue; // Transient OOM under pressure — not the property under test.
            }
            batch.push(p);
        }
        for p in batch {
            unsafe { (*heap).dealloc(p, layout) };
        }
    });

    for h in handles {
        h.join().expect("producer thread must not panic");
    }
    owner_rounds.join().expect("owner thread must not panic");

    unsafe { HeapRegistry::recycle(heap) };
}

/// **`starved`** owner-state iteration: the owner allocates `N` blocks, then
/// `producers` remote threads free ALL of them concurrently while the owner
/// does ABSOLUTELY NOTHING (joined on the producer threads — no interleaved
/// alloc, no interleaved drain) for the entire burst — matching
/// `tests/remote_fanin.rs`'s harness 2. Once every producer has joined, the
/// owner performs a single reclaim pass (`N` more allocations), which is
/// where `push_with_overflow_retry` / `HeapOverflow`'s second-chance ring
/// get drained.
fn run_starved(producers: usize) {
    let layout = Layout::from_size_align(BLOCK_SIZE, 8).unwrap();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "owner pre-alloc returned null");
        ptrs.push(p);
    }

    let addrs: Vec<usize> = ptrs.iter().map(|&p| p as usize).collect();
    let chunk = N.div_ceil(producers);
    let mut handles = Vec::with_capacity(producers);
    for slice in addrs.chunks(chunk) {
        let slice = slice.to_vec();
        handles.push(thread::spawn(move || {
            let _ = bootstrap::ensure();
            let remote_heap = HeapRegistry::claim();
            assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
            for addr in slice {
                let p = addr as *mut u8;
                unsafe { (*remote_heap).dealloc(p, layout) };
            }
            unsafe { HeapRegistry::recycle(remote_heap) };
        }));
    }

    // The owner does NOTHING here — no alloc, no drain — for the entire
    // producer burst. This is the deliberately pathological shape.
    for h in handles {
        h.join().expect("producer thread must not panic");
    }

    // Owner wakes up AFTER the whole starved burst and performs a single
    // reclaim pass, exactly as tests/remote_fanin.rs's harness 2 does.
    let mut reclaimed = 0usize;
    for _ in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        if p.is_null() {
            break;
        }
        reclaimed += 1;
        unsafe { (*heap).dealloc(p, layout) };
    }
    black_box(reclaimed);

    unsafe { HeapRegistry::recycle(heap) };
}

fn bench_fanin_active(c: &mut Criterion) {
    let _ = bootstrap::ensure();

    let mut group = c.benchmark_group("heap_fanin_production_active");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_millis(1500));

    for &producers in PRODUCER_COUNTS {
        let before = snapshot_counters();
        group.bench_function(format!("producers={producers}"), |b| {
            b.iter(|| run_active(producers));
        });
        let after = snapshot_counters();
        report_delta(
            &format!("heap_fanin_production_active/producers={producers}"),
            before,
            after,
        );
    }

    group.finish();
}

fn bench_fanin_starved(c: &mut Criterion) {
    let _ = bootstrap::ensure();

    let mut group = c.benchmark_group("heap_fanin_production_starved");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_millis(1500));

    for &producers in PRODUCER_COUNTS {
        let before = snapshot_counters();
        group.bench_function(format!("producers={producers}"), |b| {
            b.iter(|| run_starved(producers));
        });
        let after = snapshot_counters();
        report_delta(
            &format!("heap_fanin_production_starved/producers={producers}"),
            before,
            after,
        );
    }

    group.finish();
}

criterion_group!(benches, bench_fanin_active, bench_fanin_starved);
criterion_main!(benches);
