//! R6-T39 / PF-10: an own-thread Large free must NOT re-resolve the segment
//! root inside `AllocCore` — the `OwnedBlock` witness already carries it.
//! Oracle: the Tier-1 canonical-lookup counters. The whole `HeapCore::dealloc`
//! for a Large block performs exactly ONE canonical lookup (the routing
//! `owned_block_of`); before PF-10 the tail call `AllocCore::dealloc` performed
//! a SECOND one (`canonical_block_of`), so this test is red on pre-PF-10 code
//! (delta == 2) and green after (delta == 1).

#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapRegistry};
use std::sync::Mutex;

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn large_own_free_performs_single_canonical_lookup() {
    let _guard = TEST_LOCK.lock().unwrap();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim heap");
    let heap = lease.core();
    // 4 MiB: unconditionally above every small/medium class ceiling, so
    // `class_for` yields None and the free takes the Large tail path.
    let layout = Layout::from_size_align(4 * 1024 * 1024, 16).unwrap();
    let p = heap.alloc(layout);
    assert!(!p.is_null(), "large alloc failed");
    let lookups_before =
        AllocCore::dbg_contains_base_tier1_hits() + AllocCore::dbg_contains_base_tier1_misses();
    // SAFETY: `p` is a live allocation of this heap, freed once with its
    // allocation layout.
    unsafe { heap.dealloc(p, layout) };
    let lookups_after =
        AllocCore::dbg_contains_base_tier1_hits() + AllocCore::dbg_contains_base_tier1_misses();
    assert_eq!(
        lookups_after - lookups_before,
        1,
        "own-thread Large free must perform exactly one canonical lookup \
         (routing); a second one means the PF-10 re-resolve came back"
    );
    // Correctness: the freed Large must be reusable afterwards.
    let q = heap.alloc(layout);
    assert!(!q.is_null(), "large alloc after free failed");
    // SAFETY: same contract as above.
    unsafe { heap.dealloc(q, layout) };
}
