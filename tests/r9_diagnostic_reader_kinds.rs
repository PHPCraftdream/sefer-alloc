//! R9-01: Small-only diagnostic readers must reject a live Large segment.

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use core::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

const FREE_LIST_NULL: u32 = u32::MAX;

fn live_large() -> (AllocCore, *mut u8, Layout) {
    let mut core = AllocCore::new().expect("primordial segment reservation");
    let layout = Layout::from_size_align(2 * 1024 * 1024, 16).unwrap();
    let ptr = core.alloc(layout);
    assert!(!ptr.is_null());
    assert_eq!(core.dbg_kind_at_tag(ptr), 2, "fixture must be Large");
    (core, ptr, layout)
}

#[test]
fn large_has_no_small_freelist_head() {
    let (mut core, ptr, layout) = live_large();
    // Native negative control: without the kind gate, the Small head reads
    // these zeroed Large payload bytes as 0 rather than FREE_LIST_NULL.
    #[cfg(not(miri))]
    {
        // SAFETY: ptr is a live, writable allocation of layout.size() bytes.
        unsafe { ptr.write_bytes(0, layout.size()) };
    }
    let biased = core::ptr::without_provenance_mut::<u8>(ptr.addr());
    assert_eq!(core.dbg_freelist_head_for(ptr, 0), FREE_LIST_NULL);
    assert_eq!(core.dbg_freelist_head_for(biased, 0), FREE_LIST_NULL);
    // SAFETY: ptr is still live and is freed exactly once with its layout.
    unsafe { core.dealloc(ptr, layout) };
}

#[test]
fn large_has_no_small_free_bit() {
    let (mut core, ptr, layout) = live_large();
    // Native negative control: without the kind gate, the Small bitmap reads
    // a set bit from these Large payload bytes and incorrectly returns true.
    #[cfg(not(miri))]
    {
        // SAFETY: ptr is a live, writable allocation of layout.size() bytes.
        unsafe { ptr.write_bytes(0xff, layout.size()) };
    }
    let biased = core::ptr::without_provenance_mut::<u8>(ptr.addr());
    assert!(!core.dbg_is_free_for(ptr));
    assert!(!core.dbg_is_free_for(biased));
    // SAFETY: ptr is still live and is freed exactly once with its layout.
    unsafe { core.dealloc(ptr, layout) };
}

#[test]
fn small_still_exposes_real_head_and_free_bit() {
    let mut core = AllocCore::new().expect("primordial segment reservation");
    let layout = Layout::from_size_align(64, 8).unwrap();
    let anchor = core.alloc(layout);
    let freed = core.alloc(layout);
    assert!(!anchor.is_null() && !freed.is_null());
    assert_eq!(
        anchor.addr() & !(SegmentLayout::SEGMENT - 1),
        freed.addr() & !(SegmentLayout::SEGMENT - 1),
        "fixture blocks must share a segment"
    );
    assert!(core.dbg_kind_at_tag(anchor) <= 1);
    let class = core.dbg_layout_class_for(layout).expect("small class");
    assert!(!core.dbg_is_free_for(freed));
    // SAFETY: freed is a live allocation of this core, freed once with its layout.
    unsafe { core.dealloc(freed, layout) };
    assert_ne!(core.dbg_freelist_head_for(anchor, class), FREE_LIST_NULL);
    assert!(core.dbg_is_free_for(freed));
    assert!(!core.dbg_is_free_for(anchor));
    // SAFETY: anchor remains live and is freed once with its layout.
    unsafe { core.dealloc(anchor, layout) };
}
