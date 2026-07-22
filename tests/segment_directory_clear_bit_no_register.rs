//! R13-2 (task #272), finding #2: `SegmentDirectory::clear_bit` must use a
//! READ-ONLY node-bucket lookup, never the REGISTERING `node_bucket_mut`.
//!
//! ## The defect
//!
//! Before this fix, `clear_bit` called the same registering
//! `node_bucket_mut` that `set_bit` uses. This means a no-op clear for a
//! node that has NEVER published a non-empty class (so its `node_ids` slot
//! was never claimed) would nonetheless CLAIM a bucket slot for that node —
//! wasting one of the limited `MAX_NODES` (8) registration slots on a call
//! that clears a bit that was already 0.
//!
//! The real production trigger: `sync_directory_for_segment_classes`
//! (`src/alloc_core/alloc_core_small.rs`) calls `clear_bit` whenever a
//! ring-drain pass reclaims blocks into a class but the class's `BinTable`
//! head ends up `FREE_LIST_NULL` again by the time the drain finishes (a
//! reclaim immediately consumed by a synchronous re-pop). If that segment's
//! node had never registered a bucket before (this was the very FIRST
//! directory-observable event for that node), the pre-fix `clear_bit` would
//! silently burn a registration slot for a class that was — and remains —
//! empty on that node.
//!
//! ## The fix
//!
//! `clear_bit` now calls the READ-ONLY `node_bucket` (added in R12-2,
//! previously only used by `get_bit`) instead of `node_bucket_mut`. A node
//! with no claimed bucket resolves to the shared unknown bucket, and
//! clearing an already-0 bit there is a harmless, slot-free no-op.
//!
//! ## Construction note
//!
//! Note that a cold `alloc()` carve amortises via a REFILL BATCH (up to 31
//! extra blocks, `carve_block_with_refill`'s `REFILL_BATCH` const), and each
//! refill block is pushed onto its segment's free list via an internal
//! `dealloc_small` call. That is an empty->non-empty transition that DOES
//! register and occupy a bucket, even though this test never explicitly
//! frees anything, so simply allocating past the materialisation threshold
//! is not enough to keep a node's bucket unregistered (diagnosed by hand:
//! the node showed up in bucket 0 immediately after the very first
//! allocation). This test instead drains the allocated class fully back to
//! idle first, using the same technique as
//! `segment_directory_numa_bucket_reuse.rs`'s `drive_class_to_idle`: pop
//! everything via `alloc()` until a genuinely FRESH segment is carved (the
//! only conclusive "nothing free remains anywhere" signal), leaking every
//! popped pointer so nothing is freed back and re-sets the bit. That drain
//! correctly relies on the R13-2 fix itself to reach "unregistered", so this
//! test's construction doubles as an end-to-end sanity check that R13-2's
//! reuse mechanism is active.
//!
//! ## This test
//!
//! After the node's bucket is driven back to unregistered, force-clears a
//! bit for a class that is (and always was) empty on that segment via
//! [`AllocCore::dbg_directory_force_clear_bit`] (the same test-only hook
//! `tests/r9_8_directory_drift_recovery.rs` and others use to manufacture
//! directory drift). Asserts the node's bucket stays the shared unknown
//! bucket afterward — i.e. the clear did NOT claim a registration slot.
//!
//! Build/run:
//!   cargo test --features "numa-aware-mock alloc-segment-directory" \
//!       --test segment_directory_clear_bit_no_register

#![cfg(all(feature = "numa-aware-mock", feature = "alloc-segment-directory"))]

use std::alloc::Layout;

use numa_shim::mock;
use sefer_alloc::AllocCore;

/// Never-before-seen node id for this test.
const NODE: u32 = 42;

fn layout_for_class(class_idx: usize) -> Layout {
    let size = AllocCore::dbg_block_size(class_idx);
    Layout::from_size_align(size, 1).unwrap()
}

#[test]
fn clear_bit_on_never_registered_node_does_not_claim_a_bucket() {
    mock::set_current_node(NODE);
    let _ = mock::drain();
    let mut core = AllocCore::new().expect("bootstrap");

    let small_class_count = AllocCore::dbg_small_class_count();
    assert!(
        small_class_count >= 2,
        "this test needs >= 2 distinct small classes"
    );
    // The largest small class: fewest blocks per segment, so the fewest
    // allocations are needed to cross the materialisation threshold.
    let allocated_class = small_class_count - 1;
    let untouched_class = small_class_count - 2;
    let layout = layout_for_class(allocated_class);

    // Allocate past the materialisation threshold, then drain the class back
    // to idle (see the module doc's construction note) — leaking every
    // popped pointer so the drain is not undone.
    let threshold = AllocCore::dbg_directory_materialize_threshold();
    let mut live: Vec<*mut u8> = Vec::new();
    loop {
        let p = core.alloc(layout);
        assert!(!p.is_null(), "allocation must succeed");
        live.push(p);
        if core.dbg_table_count() > threshold {
            break;
        }
    }
    assert!(
        core.dbg_directory_is_materialised(),
        "directory must be materialised for this test to exercise clear_bit \
         through a real sidecar"
    );

    // Free everything, then pop it all straight back out (leaking every
    // popped pointer) until a genuinely fresh segment is carved — the only
    // conclusive signal that nothing free remains anywhere for this class.
    for &p in &live {
        unsafe { core.dealloc(p, layout) };
    }
    let table_count_before_drain = core.dbg_table_count();
    loop {
        let p = core.alloc(layout);
        assert!(!p.is_null(), "re-allocation during drain must succeed");
        if core.dbg_table_count() != table_count_before_drain {
            // Conclusive: every PRE-EXISTING segment's free list for this
            // class is now exhausted. But `p` came from a FRESH segment
            // whose cold-carve refill batch just pushed extra blocks onto
            // ITS OWN free list (an empty->non-empty transition) — drain
            // that one segment too, via its own head, before stopping for
            // real (same second-order trap documented in
            // `segment_directory_numa_bucket_reuse.rs`'s
            // `drive_class_to_idle`).
            while core.dbg_freelist_head_for(p, allocated_class) != u32::MAX {
                let extra = core.alloc(layout);
                assert!(
                    !extra.is_null(),
                    "draining the fresh segment's refill batch must succeed"
                );
                assert_eq!(
                    core.dbg_segment_id_of(extra),
                    core.dbg_segment_id_of(p),
                    "draining the fresh segment's refill batch pulled a \
                     block from a DIFFERENT segment than expected"
                );
            }
            break;
        }
    }

    let unknown_bucket = AllocCore::dbg_directory_node_bitmaps() - 1;
    let bucket_before = core
        .dbg_directory_node_bucket_for(NODE)
        .expect("directory materialised");
    assert_eq!(
        bucket_before, unknown_bucket,
        "construction check: node {NODE} must NOT hold a real bucket before \
         the force-clear below (the allocated class was fully drained back \
         to idle, and R13-2's reuse mechanism must have freed the slot) — \
         otherwise this test cannot distinguish 'clear_bit registered a \
         bucket' from 'a bucket was already registered by something else'"
    );

    // Force-clear a bit for a class that has NEVER been touched on this
    // node's segments — an already-0 bit, so this is a genuine no-op at the
    // bit level. Use the FIRST segment slot (table.count() > threshold
    // guarantees slot 0 exists and is owned by this node).
    let cleared = core.dbg_directory_force_clear_bit(untouched_class, 0);
    assert!(
        cleared,
        "dbg_directory_force_clear_bit must reach the materialised sidecar"
    );

    let bucket_after = core
        .dbg_directory_node_bucket_for(NODE)
        .expect("directory materialised");
    assert_eq!(
        bucket_after, unknown_bucket,
        "RED (pre-fix)/GREEN (post-fix) assertion: clearing an already-0 bit \
         for a node that has NEVER registered a bucket must NOT claim one. \
         Pre-fix, `clear_bit` called the REGISTERING `node_bucket_mut` (the \
         same one `set_bit` uses), so this genuinely no-op clear would \
         silently burn one of the limited MAX_NODES registration slots on \
         node {NODE} — `bucket_after` would show a real bucket index here \
         instead of the unknown bucket."
    );
}
