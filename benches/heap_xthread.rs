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
use std::cell::RefCell;
use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use sefer_alloc::{AllocCore, SegmentLayout};

/// Number of push/drain cycles per bench iteration.
const BATCH: usize = 256;

/// Size class for the bench blocks. 64 B is a representative small class that
/// fits many blocks per segment page.
const BLOCK_SIZE: usize = 64;

/// The size-class index production `AllocCore::alloc` resolves a `BLOCK_SIZE` /
/// alignment-8 layout to. Computed via the SAME public mapping the allocator
/// uses ([`SegmentLayout::class_for`] delegates to `SizeClasses::class_for`,
/// which `AllocCore::classify` calls) so the bench cannot drift from
/// production if size classes are retuned. Passed to `dbg_push_to_ring` per
/// the seam contract (`src/alloc_core/alloc_core_small_reclaim.rs:324-363`):
/// the caller MUST supply the class the block was actually allocated under —
/// the owner must never re-derive it from `page_map`.
const CLASS_IDX: usize = {
    let Some(idx) = SegmentLayout::class_for(BLOCK_SIZE, 8) else {
        panic!("BLOCK_SIZE must resolve to a small size class");
    };
    idx
};

/// Verify `CLASS_IDX` is the same index `AllocCore`'s own allocation path
/// derives for `BLOCK_SIZE`. Counterfactual guard so a future retune of the
/// size-class table cannot silently make the `const CLASS_IDX` above disagree
/// with `AllocCore::classify` (e.g. someone tunes the table but forgets to
/// rebuild this bench). Called once at bench setup, not in the hot loop.
///
/// Uses a real `assert_eq!` (not `debug_assert_eq!`): benches run under the
/// release `bench` profile, where `debug_assert` compiles out — a no-op guard
/// would defeat the point. The single `dbg_layout_class_for` call is setup-only
/// (once per `bench_function`, outside the timed region), so its cost is
/// irrelevant to the measurement.
fn assert_class_idx_matches_alloc_path(core: &AllocCore, layout: Layout) {
    let derived = core
        .dbg_layout_class_for(layout)
        .expect("BLOCK_SIZE must classify as a small class under AllocCore");
    assert_eq!(
        derived, CLASS_IDX,
        "CLASS_IDX ({CLASS_IDX}) disagrees with AllocCore::classify ({derived}) for BLOCK_SIZE={BLOCK_SIZE}; \
         the const lookup and the production classify path have drifted — update CLASS_IDX's source."
    );
}

/// Bench the full push→drain cycle:
///   1. Alloc `BATCH` blocks (primes the free list / segment state).
///   2. Push each block's offset into the ring via `dbg_push_to_ring`
///      (simulates a cross-thread freer).
///   3. Drain all rings via `dbg_drain_all_rings` (simulates the owner's lazy
///      reclaim on its next alloc miss).
///
/// The timing loop covers steps 2 and 3 only; step 1 is in the per-sample
/// setup closure so each criterion sample operates on fresh, validly-allocated
/// blocks (a prior version allocated once for the whole `bench_function` and
/// then republished already-freed pointers on every iteration past the first,
/// measuring duplicate/stale-free handling instead of a realistic cross-thread
/// free stream).
fn bench_ring_push_drain(c: &mut Criterion) {
    let mut group = c.benchmark_group("heap_xthread_ring");
    group.sample_size(10);
    group.warm_up_time(Duration::from_secs(1));
    group.measurement_time(Duration::from_secs(3));

    let layout = Layout::from_size_align(BLOCK_SIZE, 8).unwrap();

    group.bench_function("push_drain_256", |b| {
        // One long-lived `AllocCore` (so the segment is seeded once), wrapped
        // in a `RefCell` because `iter_batched` holds the setup and routine
        // closures simultaneously and both need `&mut AllocCore` (alloc in
        // setup, push+drain in routine). The borrow is uncontended — only the
        // routine or the setup is ever active on the single bench thread.
        let core = RefCell::new(AllocCore::new().unwrap());
        assert_class_idx_matches_alloc_path(&core.borrow(), layout);

        b.iter_batched(
            || {
                // Untimed per-sample setup: allocate a FRESH batch of blocks.
                // After the routine closure runs the previous batch is freed
                // by the drain, so reusing those pointers would publish
                // already-freed memory; re-allocating here keeps every sample
                // honest.
                let mut core = core.borrow_mut();
                let mut ptrs: [*mut u8; BATCH] = [core::ptr::null_mut(); BATCH];
                for slot in ptrs.iter_mut() {
                    let p = core.alloc(layout);
                    assert!(!p.is_null(), "pre-alloc OOM");
                    *slot = p;
                }
                ptrs
            },
            |ptrs| {
                // Timed region: push all block offsets into the ring
                // (simulated cross-thread free), then drain (owner reclaim).
                let mut core = core.borrow_mut();
                let mut pushed = 0usize;
                for &ptr in &ptrs {
                    // SAFETY (R6-MS-4): `ptr` is a fresh live allocation owned by
                    // `core` (allocated in the setup closure above); this push is
                    // its single logical remote free — the block is NOT dealloc'd
                    // nor re-issued before the `dbg_drain_all_rings` below reclaims
                    // it. `CLASS_IDX` is the block's actual class (asserted by
                    // `assert_class_idx_matches_alloc_path` above).
                    if unsafe { core.dbg_push_to_ring(ptr, CLASS_IDX) } {
                        pushed += 1;
                    }
                }
                black_box(pushed);

                core.dbg_drain_all_rings();
                black_box(&mut *core);
            },
            BatchSize::SmallInput,
        );

        // (AllocCore drops here — segments are released via `Drop`.)
    });

    // Variant: alloc + push + drain in the hot loop (no setup separation).
    // This measures the fully-integrated path including the alloc cost —
    // closer to a real async-task allocation pattern. This bench is already
    // correct on Defect 2 (it re-allocates `ptrs` fresh inside `b.iter` each
    // sample); only Defect 1 (the hardcoded class) was fixed here.
    group.bench_function("alloc_push_drain_256", |b| {
        let mut core = AllocCore::new().unwrap();
        assert_class_idx_matches_alloc_path(&core, layout);

        b.iter(|| {
            // Alloc BATCH blocks.
            let mut ptrs: [*mut u8; BATCH] = [core::ptr::null_mut(); BATCH];
            for slot in ptrs.iter_mut() {
                *slot = core.alloc(layout);
            }
            black_box(&ptrs);

            // Push to ring (simulated cross-thread free).
            let mut pushed = 0usize;
            for &ptr in &ptrs {
                // SAFETY (R6-MS-4): `ptr` is a fresh live allocation owned by
                // `core`; this push is its single logical remote free — the block
                // is reclaimed by the `dbg_drain_all_rings` below (no dealloc /
                // re-issue of `ptr` in between). `CLASS_IDX` is the actual class
                // (asserted by `assert_class_idx_matches_alloc_path` above).
                if !ptr.is_null() && unsafe { core.dbg_push_to_ring(ptr, CLASS_IDX) } {
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
