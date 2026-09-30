//! Criterion full-round fan-in through real terminal descriptors.
//! The owner runs concurrently with remote producers (active) or waits for
//! publication to finish (starved). Each round includes thread setup/teardown;
//! use heap_fanin_persistent for isolated per-free latency observations.
//! Retired ring/retry/spill pressure metrics are not manufactured or reported.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]
#![allow(clippy::cast_possible_truncation, clippy::needless_pass_by_value)]

use std::alloc::Layout;
use std::thread;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};

use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

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
    // SAFETY: the owner and producers have joined; this thread retains the
    // unique claim and completes any remaining descriptor obligations.
    unsafe {
        (*heap).dbg_drain_sidecar_ingress();
    }

    unsafe { HeapRegistry::recycle(heap) };
}

/// **`starved`** owner-state iteration: the owner allocates `N` blocks, then
/// `producers` remote threads free ALL of them concurrently while the owner
/// does ABSOLUTELY NOTHING (joined on the producer threads — no interleaved
/// alloc, no interleaved drain) for the entire burst.
/// After every producer joins, one real owner-side descriptor sweep retires
/// exactly the burst's publications before the slot is recycled.
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

    // SAFETY: every producer has joined and this thread retains the unique
    // owner claim. The bounded sweep measures actual logical retirement.
    let reclaimed = unsafe { (*heap).dbg_drain_sidecar_ingress() };
    assert_eq!(reclaimed, N);

    unsafe { HeapRegistry::recycle(heap) };
}

fn bench_fanin_active(c: &mut Criterion) {
    let _ = bootstrap::ensure();

    let mut group = c.benchmark_group("heap_fanin_production_active");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_millis(1500));

    for &producers in PRODUCER_COUNTS {
        group.bench_function(format!("producers={producers}"), |b| {
            b.iter(|| run_active(producers));
        });
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
        group.bench_function(format!("producers={producers}"), |b| {
            b.iter(|| run_starved(producers));
        });
    }

    group.finish();
}

criterion_group!(benches, bench_fanin_active, bench_fanin_starved);
criterion_main!(benches);
