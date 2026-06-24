//! Phase 2 — container-choice benches.
//!
//! Empirically chooses the single-threaded backing container for
//! [`sefer_alloc::Region`] by measuring four candidates across three axes at
//! ~10 000 live entries of a small `Copy` payload (`[u64; 4]`, wide enough
//! that iteration locality matters):
//!
//! - **iterate** — sum a field across all live entries. Dense layouts
//!   (`DenseSlotMap`, `SlotMap`) should beat the pointer-chased `Vec<Box<T>>`.
//! - **lookup** — resolve a pre-saved set of handles/keys in random order.
//!   The standard `SlotMap`'s single indirection should win over
//!   `DenseSlotMap`'s double indirection.
//! - **churn** — interleaved insert/remove throughput, vs `HashMap` and
//!   `Vec<Box<T>>`.
//!
//! The verdict is recorded in `docs/BENCHMARKS.md`. Random orderings use a
//! fixed-seed xorshift LCG (no `rand` dependency).

// Benchmarks are not shipped code. Pedantic lints that flag intentional,
// well-understood patterns here (truncation casts on a fixed N=10_000, the
// pointer-chased `Vec<Box<T>>` baseline that exists *to* measure needless
// boxing, and criterion closure formatting) are allowed at the file level
// rather than littered across every call site. The library itself stays
// fully pedantic-clean.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_lossless,
    clippy::vec_box,
    clippy::replace_box,
    clippy::semicolon_if_nothing_returned,
    clippy::needless_pass_by_value
)]

use std::collections::HashMap;
use std::hint::black_box;
use std::time::Duration;

use criterion::measurement::WallTime;
use criterion::{criterion_group, criterion_main, BenchmarkGroup, Criterion};
use sefer_alloc::Region;
use slotmap::{DefaultKey, DenseSlotMap, SlotMap};

/// Quick criterion profile: a deliberately SHORT scenario so the whole bench
/// suite finishes in a few seconds and never stalls the dev loop. Numbers are
/// rough but the relative ordering across containers stays clear. A thorough
/// long-run profile belongs to Phase 5 hardening, not the everyday loop.
fn quick(group: &mut BenchmarkGroup<'_, WallTime>) {
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(150));
    group.measurement_time(Duration::from_millis(600));
}

/// Payload: four `u64`s — 32 bytes, wide enough that cache locality matters.
type Payload = [u64; 4];

const N: usize = 10_000;

/// xorshift64 LCG with a fixed seed — deterministic, no `rand` dependency.
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        // Guard against the all-zero state, which xorshift would get stuck in.
        Self(if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed })
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Builds a random permutation of `0..n` with a fixed seed — the lookup
/// benchmarks walk handles in this order to defeat any sequential layout bias.
fn shuffled_indices(n: usize) -> Vec<usize> {
    let mut rng = Lcg::new(0xC0F_FEE);
    let mut idx: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = (rng.next_u64() as usize) % (i + 1);
        idx.swap(i, j);
    }
    idx
}

// ---------------------------------------------------------------------------
// iterate
// ---------------------------------------------------------------------------

fn bench_iterate(c: &mut Criterion) {
    let mut group = c.benchmark_group("iterate");
    quick(&mut group);

    let region = build_region(N);
    group.bench_function("Region<T> (SlotMap)", |b| {
        b.iter(|| {
            let mut sum = 0u64;
            for v in region.iter() {
                sum = sum.wrapping_add(black_box(v[0]));
            }
            black_box(sum)
        })
    });

    let slotmap = build_slotmap(N);
    group.bench_function("SlotMap", |b| {
        b.iter(|| {
            let mut sum = 0u64;
            for v in slotmap.values() {
                sum = sum.wrapping_add(black_box(v[0]));
            }
            black_box(sum)
        })
    });

    let dense = build_dense(N);
    group.bench_function("DenseSlotMap", |b| {
        b.iter(|| {
            let mut sum = 0u64;
            for v in dense.values() {
                sum = sum.wrapping_add(black_box(v[0]));
            }
            black_box(sum)
        })
    });

    let map = build_hashmap(N);
    group.bench_function("HashMap", |b| {
        b.iter(|| {
            let mut sum = 0u64;
            for v in map.values() {
                sum = sum.wrapping_add(black_box(v[0]));
            }
            black_box(sum)
        })
    });

    let boxed = build_boxed(N);
    group.bench_function("Vec<Box<T>>", |b| {
        b.iter(|| {
            let mut sum = 0u64;
            for v in &boxed {
                sum = sum.wrapping_add(black_box(v[0]));
            }
            black_box(sum)
        })
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// lookup
// ---------------------------------------------------------------------------

fn bench_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("lookup");
    quick(&mut group);
    let order = shuffled_indices(N);

    // Each builder returns (container, handles/keys) so the lookup bench can
    // re-derive the handle for logical entry `i` while the container stays
    // live. Region has no `keys()` iterator, so handles are captured at insert.
    let (region, region_handles) = build_region_with_handles(N);
    group.bench_function("Region<T> (SlotMap)", |b| {
        b.iter(|| {
            let mut sum = 0u64;
            for &i in &order {
                sum = sum.wrapping_add(black_box(region.get(region_handles[i]).unwrap()[0]));
            }
            black_box(sum)
        })
    });

    let (slotmap, slotmap_keys) = build_slotmap_with_keys(N);
    group.bench_function("SlotMap", |b| {
        b.iter(|| {
            let mut sum = 0u64;
            for &i in &order {
                sum = sum.wrapping_add(black_box(slotmap.get(slotmap_keys[i]).unwrap()[0]));
            }
            black_box(sum)
        })
    });

    let (dense, dense_keys) = build_dense_with_keys(N);
    group.bench_function("DenseSlotMap", |b| {
        b.iter(|| {
            let mut sum = 0u64;
            for &i in &order {
                sum = sum.wrapping_add(black_box(dense.get(dense_keys[i]).unwrap()[0]));
            }
            black_box(sum)
        })
    });

    let map = build_hashmap(N);
    let map_keys: Vec<u32> = (0..N as u32).collect();
    group.bench_function("HashMap", |b| {
        b.iter(|| {
            let mut sum = 0u64;
            for &i in &order {
                sum = sum.wrapping_add(black_box(map.get(&map_keys[i]).unwrap()[0]));
            }
            black_box(sum)
        })
    });

    let boxed = build_boxed(N);
    group.bench_function("Vec<Box<T>>", |b| {
        b.iter(|| {
            let mut sum = 0u64;
            for &i in &order {
                sum = sum.wrapping_add(black_box(boxed[i][0]));
            }
            black_box(sum)
        })
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// churn
// ---------------------------------------------------------------------------

fn bench_churn(c: &mut Criterion) {
    let mut group = c.benchmark_group("churn");
    quick(&mut group);

    group.bench_function("Region<T> (SlotMap)", |b| {
        let mut region = Region::with_capacity(N);
        let mut handles: Vec<_> = (0..N).map(|i| region.insert(payload(i as u64))).collect();
        let mut head = 0usize;
        b.iter(|| {
            // Remove one, insert one — steady-state throughput.
            let removed = region.remove(handles[head]);
            black_box(removed);
            handles[head] = region.insert(payload(head as u64 + N as u64));
            head = head.wrapping_add(1) % N;
        })
    });

    group.bench_function("SlotMap", |b| {
        let mut sm = SlotMap::with_capacity(N);
        let mut keys: Vec<_> = (0..N).map(|i| sm.insert(payload(i as u64))).collect();
        let mut head = 0usize;
        b.iter(|| {
            sm.remove(keys[head]);
            keys[head] = sm.insert(payload(head as u64 + N as u64));
            head = head.wrapping_add(1) % N;
        })
    });

    group.bench_function("DenseSlotMap", |b| {
        let mut sm = DenseSlotMap::with_capacity(N);
        let mut keys: Vec<_> = (0..N).map(|i| sm.insert(payload(i as u64))).collect();
        let mut head = 0usize;
        b.iter(|| {
            sm.remove(keys[head]);
            keys[head] = sm.insert(payload(head as u64 + N as u64));
            head = head.wrapping_add(1) % N;
        })
    });

    group.bench_function("HashMap", |b| {
        let mut map: HashMap<u32, Payload> = HashMap::with_capacity(N);
        for i in 0..N {
            map.insert(i as u32, payload(i as u64));
        }
        let mut head = 0u32;
        b.iter(|| {
            map.remove(&head);
            map.insert(head.wrapping_add(N as u32), payload(head as u64 + N as u64));
            head = head.wrapping_add(1) % N as u32;
        })
    });

    group.bench_function("Vec<Box<T>>", |b| {
        let mut boxed: Vec<Box<Payload>> = (0..N).map(|i| Box::new(payload(i as u64))).collect();
        let mut head = 0usize;
        b.iter(|| {
            boxed[head] = Box::new(payload(head as u64 + N as u64));
            head = head.wrapping_add(1) % N;
        })
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// builders (kept out of the timed loop)
// ---------------------------------------------------------------------------

fn payload(i: u64) -> Payload {
    [i, i.wrapping_mul(2), i.wrapping_mul(3), i.wrapping_mul(4)]
}

fn build_region(n: usize) -> Region<Payload> {
    let mut r = Region::with_capacity(n);
    for i in 0..n {
        r.insert(payload(i as u64));
    }
    r
}

/// Builds a region and returns it alongside the handles captured at insert
/// time, so the lookup bench can re-derive the handle for logical entry `i`
/// while the region stays live. `Region` exposes no `keys()` iterator.
fn build_region_with_handles(
    n: usize,
) -> (Region<Payload>, Vec<sefer_alloc::Handle<Payload>>) {
    let mut r = Region::with_capacity(n);
    let mut handles = Vec::with_capacity(n);
    for i in 0..n {
        handles.push(r.insert(payload(i as u64)));
    }
    (r, handles)
}

fn build_slotmap(n: usize) -> SlotMap<DefaultKey, Payload> {
    let mut sm = SlotMap::with_capacity(n);
    for i in 0..n {
        sm.insert(payload(i as u64));
    }
    sm
}

fn build_slotmap_with_keys(n: usize) -> (SlotMap<DefaultKey, Payload>, Vec<DefaultKey>) {
    let mut sm = SlotMap::with_capacity(n);
    let mut keys = Vec::with_capacity(n);
    for i in 0..n {
        keys.push(sm.insert(payload(i as u64)));
    }
    (sm, keys)
}

fn build_dense(n: usize) -> DenseSlotMap<DefaultKey, Payload> {
    let mut sm = DenseSlotMap::with_capacity(n);
    for i in 0..n {
        sm.insert(payload(i as u64));
    }
    sm
}

fn build_dense_with_keys(
    n: usize,
) -> (DenseSlotMap<DefaultKey, Payload>, Vec<DefaultKey>) {
    let mut sm = DenseSlotMap::with_capacity(n);
    let mut keys = Vec::with_capacity(n);
    for i in 0..n {
        keys.push(sm.insert(payload(i as u64)));
    }
    (sm, keys)
}

fn build_hashmap(n: usize) -> HashMap<u32, Payload> {
    let mut m = HashMap::with_capacity(n);
    for i in 0..n {
        m.insert(i as u32, payload(i as u64));
    }
    m
}

fn build_boxed(n: usize) -> Vec<Box<Payload>> {
    (0..n).map(|i| Box::new(payload(i as u64))).collect()
}

criterion_group!(benches, bench_iterate, bench_lookup, bench_churn);
criterion_main!(benches);
