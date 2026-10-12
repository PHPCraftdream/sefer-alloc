//! PRF-03 decision-data harness (task r20): MEASUREMENT-ONLY workload runner
//! for the Large-cache payload-change probe
//! (`sefer_alloc::alloc_core::r20_prf03_large_probe_snapshot`).
//!
//! This binary installs nothing exotic: `SeferAlloc` is declared as this
//! process's `#[global_allocator]` below (the crate's standard pattern —
//! see `examples/global_allocator.rs`), and every workload below runs its
//! alloc/dealloc traffic through that real global allocator. Each workload
//! is a SYNTHETIC MODEL of a real allocation pattern, not a real
//! application; the JSON lines it emits are decision data for the PRF-03
//! "is the changed-payload hit rate worth the retained-route fix" question,
//! not benchmark results.
//!
//! Per workload, one JSON line of probe-counter DELTAS:
//! `{"workload": ..., "models": ..., "large_requests": N, "hits": N,
//!   "misses": N, "changed_payload_hits": N}` plus a TOTAL aggregate.
//!
//! Measurement-only: the probe behind these counters is compiled out
//! entirely without `bench-internals` + `internals` and mutates nothing.
//!
//! Run with:
//! `cargo run --locked --release --features "production internals bench-internals" \
//!    --example r20_prf03_large_hit_activation`

#[global_allocator]
static GLOBAL: sefer_alloc::SeferAlloc = sefer_alloc::SeferAlloc::new();

use sefer_alloc::alloc_core::{r20_prf03_large_probe_snapshot, LargePayloadProbeSnapshot};
use std::alloc::{alloc, dealloc, Layout};

/// Large classification threshold: allocations LARGER than `SMALL_MAX` take
/// the Large dedicated-segment path (`src/alloc_core/platform/size_classes.rs`).
/// `SMALL_MAX` is the last entry of the small-class table, ~252 KiB (the
/// geometric series' top, the "~253 KiB" geo figure in size_classes.rs) —
/// NOT 1 MiB (a stale doc claim). Measured empirically below: 258048 B is
/// small-class, 259072 B is Large.
const LARGE_THRESHOLD_NOTE: &str = "Large path engages for size > SMALL_MAX (~252 KiB geo-top; first Large at 253 KiB; 1.75 MiB only under medium-classes)";

type Delta = [u64; 4];

fn delta(before: LargePayloadProbeSnapshot, after: LargePayloadProbeSnapshot) -> Delta {
    [
        after.requests - before.requests,
        after.hits - before.hits,
        after.misses - before.misses,
        after.changed_payload_hits - before.changed_payload_hits,
    ]
}

fn emit(workload: &str, models: &str, d: Delta) {
    println!(
        "{{\"workload\": \"{}\", \"models\": \"{}\", \"large_requests\": {}, \"hits\": {}, \
         \"misses\": {}, \"changed_payload_hits\": {}}}",
        workload, models, d[0], d[1], d[2], d[3]
    );
}

fn snap() -> LargePayloadProbeSnapshot {
    r20_prf03_large_probe_snapshot()
}

/// Workload 1 — model of `npm run bench:table` (`benches/global_alloc.rs`):
/// its sized scenarios allocate 16 B..1024 B, i.e. entirely small-class.
/// Expected (and itself a finding): ~0 Large requests.
fn bench_table_replay_model() {
    let sizes: [usize; 8] = [16, 24, 48, 64, 128, 256, 512, 1024];
    for round in 0..2000 {
        let mut keep: Vec<Vec<u8>> = Vec::with_capacity(8);
        for s in sizes {
            let mut v = vec![0u8; s];
            v[0] = round as u8;
            keep.push(v);
        }
        drop(keep);
    }
}

/// Workload 2 — Vec<u8> doubling growth chains 64 KiB -> 64 MiB (realloc
/// pattern; models log/video buffering). Growth reallocs move through
/// progressively larger Large spans; the final frees deposit into the cache.
fn growing_vec_u8() {
    for _ in 0..60 {
        let mut v: Vec<u8> = Vec::with_capacity(64 * 1024);
        while v.capacity() < 64 * 1024 * 1024 {
            v.reserve(v.capacity().max(1));
            v.push(1);
        }
        drop(v);
    }
}

/// Workload 3 — allocate-into-buffer of varying large sizes, free, repeat
/// (models reading whole files of varied sizes): mixes same-size churn
/// (cache hits) with size jumps (best-fit misses).
fn file_read_whole() {
    let base_sizes: [usize; 6] = [
        2 * 1024 * 1024,
        3 * 1024 * 1024,
        2 * 1024 * 1024,
        5 * 1024 * 1024,
        2 * 1024 * 1024,
        8 * 1024 * 1024,
    ];
    for round in 0..100 {
        for s in base_sizes {
            let mut buf = vec![0u8; s + (round * 4096)];
            buf[s - 1] = 0xAA;
            drop(buf);
        }
    }
}

/// Workload 4 — repeatedly serialize a large in-memory structure into a
/// reused scratch buffer (clear + refill same capacity, occasionally a
/// larger capacity; models per-request serialization buffers).
fn serialize_reuse() {
    for round in 0..400 {
        // Fresh buffer per cycle (per-request serialization); mostly the
        // same size (same-size free-then-alloc cache hits), every 8th
        // cycle slightly larger (best-fit variation).
        let cap = 3 * 1024 * 1024 + if round % 8 == 7 { 256 * 1024 } else { 0 };
        let mut scratch: Vec<u8> = Vec::with_capacity(cap);
        scratch.resize(cap - 64 * 1024, 0x5A);
        scratch[0] = round as u8;
        // black_box: without it LLVM deletes the whole alloc/fill/free body
        // (stores to a fresh malloc are not externally observable).
        std::hint::black_box(scratch.as_ptr());
        drop(scratch);
    }
}

/// Workload 5 — pool of N fixed-size large buffers with acquire/release
/// churn (models connection/object pools of large buffers). Same-size
/// free-then-alloc is the canonical cache-hit shape.
fn fixed_size_pool() {
    const N: usize = 8;
    const BUF: usize = 2 * 1024 * 1024;
    for _ in 0..100 {
        let mut pool: Vec<Vec<u8>> = Vec::with_capacity(N);
        for i in 0..N {
            let mut b = vec![0u8; BUF];
            b[0] = i as u8;
            pool.push(b);
        }
        drop(pool); // release: deposits all N into the large cache
    }
}

/// Workload 6 — alternate layouts with align 16 / 4096 / 64 KiB (all still
/// < 4 MiB SEGMENT) on the same sizes via raw alloc/dealloc — deliberately
/// MAXIMIZES changed-payload hits (UPPER BOUND, stated as such: a hit's new
/// payload offset `align_up(hdr, align.max(page))` differs from the cached
/// one only when `align` exceeds the runtime page size, so the 64 KiB arm is
/// the one that activates on 4 KiB-page hosts). Models aligned/DMA-like
/// buffers.
fn alternating_align() {
    let size = 2 * 1024 * 1024;
    let aligns: [usize; 3] = [16, 4096, 64 * 1024];
    for round in 0..120 {
        let align = aligns[round % aligns.len()];
        let layout = Layout::from_size_align(size, align).expect("valid layout");
        // SAFETY: layout has non-zero size; the block is freed with the
        // same layout below (GlobalAlloc contract).
        let ptr = unsafe { alloc(layout) };
        assert!(!ptr.is_null(), "alloc failed");
        // Touch the block so LLVM cannot remove the alloc/free pair.
        // SAFETY: `ptr` covers `size` >= 1 bytes.
        unsafe { *ptr = 1 };
        // black_box: without it LLVM deletes the alloc/free pair outright.
        std::hint::black_box(ptr);
        // SAFETY: `ptr` was allocated with exactly this layout.
        unsafe { dealloc(ptr, layout) };
    }
}

/// Size sweep around the Large threshold — confirms where the Large path
/// engages by observing probe deltas just below / at / above it.
fn size_sweep() {
    // Bracket the real boundary empirically (measured on this host):
    // 258048 B (252 KiB) is still small-class, 259072 B (253 KiB) already
    // takes the Large dedicated-segment path — SMALL_MAX is ~252 KiB (the
    // geo series' last entry), consistent with size_classes.rs' "~253 KiB"
    // geo-top figure. NOT 1 MiB (a stale doc claim elsewhere).
    let sizes: [usize; 4] = [64 * 1024, 258048, 259072, 1024 * 1024];
    for s in sizes {
        let before = snap();
        let mut v = vec![0u8; s];
        // Touch the buffer: an untouched alloc/free pair would be elided by
        // LLVM (malloc/free pair removal), falsifying the probe delta.
        v[0] = 1;
        // black_box: without it LLVM deletes the calloc/free pair outright
        // (allocator attributes), falsifying the probe delta.
        std::hint::black_box(&v as *const Vec<u8>);
        drop(v);
        let d = delta(before, snap());
        println!(
            "{{\"workload\": \"size_sweep\", \"size\": {}, \"large_requests\": {}}}",
            s, d[0]
        );
    }
}

fn main() {
    println!("note: {}", LARGE_THRESHOLD_NOTE);
    let workloads: [(&str, &str, fn()); 6] = [
        (
            "bench_table_replay_model",
            "benches/global_alloc.rs sized scenarios (16B..1024B, small-class)",
            bench_table_replay_model,
        ),
        (
            "growing_vec_u8",
            "log/video-buffer realloc doubling growth 64KiB..64MiB",
            growing_vec_u8,
        ),
        (
            "file_read_whole",
            "whole-file read buffers of varied sizes, freed and repeated",
            file_read_whole,
        ),
        (
            "serialize_reuse",
            "per-request serialization into a reused scratch buffer",
            serialize_reuse,
        ),
        (
            "fixed_size_pool",
            "connection/object pool of fixed-size large buffers churning",
            fixed_size_pool,
        ),
        (
            "alternating_align",
            "align 16/4096/64KiB alternation on one size (changed-payload UPPER BOUND)",
            alternating_align,
        ),
    ];
    let total_start = snap();
    for (name, models, f) in workloads {
        let before = snap();
        f();
        emit(name, models, delta(before, snap()));
    }
    size_sweep();
    emit(
        "TOTAL",
        "aggregate of all workloads",
        delta(total_start, snap()),
    );
}
