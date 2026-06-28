//! Cross-thread free micro-bench (task #62) — low-noise profile target.
//!
//! Exercises the `RemoteFreeRing` push→drain cycle DIRECTLY via
//! `AllocCore::dbg_push_to_ring` / `AllocCore::dbg_drain_all_rings`, the same
//! test seams used by the ring unit tests. This eliminates:
//!   - criterion KDE / rayon statistical overhead (still present but smaller
//!     relative to a longer inner loop)
//!   - tokio scheduler noise
//!   - mpsc channel overhead
//!   - mimalloc comparison overhead
//!
//! The inner loop: alloc 256 small blocks → push each offset into the segment's
//! ring (simulating a cross-thread freer) → drain all rings (simulating the
//! owner's lazy reclaim). Repeat for the criterion sampling window.
//!
//! **Gated on `alloc-core` + `alloc-xthread`**: the `dbg_push_to_ring` and
//! `dbg_drain_all_rings` seams live on `AllocCore` behind `alloc-xthread`.
//! `AllocCore` is accessible because `lib.rs` re-exports it under `alloc-core`
//! (as `#[doc(hidden)] pub mod alloc_core`).

#![cfg(all(feature = "alloc-core", feature = "alloc-xthread"))]
#![allow(
    clippy::cast_possible_truncation,
    clippy::needless_pass_by_value,
    clippy::semicolon_if_nothing_returned
)]

use std::alloc::Layout;
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};
use sefer_alloc::AllocCore;

/// Number of push/drain cycles per bench iteration.
const BATCH: usize = 256;

/// Size class for the bench blocks. 64 B is a representative small class that
/// fits many blocks per segment page.
const BLOCK_SIZE: usize = 64;

/// Bench the full push→drain cycle:
///   1. Alloc `BATCH` blocks (primes the free list / segment state).
///   2. Push each block's offset into the ring via `dbg_push_to_ring`
///      (simulates a cross-thread freer).
///   3. Drain all rings via `dbg_drain_all_rings` (simulates the owner's lazy
///      reclaim on its next alloc miss).
///
/// The timing loop covers steps 2 and 3 only; step 1 is in the setup closure
/// so the free lists are warm before each sample.
fn bench_ring_push_drain(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap_xthread_ring");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));

    let layout = Layout::from_size_align(BLOCK_SIZE, 8).unwrap();

    group.bench_function("push_drain_256", |b| {
        // Setup: create a fresh AllocCore and pre-alloc BATCH blocks so the
        // segment is seeded and the free list is populated.
        let mut core = AllocCore::new().unwrap();

        // Pre-alloc: put BATCH blocks in flight.
        let mut ptrs: [*mut u8; BATCH] = [core::ptr::null_mut(); BATCH];
        for slot in ptrs.iter_mut() {
            let p = core.alloc(layout);
            assert!(!p.is_null(), "pre-alloc OOM");
            *slot = p;
        }

        b.iter(|| {
            // Timed region: push all block offsets into the ring (simulated
            // cross-thread free), then drain (owner reclaim).
            let mut pushed = 0usize;
            for &ptr in &ptrs {
                if core.dbg_push_to_ring(ptr, 0) {
                    pushed += 1;
                }
            }
            black_box(pushed);

            core.dbg_drain_all_rings();
            black_box(&core);
        });

        // (AllocCore drops here — segments are released via `Drop`.)
    });

    // Variant: alloc + push + drain in the hot loop (no setup separation).
    // This measures the fully-integrated path including the alloc cost —
    // closer to a real async-task allocation pattern.
    group.bench_function("alloc_push_drain_256", |b| {
        let mut core = AllocCore::new().unwrap();
        let layout64 = Layout::from_size_align(64, 8).unwrap();

        b.iter(|| {
            // Alloc BATCH blocks.
            let mut ptrs: [*mut u8; BATCH] = [core::ptr::null_mut(); BATCH];
            for slot in ptrs.iter_mut() {
                *slot = core.alloc(layout64);
            }
            black_box(&ptrs);

            // Push to ring (simulated cross-thread free).
            let mut pushed = 0usize;
            for &ptr in &ptrs {
                if !ptr.is_null() && core.dbg_push_to_ring(ptr, 0) {
                    pushed += 1;
                }
            }
            black_box(pushed);

            // Drain (owner reclaim).
            core.dbg_drain_all_rings();
            black_box(&core);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_ring_push_drain);
criterion_main!(benches);
