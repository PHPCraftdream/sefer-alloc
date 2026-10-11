#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use sefer_alloc::alloc_core::{AllocCore, SegmentHashHarness};
use sefer_alloc::registry::segment_route::{RouteDirectory, RouteKind, SmallSidecar};
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::SegmentLayout;

type Totals = (usize, usize, usize, usize);

fn totals() -> Totals {
    SmallSidecar::dbg_route_slots_system_totals()
}

fn delta(before: Totals) -> Totals {
    let after = totals();
    (
        after
            .0
            .checked_sub(before.0)
            .expect("alloc counter monotonic"),
        after
            .1
            .checked_sub(before.1)
            .expect("free counter monotonic"),
        after
            .2
            .checked_sub(before.2)
            .expect("alloc bytes monotonic"),
        after.3.checked_sub(before.3).expect("free bytes monotonic"),
    )
}

fn assert_live(before: Totals, arrays: usize, capacity: usize) {
    let (alloc, free, bytes, freed_bytes) = delta(before);
    assert_eq!(alloc.checked_sub(free), Some(arrays), "live arrays");
    assert_eq!(
        bytes.checked_sub(freed_bytes),
        Some(
            capacity
                .checked_mul(SmallSidecar::dbg_route_slot_bytes())
                .unwrap()
        ),
        "requested live System bytes (not RSS)"
    );
}

// SINGLE test: the counters are process-global. No installed SeferAlloc or
// background maintenance can materialize unrelated registry heaps here.
// Counterfactuals: skip the growth dealloc ->
// live arrays/bytes fail at 9; replace routes on attachment -> cumulative
// totals fail; skip final Drop dealloc -> zero-live checkpoint fails.
#[test]
fn route_slots_bounded_per_slot() {
    let max = AllocCore::dbg_max_segments();
    assert!(max >= 64);
    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX + 1, 16).unwrap();

    // The research claimed an owned Drop surface was unavailable. The current
    // dbg_new_routed_for_test surface constructs a real owned routed core.
    let owned_before = totals();
    {
        let mut core = AllocCore::dbg_new_routed_for_test().expect("owned routed core");
        assert_live(owned_before, 1, 8);
        let mut ptrs = Vec::with_capacity(16);
        for _ in 0..16 {
            let ptr = core.alloc(layout);
            assert!(!ptr.is_null());
            let pin = RouteDirectory::global()
                .lookup(ptr)
                .expect("real Large route");
            assert_eq!(pin.kind(), RouteKind::Large);
            ptrs.push(ptr);
        }
        assert_eq!(core.dbg_table_count(), 17);
        assert_live(owned_before, 1, 32);
        for ptr in ptrs {
            // SAFETY: each pointer is a unique live allocation of this core,
            // with its original layout, freed exactly once on the owner thread.
            unsafe { core.dealloc(ptr, layout) };
        }
        assert_live(owned_before, 1, 32);
    }
    assert_live(owned_before, 0, 0);
    let owned = delta(owned_before);
    assert_eq!(owned.0, owned.1);
    assert_eq!(owned.2, owned.3);

    // Actual HeapRegistry lease/claim, not a routed standalone substitute.
    let registry_before = totals();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("registry claim");
    let slot = lease.slot_index();
    let mut generation = lease.generation();
    assert_live(registry_before, 1, 8);
    let mut ptrs = Vec::with_capacity(63);
    for count in 2usize..=64 {
        let heap = lease.core();
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null(), "allocation at count {count}");
        let pin = RouteDirectory::global()
            .lookup(ptr)
            .expect("registered Large route");
        assert_eq!(pin.kind(), RouteKind::Large);
        assert_eq!(pin.owner(), usize::try_from(slot).unwrap());
        assert_eq!(heap.dbg_table_count() as usize, count);
        ptrs.push(ptr);
        assert_live(registry_before, 1, count.next_power_of_two().max(8));
    }
    let grown = delta(registry_before);
    assert_eq!((grown.0, grown.1), (4, 3), "8 -> 16 -> 32 -> 64");
    for ptr in ptrs {
        // SAFETY: this lease exclusively owns each live allocation; original
        // layout and exactly one deallocation per pointer.
        unsafe { lease.core().dealloc(ptr, layout) };
    }
    drop(lease);
    assert_live(registry_before, 1, 64); // process-lifetime retention, not zero
    let retained = totals();
    for _ in 0..16 {
        let mut lease = HeapRegistry::dbg_claim_lease().expect("reclaim same slot");
        assert_eq!(lease.slot_index(), slot, "actual registry reuse");
        generation += 1;
        assert_eq!(lease.generation(), generation);
        let ptr = lease.core().alloc(layout);
        assert!(!ptr.is_null());
        let pin = RouteDirectory::global()
            .lookup(ptr)
            .expect("route after reuse");
        assert_eq!(pin.owner(), usize::try_from(slot).unwrap());
        drop(pin);
        // SAFETY: unique live allocation from this owner lease, original layout.
        unsafe { lease.core().dealloc(ptr, layout) };
        drop(lease);
        assert_eq!(
            totals(),
            retained,
            "reuse must not allocate even transiently"
        );
        assert_live(registry_before, 1, 64);
    }

    // Boundary-scale supplement: real SegmentTable attachment/registration and
    // RouteSlots, address-only roots (never issued/dereferenced). Reserving
    // MAX_SEGMENTS physical Large spans would cost >=16 GiB under production.
    let boundary_before = totals();
    {
        let mut table = SegmentHashHarness::new();
        assert!(table.attach_routes_for_test());
        assert_live(boundary_before, 1, 8);
        let attached = totals();
        for _ in 0..16 {
            assert!(!table.attach_routes_for_test());
            assert_eq!(totals(), attached, "reattachment rejects before allocation");
        }
        for count in 2..=max {
            assert_eq!(table.register_route_for_test(), Some((count - 1) as u32));
            assert_live(boundary_before, 1, count.next_power_of_two().max(8));
        }
        let full = totals();
        assert_eq!(table.register_route_for_test(), None);
        assert!(!table.attach_routes_for_test());
        assert_eq!(totals(), full, "full/attached rejection is allocation-free");
    }
    assert_live(boundary_before, 0, 0);
    assert_live(registry_before, 1, 64);
}
