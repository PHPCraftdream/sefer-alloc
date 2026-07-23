//! R9-6 judge: class-aware dirty routing — measurement-only.
//!
//! Measures the O(D) vs O(D_class) gap the external review flagged in
//! `drain_dirty_segments` (`src/alloc_core/alloc_core_small.rs`): the drain
//! visits EVERY segment whose per-segment dirty bit is set regardless of which
//! size class the caller is searching for, so under a mixed-class remote-fan-in
//! workload (multiple producer threads concurrently freeing blocks of DIFFERENT
//! size classes into a shared owner) a class-A miss pays the full drain cost of
//! segments that are dirty ONLY for unrelated classes B, C, D, ... .
//!
//! This file does NOT implement per-(segment,class) dirty routing — it only
//! measures the gap, via the new diagnostic counter `WASTED_DIRTY_DRAINS`
//! (added in `directory_stats.rs` / `alloc_core_core_diag.rs` as the single
//! additive `src/` change for this task). The counter is bumped once per drain
//! visit where the segment's ring, once drained, produced ZERO reclaimed blocks
//! of the `class_idx` the caller is searching for. The ratio
//! `dbg_wasted_dirty_drains() / dbg_dirty_segments_drained()` directly
//! characterises the gap.
//!
//! ## Harness shape
//!
//! For each producer-class-count N in `[1, 2, 4, 8]`:
//!   1. Owner thread materialises the directory sidecar by carving segments
//!      across many distinct classes (crosses `DIRECTORY_MATERIALIZE_THRESHOLD`
//!      = 32 segments).
//!   2. Owner pre-allocates `BLOCKS_PER_CLASS` blocks of each of N distinct
//!      producer classes — these blocks are OWNED by the owner's `HeapCore`.
//!   3. Snapshot `dbg_dirty_segments_drained()` and `dbg_wasted_dirty_drains()`.
//!
//!   4. N producer threads spawn; producer i remotely frees all of class i's
//!      blocks (genuine cross-thread free via `HeapCore::dealloc` →
//!      `dealloc_foreign_slow` → `push_with_overflow_retry` →
//!      `set_dirty_bit_for_segment`).
//!   5. Owner concurrently allocates TARGET_CLASS blocks WITHOUT self-freeing
//!      mid-batch (forcing every alloc to fall through to the magazine refill
//!      → `find_segment_with_free_impl(TARGET_CLASS)` → `drain_dirty_segments`,
//!      exactly as `tests/remote_fanin.rs` harness 1 explains is necessary to
//!      force genuine drains). TARGET_CLASS IS the FIRST producer class — so
//!      drains of the TARGET segment are USEFUL (it has TARGET_CLASS blocks in
//!      its ring), while drains of the other N-1 producer segments are WASTED
//!      from TARGET's perspective. At N=1 this predicts ~0% waste (the only
//!      dirty segment IS the target's); at N=8 it predicts ~7/8 = 87.5% waste.
//!   6. Join producers, drain residual, snapshot counters, report deltas.
//!
//! The expected qualitative shape: as N grows, the waste ratio grows (more
//! producer classes = more unrelated dirty segments visited per owner drain).
//! The exact ratio is timing-dependent (real threads), so the test reports the
//! measured ratio and asserts only the SANITY property that the ratio at N=8 is
//! strictly greater than at N=1 (monotonicity of waste with class-count).
//!
//! ## Feature gating
//!
//! Same gate as `tests/dirty_segments_a4.rs` (`alloc-global`,
//! `alloc-xthread`, `alloc-segment-directory`) plus `alloc-stats` (the new
//! counter's increment site) and `not(numa-aware)` (the drain itself is
//! compiled out under `numa-aware`).
//!
//! Under other feature configurations this file compiles as an empty test
//! binary (0 tests, pass by absence).

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-segment-directory",
    feature = "alloc-stats",
    not(feature = "numa-aware")
))]

extern crate sefer_alloc;

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};

// Serialise against other tests in this binary: the diagnostic counters are
// process-global statics shared across every AllocCore/HeapCore in the process,
// and the per-N delta arithmetic would be corrupted by a parallel sibling test
// bumping the same counters mid-run.
static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

/// Blocks of each producer class pre-allocated (and then remotely freed) per
/// measurement round. Tuned large enough to drive multiple drain cycles during
/// the producer burst (so the counter accumulation is meaningful) but small
/// enough to keep the whole N=1..8 sweep inside a few seconds wall-clock.
const BLOCKS_PER_CLASS: usize = 4_000;

/// Classes whose remote-free activity competes for the owner's drain attention.
/// These are DISTINCT from each other, AND chosen ABOVE the materialisation-
/// phase carve range (0..40) — so a block freed by producer i lands on a fresh
/// segment carved specifically for class i, whose ONLY ring activity is class i.
///
/// Indices chosen in the upper range of the size-class table (above the 40
/// materialisation-carve classes), all valid small classes under both the
/// baseline (49-class) and `medium-classes` (58-class) feature sets. Each
/// index must satisfy `idx < SMALL_CLASS_COUNT` for the active feature set.
const PRODUCER_CLASS_INDICES: &[usize] = &[40, 41, 42, 43, 44, 45, 46, 47];

/// The class the OWNER allocates continuously during the measured window. This
/// IS the FIRST producer class (`PRODUCER_CLASS_INDICES[0]`) — deliberately,
/// to model the review's scenario: the owner is an active CONSUMER of class A
/// (so its magazine refill drives `find_segment_with_free_impl(A)` →
/// `drain_dirty_segments` calls), while N-1 OTHER producer classes are
/// concurrently dirtying their own segments. The owner's drain visits ALL
/// dirty segments; drains of the TARGET segment are USEFUL (class A blocks in
/// its ring), drains of the other N-1 are WASTED from class A's perspective.
/// At N=1 this predicts ~0% waste (the only dirty segment IS the target's);
/// at N=8 it predicts ~7/8 = 87.5% waste.
const TARGET_CLASS: usize = 40;

/// Owner-thread batch size — accumulate this many TARGET_CLASS allocs without
/// self-freeing before draining the batch (own-thread free, off the ring path).
/// Same rationale as `tests/remote_fanin.rs` harness 1's `OWNER_BATCH`: keeps
/// `small_cur`'s own free list empty so every alloc falls through to
/// `find_segment_with_free_impl` and actually triggers `drain_dirty_segments`.
const OWNER_BATCH: usize = 4_096;

/// Minimum owner allocs per measurement round (lower bound on drain cycles
/// observed). The owner loop continues past this until every producer has
/// joined (matching `remote_fanin.rs`'s R6-REGRESSION-2 liveness fix).
const MIN_OWNER_ITERS: usize = 4_000;

#[derive(Clone, Copy, Default)]
struct CounterSnap {
    drained: u64,
    wasted: u64,
}

impl CounterSnap {
    fn snapshot() -> Self {
        Self {
            drained: AllocCore::dbg_dirty_segments_drained(),
            wasted: AllocCore::dbg_wasted_dirty_drains(),
        }
    }
}

/// Force the owner's `HeapCore` past the directory-materialise threshold by
/// carving one block of each of the first `threshold + slack` distinct classes.
/// Each distinct class carves its own first segment (small classes are
/// segment-pinned), so allocating one block each of classes 0..K produces ~K
/// segments and crosses `DIRECTORY_MATERIALIZE_THRESHOLD` (= 32) once K > 32.
/// Returns the live pointers (caller keeps them live for the rest of the
/// measurement so the segments don't get recycled by decommit). Materialisation
/// itself happens lazily inside `carve_block`'s `maybe_materialize_directory`
/// call — once `table.count() > threshold`, the sidecar is built. We do not
/// verify via a `dbg_*` read here because the HeapCore→AllocCore seam is
/// `pub(crate)`; instead the per-round delta on `dbg_dirty_segments_drained()`
/// being non-zero IS the verification (drain_dirty_segments is a no-op until
/// the sidecar is materialised, so a non-zero drain delta proves it happened).
fn materialise_directory(heap: *mut HeapCore) -> Vec<*mut u8> {
    let threshold = AllocCore::dbg_directory_materialize_threshold() as usize;
    let class_count = AllocCore::dbg_small_class_count();
    // Carve one block of each of the first `threshold + slack` classes. Cap at
    // `PRODUCER_CLASS_INDICES[0]` so the carve range stays clear of the
    // producer classes (which must carve fresh segments in Phase 1).
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

/// Run ONE measurement round with `n_producer_classes` concurrent producer
/// classes. Returns the (drained_delta, wasted_delta, owner_alloc_count) tuple.
fn run_round(n_producer_classes: usize) -> (u64, u64, usize) {
    assert!(
        n_producer_classes >= 1 && n_producer_classes <= PRODUCER_CLASS_INDICES.len(),
        "n_producer_classes out of range: {n_producer_classes}"
    );
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let heap_addr = heap as usize;

    // Phase 0: materialise the directory on the owner's heap (and keep those
    // segments live for the whole round so decommit doesn't recycle them).
    let _keep_alive = materialise_directory(heap);

    // Phase 1: pre-allocate BLOCKS_PER_CLASS blocks of each producer class.
    // Each class's blocks are owned by the owner's HeapCore (so the remote free
    // from the producer threads will route through dealloc_foreign_slow and
    // set the dirty bit on the owner's HeapSlot).
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

    // Phase 2: snapshot counters BEFORE the measured window.
    let before = CounterSnap::snapshot();

    // Phase 3: spawn producer threads + the owner's TARGET_CLASS alloc loop.
    let producers_done = Arc::new(AtomicBool::new(false));
    let producers_done_owner = Arc::clone(&producers_done);

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
                // Spread the frees across wall-clock time so the owner's
                // drain cycles actually overlap with producer activity
                // (without this, a single producer can push its entire burst
                // into HeapOverflow before the owner's first magazine refill,
                // yielding a vacuous 0-drain measurement at N=1).
                if i & 0x3F == 0 {
                    std::thread::yield_now();
                }
            }
            unsafe { HeapRegistry::recycle(remote_heap) };
        }));
    }

    // Owner concurrently allocates TARGET_CLASS blocks WITHOUT self-freeing
    // mid-batch (forcing find_segment_with_free_impl → drain_dirty_segments on
    // every miss — same harness rationale as remote_fanin.rs's OWNER_BATCH).
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
                // Own-thread free — does NOT touch the ring path, just refills
                // the current segment's free list. We want the free list to
                // drain AGAIN so subsequent allocs re-enter
                // find_segment_with_free_impl. Freeing into the current
                // segment's own list keeps the batch bounded.
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

    // Join producers, then owner.
    for h in handles {
        h.join().expect("producer thread must not panic");
    }
    producers_done.store(true, Ordering::Release);
    let owner_allocs = owner_handle.join().expect("owner thread must not panic");

    // Phase 4: snapshot counters AFTER, compute deltas.
    let after = CounterSnap::snapshot();
    let drained_delta = after.drained.saturating_sub(before.drained);
    let wasted_delta = after.wasted.saturating_sub(before.wasted);

    unsafe { HeapRegistry::recycle(heap) };

    (drained_delta, wasted_delta, owner_allocs)
}

/// The judge: run measurement rounds for N = 1, 2, 4, 8 producer classes and
/// print the waste-ratio table to stderr. Asserts only the SANITY property
/// that more producer classes yield strictly more waste (the qualitative
/// scaling the review predicted) — the absolute ratios are timing-dependent
/// and reported, not asserted.
// R14 hotfix (task #299): `class-aware-dirty` was promoted into `production`
// in R13-9 (`da77b38`), which invalidated this test's own "not reachable
// from the documented CI matrix" assumption below (see the comment on the
// assertion) — any CI step that tests plain `production` (or
// `--all-features`) now ALSO enables `class-aware-dirty`, tripping this
// EXPECTED-to-fail-under-that-feature assertion. Compiling the test out
// under the feature is the correct fix (the alternative — chasing every CI
// step with `--skip` — does not scale and already missed two steps: `test
// (gated bodies + all-features)`'s `production alloc-stats` step and its
// `--all-features` step, both red on `c7e7a79`/`cb24266`).
#[cfg(not(feature = "class-aware-dirty"))]
#[test]
fn r9_6_class_aware_dirty_waste_ratio_scales_with_class_count() {
    let _g = SerialGuard::acquire();

    let sweep: &[usize] = &[1, 2, 4, 8];
    let mut results: Vec<(usize, u64, u64, usize)> = Vec::with_capacity(sweep.len());

    eprintln!("\n═══════════════════════════════════════════════════════════════════");
    eprintln!("R9-6 class-aware dirty routing judge — wasted-drain measurement");
    eprintln!(
        "TARGET_CLASS = {TARGET_CLASS} (block_size = {} B); BLOCKS_PER_CLASS = {BLOCKS_PER_CLASS}",
        AllocCore::dbg_block_size(TARGET_CLASS)
    );
    eprintln!(
        "Producer class indices (each its own segment): {:?}",
        PRODUCER_CLASS_INDICES
    );
    eprintln!("═══════════════════════════════════════════════════════════════════");
    eprintln!(
        "{:>4}  {:>14}  {:>14}  {:>10}  {:>12}",
        "N", "drained_delta", "wasted_delta", "owner_alloc", "waste_ratio"
    );

    for &n in sweep {
        let (drained, wasted, owner_allocs) = run_round(n);
        let ratio = if drained == 0 {
            0.0
        } else {
            (wasted as f64) / (drained as f64)
        };
        eprintln!(
            "{:>4}  {:>14}  {:>14}  {:>10}  {:>11.1}%",
            n,
            drained,
            wasted,
            owner_allocs,
            ratio * 100.0
        );
        results.push((n, drained, wasted, owner_allocs));
    }
    eprintln!("═══════════════════════════════════════════════════════════════════\n");

    // Sanity assertion: waste at the largest N must exceed waste at N=1. This
    // is the qualitative scaling the review predicted (more concurrently-active
    // classes ⇒ larger fraction of drains are wasted from any one caller's
    // perspective). The absolute ratios are timing-dependent (real OS scheduler
    // jitter), so they are reported, not asserted to a fixed value.
    //
    // R12-7 stage 2 note: this assertion is scoped to the STAGE-1 baseline
    // (per-segment dirty routing, this file's whole reason for existing —
    // see its module doc). It is EXPECTED to fail if run with the
    // `class-aware-dirty` feature (R12-7 stage 2, `alloc_core::dirty_by_class`)
    // additionally enabled: the sought class's own drain visits become
    // (near-)exclusively useful once routing is class-scoped, so waste stays
    // near-zero at every N instead of scaling — see
    // `tests/class_aware_dirty_routing.rs::wasted_dirty_drains_stays_low_under_class_aware_routing`
    // for the equivalent measurement WITH the feature on. This file is
    // deliberately left unmodified otherwise (measurement-only judge,
    // unchanged base algorithm). R14 hotfix (task #299): `class-aware-dirty`
    // is now part of `production` (R13-9), so the interaction IS reachable
    // from plain `production`/`--all-features` — the whole test function is
    // compiled out under `#[cfg(not(feature = "class-aware-dirty"))]` above
    // instead of relying on CI-step `--skip` flags (which do not cover every
    // step that happens to enable `production`).
    let waste_for = |n: usize| -> u64 {
        results
            .iter()
            .find(|(cn, _, _, _)| *cn == n)
            .map(|(_, _, w, _)| *w)
            .unwrap_or(0)
    };
    let waste_n1 = waste_for(1);
    let waste_n8 = waste_for(8);
    assert!(
        waste_n8 > waste_n1,
        "waste at N=8 ({waste_n8}) should exceed waste at N=1 ({waste_n1}) — \
         the review's predicted scaling did not materialise. If this fires \
         honestly (the measurement is sound and waste truly does not scale), \
         that is evidence AGAINST implementing per-(segment,class) dirty \
         routing — see the report's recommendation."
    );
}
