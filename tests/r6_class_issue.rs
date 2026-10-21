#![cfg(all(feature = "alloc-global", feature = "internals"))]

use std::alloc::Layout;

use sefer_alloc::registry::segment_route::RouteRegistration;
use sefer_alloc::registry::HeapRegistry;

/// D1′ phase-1b: the address observer reads ONLY the pinned sidecar's
/// atomic banks. A live issued block start reports `(start: true, pending:
/// false)` — the class itself lives in the owner state byte and is pinned
/// by the lease-held state-byte suites (`state_byte_*.rs`).
fn issued_start_at(ptr: *mut u8) -> Option<(bool, bool)> {
    RouteRegistration::start_state_at_global_address_for_test(ptr.addr())
}

#[test]
fn scalar_issue_reissue_and_narrow_reborrow_have_class() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    for size in 1..=7 {
        let layout = Layout::from_size_align(size, 1).unwrap();
        let first = heap.alloc(layout);
        assert!(!first.is_null());
        assert_eq!(issued_start_at(first), Some((true, false)));
        // SAFETY: the live allocation contains at least one byte.
        let narrow = unsafe { &mut *first as *mut u8 };
        // SAFETY: narrow starts the sole live block and the layout is exact.
        unsafe { heap.dealloc(narrow, layout) };
        let second = heap.alloc(layout);
        assert!(!second.is_null());
        assert_eq!(issued_start_at(second), Some((true, false)));
        // SAFETY: second is live and was allocated with layout.
        unsafe { heap.dealloc(second, layout) };
    }
    #[cfg(feature = "fastbin")]
    heap.dbg_flush_all();
    drop(lease);
}

#[test]
fn realloc_move_issues_new_class_and_inplace_keeps_class() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let old = Layout::from_size_align(1, 1).unwrap();
    let first = heap.alloc(old);
    assert!(!first.is_null());
    assert_eq!(issued_start_at(first), Some((true, false)));
    // SAFETY: first is a live allocation with old layout.
    let inplace = unsafe { heap.realloc(first, old, 7) };
    assert_eq!(inplace.addr(), first.addr());
    assert_eq!(issued_start_at(inplace), Some((true, false)));
    let seven = Layout::from_size_align(7, 1).unwrap();
    // SAFETY: inplace is live with the seven-byte layout.
    let moved = unsafe { heap.realloc(inplace, seven, 4097) };
    assert!(!moved.is_null());
    assert_ne!(moved.addr(), inplace.addr());
    assert_eq!(issued_start_at(moved), Some((true, false)));
    // SAFETY: moved is the only live allocation with this layout.
    unsafe { heap.dealloc(moved, Layout::from_size_align(4097, 1).unwrap()) };
    #[cfg(feature = "fastbin")]
    heap.dbg_flush_all();
    drop(lease);
}

#[test]
fn later_small_segment_is_registered_before_issue() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layout = Layout::from_size_align(4096, 16).unwrap();
    let mut blocks = Vec::new();
    let first = heap.alloc(layout);
    assert!(!first.is_null());
    let primordial = heap.dbg_segment_base_of_ptr(first);
    blocks.push(first);
    let mut later = None;
    for _ in 0..1200 {
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null());
        assert_eq!(issued_start_at(ptr), Some((true, false)));
        if heap.dbg_segment_base_of_ptr(ptr) != primordial {
            later = Some(ptr);
            blocks.push(ptr);
            break;
        }
        blocks.push(ptr);
    }
    assert!(later.is_some(), "expected a second Small segment");
    for ptr in blocks {
        // SAFETY: every pointer is a distinct live allocation of layout.
        unsafe { heap.dealloc(ptr, layout) };
    }
    #[cfg(feature = "fastbin")]
    heap.dbg_flush_all();
    drop(lease);
}

#[cfg(all(feature = "fastbin", feature = "alloc-decommit"))]
#[test]
fn magazine_hit_keeps_one_outstanding_credit() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layout = Layout::from_size_align(64, 8).unwrap();
    let first = heap.alloc(layout);
    assert!(!first.is_null());
    assert_eq!(issued_start_at(first), Some((true, false)));
    let before = heap.dbg_live_count_for(first).unwrap();
    let second = heap.alloc(layout);
    assert!(!second.is_null());
    assert_eq!(issued_start_at(second), Some((true, false)));
    assert_eq!(heap.dbg_live_count_for(second), Some(before));
    // SAFETY: first and second are distinct live blocks of this layout.
    unsafe {
        heap.dealloc(first, layout);
        heap.dealloc(second, layout);
    }
    heap.dbg_flush_all();
    drop(lease);
}

#[cfg(all(feature = "fastbin", feature = "batch-api", feature = "alloc-decommit"))]
#[test]
fn batch_freelist_reissue_records_class_once_per_credit() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let layout = Layout::from_size_align(128, 8).unwrap();
    let mut first = [std::ptr::null_mut(); 8];
    assert_eq!(heap.alloc_batch(layout, &mut first), first.len());
    for &ptr in &first {
        assert_eq!(issued_start_at(ptr), Some((true, false)));
        // SAFETY: every batch slot holds one distinct live allocation.
        unsafe { heap.dealloc(ptr, layout) };
    }
    heap.dbg_flush_all();
    let mut second = [std::ptr::null_mut(); 8];
    assert_eq!(heap.alloc_batch(layout, &mut second), second.len());
    for &ptr in &second {
        assert_eq!(issued_start_at(ptr), Some((true, false)));
        // SAFETY: every batch slot holds one distinct live allocation.
        unsafe { heap.dealloc(ptr, layout) };
    }
    heap.dbg_flush_all();
    drop(lease);
}
