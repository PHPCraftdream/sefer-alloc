//! R12-7 stage 2 (`class-aware-dirty`, EXPERIMENTAL) — correctness tests for
//! the per-(segment, class) dirty-bit sidecar (`alloc_core::dirty_by_class`).
//!
//! ## What this file proves
//!
//! 1. **`class_a_refill_reclaims_class_b_entries_in_the_same_pass`** — the
//!    central lost-wakeup counterfactual. Constructs the exact scenario the
//!    task brief calls out: a single per-segment ring holds entries of BOTH
//!    class A and class B; the owner's refill is triggered by a search for
//!    class A. Asserts class B's blocks are ALSO reclaimed (visible to a
//!    subsequent class-B allocation) by the SAME drain pass that class A's
//!    search triggered — proving the design decision (drain body stays
//!    full-ring regardless of which class's bit triggered the visit) holds in
//!    the real production path, not just in the module doc's argument.
//!
//! 2. **`naive_partial_drain_would_lose_class_b_entry` (counterfactual,
//!    `#[should_panic]`)** — a STANDALONE model (not the production dirty
//!    bitmap machinery) of the REJECTED alternative design: a per-class
//!    dirty-drain that reads only ITS OWN class's ring slots instead of
//!    draining the whole ring. Proves that design would genuinely lose a
//!    class-B entry when triggered only by class A's search — the concrete
//!    justification for why this implementation's drain body is
//!    unconditionally full-ring (option (a) from the task brief) rather than
//!    a genuinely partial per-class drain. Mirrors this project's established
//!    "standalone broken model, not a hypothetical" counterfactual discipline
//!    (see `tests/loom_dirty_publish.rs`'s
//!    `counterfactual_non_atomic_push_loses_entry`).
//!
//! 3. **`wasted_dirty_drains_stays_low_under_class_aware_routing`** — reruns
//!    `tests/r9_6_class_aware_dirty_judge.rs`'s exact N=1/2/4/8 mixed-class
//!    workload shape with `class-aware-dirty` ON, and asserts
//!    `dbg_wasted_dirty_drains()` stays near-zero even at N=8 (where the
//!    baseline — R9-6's report, §6 — measured ~95% waste). This is the
//!    end-to-end proof that the class-scoped scan actually eliminates the
//!    wasted visits the whole feature exists to avoid, using the SAME
//!    diagnostic counter the stage-1 gate's judge used, not a new metric.
//!
//! ## Feature gating
//!
//! `alloc-global`, `alloc-xthread`, `alloc-segment-directory`,
//! `class-aware-dirty`, `alloc-stats` (diagnostic counters), `not(numa-aware)`
//! (the directory-driven drain is compiled out under `numa-aware`). Under
//! other feature configurations this file compiles as an empty test binary
//! (0 tests, pass by absence).

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-segment-directory",
    feature = "class-aware-dirty",
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

// Serialise against sibling tests in this binary: the diagnostic counters
// (`dbg_wasted_dirty_drains` / `dbg_dirty_segments_drained`) are process-wide
// statics — same rationale as `tests/r9_6_class_aware_dirty_judge.rs`'s
// `SerialGuard`.
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

// =========================================================================
// Test 1: a class-A-triggered drain reclaims class-B entries in the SAME
// pass (the lost-wakeup counterfactual, exercised against the REAL
// production path).
// =========================================================================

/// Force the owner's `HeapCore` past `DIRECTORY_MATERIALIZE_THRESHOLD` by
/// carving one block of each of the first `threshold + slack` distinct
/// classes, staying clear of `carve_ceiling`. Mirrors
/// `tests/r9_6_class_aware_dirty_judge.rs::materialise_directory`.
fn materialise_directory(heap: *mut HeapCore, carve_ceiling: usize) -> Vec<*mut u8> {
    let threshold = AllocCore::dbg_directory_materialize_threshold() as usize;
    let target = (threshold + 8).min(carve_ceiling);
    assert!(
        target > threshold,
        "size-class table too small for materialisation carve (need > {threshold} classes below producer range, have {carve_ceiling})"
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

/// Class A and class B both free blocks into the SAME owner segment (via a
/// remote producer thread), interleaved so both classes' ring entries land in
/// the same per-segment `RemoteFreeRing` before the owner's next alloc. The
/// owner then allocates ONLY class A (forcing `find_segment_with_free_impl
/// (class_a)` -> `drain_dirty_segments(class_a)`, which — under
/// `class-aware-dirty` — scans class A's per-class word slice, NOT class B's).
/// Asserts a subsequent class-B allocation is satisfied WITHOUT a fresh
/// segment carve (i.e. class B's cross-thread-freed block WAS reclaimed by
/// the class-A-triggered drain, not left stranded).
#[test]
fn class_a_refill_reclaims_class_b_entries_in_the_same_pass() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    const CLASS_A: usize = 40;
    const CLASS_B: usize = 41;
    // Large enough that, combined with the trigger-batch below, the magazine
    // genuinely misses for class A (rather than the whole scenario being
    // silently satisfied from magazine-cached blocks, which would make the
    // lost-wakeup assertion below vacuous). Sized with headroom above the
    // minimum needed (24) so a stray mid-loop segment rollover under host
    // load still leaves a healthy `pair_count` (see the segment-bucketing
    // comment below) rather than shrinking the trigger batch to near-zero.
    const N_PER_CLASS: usize = 40;

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let heap_addr = heap as usize;

    let _keep_alive = materialise_directory(heap, CLASS_A);

    // `materialise_directory`'s "one block of each distinct class" loop does
    // NOT by itself cross `DIRECTORY_MATERIALIZE_THRESHOLD`: small-class
    // segments are class-agnostic (any class can carve from the current
    // shared bump segment, `small_cur` -- there is no per-class segment
    // pinning anywhere in the carve path), so 40 distinct classes' magazine-
    // refill batches all comfortably fit in ONE 4 MiB segment. Force real
    // segment-count volume by allocating (and own-thread-freeing) many
    // blocks of a mid-range class, which rolls through segments quickly.
    // Freeing them immediately keeps this heap's LIVE footprint small while
    // still bumping `SegmentTable::count()` (the table only grows on segment
    // RESERVE, never shrinks on an own-thread free within the same segment)
    // past the threshold.
    {
        // A mid-range class (not the smallest) keeps `blocks_needed` (and
        // this loop's wall-clock cost) reasonable while still comfortably
        // rolling through 32+ segments -- the smallest class needs far more
        // iterations (tiny blocks -> huge per-segment block count) for the
        // same segment-count effect.
        let filler_class = 20usize;
        let filler_size = AllocCore::dbg_block_size(filler_class);
        let filler_layout =
            Layout::from_size_align(filler_size, 8).expect("filler class block size is valid");
        // Each 4 MiB segment holds roughly SEGMENT / filler_size blocks;
        // comfortably over-provision to guarantee >= DIRECTORY_MATERIALIZE_THRESHOLD
        // (32) distinct segment reservations regardless of per-segment
        // metadata overhead.
        const SEGMENT_BYTES: usize = 1 << 22;
        let threshold = AllocCore::dbg_directory_materialize_threshold() as usize;
        let blocks_needed = (threshold + 8) * (SEGMENT_BYTES / filler_size + 1);
        let mut filler: Vec<*mut u8> = Vec::with_capacity(blocks_needed);
        for _ in 0..blocks_needed {
            let p = unsafe { (*heap).alloc(filler_layout) };
            assert!(!p.is_null(), "filler-class alloc returned null");
            filler.push(p);
        }
        for p in filler {
            unsafe { (*heap).dealloc(p, filler_layout) };
        }
    }

    let layout_a =
        Layout::from_size_align(AllocCore::dbg_block_size(CLASS_A), 8).expect("valid layout");
    let layout_b =
        Layout::from_size_align(AllocCore::dbg_block_size(CLASS_B), 8).expect("valid layout");

    // Owner allocates class A and class B INTERLEAVED (one of each per
    // round) rather than "all of A, then all of B" -- interleaving keeps
    // both classes carving from the SAME currently-active `small_cur` for
    // as much of the loop as possible, which is far more robust under host
    // load / scheduling variance than "all of A first" (which risks A's OWN
    // batch rolling `small_cur` over to a fresh segment before B ever
    // starts). Even so, under enough concurrent host load a segment CAN
    // roll over mid-loop, or even on nearly every iteration under heavy
    // contention (measured: occasional under parallel `cargo test` load) --
    // rather than requiring a FIXED batch to land together (fragile), keep
    // allocating rounds, bucket every pointer by its actual segment, and
    // stop once ANY segment accumulates at least `MIN_PAIRS` co-located
    // (class A, class B) pairs, up to a generous round cap. This tolerates
    // an arbitrary rollover RATE (not just an occasional stray one) by
    // simply allocating more until enough of both classes land together.
    const MIN_PAIRS: usize = 20;
    const MAX_ROUNDS: usize = 200;
    const SEGMENT: usize = 1 << 22;
    let seg_of = |p: usize| p & !(SEGMENT - 1);

    let mut by_segment: std::collections::HashMap<usize, (Vec<usize>, Vec<usize>)> =
        std::collections::HashMap::new();
    let mut rounds = 0usize;
    let (chosen_a, chosen_b) = loop {
        for _ in 0..N_PER_CLASS {
            let pa = unsafe { (*heap).alloc(layout_a) };
            assert!(!pa.is_null(), "class-A pre-alloc returned null");
            by_segment
                .entry(seg_of(pa as usize))
                .or_default()
                .0
                .push(pa as usize);
            let pb = unsafe { (*heap).alloc(layout_b) };
            assert!(!pb.is_null(), "class-B pre-alloc returned null");
            by_segment
                .entry(seg_of(pb as usize))
                .or_default()
                .1
                .push(pb as usize);
        }
        rounds += 1;
        let best_seg = by_segment
            .iter()
            .max_by_key(|(_, (a, b))| a.len().min(b.len()))
            .map(|(&seg, (a, b))| (seg, a.len().min(b.len())));
        if let Some((seg, pairs)) = best_seg {
            if pairs >= MIN_PAIRS {
                let (a, b) = by_segment.remove(&seg).unwrap();
                break (a, b);
            }
        }
        assert!(
            rounds < MAX_ROUNDS,
            "after {rounds} rounds ({} class-A / {} class-B blocks total), no \
             single segment accumulated {MIN_PAIRS} co-located (class A, class B) \
             pairs -- this test requires both classes to share a segment's \
             remote-free ring to exercise the lost-wakeup scenario; the \
             allocator's carve policy must have changed",
            rounds * N_PER_CLASS,
            rounds * N_PER_CLASS,
        );
    };
    // Use the SAME count from each side (paired 1:1 below) so the trigger
    // batch / re-issue counts stay exact.
    let pair_count = chosen_a.len().min(chosen_b.len());
    let a_ptrs: Vec<usize> = chosen_a[..pair_count].to_vec();
    let b_ptrs: Vec<usize> = chosen_b[..pair_count].to_vec();

    // Snapshot the class-B address set BEFORE `b_ptrs` moves into the
    // producer closure below — used as the lost-wakeup oracle after the
    // class-A-triggered drain.
    let b_ptr_set: std::collections::HashSet<usize> = b_ptrs.iter().copied().collect();

    // A remote producer thread frees the chosen (co-located) class-A and
    // class-B blocks, interleaved, via the real cross-thread free path —
    // landing entries of BOTH classes in the SAME segment's ring (dirtying
    // both the per-segment bit AND, under `class-aware-dirty`, each class's
    // own per-class bit).
    let producer = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
        for i in 0..pair_count {
            let pa = a_ptrs[i] as *mut u8;
            unsafe { (*remote_heap).dealloc(pa, layout_a) };
            let pb = b_ptrs[i] as *mut u8;
            unsafe { (*remote_heap).dealloc(pb, layout_b) };
        }
        unsafe { HeapRegistry::recycle(remote_heap) };
    });
    producer.join().expect("producer thread must not panic");

    // Owner allocates ONLY class A — this is the search that triggers
    // `drain_dirty_segments(CLASS_A)`. Under `class-aware-dirty`, the scan
    // reads ONLY class A's per-class word slice; class B's per-class bit is
    // never consulted by this call. The drain body itself is still
    // full-ring, so class B's entries in the SAME ring are reclaimed too.
    let owner_heap = heap_addr as *mut HeapCore;

    // Force a GENUINE magazine miss for class A: allocate a batch WITHOUT
    // freeing in between (an interleaved own-thread free would immediately
    // replenish the same segment's free list from the just-freed block,
    // masking a real miss) so the magazine's cache for class A is exhausted
    // and the search reaches `find_segment_with_free_impl` ->
    // `drain_dirty_segments(CLASS_A)` -- the trigger this test exists to
    // exercise. A single `alloc` call is NOT sufficient here: it may be
    // satisfied entirely from magazine-cached blocks left over from
    // materialisation, never reaching the drain at all.
    let drained_before = AllocCore::dbg_dirty_segments_drained();
    let mut trigger_batch: Vec<*mut u8> = Vec::with_capacity(32);
    for _ in 0..32 {
        let p = unsafe { (*owner_heap).alloc(layout_a) };
        assert!(!p.is_null(), "class-A trigger-batch alloc returned null");
        trigger_batch.push(p);
    }
    let drained_after = AllocCore::dbg_dirty_segments_drained();
    assert!(
        drained_after > drained_before,
        "the class-A trigger batch never reached drain_dirty_segments \
         (drained_before={drained_before}, drained_after={drained_after}) -- \
         this test is vacuous unless a genuine magazine miss occurs"
    );
    for p in trigger_batch {
        unsafe { (*owner_heap).dealloc(p, layout_a) };
    }

    // The critical assertion: every one of the co-located class-B
    // cross-thread-freed addresses must be re-issuable by allocating
    // `pair_count` class-B blocks NOW (immediately after the class-A-
    // triggered drain, with no intervening class-B-triggered drain) --
    // proving those blocks were reclaimed into the BinTable by the class-A
    // visit, not left stranded in the ring. A direct pointer-identity oracle
    // (not a segment-reservation-counter proxy, which a pool/decommit cache
    // could satisfy from EITHER a genuinely-reclaimed block OR an unrelated
    // previously-pooled segment, making that proxy ambiguous).
    let mut reissued: Vec<*mut u8> = Vec::with_capacity(pair_count);
    let mut matched_original = 0usize;
    for _ in 0..pair_count {
        let p = unsafe { (*owner_heap).alloc(layout_b) };
        assert!(
            !p.is_null(),
            "class-B alloc after class-A-triggered drain returned null"
        );
        if b_ptr_set.contains(&(p as usize)) {
            matched_original += 1;
        }
        reissued.push(p);
    }
    assert_eq!(
        matched_original, pair_count,
        "only {matched_original}/{pair_count} class-B allocations after the class-A-triggered \
         drain reused one of the original cross-thread-freed addresses -- the remainder were \
         satisfied from FRESH segment carves, proving class B's cross-thread-freed blocks were \
         lost by the class-A drain visit (lost-wakeup regression)"
    );
    for p in reissued {
        unsafe { (*owner_heap).dealloc(p, layout_b) };
    }

    unsafe { HeapRegistry::recycle(heap) };
}

// =========================================================================
// Test 2 (counterfactual, #[should_panic]): a NAIVE partial-ring-drain
// design (NOT this implementation) genuinely loses a class-B entry when
// triggered only by class A. Standalone model, mirroring
// `tests/loom_dirty_publish.rs`'s established counterfactual discipline.
// =========================================================================

/// A tiny standalone model of a per-segment ring holding 2 entries of
/// DIFFERENT classes, plus a per-class dirty bitmap covering 2 classes.
/// Models the REJECTED alternative design: instead of draining the WHOLE
/// ring once a segment is visited (this implementation's actual behaviour),
/// a "genuinely partial" drain reads ONLY the ring slots it can attribute to
/// the sought class — which requires per-entry class tagging the ring
/// protocol does not expose without a full drain pass, so the naive design
/// approximates this by draining slots in FIFO order but STOPPING as soon as
/// it has reclaimed one entry of the sought class (a plausible "optimise for
/// the common case" bug: someone might implement "stop early once I found
/// what I was looking for").
struct NaivePartialDrainModel {
    // Ring: 2 slots, FIFO. `(class, offset)`.
    ring: Vec<(usize, u32)>,
}

impl NaivePartialDrainModel {
    fn new() -> Self {
        Self { ring: Vec::new() }
    }

    fn push(&mut self, class: usize, offset: u32) {
        self.ring.push((class, offset));
    }

    /// The REJECTED naive drain: stop as soon as an entry of `sought_class`
    /// is found and reclaimed, leaving any later ring entries (of OTHER
    /// classes) UNDRAINED and their dirty bits UNCLEARED for classes that
    /// happened to be behind the sought entry in FIFO order.
    fn naive_partial_drain(&mut self, sought_class: usize) -> Vec<(usize, u32)> {
        let mut reclaimed = Vec::new();
        let mut i = 0;
        while i < self.ring.len() {
            let (class, offset) = self.ring[i];
            if class == sought_class {
                reclaimed.push((class, offset));
                self.ring.remove(i);
                // BUG: stop after finding the sought class, instead of
                // draining the rest of the ring.
                break;
            }
            i += 1;
        }
        reclaimed
    }
}

/// Push class-B THEN class-A into the same ring (class B is FIRST in FIFO
/// order — i.e. it is NOT the sought class and sits ahead of the entry the
/// naive drain is looking for). A search for class A finds and reclaims
/// class A immediately (it's the head-adjacent match after the scan), and
/// the naive "stop early" drain never revisits class B's entry — modelling a
/// permanently-lost entry the SAME way `loom_dirty_publish.rs`'s
/// non-atomic-push counterfactual models a lost ring slot.
///
/// `#[should_panic]` because the naive model DOES lose the class-B entry —
/// proving this counterfactual is non-vacuous, i.e. the design property this
/// implementation actually upholds (full-ring drain regardless of trigger
/// class) is doing real work, not a redundant safety margin.
#[test]
#[should_panic(expected = "naive partial drain lost")]
fn naive_partial_drain_would_lose_class_b_entry() {
    let mut model = NaivePartialDrainModel::new();
    // Class B pushed first (offset 100), class A pushed second (offset 200).
    model.push(1 /* class B */, 100);
    model.push(0 /* class A */, 200);

    // A search for class A (sought_class = 0) triggers the naive drain.
    let reclaimed = model.naive_partial_drain(0);

    // The naive drain found class A (offset 200) but, per its "stop early"
    // bug, never looked at class B's entry (offset 100) — it remains
    // stranded in `model.ring` forever (no later class-A search will ever
    // revisit it, since the naive drain always stops at its first match).
    let found_class_b = reclaimed.iter().any(|&(c, _)| c == 1);
    assert!(
        found_class_b,
        "naive partial drain lost class B's entry (offset 100): the search \
         for class A (0) reclaimed only {reclaimed:?}, leaving class B's \
         entry permanently stranded in the ring -- this is exactly the \
         lost-wakeup hazard this implementation's full-ring-drain design \
         avoids by construction (see `alloc_core::dirty_by_class`'s module \
         doc)"
    );
}

// =========================================================================
// Test 3: end-to-end — wasted_dirty_drains stays near-zero under
// class-aware routing, on the SAME mixed-class workload shape the R9-6
// judge used to measure ~82%/~95% waste at N=4/N=8 under the baseline.
// =========================================================================

const PRODUCER_CLASS_INDICES: &[usize] = &[40, 41, 42, 43, 44, 45, 46, 47];
const TARGET_CLASS: usize = 40;
const BLOCKS_PER_CLASS: usize = 800;
const OWNER_BATCH: usize = 512;
const MIN_OWNER_ITERS: usize = 800;

fn run_round(n_producer_classes: usize) -> (u64, u64) {
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let heap_addr = heap as usize;

    let _keep_alive = materialise_directory(heap, PRODUCER_CLASS_INDICES[0]);

    let producer_classes = &PRODUCER_CLASS_INDICES[..n_producer_classes];
    let mut blocks_per_producer: Vec<Vec<usize>> = Vec::with_capacity(n_producer_classes);
    for &cls in producer_classes {
        let block_size = AllocCore::dbg_block_size(cls);
        let layout = Layout::from_size_align(block_size, 8).expect("valid layout");
        let mut v: Vec<usize> = Vec::with_capacity(BLOCKS_PER_CLASS);
        for _ in 0..BLOCKS_PER_CLASS {
            let p = unsafe { (*heap).alloc(layout) };
            assert!(!p.is_null(), "producer-class pre-alloc returned null");
            v.push(p as usize);
        }
        blocks_per_producer.push(v);
    }

    let drained_before = AllocCore::dbg_dirty_segments_drained();
    let wasted_before = AllocCore::dbg_wasted_dirty_drains();

    let producers_done = Arc::new(AtomicBool::new(false));
    let producers_done_owner = Arc::clone(&producers_done);

    let mut handles = Vec::with_capacity(n_producer_classes);
    for (i, &cls) in producer_classes.iter().enumerate() {
        let addrs = std::mem::take(&mut blocks_per_producer[i]);
        let block_size = AllocCore::dbg_block_size(cls);
        let layout = Layout::from_size_align(block_size, 8).expect("valid layout");
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
    let target_layout = Layout::from_size_align(target_block_size, 8).expect("valid layout");
    let target_heap_addr = heap_addr;
    let owner_handle: thread::JoinHandle<()> = thread::spawn(move || {
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
    });

    for h in handles {
        h.join().expect("producer thread must not panic");
    }
    producers_done.store(true, Ordering::Release);
    owner_handle.join().expect("owner thread must not panic");

    let drained_after = AllocCore::dbg_dirty_segments_drained();
    let wasted_after = AllocCore::dbg_wasted_dirty_drains();

    unsafe { HeapRegistry::recycle(heap) };

    (
        drained_after.saturating_sub(drained_before),
        wasted_after.saturating_sub(wasted_before),
    )
}

/// Reruns the R9-6 judge's exact mixed-class workload shape (N producer
/// classes concurrently freeing into a shared owner while the owner
/// continuously allocates TARGET_CLASS = the first producer class) with
/// `class-aware-dirty` ON, at N=8 (the point where the baseline measured
/// ~95% waste — R9-6 report §6). Asserts the waste ratio stays LOW: the
/// class-scoped scan should make the vast majority of drain visits useful
/// (the sought class's own bit only gets set by entries of that class), not
/// just "less than the baseline" but genuinely near-zero, modulo the same
/// small noise floor the R9-6 report's N=1 point already documents (a racy
/// drain of a just-cleared target segment, or residual cross-class ring
/// sharing when two classes happen to carve the same segment).
#[test]
fn wasted_dirty_drains_stays_low_under_class_aware_routing() {
    let _g = SerialGuard::acquire();

    let (drained, wasted) = run_round(8);

    let ratio = if drained == 0 {
        0.0
    } else {
        (wasted as f64) / (drained as f64)
    };
    eprintln!(
        "class-aware-dirty N=8: drained_delta={drained} wasted_delta={wasted} ratio={:.1}%",
        ratio * 100.0
    );

    // R9-6's baseline measured ~95% waste at N=8 (report §6). Under
    // class-aware routing the scan only visits segments dirty for the sought
    // class, so the waste ratio should be a small fraction of that — 20% is
    // a generous ceiling (well above any expected noise) that still clearly
    // falsifies "class-aware routing has no effect" (which would read ~95%,
    // matching the baseline) while tolerating real-world scheduler jitter
    // and the rare cross-class same-segment carve.
    assert!(
        ratio < 0.20,
        "class-aware-dirty waste ratio at N=8 was {:.1}% (drained={drained}, wasted={wasted}) \
         -- expected well under the R9-6 baseline's ~95%, since the class-scoped scan should \
         only visit segments actually dirty for the sought class",
        ratio * 100.0
    );
}
