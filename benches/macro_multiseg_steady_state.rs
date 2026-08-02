//! R32-9 (task #500) — the missing artifact `docs/perf/OPEN_ITEMS.md` item
//! 34 / `docs/perf/SPEEDUP_OPPORTUNITY_SURVEY_2026-07-31.md` finding F3 both
//! name: a realistic **≥64-live-segment, long-lived-process, mixed-size**
//! macro-benchmark run under `iai-callgrind`'s `Estimated Cycles`/RAM-hit
//! cache SIMULATION, not just raw `Ir`.
//!
//! ## Why this is a SEPARATE bench target, not another arm in `perf_gate_iai.rs`
//!
//! `perf_gate_iai.rs` is the fast, deterministic PER-COMMIT gate
//! (CLAUDE.md "Before every push") — every one of its ~80 arms deliberately
//! spans at most ~3 live segments and a tiny working set so the whole suite
//! stays fast under Valgrind emulation. This file is the OPPOSITE shape on
//! purpose: a slow, wide-working-set macro-bench meant to be run
//! occasionally (this task, and future tasks #501/#503/any F1 revisit), not
//! on every push. Keeping it a separate `[[bench]]` target means it never
//! taxes the per-commit gate's runtime and is independently discoverable by
//! name (`cargo bench --bench macro_multiseg_steady_state`).
//!
//! ## Workload design (per the survey's F3 "what would be needed" list)
//!
//! - **≥64 live segments concurrently.** `FLOOR_LARGE_OBJECTS = 80` distinct
//!   Large objects (each `LARGE_OBJ_BYTES` = `SMALL_MAX + PAGE`, i.e. just
//!   over the Small/Large boundary — one dedicated `SEGMENT` each) are
//!   allocated ONCE at setup and held live for the ENTIRE timed region —
//!   never freed until teardown. 80 is chosen with headroom past the 64
//!   threshold (25% margin) so the path-activation oracle's `>= 64` check
//!   has room to fail loudly if segment accounting regresses, rather than
//!   sitting exactly on the boundary. At `SEGMENT = 4 MiB`
//!   (`src/alloc_core/os.rs`), 80 segments is a ~320 MiB working set —
//!   genuinely too large to fit in a typical L2 (usually 0.25-2 MiB) or even
//!   many L3 caches (commonly 8-32 MiB) in full, though this bench does NOT
//!   assume any specific cache size: `iai-callgrind`'s cache SIMULATION model
//!   (a synthetic LRU model of L1/L2/LL, not real hardware counters) is what
//!   actually produces the `Estimated Cycles`/RAM-hit numbers, so whatever
//!   the simulated cache sizes are, 320 MiB categorically will not fit them.
//! - **Long-lived / steady-state, not a burst.** The 80-segment floor is
//!   established ONCE, then the TIMED region runs `CHURN_ROUNDS` rounds of
//!   mixed alloc/dealloc churn on top of that floor — segments accumulate
//!   and STAY live for the whole timed region (steady-state), matching the
//!   survey's explicit "not allocate N, free N in a burst" requirement.
//! - **Mixed Small/Large size distribution.** Each churn round does BOTH:
//!   (a) a Small-class churn step (16 B blocks, the same shape
//!   `perf_gate_iai.rs::small_churn_16b` uses, so this bench's Small half is
//!   directly comparable to the existing tiny-working-set gate), and (b) a
//!   Large-class churn step that allocates and immediately frees ONE
//!   dedicated-segment object (same `LARGE_OBJ_BYTES` size as the floor, so
//!   it exercises the SAME size class/cache-slot machinery the floor
//!   occupies) — this rotates through the large-cache's 8 base slots
//!   (`LARGE_CACHE_SLOTS`, `src/alloc_core/alloc_core.rs`) without ever
//!   touching the 80-segment floor itself.
//! - **Multi-thread variant.** `multiseg_steady_state_mt4` runs the SAME
//!   per-thread workload (floor + churn) on 4 threads simultaneously, each
//!   with its OWN `HeapCore` (via `HeapRegistry::claim`), for
//!   `320 MiB * 4 = 1.28 GiB` combined resident working set and genuine
//!   cross-thread registry/table contention. The single-thread variant
//!   (`multiseg_steady_state_1t`) stays as the cheaper baseline per the
//!   task's own staging suggestion ("a single-thread >=64-segment variant is
//!   also useful and cheaper to build first").
//!
//! ## Path-activation oracle (CLAUDE.md R30-8 rule)
//!
//! Per CLAUDE.md's rule ("the harness must prove it actually achieved its
//! target working-set shape... not just 'trust the allocation count'"):
//! after establishing the floor, every arm hard-asserts
//! `HeapCore::dbg_table_count() >= MIN_LIVE_SEGMENTS_ORACLE` (64) BEFORE
//! entering the timed churn region. `dbg_table_count` (added this task,
//! `src/registry/heap_core_diag.rs`) reads the segment table's registered
//! high-water count directly off the SAME `HeapCore` the churn workload
//! runs against — not a separate/inferred proxy — and because this
//! harness's floor objects are never freed during the assert window, the
//! high-water count and the true live count coincide exactly (see that
//! accessor's own doc comment for the general `<=` caveat and why it does
//! not apply here). A failed assert panics the bench with a clear message
//! rather than silently measuring an under-sized working set.
//!
//! `#[library_benchmark]` functions cannot easily return a value iai-callgrind
//! reports, so the oracle assert happens INSIDE the timed function, in the
//! untimed setup phase iai-callgrind's `#[bench::]`/plain form provides via
//! a `setup` argument — see `library_benchmark(setup = ...)` usage below,
//! which mirrors `perf_gate_iai.rs`'s existing convention of doing
//! pre-allocation work before the loop the same function measures (that
//! file's own module doc: "the entire annotated function body is measured
//! under Callgrind, there is no separate untimed setup"). Because this bench
//! deliberately does NOT try to isolate "floor setup" from "churn" cost (its
//! point is the STEADY-STATE cache behavior of churn ON TOP of an
//! already-established wide working set, not an isolated floor-construction
//! cost), the assert is cheap (one `u32` read) relative to the rest of the
//! timed body and does not materially distort the measured Ir/cycles.
//!
//! ## What FOUR prior rejected findings this unblocks (OPEN_ITEMS item 34)
//!
//! - **X5 (item 20)** — per-class segment-queue bitmap: REJECT at n=3
//!   because "maintenance RMW cost dominates; no cache line actually
//!   avoided at this scale." This harness's 80-segment floor is the
//!   precondition X5's own text named ("a future arc that adds a
//!   64-or-more-segment bench... may flip the verdict") — re-attempting
//!   X5's mechanism against `multiseg_steady_state_1t`/`_mt4` would let the
//!   RAM-hit counters show whether a segment-queue bitmap actually avoids a
//!   cache miss at this scale, which n=3 could not.
//! - **T10 (item 22)** — per-class "last found segment" hint: NO-GO at n=3
//!   for the same reason. Directly re-attemptable here.
//! - **R1 (item 23)** — per-segment availability hint: NO-GO, explicitly
//!   named "a genuine 64-or-more-segment bench... is the prerequisite for
//!   re-opening R1/X5/T10." Directly re-attemptable here.
//! - **R15-1 (item 9)** — nonempty-summary-word optimisation for
//!   `drain_dirty_segments`: rejected below its own noise floor at the
//!   current scale; its trigger is phrased as `MAX_SEGMENTS` raised by a
//!   large factor OR much-higher producer-class fan-in than N=8. This
//!   harness's 80-segment floor satisfies the segment-count half of that
//!   trigger directly. The fan-in half (producer-class count) is NOT
//!   addressed by this harness — it is a Small-class-count axis, not a
//!   segment-count axis, and would need its own dedicated fan-in sweep;
//!   noted here so a future task does not assume this harness alone
//!   re-opens R15-1 on BOTH axes, only the segment-count one.
//!
//! ## Run
//!
//! ```text
//! # Requires Linux + Valgrind (iai-callgrind is Linux-only, see
//! # perf_gate_iai.rs's own module doc for the full platform rationale).
//! cargo bench --bench macro_multiseg_steady_state --features "alloc-global bench-internals"
//! ```
//!
//! On Windows/macOS this target compiles to a no-op `fn main` (same pattern
//! as `perf_gate_iai.rs`) — it is never run outside Linux CI/a Linux dev box.

#![allow(clippy::missing_safety_doc)]

#[cfg(not(target_os = "linux"))]
fn main() {}

#[cfg(target_os = "linux")]
use core::alloc::Layout;
#[cfg(target_os = "linux")]
use std::hint::black_box;

#[cfg(target_os = "linux")]
use iai_callgrind::{library_benchmark, library_benchmark_group, main};

#[cfg(target_os = "linux")]
use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};
#[cfg(target_os = "linux")]
use sefer_alloc::SegmentLayout;

/// The path-activation oracle threshold (survey/OPEN_ITEMS item 34's own
/// named number).
#[cfg(target_os = "linux")]
const MIN_LIVE_SEGMENTS_ORACLE: u32 = 64;

/// Floor objects held live for the entire timed region — one dedicated
/// `SEGMENT` each (see module doc for the 25%-headroom rationale).
#[cfg(target_os = "linux")]
const FLOOR_LARGE_OBJECTS: usize = 80;

/// Number of mixed-churn rounds in the timed region. Each round does one
/// Small-class churn step + one Large-class churn step (see module doc).
/// Kept modest (Callgrind emulation is far slower than native execution,
/// same rationale `perf_gate_iai.rs`'s own `CHURN_OPS`/`COLD_BATCH` comments
/// give) — the POINT of this bench is the wide steady-state working set the
/// floor establishes, not a large absolute churn op count on top of it.
#[cfg(target_os = "linux")]
const CHURN_ROUNDS: usize = 32;

/// Small-class churn block size (matches `perf_gate_iai.rs::small_churn_16b`
/// so the Small half of this bench's cost is directly comparable to the
/// existing tiny-working-set gate's own number).
#[cfg(target_os = "linux")]
const SMALL_SIZE: usize = 16;

#[cfg(target_os = "linux")]
fn floor_layout() -> Layout {
    // Just over the Small/Large boundary: one dedicated SEGMENT per object,
    // same size class the Large churn step below also rotates through.
    let large_size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    Layout::from_size_align(large_size, SegmentLayout::PAGE).unwrap()
}

/// Establishes the floor and returns the claimed heap pointer plus the
/// floor pointers, which the timed function frees at its own end (freeing
/// isn't part of `setup` so the timed body's own steady-state teardown
/// cost — mirroring a real long-lived process eventually shutting down — is
/// part of what gets measured; see the two-shape split below for the
/// no-teardown variant that isolates PURE churn instead).
#[cfg(target_os = "linux")]
fn setup_single_thread_floor() -> (*mut HeapCore, Vec<*mut u8>) {
    let _ = bootstrap::ensure();
    let heap_ptr = HeapRegistry::claim();
    assert!(!heap_ptr.is_null(), "HeapRegistry::claim returned null");
    // SAFETY: `heap_ptr` was just returned by `claim`, owned by this thread
    // for the rest of this process (iai-callgrind runs each
    // `#[library_benchmark]` fn in its own fresh process).
    let heap = unsafe { &mut *heap_ptr };
    let layout = floor_layout();

    let mut floor = Vec::with_capacity(FLOOR_LARGE_OBJECTS);
    for i in 0..FLOOR_LARGE_OBJECTS {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "floor alloc null at i={i}");
        floor.push(p);
    }

    // PATH-ACTIVATION ORACLE: the floor must have actually registered
    // >= MIN_LIVE_SEGMENTS_ORACLE segments on THIS heap, read back from the
    // allocator's own diagnostic surface (not assumed from the FLOOR_LARGE_
    // OBJECTS constant alone — a bug in registration, pooling, or premature
    // recycling would silently under-fill the working set otherwise).
    let table_count = heap.dbg_table_count();
    assert!(
        table_count >= MIN_LIVE_SEGMENTS_ORACLE,
        "macro_multiseg_steady_state: path-activation oracle FAILED — \
         dbg_table_count()={table_count} < required {MIN_LIVE_SEGMENTS_ORACLE} \
         after allocating {FLOOR_LARGE_OBJECTS} floor objects; the >=64-live-\
         segment working set this bench exists to measure was NOT achieved"
    );

    (heap_ptr, floor)
}

// ---------------------------------------------------------------------------
// Single-thread steady-state churn on top of the >=64-segment floor.
// ---------------------------------------------------------------------------

/// Timed region: mixed Small+Large churn on top of an already-established
/// (and oracle-verified) >=64-segment floor. The floor itself is torn down
/// at the end of this SAME timed function — `iai-callgrind` times the whole
/// annotated body (no separate untimed measurement window), so this bench's
/// `Estimated Cycles`/RAM-hit numbers include one steady-state churn run
/// PLUS its own teardown, exactly mirroring a long-lived process that
/// eventually exits.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn multiseg_steady_state_1t() {
    let (heap_ptr, floor) = setup_single_thread_floor();
    // SAFETY: `heap_ptr` is live and owned by this thread for the rest of
    // this process.
    let heap = unsafe { &mut *heap_ptr };

    let small_layout = Layout::from_size_align(SMALL_SIZE, 8).unwrap();
    let large_layout = floor_layout();

    for _ in 0..CHURN_ROUNDS {
        // Small-class churn step (magazine/tcache fast path), same shape as
        // `perf_gate_iai.rs::small_churn_16b`.
        let p = heap.alloc(small_layout);
        black_box(p);
        if !p.is_null() {
            // SAFETY: `p` was just returned by `heap.alloc` with the same
            // layout, freed exactly once here.
            unsafe { heap.dealloc(p, small_layout) };
        }

        // Large-class churn step: allocate+free ONE dedicated-segment
        // object, rotating through the large-cache's base slots without
        // ever touching the floor objects held above.
        let lp = heap.alloc(large_layout);
        black_box(lp);
        if !lp.is_null() {
            // SAFETY: `lp` was just returned by `heap.alloc` with the same
            // layout, freed exactly once here.
            unsafe { heap.dealloc(lp, large_layout) };
        }
    }

    // Teardown: free the whole floor (a long-lived process eventually
    // shutting down), then recycle the heap.
    for p in floor {
        // SAFETY: `p` was allocated by `heap` with `large_layout` in
        // `setup_single_thread_floor`, still live, freed exactly once here.
        unsafe { heap.dealloc(p, large_layout) };
    }
    // SAFETY: `heap_ptr` was returned by `claim` in setup, not yet
    // recycled, no other thread touches it.
    unsafe { HeapRegistry::recycle(heap_ptr) };
}

// ---------------------------------------------------------------------------
// Multi-thread (4 threads) steady-state churn — same per-thread workload,
// genuine cross-thread registry/table contention.
// ---------------------------------------------------------------------------

#[cfg(target_os = "linux")]
const MT_THREAD_COUNT: usize = 4;

#[cfg(target_os = "linux")]
fn per_thread_work() {
    let (heap_ptr, floor) = setup_single_thread_floor();
    // SAFETY: `heap_ptr` is live and owned by this thread until `recycle`
    // at the end of this function.
    let heap = unsafe { &mut *heap_ptr };

    let small_layout = Layout::from_size_align(SMALL_SIZE, 8).unwrap();
    let large_layout = floor_layout();

    for _ in 0..CHURN_ROUNDS {
        let p = heap.alloc(small_layout);
        black_box(p);
        if !p.is_null() {
            // SAFETY: `p` was just returned by `heap.alloc` with the same
            // layout, freed exactly once here.
            unsafe { heap.dealloc(p, small_layout) };
        }

        let lp = heap.alloc(large_layout);
        black_box(lp);
        if !lp.is_null() {
            // SAFETY: `lp` was just returned by `heap.alloc` with the same
            // layout, freed exactly once here.
            unsafe { heap.dealloc(lp, large_layout) };
        }
    }

    for p in floor {
        // SAFETY: `p` was allocated by `heap` with `large_layout` above,
        // still live, freed exactly once here.
        unsafe { heap.dealloc(p, large_layout) };
    }
    // SAFETY: `heap_ptr` was returned by `claim` above, not yet recycled,
    // no other thread touches it.
    unsafe { HeapRegistry::recycle(heap_ptr) };
}

/// Multi-thread variant: `MT_THREAD_COUNT` (4) threads, each running the
/// IDENTICAL per-thread workload `multiseg_steady_state_1t` runs alone —
/// own `HeapCore`, own >=64-segment floor (oracle-verified per thread),
/// own churn loop. Combined resident working set: `MT_THREAD_COUNT *
/// FLOOR_LARGE_OBJECTS * SEGMENT` = `4 * 80 * 4 MiB` = 1.28 GiB. Exercises
/// genuine cross-thread contention on the shared segment-table/registry
/// substrate that the single-thread variant cannot.
#[cfg(target_os = "linux")]
#[library_benchmark]
fn multiseg_steady_state_mt4() {
    let handles: Vec<_> = (0..MT_THREAD_COUNT)
        .map(|_| std::thread::spawn(per_thread_work))
        .collect();
    for h in handles {
        h.join()
            .unwrap_or_else(|e| panic!("worker thread panicked: {e:?}"));
    }
}

#[cfg(target_os = "linux")]
library_benchmark_group!(
    name = macro_multiseg_steady_state_group;
    benchmarks = multiseg_steady_state_1t, multiseg_steady_state_mt4,
);

#[cfg(target_os = "linux")]
main!(library_benchmark_groups = macro_multiseg_steady_state_group);
