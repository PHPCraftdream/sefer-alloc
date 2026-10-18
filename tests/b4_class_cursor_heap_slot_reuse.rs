#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "internals",
    feature = "bench-internals"
))]

//! B4 §11 (docs/perf/B4_MT_COLLAPSE_DIAGNOSIS): a re-claimed registry heap
//! slot must NOT inherit the previous owner's per-class discovery cursors.
//!
//! This answers the B4 observation that a released thread's heap slot keeps
//! its segments and per-segment bins when the slot is published FREE on
//! thread exit: a whole-slot reuse (the recycle branch of
//! `HeapRegistry::claim`, `src/registry/heap_registry/claim.rs`) hands the
//! SAME `HeapCore` to a new owner — without `claim_impl`'s
//! `invalidate_class_cursors` call the new owner would start life with the
//! previous owner's per-class cursor, a dangling hint across an owner
//! change (exactly the staleness class the NUMA node-cache invalidation,
//! `tests/numa_cache_invalidation.rs`, already guards against; this test
//! mirrors that slot-recycle forcing mechanism: claim via TLS bind, exit to
//! publish FREE, re-claim on the next allocation).
//!
//! Mechanism: thread A allocates through the global allocator (binding its
//! TLS heap = one `SeferAlloc` registry slot), records a cursor by freeing
//! two blocks into a non-current segment and bin-missing into it, asserts
//! the cursor is visible via `dbg_class_cursor_slot_for_current_thread`,
//! and exits WITHOUT freeing the rest (its TLS heap is published FREE with
//! segments/bins retained — the exact production state from the B4 data).
//! The claiming thread then allocates once: the lazy `claim_impl` re-claims
//! the same slot (the slot A released is the one this test produced and
//! freed) and its invalidation must present a cursor-free heap: `None` for
//! every class.

use core::alloc::Layout;
use std::alloc::GlobalAlloc;

use sefer_alloc::global::tls_heap::current_for_trim;
use sefer_alloc::{AllocCore, SeferAlloc};

/// Smallest class whose block is at least 128 KiB: a bounded number of
/// blocks per 4 MiB segment, so a few dozen allocations span at least 2
/// segments (a 4096-byte block would keep every allocation inside ONE
/// segment, and the non-current-segment free below needs 2 or more).
fn pick_class() -> (usize, Layout) {
    for i in 0..AllocCore::dbg_small_class_count() {
        let size = AllocCore::dbg_block_size(i);
        if size >= 128 * 1024 {
            return (
                i,
                Layout::from_size_align(size, 8).expect("block layout valid"),
            );
        }
    }
    panic!("no small class with >= 128 KiB blocks");
}

/// This thread's registry slot id (the heap the TLS is bound to).
fn current_heap_id() -> u32 {
    let heap = current_for_trim().expect("allocation must bind TLS");
    // SAFETY: `heap` is non-null and points to a live `HeapCore` in a slot
    // owned by THIS thread (the `current_for_trim` contract).
    unsafe { (*heap).id() }
}

#[test]
fn b4_reclaimed_heap_slot_does_not_inherit_class_cursors() {
    let allocator = SeferAlloc::new();
    let (class_idx, layout) = pick_class();

    let (slot_a, cursor_slot) = std::thread::scope(|scope| {
        // -- Thread A: bind a heap, build a cursor on a non-current segment,
        // exit WITHOUT freeing (TLS heap published FREE, bins retained).
        let allocator = &allocator;
        let handle = scope.spawn(move || {
            let mut blocks: Vec<*mut u8> = Vec::new();
            for _ in 0..96 {
                // SAFETY: valid layout; the remaining blocks are
                // intentionally not freed (see the module doc).
                let p = unsafe { allocator.alloc(layout) };
                assert!(!p.is_null());
                blocks.push(p);
            }
            let first_base = sefer_alloc::SegmentLayout::segment_base_of(blocks[0] as usize);
            assert!(
                blocks.iter().any(|p| {
                    sefer_alloc::SegmentLayout::segment_base_of(*p as usize) != first_base
                }),
                "96 blocks must span >= 2 segments"
            );
            // Free 2 blocks sharing a non-current (the FIRST) segment's
            // base. Their bin is then the earliest non-empty bin the
            // slot-ordered discovery scan can find.
            let freed: Vec<*mut u8> = blocks
                .iter()
                .copied()
                .filter(|p| sefer_alloc::SegmentLayout::segment_base_of(*p as usize) == first_base)
                .take(2)
                .collect();
            assert_eq!(freed.len(), 2, "the first segment must hold >= 2 blocks");
            for p in &freed {
                // SAFETY: live `layout` blocks of THIS thread's heap.
                unsafe { allocator.dealloc(*p, layout) };
                let idx = blocks.iter().position(|b| b == p).expect("present");
                blocks.remove(idx);
            }
            // Drain: allocate (retained) until the freed bin has been
            // discovered and the cursor recorded -- the current segment's
            // bump room and refill leftovers must be exhausted first.
            let mut cursor_slot = None;
            for step in 0..512 {
                // SAFETY: valid layout; intentionally retained on exit.
                let p = unsafe { allocator.alloc(layout) };
                assert!(!p.is_null());
                blocks.push(p);
                cursor_slot = SeferAlloc::dbg_class_cursor_slot_for_current_thread(class_idx);
                if cursor_slot.is_some() {
                    break;
                }
                assert!(step < 511, "no class cursor was ever recorded");
            }
            let slot_a = current_heap_id();
            (
                slot_a,
                cursor_slot.expect("thread A must record a class cursor before exiting"),
            )
        });
        handle.join().expect("thread A must not panic")
    });

    // Thread A has exited: its slot is FREE again with segments/bins (and,
    // without the invalidation, its cursor) retained. This thread's next
    // allocation lazily re-claims that slot via `claim_impl`.
    //
    // SAFETY: valid layout; freed immediately below with the same layout.
    let small = Layout::from_size_align(64, 8).unwrap();
    let p = unsafe { allocator.alloc(small) };
    assert!(!p.is_null(), "claiming thread's allocation must work");
    // SAFETY: `p` is the live 64-byte block allocated just above.
    unsafe { allocator.dealloc(p, small) };

    assert_eq!(
        current_heap_id(),
        slot_a,
        "the claiming thread must re-claim the SAME registry slot thread A \
         released (whole-slot reuse)"
    );
    assert_eq!(
        SeferAlloc::dbg_class_cursor_slot_for_current_thread(class_idx),
        None,
        "the new owner of the recycled slot must not inherit thread A's \
         cursor for class {class_idx} (was slot {cursor_slot})"
    );
}
