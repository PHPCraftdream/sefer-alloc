#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-decommit",
    feature = "internals"
))]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::Mutex;

use sefer_alloc::{SeferAlloc, SegmentLayout};

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn empty_current_small_releases_and_next_allocation_is_safe() {
    let _guard = TEST_LOCK.lock().unwrap();
    let a = SeferAlloc::new();
    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 8).unwrap();
    let mut ptrs = Vec::new();
    let mut saw_small = false;
    for _ in 0..32 {
        // SAFETY: layout is valid and each successful pointer is freed below.
        let p = unsafe { a.alloc(layout) };
        assert!(!p.is_null());
        ptrs.push(p);
        if SegmentLayout::segment_base_of(p as usize)
            != SegmentLayout::segment_base_of(ptrs[0] as usize)
        {
            saw_small = true;
            break;
        }
    }
    assert!(
        saw_small,
        "bounded allocations must advance out of primordial"
    );
    for p in ptrs {
        // SAFETY: each pointer came from this allocator with this layout.
        unsafe { a.dealloc(p, layout) };
    }
    let before = a.stats().segments_released_total;
    a.trim_current_thread();
    let after = a.stats().segments_released_total;
    assert_eq!(
        after - before,
        1,
        "empty current Small must reach OS release"
    );

    // A reused heap must never retain a dangling cursor into the released VM.
    // SAFETY: layout is valid; the pointer is freed exactly once below.
    let p = unsafe { a.alloc(layout) };
    assert!(!p.is_null());
    // SAFETY: p is live and large enough for this byte.
    unsafe { p.write(0xA5) };
    // SAFETY: p was returned for layout and has not been freed.
    assert_eq!(unsafe { p.read() }, 0xA5);
    // SAFETY: p came from this allocator with this layout.
    unsafe { a.dealloc(p, layout) };
    a.trim_current_thread();
}

#[test]
fn live_current_small_is_not_released() {
    let _guard = TEST_LOCK.lock().unwrap();
    let a = SeferAlloc::new();
    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 8).unwrap();
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
    let live = *ptrs.last().unwrap();
    for p in ptrs.into_iter().filter(|&p| p != live) {
        // SAFETY: p is a distinct live allocation from this allocator.
        unsafe { a.dealloc(p, layout) };
    }
    assert_ne!(SegmentLayout::segment_base_of(live as usize), primordial);
    // SAFETY: live remains allocated across trim.
    unsafe { live.write(0x5A) };
    let before = a.stats().segments_released_total;
    a.trim_current_thread();
    assert_eq!(a.stats().segments_released_total, before);
    // SAFETY: trim must preserve this outstanding allocation.
    assert_eq!(unsafe { live.read() }, 0x5A);
    // SAFETY: live came from this allocator with layout.
    unsafe { a.dealloc(live, layout) };
    a.trim_current_thread();
    assert!(a.stats().segments_released_total > before);
}
