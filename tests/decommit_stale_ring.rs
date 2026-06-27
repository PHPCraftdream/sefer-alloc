//! Phase 35 (M6 decommit) — stale-ring-into-decommitted-segment safety test
//! (`alloc-decommit` + `alloc-xthread`), design §5.
//!
//! The worst failure of M6 is a use-after-free / write-to-unmapped on a page we
//! returned to the OS. The danger path: a cross-thread free pushes a block's
//! offset into a segment's `RemoteFreeRing`; the segment later empties and is
//! decommitted + reset (bump → `small_meta_end`, bitmap zeroed); then the owner
//! drains that STALE ring entry. Without a guard, `reclaim_offset` would pass
//! the (reset-cleared) bitmap `is_free` check and `write_next` into a
//! decommitted payload page.
//!
//! This test forces exactly that interleaving SINGLE-THREADED (no races, just the
//! logic): allocate enough to spill into fresh `Small` segments, free everything
//! so a non-current `Small` segment decommits, then push a stale offset for a
//! block in that decommitted segment via the `dbg_push_to_ring` seam and drain.
//! The drain MUST be a no-op (the bump guard rejects the offset, which is now
//! `>= bump` after the reset) — no panic, no access to the decommitted page.
//!
//! Under miri the decommit is a no-op so the page stays mapped; the test still
//! proves the LOGIC (the stale entry is rejected, the free list is not corrupted)
//! — re-allocating after the drain must hand out valid, distinct pointers.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-xthread",
    feature = "alloc-decommit"
))]

use core::alloc::Layout;
use std::collections::HashSet;

use sefer_alloc::alloc_core::AllocCore;

#[test]
fn stale_ring_entry_into_decommitted_segment_is_noop() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(256, 8).unwrap(); // class for 256 B

    // Spill past the primordial into several fresh Small segments.
    const N: usize = 60_000;
    let mut ptrs = Vec::with_capacity(N);
    for _ in 0..N {
        let p = ac.alloc(layout);
        assert!(!p.is_null());
        ptrs.push(p);
    }

    // The class index 256 B maps to (we derive it from the dbg seam to stay
    // robust against the size-class table layout).
    let class_idx = ac
        .dbg_layout_class_for(layout)
        .expect("256 B is a small class");

    // Free everything. Non-current Small segments empty → decommit.
    for &p in &ptrs {
        ac.dealloc(p, layout);
    }

    // Find a pointer whose segment is now decommitted.
    let decommitted_ptr = ptrs
        .iter()
        .copied()
        .find(|&p| ac.dbg_is_decommitted_for(p) == Some(true));

    let stale = match decommitted_ptr {
        Some(p) => p,
        None => {
            // If no segment decommitted (e.g. everything coalesced into the
            // current segment under this allocator's reuse policy), the test's
            // precondition isn't met — but that is itself a signal worth failing
            // on, because the soak test asserts decommit DOES fire. Re-assert
            // here so a silent no-decommit run doesn't pass vacuously.
            panic!(
                "no segment decommitted after free-all — cannot exercise the \
                 stale-ring path (precondition unmet)"
            );
        }
    };

    // Push a STALE offset for a block in the decommitted segment, exactly as a
    // late cross-thread freer would. The block was valid before the segment
    // emptied; after the reset its offset is `>= bump`, so reclaim must reject
    // it. `dbg_push_to_ring` returns false only on ring overflow; assert it
    // accepted the push so the drain genuinely processes the stale entry.
    assert!(
        ac.dbg_push_to_ring(stale, class_idx),
        "ring push of the stale offset was rejected (overflow) — test inconclusive"
    );

    // Drain every ring → reclaim_offset. The stale entry must be a NO-OP: the
    // bump guard rejects `off >= bump` before any write into the decommitted
    // page. No panic, no fault.
    ac.dbg_drain_all_rings();

    // Sanity: the allocator is still healthy — re-allocate a batch and assert
    // every pointer is valid and distinct (a corrupted free list from a botched
    // stale reclaim would hand out a duplicate or a wild pointer).
    let mut ptrs2 = Vec::with_capacity(1000);
    for _ in 0..1000 {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "re-alloc null after stale drain");
        // Non-vacuous: write + readback proves the (recommitted) page is usable.
        unsafe {
            core::ptr::write_bytes(p, 0xAB, 256);
            assert_eq!(p.read(), 0xAB);
        }
        ptrs2.push(p);
    }
    let set: HashSet<usize> = ptrs2.iter().map(|&p| p as usize).collect();
    assert_eq!(set.len(), ptrs2.len(), "duplicate pointer after stale drain");

    for &p in &ptrs2 {
        ac.dealloc(p, layout);
    }
}
