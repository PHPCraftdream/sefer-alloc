//! R12-7 stage 1 — wall-clock gate for class-aware dirty routing.
//!
//! `docs/perf/R9_6_CLASS_AWARE_DIRTY_ROUTING_JUDGE.md` (R9-6) measured the
//! O(D) vs O(D_class) gap in `drain_dirty_segments` at the COUNTER level only
//! (`WASTED_DIRTY_DRAINS / DIRTY_SEGMENTS_DRAINED`) and explicitly deferred
//! the wall-clock question to a follow-up (its §9 "Next step before
//! implementing"): *"run a criterion bench ... comparing the current drain
//! against a per-(segment,class) prototype on the SAME workload shape,
//! reporting ns/op at N=1/2/4/8. If the wall-clock win at N=4 is >5% ...
//! upgrade to GO ... If it is <5%, the complexity is not justified and the
//! recommendation becomes NO-GO."*
//!
//! This bench is that follow-up's FIRST half: it measures the CURRENT
//! (per-segment, not per-class) drain's wall-clock cost on the identical
//! workload shape `tests/r9_6_class_aware_dirty_judge.rs` used to produce the
//! counter ratios, so the "is the waste ratio big enough to matter in
//! wall-clock terms" question has an actual number instead of the R9-6
//! report's "~50-200ns/visit, estimated, NOT measured" placeholder (its §8).
//!
//! There is (deliberately) no "prototype" arm in this file — per the R12-7
//! task's two-stage gate, a per-(segment,class) implementation is stage 2,
//! built ONLY if this stage-1 measurement shows the waste ratio translates
//! into a material wall-clock cost. This bench's number is the GO/NO-GO
//! signal for whether stage 2 happens at all.
//!
//! ## Reading the numbers
//!
//! The bench reports total ns for a fixed-size round (`BLOCKS_PER_CLASS`
//! remote frees per producer class + a fixed number of owner allocs) at each
//! producer-class count N in `[1, 2, 4, 8]`. What matters is NOT the absolute
//! ns (which includes N threads' worth of remote-free work, not just the
//! owner's drain cost) but the MARGINAL cost of adding wasted-drain classes:
//! compare N=1 (owner-only useful drains, ~0% waste per R9-6 §6) against
//! N=4/N=8 (82%/95% waste per R9-6 §6) using the SAME `BLOCKS_PER_CLASS`
//! per-producer workload, so the per-producer remote-free cost is constant
//! and the delta is attributable to the owner's drain-loop cost scaling with
//! the number of concurrently-dirty (but often irrelevant) segments.
//!
//! Because criterion's `iter()` includes the full round (spawn N producer
//! threads + owner alloc loop + join), thread-spawn/join overhead is present
//! in every arm equally (each round spawns exactly N+1 threads) — it does not
//! explain an N=1 vs N=8 divergence beyond the linear thread-count term,
//! which is a separate, already-understood cost unrelated to the dirty-bitmap
//! question. The owner's **allocation throughput** (allocs/sec, reported as a
//! diagnostic alongside the timing) isolates the drain-loop's per-alloc
//! overhead more directly than raw round time.
//!
//! ## Two axes, printed side by side (R14-3, task #288)
//!
//! Three independent Round-13 reviews flagged that this bench's headline
//! `ns/owner_alloc` figure (e.g. R13-9's "21.71x at N=8") is built from a
//! **sub-window** timer: `run_round`'s own `start = Instant::now()` begins
//! AFTER the pre-alloc of `BLOCKS_PER_CLASS * n` producer blocks and stops
//! BEFORE `HeapRegistry::recycle`. Criterion's `iter()` closure, by contrast,
//! wraps the ENTIRE `run_round` call — pre-alloc, the timed window, AND
//! recycle — so criterion's own reported "time:" is the full-round cost, and
//! it does NOT shrink by anywhere near the same factor the window metric
//! does (e.g. one N=8 pairing showed ~20.6ms -> ~18.4ms full-round, an ~11%
//! improvement, alongside an 18.8ms -> 1.35ms *window* improvement) — most of
//! the ~17ms of drain work the window metric "removes" at N=8 did not
//! disappear, it moved into the unmeasured pre-alloc/recycle part of the same
//! round (see `docs/perf/R13_9_CLASS_AWARE_DIRTY_PRODUCTION_GATE.md`'s
//! correction note and `docs/perf/R14_3_CLASS_AWARE_DIRTY_FIXED_WORK_AB.md`).
//!
//! To make this divergence visible in this bench's own output (not just in a
//! separate report), each producer-count `n` now ALSO accumulates a
//! `full_round_ns` figure from a second, OUTER `Instant` pair that spans the
//! identical region criterion's `iter()` closure times (i.e. all of
//! `run_round`, including the parts the window timer excludes). Both axes are
//! printed on the same line — `ns/owner_alloc` (the sub-window metric) and
//! `ns/full_round` (the whole-round metric) — so a reader sees the gap
//! directly instead of having to cross-reference criterion's separate "time:"
//! output by hand.
//!
//! ## Harness shape
//!
//! Deliberately IDENTICAL in structure to `tests/r9_6_class_aware_dirty_judge.rs`'s
//! `run_round`, just wrapped in a `Criterion::bench_function` iterator instead
//! of a plain `#[test]`, and with the round size cut down to fit this
//! project's fast-bench-profile budget (`sample_size(10)`, short
//! warm-up/measurement — CLAUDE.md "Speed: short scenario by default").
//! `BLOCKS_PER_CLASS` is reduced from the judge test's 4000 to 800 (still
//! enough to force multiple drain cycles per round, per the same reasoning
//! `heap_fanin_production.rs` used to size its own `N` constant down from
//! 2000) to keep the full N=1/2/4/8 sweep inside a couple of minutes.
//!
//! ## Feature gating
//!
//! Compiles under `alloc-global`, `alloc-xthread`, `alloc-segment-directory`,
//! `alloc-stats` (drain-count diagnostics) regardless of `numa-aware` — a
//! `harness = false` criterion bench target needs a `main` unconditionally
//! (unlike a `tests/` integration test, which tolerates "0 tests, compiled
//! empty" gracefully under `#![cfg]`, `cargo bench`/`clippy --all-targets
//! --all-features` cannot build a target whose ENTIRE file compiled away).
//! `AllocCore::drain_dirty_segments` (the mechanism under measurement) is
//! itself compiled out under `numa-aware` (directory-driven lookup is
//! `numa-aware`-incompatible — see that method's own doc comment), so the
//! measurement is meaningless there; `bench_class_aware_dirty_wallclock`
//! detects this at runtime (`cfg!(feature = "numa-aware")`) and reports a
//! skip message instead of running the (numerically vacuous) sweep.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-segment-directory",
    feature = "alloc-stats"
))]
#![allow(clippy::cast_precision_loss)]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, Criterion};

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

/// Blocks of each producer class pre-allocated (and then remotely freed) per
/// measurement round. Cut from the judge test's 4000 to keep the N=1..8
/// sweep inside this project's fast-bench-profile budget while still forcing
/// multiple drain cycles per round (each producer's burst spans several
/// `RING_CAP`-sized ring fills, so the owner's drain loop fires repeatedly
/// during the round, not just once).
const BLOCKS_PER_CLASS: usize = 800;

/// Same producer-class index set as the judge test — distinct classes above
/// the materialisation-carve range, each carving its own fresh segment.
const PRODUCER_CLASS_INDICES: &[usize] = &[40, 41, 42, 43, 44, 45, 46, 47];

/// The class the owner allocates continuously — the FIRST producer class, so
/// its drains are useful while the other N-1 producer segments' drains (which
/// the owner's per-SEGMENT dirty bitmap forces it to visit anyway) are
/// wasted. Identical rationale to the judge test's `TARGET_CLASS`.
const TARGET_CLASS: usize = 40;

/// Owner-thread self-free batch size, same rationale as the judge test's
/// `OWNER_BATCH` and `heap_fanin_production.rs`'s `N`: keeps the free list
/// empty long enough that owner allocs keep re-entering
/// `find_segment_with_free_impl` -> `drain_dirty_segments` instead of being
/// satisfied by the fast free-list path.
const OWNER_BATCH: usize = 512;

/// Minimum owner allocs per round (matches the judge test's liveness floor).
const MIN_OWNER_ITERS: usize = 800;

fn materialise_directory(heap: *mut HeapCore) -> Vec<*mut u8> {
    let threshold = AllocCore::dbg_directory_materialize_threshold() as usize;
    let class_count = AllocCore::dbg_small_class_count();
    let carve_ceiling = PRODUCER_CLASS_INDICES[0];
    let target = (threshold + 8).min(carve_ceiling);
    assert!(
        target > threshold,
        "size-class table too small for materialisation carve (need > {threshold} classes below producer range, have {carve_ceiling})"
    );
    assert!(
        *PRODUCER_CLASS_INDICES.last().unwrap() < class_count,
        "producer class indices exceed SMALL_CLASS_COUNT ({class_count})"
    );
    let mut keep_alive: Vec<*mut u8> = Vec::with_capacity(target);
    for cls in 0..target {
        let block_size = AllocCore::dbg_block_size(cls);
        let layout =
            Layout::from_size_align(block_size, 8).expect("class block size is a valid layout");
        let p = unsafe { (*heap).alloc(layout) };
        assert!(
            !p.is_null(),
            "materialise alloc for class {cls} returned null"
        );
        keep_alive.push(p);
    }
    keep_alive
}

/// One measurement round with `n_producer_classes` concurrent producer
/// classes — identical shape to `tests/r9_6_class_aware_dirty_judge.rs::run_round`,
/// but returns (elapsed, owner_alloc_count) for wall-clock reporting instead
/// of counter deltas.
fn run_round(n_producer_classes: usize) -> (Duration, usize) {
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let heap_addr = heap as usize;

    let _keep_alive = materialise_directory(heap);

    let producer_classes = &PRODUCER_CLASS_INDICES[..n_producer_classes];
    let mut blocks_per_producer: Vec<Vec<usize>> = Vec::with_capacity(n_producer_classes);
    for &cls in producer_classes {
        let block_size = AllocCore::dbg_block_size(cls);
        let layout = Layout::from_size_align(block_size, 8)
            .expect("producer class block size is a valid layout");
        let mut v: Vec<usize> = Vec::with_capacity(BLOCKS_PER_CLASS);
        for _ in 0..BLOCKS_PER_CLASS {
            let p = unsafe { (*heap).alloc(layout) };
            assert!(!p.is_null(), "producer-class pre-alloc returned null");
            v.push(p as usize);
        }
        blocks_per_producer.push(v);
    }

    let producers_done = Arc::new(AtomicBool::new(false));
    let producers_done_owner = Arc::clone(&producers_done);

    let start = Instant::now();

    let mut handles = Vec::with_capacity(n_producer_classes);
    for (i, &cls) in producer_classes.iter().enumerate() {
        let addrs = std::mem::take(&mut blocks_per_producer[i]);
        let block_size = AllocCore::dbg_block_size(cls);
        let layout = Layout::from_size_align(block_size, 8)
            .expect("producer class block size is a valid layout");
        handles.push(thread::spawn(move || {
            let _ = bootstrap::ensure();
            let remote_heap = HeapRegistry::claim();
            assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
            for (i, addr) in addrs.iter().enumerate() {
                let p = *addr as *mut u8;
                unsafe { (*remote_heap).dealloc(p, layout) };
                if i & 0x3F == 0 {
                    std::thread::yield_now();
                }
            }
            unsafe { HeapRegistry::recycle(remote_heap) };
        }));
    }

    let target_block_size = AllocCore::dbg_block_size(TARGET_CLASS);
    let target_layout = Layout::from_size_align(target_block_size, 8)
        .expect("TARGET_CLASS block size is a valid layout");
    let target_heap_addr = heap_addr;
    let owner_handle: thread::JoinHandle<usize> = thread::spawn(move || {
        let target_heap = target_heap_addr as *mut HeapCore;
        let mut batch: Vec<*mut u8> = Vec::with_capacity(OWNER_BATCH);
        let mut i = 0usize;
        loop {
            let p = unsafe { (*target_heap).alloc(target_layout) };
            if !p.is_null() {
                batch.push(p);
            }
            i += 1;
            if batch.len() >= OWNER_BATCH {
                for p in batch.drain(..) {
                    unsafe { (*target_heap).dealloc(p, target_layout) };
                }
            }
            if i >= MIN_OWNER_ITERS && producers_done_owner.load(Ordering::Acquire) {
                break;
            }
        }
        for p in batch.drain(..) {
            unsafe { (*target_heap).dealloc(p, target_layout) };
        }
        i
    });

    for h in handles {
        h.join().expect("producer thread must not panic");
    }
    producers_done.store(true, Ordering::Release);
    let owner_allocs = owner_handle.join().expect("owner thread must not panic");

    let elapsed = start.elapsed();

    unsafe { HeapRegistry::recycle(heap) };

    (elapsed, owner_allocs)
}

fn bench_class_aware_dirty_wallclock(c: &mut Criterion) {
    // `AllocCore::drain_dirty_segments` (the mechanism this bench measures)
    // is compiled out entirely under `numa-aware` (directory-driven lookup
    // is `numa-aware`-incompatible), so a sweep here would measure nothing
    // meaningful. Skip at runtime rather than compiling the whole file away
    // (see this file's module doc "Feature gating" section for why the
    // file-level `#![cfg]` cannot express "not numa-aware" and still produce
    // a valid `harness = false` bench `main` under `--all-features`).
    if cfg!(feature = "numa-aware") {
        eprintln!(
            "\nr12_7_class_aware_dirty_wallclock: SKIPPED under `numa-aware` \
             (drain_dirty_segments is compiled out; this bench's measurement \
             would be vacuous)."
        );
        return;
    }

    let _ = bootstrap::ensure();

    let mut group = c.benchmark_group("r12_7_class_aware_dirty_wallclock");
    group.sample_size(10);
    group.warm_up_time(Duration::from_millis(300));
    group.measurement_time(Duration::from_millis(1500));

    eprintln!("\n═══════════════════════════════════════════════════════════════════");
    eprintln!("R12-7 stage 1 — class-aware dirty routing wall-clock gate");
    eprintln!(
        "TARGET_CLASS = {TARGET_CLASS} (block_size = {} B); BLOCKS_PER_CLASS = {BLOCKS_PER_CLASS}",
        AllocCore::dbg_block_size(TARGET_CLASS)
    );
    eprintln!("═══════════════════════════════════════════════════════════════════");

    let sweep: &[usize] = &[1, 2, 4, 8];
    // (n, ns_per_round_window, ns_per_owner_alloc_window, ns_per_full_round)
    let mut totals: Vec<(usize, f64, f64, f64)> = Vec::with_capacity(sweep.len());

    for &n in sweep {
        // Manual timing accumulation alongside criterion's own measurement,
        // so we can print an ns/owner-alloc figure comparable across N even
        // though the total round work (N producer threads' worth of remote
        // frees) grows with N. `total_ns` / `total_allocs` isolates the
        // owner-side cost per alloc, which is what the dirty-drain's O(D)
        // vs O(D_class) character governs.
        //
        // `total_full_round_ns` is a SEPARATE outer timer around the exact
        // same region criterion's own `iter()` closure times (all of
        // `run_round` — pre-alloc, the timed window, AND recycle), not just
        // `run_round`'s internal sub-window. See this file's module doc "Two
        // axes, printed side by side" for why both are printed rather than
        // treating the sub-window figure as if it were the whole round's cost
        // (R14-3, task #288 — three independent Round-13 reviews flagged the
        // prior single-axis output as misleading on this exact point).
        let mut total_ns = 0.0f64;
        let mut total_allocs = 0.0f64;
        let mut total_full_round_ns = 0.0f64;
        let mut iters = 0u64;

        group.bench_function(format!("producers={n}"), |b| {
            b.iter(|| {
                let full_round_start = Instant::now();
                let (elapsed, owner_allocs) = run_round(n);
                total_full_round_ns += full_round_start.elapsed().as_nanos() as f64;
                total_ns += elapsed.as_nanos() as f64;
                total_allocs += owner_allocs as f64;
                iters += 1;
                std::hint::black_box(owner_allocs);
            });
        });

        let ns_per_owner_alloc = if total_allocs > 0.0 {
            total_ns / total_allocs
        } else {
            0.0
        };
        let ns_per_round = if iters > 0 {
            total_ns / iters as f64
        } else {
            0.0
        };
        let ns_per_full_round = if iters > 0 {
            total_full_round_ns / iters as f64
        } else {
            0.0
        };
        eprintln!(
            "producers={n:<2}  ns/round(window)={ns_per_round:>12.0}  ns/owner_alloc(window)={ns_per_owner_alloc:>8.1}  ns/full_round={ns_per_full_round:>12.0}"
        );
        totals.push((n, ns_per_round, ns_per_owner_alloc, ns_per_full_round));
    }

    eprintln!("───────────────────────────────────────────────────────────────────");
    if let (Some(&(_, _, n1, full1)), Some(&(_, _, n4, full4))) = (
        totals.iter().find(|(n, _, _, _)| *n == 1),
        totals.iter().find(|(n, _, _, _)| *n == 4),
    ) {
        let delta_pct = if n1 > 0.0 {
            (n4 - n1) / n1 * 100.0
        } else {
            0.0
        };
        let full_delta_pct = if full1 > 0.0 {
            (full4 - full1) / full1 * 100.0
        } else {
            0.0
        };
        eprintln!(
            "N=1 -> N=4 ns/owner_alloc(window) delta: {n1:.1} -> {n4:.1} ({delta_pct:+.1}%) \
             [R9-6 gate threshold: >5% => GO on stage 2, else NO-GO]"
        );
        eprintln!(
            "N=1 -> N=4 ns/full_round delta: {full1:.0} -> {full4:.0} ({full_delta_pct:+.1}%) \
             [companion axis — see module doc \"Two axes, printed side by side\"; \
             this is the SAME region criterion's own \"time:\" output measures]"
        );
    }
    eprintln!("═══════════════════════════════════════════════════════════════════\n");

    group.finish();
}

criterion_group!(benches, bench_class_aware_dirty_wallclock);
criterion_main!(benches);
