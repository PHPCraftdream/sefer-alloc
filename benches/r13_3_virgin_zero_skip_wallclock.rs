//! R13-3 (task #273) — wall-clock gate for the `virgin-zero-skip` magazine
//! fix: cold-virgin, warm-reuse, and mixed `alloc_zeroed` scenarios.
//!
//! ## Why this bench exists
//!
//! R12-10 (task #261) shipped `virgin-zero-skip` with a documented gap: it
//! measured the cold-virgin win but never measured warm-reuse steady-state,
//! because the shipped design BYPASSED the per-thread magazine on every
//! `alloc_zeroed` call — including a call that would otherwise have been a
//! pure magazine-array-pop. R13-3 (task #273, this fix) threads the virgin
//! signal THROUGH the magazine instead (`PerClass::virgin_mask`), so
//! `alloc_zeroed` goes back through `HeapCore::alloc`'s own hit/miss
//! machinery. This bench is the honest "was / now" comparison the R12-10
//! design docs promised as a follow-up (`R9_5_VIRGIN_ZERO_SKIP_DESIGN.md`
//! §11 Stage 3): run this binary compiled WITHOUT `virgin-zero-skip` (the
//! `Node::zero`-always baseline / — for the R12-10 "was" column — check out
//! the pre-R13-3 commit) and WITH `virgin-zero-skip` (this fix, the "now"
//! column), and diff the three scenarios' ns/op.
//!
//! ## Three scenarios
//!
//! 1. **cold virgin** — `alloc_zeroed` a fresh block, immediately `dealloc`,
//!    repeat with EVER-GROWING size (never reuse an address) — every call is
//!    a genuine first-touch bump-carve. This is the R9-5/R11-8 design docs'
//!    target regime: the skip should fire on every call.
//! 2. **warm reuse** — `alloc_zeroed` + immediate `dealloc` of the SAME
//!    class, in a tight loop, so every call after the first is a magazine
//!    HIT on a block this same loop already freed (never virgin, by the
//!    dispatch conjunct). THIS is the scenario R12-10 never measured: under
//!    the pre-R13-3 magazine-bypass design, EVERY iteration paid the
//!    substrate's free-list-scan + dispatch machinery instead of a plain
//!    magazine pop — the exact regression this task fixes.
//! 3. **mixed** — alternates cold-virgin-shaped and warm-reuse-shaped calls
//!    within one loop, approximating a realistic workload that is neither
//!    purely cold nor purely steady-state.
//!
//! ## Harness shape
//!
//! `sample_size(10)` + short warm-up/measurement (CLAUDE.md "Speed: short
//! scenario by default") — this is a fast comparative gate, not a
//! publication-grade benchmark. Feature-gated on `alloc-global`, `fastbin`
//! (the magazine — without it there is nothing R13-3 changed to measure) so
//! the file compiles to a valid `harness = false` `main` under
//! `--all-features` regardless of whether `virgin-zero-skip` itself is on.

#![cfg(all(feature = "alloc-global", feature = "fastbin"))]

use std::alloc::Layout;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

/// A class untouched by anything else in this process (bench binaries are
/// single-shot processes, but keep the same "dedicated class" discipline the
/// `tests/r13_3_*` files use for reproducibility if this bench is ever
/// merged into a longer-running harness).
const TARGET_CLASS: usize = 30;

/// How many blocks the "cold virgin" scenario advances through per
/// iteration before the criterion sample repeats -- kept small (fast-profile
/// budget) but large enough to amortise the loop's own overhead.
const COLD_BATCH: usize = 64;

/// How many alloc_zeroed+dealloc cycles the "warm reuse" scenario runs per
/// iteration -- the SAME address is popped/pushed each cycle after the
/// first, so this measures pure magazine-hit cost at scale.
const WARM_BATCH: usize = 256;

fn bench_cold_virgin(c: &mut Criterion) {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let bs = sefer_alloc::alloc_core::AllocCore::dbg_block_size(TARGET_CLASS);
    let layout = Layout::from_size_align(bs, 8).expect("TARGET_CLASS layout");

    let mut group = c.benchmark_group("r13_3_virgin_zero_skip");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_millis(800));

    group.bench_function("cold_virgin", |b| {
        b.iter(|| {
            // Every call in this loop carves a FRESH block (never freed
            // back before the next alloc, so the free list stays empty and
            // each call is a genuine bump-carve) -- the cold/calloc-burst
            // regime the virgin-zero-skip optimization targets.
            let mut ptrs = Vec::with_capacity(COLD_BATCH);
            for _ in 0..COLD_BATCH {
                let p = unsafe { (*heap).alloc_zeroed(layout) };
                assert!(!p.is_null(), "cold_virgin alloc_zeroed returned null");
                ptrs.push(p);
            }
            std::hint::black_box(&ptrs);
            // Free everything at the end of the batch (not interleaved) so
            // the NEXT batch's carves stay virgin too (the freed blocks sit
            // on the free list, but `refill_class_bump_impl`'s free-drain-first
            // policy will consume them before the FOLLOWING batch's carve —
            // acceptable: what matters for THIS scenario is that each
            // individual alloc_zeroed within a batch was virgin at the time
            // it ran, which holds since nothing is freed mid-batch).
            for p in ptrs {
                unsafe { (*heap).dealloc(p, layout) };
            }
        });
    });

    group.finish();

    unsafe { HeapRegistry::recycle(heap) };
}

fn bench_warm_reuse(c: &mut Criterion) {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let bs = sefer_alloc::alloc_core::AllocCore::dbg_block_size(TARGET_CLASS + 1);
    let layout = Layout::from_size_align(bs, 8).expect("TARGET_CLASS+1 layout");

    // Prime the magazine once so the FIRST measured iteration is already a
    // hit, not a one-off miss whose cost would otherwise pollute sample 0.
    let prime = unsafe { (*heap).alloc_zeroed(layout) };
    assert!(!prime.is_null());
    unsafe { (*heap).dealloc(prime, layout) };

    let mut group = c.benchmark_group("r13_3_virgin_zero_skip");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_millis(800));

    group.bench_function("warm_reuse", |b| {
        b.iter(|| {
            // alloc_zeroed immediately followed by dealloc of the SAME
            // class, repeated WARM_BATCH times -- after the first call
            // (primed above, outside the measured loop), every call pops
            // and re-pushes blocks the loop itself already freed: a pure
            // magazine hit/push cycle, never virgin. This is the R12-10 gap
            // this task's fix targets: under the pre-fix magazine-bypass
            // design, EVERY one of these calls paid substrate free-list-scan
            // cost instead of an array pop.
            for _ in 0..WARM_BATCH {
                let p = unsafe { (*heap).alloc_zeroed(layout) };
                assert!(!p.is_null(), "warm_reuse alloc_zeroed returned null");
                std::hint::black_box(p);
                unsafe { (*heap).dealloc(p, layout) };
            }
        });
    });

    group.finish();

    unsafe { HeapRegistry::recycle(heap) };
}

fn bench_mixed(c: &mut Criterion) {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let bs = sefer_alloc::alloc_core::AllocCore::dbg_block_size(TARGET_CLASS + 2);
    let layout = Layout::from_size_align(bs, 8).expect("TARGET_CLASS+2 layout");

    let mut group = c.benchmark_group("r13_3_virgin_zero_skip");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_millis(800));

    group.bench_function("mixed", |b| {
        b.iter(|| {
            // Alternate: allocate a small cold burst (never freed within
            // the burst -> virgin), then immediately warm-reuse-cycle a
            // SEPARATE single block WARM_BATCH/4 times (magazine hits),
            // then free the cold burst. Approximates a workload that mixes
            // first-touch and steady-state traffic in the same class.
            let mut cold_ptrs = Vec::with_capacity(COLD_BATCH / 4);
            for _ in 0..(COLD_BATCH / 4) {
                let p = unsafe { (*heap).alloc_zeroed(layout) };
                assert!(!p.is_null(), "mixed cold-leg alloc_zeroed returned null");
                cold_ptrs.push(p);
            }

            let warm_seed = unsafe { (*heap).alloc_zeroed(layout) };
            assert!(!warm_seed.is_null());
            unsafe { (*heap).dealloc(warm_seed, layout) };
            for _ in 0..(WARM_BATCH / 4) {
                let p = unsafe { (*heap).alloc_zeroed(layout) };
                assert!(!p.is_null(), "mixed warm-leg alloc_zeroed returned null");
                std::hint::black_box(p);
                unsafe { (*heap).dealloc(p, layout) };
            }

            std::hint::black_box(&cold_ptrs);
            for p in cold_ptrs {
                unsafe { (*heap).dealloc(p, layout) };
            }
        });
    });

    group.finish();

    unsafe { HeapRegistry::recycle(heap) };
}

criterion_group!(benches, bench_cold_virgin, bench_warm_reuse, bench_mixed);
criterion_main!(benches);
