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
use std::sync::mpsc;
use std::time::Duration;
use std::{mem, thread};

use criterion::{criterion_group, criterion_main, Criterion};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

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

    // Ph4c: migrated to the safe `dbg_claim_lease` API, exactly like
    // `heap_fanin_persistent.rs::run_cell`'s owner flow. `HeapLease` is
    // `!Send`, so the lease authority lives where the ownership lives: the
    // spawned owner thread claims its own lease (it IS the slot's owner),
    // pre-allocates the `N` blocks the producers will free (this setup was
    // previously on this coordinator thread BEFORE the producers spawned —
    // the mpsc rendezvous below preserves that ordering: the producer
    // threads are only created after the address list arrives, so none of
    // the setup work races or overlaps the producers; the criterion-timed
    // `b.iter` window still spans the whole call either way), and hands the
    // address list plus its slot index back over the channel. The owner
    // thread drops its lease at thread end (LIVE → FREE); the coordinator
    // re-claims the slot afterwards for the residual sidecar-ingress drain
    // — the LIFO reuse hint set by the Drop returns the same slot, asserted
    // via `slot_index()` — and the drained slot is `mem::forget`-ed to keep
    // the "never recycle within this process" discipline intact.
    let (owner_addr_tx, owner_addr_rx) = mpsc::channel::<(u32, Vec<usize>)>();
    let owner_rounds: thread::JoinHandle<()> = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let mut lease = HeapRegistry::dbg_claim_lease().expect("owner HeapRegistry::claim failed");
        let this_slot_index = lease.slot_index();

        // Pre-allocate the whole burst BEFORE any producer exists — same
        // work the coordinator did pre-Ph4c, just moved into the thread
        // that actually owns the slot.
        let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
        {
            let heap = lease.core();
            for _ in 0..N {
                let p = heap.alloc(layout);
                assert!(!p.is_null(), "owner pre-alloc returned null");
                ptrs.push(p);
            }
        }
        let addrs: Vec<usize> = ptrs.iter().map(|&p| p as usize).collect();
        owner_addr_tx
            .send((this_slot_index, addrs))
            .expect("coordinator must be receiving the owner address list");

        // The owner concurrently allocates a growing batch WITHOUT
        // self-freeing, forcing every alloc() to fall through to
        // find_segment_with_free — the call that actually drains every
        // owned segment's RemoteFreeRing — exactly as
        // tests/remote_fanin.rs's harness 1 does.
        let heap = lease.core();
        let mut batch: Vec<*mut u8> = Vec::with_capacity(N);
        for _ in 0..N {
            let p = heap.alloc(layout);
            if p.is_null() {
                continue; // Transient OOM under pressure — not the property under test.
            }
            batch.push(p);
        }
        for p in batch {
            // SAFETY: `p` was returned by `heap.alloc(layout)` above with
            // the same layout, is still live, freed once, own-thread.
            unsafe { heap.dealloc(p, layout) };
        }
        // Ph4c: recycle via lease Drop (LIVE → FREE Release) — the
        // coordinator re-claims this slot below (LIFO hint) for the
        // residual ingress drain, then forgets the lease.
        drop(lease);
    });

    let (owner_slot_index, addrs) = owner_addr_rx
        .recv()
        .expect("owner thread must send its pre-allocated address list");

    let chunk = N.div_ceil(producers);
    let mut handles = Vec::with_capacity(producers);
    for slice in addrs.chunks(chunk) {
        let slice = slice.to_vec();
        handles.push(thread::spawn(move || {
            let _ = bootstrap::ensure();
            // Ph4c: safe lease API — the lease is claimed, used and recycled
            // (via `Drop`, LIVE → FREE, exactly like the old explicit
            // `recycle`) entirely inside this producer thread.
            let mut lease =
                HeapRegistry::dbg_claim_lease().expect("remote HeapRegistry::claim failed");
            let remote_heap = lease.core();
            for addr in slice {
                let p = addr as *mut u8;
                // SAFETY: `p` was returned by the owner's `alloc(layout)`
                // with the same layout, is still live (never freed before),
                // and is freed exactly once here -- the deliberate
                // cross-thread free path this bench measures.
                unsafe { remote_heap.dealloc(p, layout) };
            }
            drop(lease);
        }));
    }

    for h in handles {
        h.join().expect("producer thread must not panic");
    }
    owner_rounds.join().expect("owner thread must not panic");
    // Ph4c: the owner thread dropped its lease on exit (LIVE → FREE), so the
    // legacy raw dereference + explicit `recycle` is replaced by a fresh
    // safe re-claim. This sweep completes any remaining descriptor
    // obligations. The lease's Drop published the LIFO reuse hint for its
    // own slot, so the re-claim deterministically returns the SAME slot
    // (asserted) — every other thread has joined, so no one else can have
    // claimed it. After the drain the lease is FORGOTTEN, not dropped: the
    // slot stays LIVE-for-the-process, exactly like the legacy leaked raw
    // pointer (the "never recycle within this process" discipline).
    let mut lease = HeapRegistry::dbg_claim_lease().expect("post-burst owner re-claim failed");
    assert_eq!(
        lease.slot_index(),
        owner_slot_index,
        "LIFO reuse hint must return the owner's slot for the untimed drain"
    );
    let _drained = lease.core().dbg_drain_sidecar_ingress();
    mem::forget(lease);
}

/// **`starved`** owner-state iteration: the owner allocates `N` blocks, then
/// `producers` remote threads free ALL of them concurrently while the owner
/// does ABSOLUTELY NOTHING (joined on the producer threads — no interleaved
/// alloc, no interleaved drain) for the entire burst.
/// After every producer joins, one real owner-side descriptor sweep retires
/// exactly the burst's publications before the slot is recycled.
fn run_starved(producers: usize) {
    let layout = Layout::from_size_align(BLOCK_SIZE, 8).unwrap();

    // Ph4c: safe lease API — owner heap claimed, used and recycled on THIS
    // thread (`dbg_drain_sidecar_ingress` + lease `Drop` below replace the old
    // explicit `recycle`).
    let mut lease = HeapRegistry::dbg_claim_lease().expect("HeapRegistry::claim returned null");
    let heap = lease.core();

    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        let p = heap.alloc(layout);
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
            // Ph4c: safe lease API — the lease is claimed, used and recycled
            // (via `Drop`, LIVE → FREE, exactly like the old explicit
            // `recycle`) entirely inside this producer thread.
            let mut lease =
                HeapRegistry::dbg_claim_lease().expect("remote HeapRegistry::claim failed");
            let remote_heap = lease.core();
            for addr in slice {
                let p = addr as *mut u8;
                // SAFETY: `p` was returned by the owner's `alloc(layout)`
                // with the same layout, is still live (never freed before),
                // and is freed exactly once here -- the deliberate
                // cross-thread free path this bench measures.
                unsafe { remote_heap.dealloc(p, layout) };
            }
            drop(lease);
        }));
    }

    // The owner does NOTHING here — no alloc, no drain — for the entire
    // producer burst. This is the deliberately pathological shape.
    for h in handles {
        h.join().expect("producer thread must not panic");
    }

    // SAFETY: every producer has joined and this thread retains the unique
    // owner claim. The bounded sweep measures actual logical retirement.
    let reclaimed = heap.dbg_drain_sidecar_ingress();
    assert_eq!(reclaimed, N);

    drop(lease); // recycle: lease `Drop` = LIVE → FREE (old explicit `recycle`)
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
