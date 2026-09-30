#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, LargeCacheConfig, SegmentLayout, SmallSegmentPoolConfig};

#[test]
fn three_real_small_roots_recycle_middle_and_drop() {
    let config = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(0));
    let mut core = AllocCore::new_with_config(config).unwrap();
    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 1).unwrap();
    assert!(SegmentLayout::class_for(SegmentLayout::SMALL_MAX, 1).is_some());
    let mut blocks = Vec::new();
    for _ in 0..96 {
        let ptr = core.alloc(layout);
        assert!(!ptr.is_null());
        let id = core.dbg_segment_id_of(ptr);
        blocks.push((ptr, id));
        if id == 2 {
            break;
        }
    }
    assert_eq!(core.dbg_active_kind_census(), (3, 0, true));
    let middle: Vec<_> = blocks
        .iter()
        .filter(|(_, id)| *id == 1)
        .map(|(ptr, _)| *ptr)
        .collect();
    assert!(!middle.is_empty());
    for ptr in middle {
        // SAFETY: each middle-segment block is live and freed once.
        unsafe { core.dealloc(ptr, layout) };
    }
    assert_eq!(core.dbg_active_kind_census(), (2, 0, true));
    for (ptr, id) in blocks {
        if id != 1 {
            // SAFETY: these blocks were not freed in the middle-segment loop.
            unsafe { core.dealloc(ptr, layout) };
        }
    }
    assert!(core.dbg_active_kind_census().2);
}
