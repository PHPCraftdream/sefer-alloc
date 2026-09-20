//! G3 (P2) regression — the batch allocation API must not bypass
//! `drain_large_deferred_free`.
//!
//! Pre-fix, `alloc_batch` classified its layout and dispatched Large batches
//! through a dedicated path that never called `drain_large_deferred_free`, so
//! a batch-only owner thread whose blocks are freed from ANOTHER thread (each
//! cross-thread free lands the block on the OWNER's deferred-free stack)
//! accumulated one held SegmentTable slot per cycle: the owner's high-water
//! table count grew O(cycles) instead of staying bounded. The fix (in
//! `src/registry/heap_core/alloc/batch.rs`) makes `alloc_batch_large` drain the
//! deferred-free stack, and makes the non-fastbin `alloc_batch` classify once
//! and delegate Large batches to `alloc_batch_large` (carrying the
//! `drain_heap_overflow` prelude for Small batches).
//!
//! Counterfactual: against the pre-fix code the high-water assertion below
//! fails at roughly `baseline + CYCLES * BATCH_SIZE` (+256, ≫ the bound of 8).
//!
//! This file complements `tests/batch_tcache.rs`, which covers only the
//! Small/same-thread batch scenario; THIS file covers the Large cross-thread
//! batch scenario it does not.
//!
//! Path coverage via feature configs: the file compiles under both
//! `--features "production batch-api internals"` (fastbin build: the single
//! test exercises the fastbin `alloc_batch` → `alloc_batch_large` path) and
//! `--features "alloc-global alloc-xthread batch-api internals"`
//! (non-fastbin build: the SAME test exercises the non-fastbin
//! `alloc_batch` Large-delegation path). Additional non-fastbin tests cover
//! Small overflow draining from both Small and Large batch entry points.
//!
//! Oracles:
//! - Leak oracle: `HeapCore::dbg_table_count` (the SegmentTable's high-water
//!   registered-slot count for THIS claimed heap only — the test thread's
//!   libtest/TLS allocations go to a different slot). Post-fix bound:
//!   `<= baseline + HIGH_WATER_GROWTH_BOUND`.
//! - Path-activation oracle (only under `#[cfg(not(feature =
//!   "alloc-decommit"))]`): the process-wide `AllocCore::dbg_segments_released_total()`
//!   must advance by `>= CYCLES / 2` over the loop — proof the drain actually
//!   FIRED (without decommit, `reclaim_large_segment` releases each reclaimed
//!   segment to the OS immediately; WITH decommit they go to the bounded large
//!   cache instead and `released_total` legitimately stays flat, so the check
//!   is disabled there). This counter is corroboration only: unrelated tests
//!   can increase it. The per-heap high-water bound is the regression oracle.
//!
//! The owner ALLOCATES via the claimed `HeapCore::alloc_batch` — the exact
//! code under test, and the only way the high-water oracle reads the table
//! the blocks actually register in — while the foreign reaper FREES via
//! `SeferAlloc::dealloc`, so the free takes the real cross-thread routing
//! onto the owner's deferred-free stack.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "batch-api",
    feature = "internals"
))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::mpsc;

use sefer_alloc::registry::{bootstrap, HeapRegistry};
use sefer_alloc::SeferAlloc;
// Only used by the path-activation oracle, which is meaningful without
// `alloc-decommit` (see module doc).
#[cfg(not(feature = "alloc-decommit"))]
use sefer_alloc::AllocCore;

// Each file in `tests/` is its own binary, so installing the global allocator
// here is isolated from the rest of the suite.
#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// Number of batches; without reclamation each retains BATCH_SIZE table slots.
const CYCLES: usize = 64;
const BATCH_SIZE: usize = 4;

/// How much the claimed heap's SegmentTable high-water may legitimately grow
/// above baseline despite the drain (headroom for table-slot reuse noise).
const HIGH_WATER_GROWTH_BOUND: u32 = 8;

/// `*mut u8` is not `Send`; the reaper only ever receives a block the owner
/// has fully published (tag written, channel send is a happens-before edge)
/// and frees it exactly once through the global allocator, so sending it is
/// sound (same idiom as `tests/race_norecycle.rs`).
struct SendPtr(*mut u8);
// SAFETY: ownership crosses the channel once; only the receiver accesses/frees it.
unsafe impl Send for SendPtr {}

#[test]
fn batch_large_cross_thread_free_high_water_bounded() {
    let _ = bootstrap::ensure();
    let heap_ptr = HeapRegistry::claim();
    assert!(!heap_ptr.is_null(), "HeapRegistry::claim returned null");
    // SAFETY: `heap_ptr` was just returned by `claim` and is owned by this
    // thread until `recycle` at the end of this test.
    let heap = unsafe { &mut *heap_ptr };

    // Large-classified: `class_for` returns None for this size, which is the
    // exact configuration that hits the G3 batch path.
    let large_size = sefer_alloc::SegmentLayout::SMALL_MAX + sefer_alloc::SegmentLayout::PAGE;
    let layout = Layout::from_size_align(large_size, sefer_alloc::SegmentLayout::PAGE).unwrap();

    // High-water registered-slot count for THIS claimed heap only.
    let baseline = heap.dbg_table_count();

    // Path-activation snapshot (see module doc: only meaningful without
    // `alloc-decommit`).
    #[cfg(not(feature = "alloc-decommit"))]
    let released_before = AllocCore::dbg_segments_released_total();

    // ONE long-lived reaper thread: reads each block's tag back (catches
    // reclaim-while-in-flight), frees it through the GLOBAL allocator (the
    // real cross-thread routing that lands the block on the OWNER's
    // deferred-free stack), then acks so the owner only starts the next
    // cycle after the previous block was fully freed.
    let (tx, rx) = mpsc::sync_channel::<SendPtr>(1);
    let (ack_tx, ack_rx) = mpsc::sync_channel::<()>(1);
    let reaper = std::thread::spawn(move || {
        for SendPtr(ptr) in rx {
            // SAFETY: the owner wrote a u64 tag at the start of this block
            // before sending it; the channel send provides the
            // happens-before edge, so this read is race-free and in bounds.
            let tag = unsafe { std::ptr::read_volatile(ptr.cast::<u64>()) };
            assert_eq!(
                tag, 0xCAFE_0000_0000_0001,
                "tag corruption: block reclaimed while in flight?"
            );
            // SAFETY: `ptr` was allocated by the claimed heap via
            // `alloc_batch` with `layout` and is freed exactly once here.
            unsafe { GLOBAL.dealloc(ptr, layout) };
            let _ = ack_tx.send(());
        }
    });

    let mut max_hwm = baseline;
    for _ in 0..CYCLES {
        // Batch API only — the owner performs NO scalar alloc/realloc. The
        // owner allocates through the CLAIMED `HeapCore`'s own batch entry
        // point (NOT the `SeferAlloc` wrapper): the wrapper would resolve the
        // calling thread's TLS heap — a DIFFERENT registry slot — so the
        // blocks would register in a table this test's `dbg_table_count`
        // oracle never reads, making the high-water assertion vacuous.
        let mut out = [std::ptr::null_mut(); BATCH_SIZE];
        let filled = heap.alloc_batch(layout, &mut out);
        assert_eq!(filled, BATCH_SIZE, "alloc_batch under-filled");
        for (i, &ptr) in out.iter().enumerate() {
            assert!(!out[..i].contains(&ptr), "batch issued duplicate pointers");
        }
        for ptr in out {
            assert!(!ptr.is_null(), "alloc_batch returned null slot");

            // SAFETY: `ptr` is a live block of `layout` (size >= 8), so the
            // first 8 bytes are in bounds and writable.
            unsafe { std::ptr::write_volatile(ptr.cast::<u64>(), 0xCAFE_0000_0000_0001) };

            tx.send(SendPtr(ptr)).expect("reaper hung up early");
            ack_rx
                .recv()
                .expect("reaper failed to ack (deadlock/crash in dealloc?)");
        }

        max_hwm = max_hwm.max(heap.dbg_table_count());
    }

    drop(tx);
    reaper.join().expect("reaper aborted");

    // THE leak assertion (G3 mechanism spelled out on failure).
    let final_hwm = heap.dbg_table_count();
    assert!(
        final_hwm <= baseline + HIGH_WATER_GROWTH_BOUND,
        "deferred-free stack never drained; held SegmentTable slots grew \
         O(cycles): baseline={baseline}, max_hwm={max_hwm}, final={final_hwm}, \
         bound={}",
        baseline + HIGH_WATER_GROWTH_BOUND
    );

    // Path-activation proof: post-fix the drain must have ACTUALLY fired.
    #[cfg(not(feature = "alloc-decommit"))]
    {
        let released_after = AllocCore::dbg_segments_released_total();
        let delta = released_after - released_before;
        assert!(
            delta >= (CYCLES as u64) / 2,
            "large-segment reclaim never fired via the batch drain: \
             released_total delta={delta} over {CYCLES} cycles (expected \
             >= {})",
            (CYCLES as u64) / 2
        );
    }

    // Drain the final deferred batch only AFTER all regression oracles.
    let cleanup = heap.alloc(layout);
    assert!(!cleanup.is_null());
    // SAFETY: cleanup is a live allocation of this layout, freed exactly once.
    unsafe { heap.dealloc(cleanup, layout) };

    // SAFETY: `heap_ptr` was returned by `claim` above, not yet recycled,
    // and no other thread touches it.
    unsafe { HeapRegistry::recycle(heap_ptr) };
}

#[cfg(all(not(feature = "fastbin"), feature = "bench-internals"))]
#[test]
fn large_batch_drains_small_overflow_without_fastbin() {
    let large = Layout::from_size_align(sefer_alloc::SegmentLayout::SMALL_MAX + 4096, 16).unwrap();
    batch_drains_small_overflow(large);
}

#[cfg(all(not(feature = "fastbin"), feature = "bench-internals"))]
#[test]
fn small_batch_drains_small_overflow_without_fastbin() {
    // Different from the freed class so the batch cannot consume the oracle block.
    batch_drains_small_overflow(Layout::from_size_align(64, 8).unwrap());
}

#[cfg(all(not(feature = "fastbin"), feature = "bench-internals"))]
fn batch_drains_small_overflow(batch_layout: Layout) {
    use sefer_alloc::alloc_core::remote_free_ring::RING_CAP;

    let heap_ptr = HeapRegistry::claim();
    assert!(!heap_ptr.is_null());
    // SAFETY: this thread owns the newly claimed heap until recycle.
    let heap = unsafe { &mut *heap_ptr };
    let small = Layout::from_size_align(16, 8).unwrap();
    let pin = heap.alloc(small);
    assert!(!pin.is_null());
    let base = heap.dbg_segment_base_of_ptr(pin);
    let mut blocks = Vec::new();
    for _ in 0..RING_CAP + 1 {
        let ptr = heap.alloc(small);
        assert!(!ptr.is_null());
        assert_eq!(heap.dbg_segment_base_of_ptr(ptr), base);
        blocks.push(SendPtr(ptr));
    }
    let overflowed = blocks.last().unwrap().0;
    std::thread::spawn(move || {
        for SendPtr(ptr) in blocks {
            // SAFETY: uniquely transferred live block with its original layout.
            unsafe { GLOBAL.dealloc(ptr, small) };
        }
    })
    .join()
    .unwrap();
    assert!(!heap.dbg_is_free_for(overflowed));

    let mut out = [std::ptr::null_mut(); 1];
    assert_eq!(heap.alloc_batch(batch_layout, &mut out), 1);
    assert!(
        heap.dbg_is_free_for(overflowed),
        "batch {batch_layout:?} skipped the non-fastbin overflow drain"
    );

    heap.dbg_drain_all_rings();
    // SAFETY: both allocations are still live, owned here, and freed once.
    unsafe {
        heap.dealloc(pin, small);
        heap.dealloc(out[0], batch_layout);
        HeapRegistry::recycle(heap_ptr);
    }
}
