#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "alloc-segment-directory",
    feature = "bench-internals",
    feature = "internals"
))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::alloc_core::dbg_sidecar_reservation_stats;
use sefer_alloc::global::tls_heap;
use sefer_alloc::{SeferAlloc, SegmentLayout};

fn directory_counts() -> (u64, u64) {
    let stats = dbg_sidecar_reservation_stats();
    #[cfg(feature = "numa-aware")]
    {
        (
            stats.numa_directory_reservations,
            stats.numa_directory_releases,
        )
    }
    #[cfg(not(feature = "numa-aware"))]
    {
        (stats.directory_reservations, stats.directory_releases)
    }
}

#[test]
fn cold_trim_drops_owner_directory_with_live_small_and_rebuilds() {
    let a = SeferAlloc::new();
    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 8).unwrap();
    let class = SegmentLayout::class_for(SegmentLayout::SMALL_MAX, 8).unwrap();
    let mut ptrs = Vec::new();
    let primordial = loop {
        // SAFETY: layout is valid; all returned pointers are freed below.
        let p = unsafe { a.alloc(layout) };
        assert!(!p.is_null());
        let base = SegmentLayout::segment_base_of(p as usize);
        if ptrs.is_empty() {
            ptrs.push(p);
            continue;
        }
        let first = SegmentLayout::segment_base_of(ptrs[0] as usize);
        ptrs.push(p);
        if base != first {
            break first;
        }
        assert!(ptrs.len() <= 32);
    };
    let sentinel = *ptrs.last().unwrap();
    assert_ne!(
        SegmentLayout::segment_base_of(sentinel as usize),
        primordial
    );
    // SAFETY: sentinel is a live allocation of at least one byte.
    unsafe { sentinel.write(0xD7) };
    // Make a free-list head in the same live Small segment for the rebuild.
    // SAFETY: valid layout; peer is freed exactly once below.
    let peer = unsafe { a.alloc(layout) };
    assert!(!peer.is_null());
    assert_eq!(
        SegmentLayout::segment_base_of(peer as usize),
        SegmentLayout::segment_base_of(sentinel as usize)
    );
    // SAFETY: peer is a distinct live allocation from this allocator.
    unsafe { a.dealloc(peer, layout) };
    for p in ptrs.into_iter().filter(|&p| p != sentinel) {
        // SAFETY: each p is a distinct live allocation from this allocator.
        unsafe { a.dealloc(p, layout) };
    }

    let heap = tls_heap::current_for_trim().expect("bound heap");
    let mut reservations = Vec::new();
    let (reserved_before, _) = directory_counts();
    for _ in 0..40 {
        if directory_counts().0 > reserved_before {
            break;
        }
        // SAFETY: heap is this thread's bound owner and remains live here.
        let handle = unsafe { (*heap).dbg_decomp_reserve_and_keep() }.expect("small reserve");
        reservations.push(handle);
    }
    assert!(
        directory_counts().0 > reserved_before,
        "directory must materialize at the threshold"
    );
    let (_, released_before_trim) = directory_counts();
    a.trim_current_thread();
    let (_, released_after_trim) = directory_counts();
    assert_eq!(
        released_after_trim - released_before_trim,
        1,
        "cold trim must release directory VM while sentinel lives"
    );
    // SAFETY: heap remains this thread's bound owner; sentinel remains live.
    assert_eq!(
        unsafe { (*heap).dbg_directory_bit_for_ptr(sentinel, class) },
        None
    );
    // SAFETY: sentinel was not freed and trim must not retire its credit.
    assert_eq!(unsafe { sentinel.read() }, 0xD7);

    let (reserved_after_trim, _) = directory_counts();
    // SAFETY: heap remains bound; this additional registration triggers the
    // existing lazy materialization and a full rebuild from the live table.
    let extra = unsafe { (*heap).dbg_decomp_reserve_and_keep() }.expect("rematerialize");
    reservations.push(extra);
    assert_eq!(directory_counts().0, reserved_after_trim + 1);
    // SAFETY: heap and sentinel are still live; the bit reflects peer's
    // post-flush free-list head in sentinel's Small segment.
    assert_eq!(
        unsafe { (*heap).dbg_directory_bit_for_ptr(sentinel, class) },
        Some(true)
    );

    for handle in reservations {
        // SAFETY: every handle was issued by this same live heap and has not
        // been released; no trim operation releases unpooled handles.
        unsafe { (*heap).dbg_decomp_release(handle) };
    }
    // SAFETY: sentinel came from this allocator with layout.
    unsafe { a.dealloc(sentinel, layout) };
    a.trim_current_thread();
}
