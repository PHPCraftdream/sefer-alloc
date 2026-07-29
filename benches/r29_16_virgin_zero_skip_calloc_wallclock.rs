//! R29-16 (task #447) -- wall-clock gate for `virgin-zero-skip` at a size
//! where the skipped `Node::zero` memset actually dominates end-to-end
//! latency, not just instruction count.
//!
//! ## Why this bench exists
//!
//! `docs/perf/R13_3_VIRGIN_ZERO_SKIP_MAGAZINE_GATE.md` is the only prior
//! wall-clock measurement of this feature. It ran at an 8 KiB class
//! (`TARGET_CLASS = 30`) and found "No scenario shows a statistically
//! significant difference at this sample size" -- but its own text says the
//! bench does not capture the shape the feature targets: a single-threaded,
//! single-heap loop where the substrate round-trip it replaces is ALSO
//! cheap, so there was nothing large enough for the skipped memset to stand
//! out against. This bench (part of the R29-16 measurement-gap closure,
//! `docs/reviews/2026-07-29-oh-acceleration-code-project-review.md` §1.3,
//! `docs/perf/OPEN_ITEMS.md` item 25) re-runs the SAME was/now comparison at
//! 64 KiB -- large enough that `memset(65536)` is a real, visible cost
//! (~2.5 us per the design docs' analytical bandwidth table,
//! `R9_5_VIRGIN_ZERO_SKIP_DESIGN.md` §8), while still comfortably inside the
//! Small-class range this feature actually gates (`SegmentLayout::SMALL_MAX`
//! is ~253 KiB under plain `production`; 64 KiB is class index 42 of 49).
//!
//! This does NOT replace R13-3's gate -- it is an ADDITIONAL, differently
//! sized measurement answering the specific "what does this look like at a
//! size where the memset is not noise" question R13-3's own text flagged as
//! unanswered.
//!
//! ## Two scenarios
//!
//! 1. **virgin** -- `alloc_zeroed` a batch of NEVER-before-served 64 KiB
//!    blocks (bump-carve only, nothing freed mid-batch so nothing can be
//!    reused), then free the whole batch at the end. Every call in the timed
//!    region is a genuine first-touch carve -- `virgin-zero-skip` should
//!    fire and skip `Node::zero` on every one.
//! 2. **recycled** -- prime ONE 64 KiB block, dirty it, then `alloc_zeroed`
//!    + immediate `dealloc` of the SAME class in a tight loop: every
//!    iteration after the first pops the just-freed (dirty) block back off
//!    the free list -- never virgin by the dispatch conjunct
//!    (`alloc_small_with_virgin`'s doc), so `Node::zero` MUST run every time.
//!
//! ## Harness shape
//!
//! `sample_size(10)` + short warm-up/measurement (CLAUDE.md "Speed: short
//! scenario by default"), mirroring `r13_3_virgin_zero_skip_wallclock.rs`
//! exactly. Feature-gated on `alloc-global` + `fastbin` only (not
//! `virgin-zero-skip`) so the SAME binary runs both WITHOUT the feature
//! (baseline: `Node::zero` always) and WITH it (this feature), for an honest
//! was/now comparison -- see the module doc pattern established by R13-3's
//! own bench file.

#![cfg(all(feature = "alloc-global", feature = "fastbin"))]

use std::alloc::Layout;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

/// 64 KiB -- see the module doc for why this size was chosen (large enough
/// for `memset` to dominate; still comfortably Small-classified).
const CALLOC_SIZE_64K: usize = 65536;

/// How many virgin 64 KiB blocks the "virgin" scenario carves per iteration
/// before freeing the whole batch -- kept small (fast-profile budget: at
/// 64 KiB/block this already touches several MiB per criterion sample) but
/// large enough to amortise the loop's own overhead.
const VIRGIN_BATCH: usize = 16;

/// How many alloc_zeroed+dealloc cycles the "recycled" scenario runs per
/// iteration -- the SAME address is popped/pushed each cycle after the
/// first, so this measures pure free-list-pop-and-zero cost at scale.
const RECYCLED_BATCH: usize = 64;

fn bench_virgin(c: &mut Criterion) {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let layout = Layout::from_size_align(CALLOC_SIZE_64K, 8).expect("64 KiB layout");

    let mut group = c.benchmark_group("r29_16_virgin_zero_skip_calloc");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_millis(800));

    group.bench_function("virgin_64k", |b| {
        b.iter(|| {
            // Every call in this loop carves a FRESH 64 KiB block (never
            // freed back before the next alloc, so the free list stays
            // empty and each call is a genuine bump-carve) -- the
            // calloc-burst regime the virgin-zero-skip optimization
            // targets, at a size where the skipped memset is real.
            let mut ptrs = Vec::with_capacity(VIRGIN_BATCH);
            for _ in 0..VIRGIN_BATCH {
                let p = unsafe { (*heap).alloc_zeroed(layout) };
                assert!(!p.is_null(), "virgin alloc_zeroed(64 KiB) returned null");
                ptrs.push(p);
            }
            std::hint::black_box(&ptrs);
            // Free everything at the end of the batch (not interleaved) so
            // the NEXT batch's carves stay virgin too (acceptable: what
            // matters for THIS scenario is that each individual
            // alloc_zeroed within a batch was virgin at the time it ran).
            for p in ptrs {
                unsafe { (*heap).dealloc(p, layout) };
            }
        });
    });

    group.finish();

    unsafe { HeapRegistry::recycle(heap) };
}

fn bench_recycled(c: &mut Criterion) {
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let layout = Layout::from_size_align(CALLOC_SIZE_64K, 8).expect("64 KiB layout");

    // Prime + dirty once: plain alloc, write a non-zero byte pattern, then
    // free -- so the FIRST measured iteration already pops a genuinely dirty
    // block off the free list, not a one-off virgin carve whose cost would
    // otherwise pollute sample 0.
    let prime = unsafe { (*heap).alloc(layout) };
    assert!(!prime.is_null(), "prime alloc(64 KiB) returned null");
    unsafe { core::ptr::write_bytes(prime, 0xAA, CALLOC_SIZE_64K) };
    unsafe { (*heap).dealloc(prime, layout) };

    let mut group = c.benchmark_group("r29_16_virgin_zero_skip_calloc");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(200));
    group.measurement_time(Duration::from_millis(800));

    group.bench_function("recycled_64k", |b| {
        b.iter(|| {
            // alloc_zeroed immediately followed by dealloc of the SAME
            // class, repeated RECYCLED_BATCH times -- after the prime call
            // above, every call pops and re-pushes the SAME dirty block:
            // never virgin, so Node::zero MUST run on every call regardless
            // of virgin-zero-skip.
            for _ in 0..RECYCLED_BATCH {
                let p = unsafe { (*heap).alloc_zeroed(layout) };
                assert!(!p.is_null(), "recycled alloc_zeroed(64 KiB) returned null");
                std::hint::black_box(p);
                unsafe { (*heap).dealloc(p, layout) };
            }
        });
    });

    group.finish();

    unsafe { HeapRegistry::recycle(heap) };
}

criterion_group!(benches, bench_virgin, bench_recycled);
criterion_main!(benches);
