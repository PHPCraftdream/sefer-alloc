use core::alloc::Layout;
use sefer_alloc::registry::segment_route::{RouteDirectory, RouteRegistration, SmallSidecar};
use sefer_alloc::registry::{HeapCore, HeapRegistry};

struct Before {
    owner: (usize, u32, u32),
    system: (usize, usize, usize, usize, usize),
    routes: (usize, usize, usize),
    table: u32,
    magazines: (u16, u16),
}

impl Before {
    fn capture(heap: &HeapCore, anchor: *mut u8) -> Self {
        Self {
            owner: owner_state(anchor),
            system: SmallSidecar::system_totals_for_test(),
            routes: RouteDirectory::global().live_route_census_for_test(),
            table: heap.dbg_table_count(),
            magazines: magazines(heap),
        }
    }

    fn assert_rollback(&self, heap: &HeapCore, anchor: *mut u8) {
        assert_eq!(owner_state(anchor), self.owner);
        assert_eq!(SmallSidecar::system_totals_for_test(), self.system);
        assert_eq!(
            RouteDirectory::global().live_route_census_for_test(),
            self.routes
        );
        assert_eq!(heap.dbg_table_count(), self.table);
        assert_eq!(magazines(heap), self.magazines);
        assert_eq!(
            RouteRegistration::class_at_global_address_for_test(anchor.addr()),
            Some(0)
        );
        assert!(!heap.dbg_is_free_for(anchor));
    }

    fn assert_success(&self, heap: &HeapCore, anchor: *mut u8, issued: *mut u8, zeroed: bool) {
        assert!(!issued.is_null(), "successful retry must issue a block");
        assert_eq!(
            RouteRegistration::class_at_global_address_for_test(issued.addr()),
            Some(1)
        );
        assert_eq!(
            RouteRegistration::class_at_global_address_for_test(anchor.addr()),
            Some(0)
        );
        let after = SmallSidecar::system_totals_for_test();
        assert_eq!(after.0 - self.system.0, 256);
        assert_eq!(after.1, self.system.1);
        assert_eq!(after.2, self.system.2);
        assert_eq!(after.3 - self.system.3, 1);
        assert_eq!(after.4, self.system.4);
        let owner = owner_state(anchor);
        let resident = magazines(heap);
        assert!(owner.0 > self.owner.0);
        assert_eq!(owner.1, self.owner.1 + 1 + u32::from(resident.1));
        assert_eq!(resident.0, self.magazines.0);
        assert_eq!(
            RouteDirectory::global().live_route_census_for_test(),
            self.routes
        );
        assert_eq!(heap.dbg_table_count(), self.table);
        assert!(!heap.dbg_is_free_for(anchor));
        assert!(!heap.dbg_is_free_for(issued));
        if zeroed {
            // SAFETY: issued is a live allocation with 32 initialized zeroed bytes.
            assert!(unsafe { core::slice::from_raw_parts(issued, 32) }
                .iter()
                .all(|&byte| byte == 0));
        }
    }
}

fn owner_state(anchor: *mut u8) -> (usize, u32, u32) {
    // SAFETY: every caller retains the owning heap and its live anchor exclusively.
    unsafe { SmallSidecar::owner_state_for_test(anchor.addr(), 1) }.unwrap()
}

fn magazines(heap: &HeapCore) -> (u16, u16) {
    #[cfg(feature = "fastbin")]
    {
        (heap.dbg_tcache_count(0), heap.dbg_tcache_count(1))
    }
    #[cfg(not(feature = "fastbin"))]
    {
        let _ = heap;
        (0, 0)
    }
}

fn allocate(heap: &mut HeapCore, zeroed: bool) -> *mut u8 {
    let layout = Layout::from_size_align(32, 16).unwrap();
    if zeroed {
        heap.alloc_zeroed(layout)
    } else {
        heap.alloc(layout)
    }
}

fn release(heap: &mut HeapCore, anchor: *mut u8, issued: *mut u8) {
    // SAFETY: both pointers are uniquely owned, live, and have these exact layouts.
    unsafe {
        heap.dealloc(issued, Layout::from_size_align(32, 16).unwrap());
        heap.dealloc(anchor, Layout::from_size_align(16, 16).unwrap());
    }
}

fn persistent(heap: &mut HeapCore, zeroed: bool) {
    let anchor = heap.alloc(Layout::from_size_align(16, 16).unwrap());
    assert!(!anchor.is_null());
    let before = Before::capture(heap, anchor);
    let attempts = if cfg!(feature = "fastbin") { 2 } else { 1 };
    SmallSidecar::fail_next_spills_for_test(attempts);
    assert!(
        allocate(heap, zeroed).is_null(),
        "persistent OOM must cover the rescue retry"
    );
    assert_eq!(SmallSidecar::spill_failures_remaining_for_test(), 0);
    before.assert_rollback(heap, anchor);
    let issued = allocate(heap, zeroed);
    before.assert_success(heap, anchor, issued, zeroed);
    release(heap, anchor, issued);
}

fn transient(heap: &mut HeapCore, zeroed: bool) {
    let anchor = heap.alloc(Layout::from_size_align(16, 16).unwrap());
    assert!(!anchor.is_null());
    let before = Before::capture(heap, anchor);
    SmallSidecar::fail_next_spills_for_test(1);
    let first_attempt = allocate(heap, zeroed);
    assert_eq!(SmallSidecar::spill_failures_remaining_for_test(), 0);
    #[cfg(feature = "fastbin")]
    let issued = {
        assert!(
            !first_attempt.is_null(),
            "single spill failure must recover inside rescue"
        );
        first_attempt
    };
    #[cfg(not(feature = "fastbin"))]
    let issued = {
        assert!(first_attempt.is_null());
        before.assert_rollback(heap, anchor);
        allocate(heap, zeroed)
    };
    before.assert_success(heap, anchor, issued, zeroed);
    release(heap, anchor, issued);
}

pub(super) fn verify(zeroed: bool) {
    // Keep both leases live so the transient fixture has a fresh uniform leaf.
    let mut persistent_lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let mut transient_lease = HeapRegistry::dbg_claim_lease().expect("claim");
    assert_ne!(persistent_lease.slot_index(), transient_lease.slot_index());
    // Successful claims give this thread exclusive ownership of distinct heaps.
    persistent(persistent_lease.core(), zeroed);
    transient(transient_lease.core(), zeroed);
    // The lease Drop recycles the slots whole.
    drop(transient_lease);
    drop(persistent_lease);
}
