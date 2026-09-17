//! The real dirty notification remains usable after its segment is released.

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "alloc-decommit",
    feature = "alloc-segment-directory",
    feature = "internals",
    feature = "bench-internals"
))]
#[test]
fn delayed_notification_survives_actual_segment_release() {
    use sefer_alloc::registry::{HeapCore, HeapRegistry};
    use sefer_alloc::SegmentLayout;
    use std::alloc::Layout;

    // System remains the global allocator; this is the binary's only registry test.
    // Explicit pool drain below also works when claim reuses an existing slot.
    let heap_ptr = HeapRegistry::claim();
    assert!(!heap_ptr.is_null());
    // SAFETY: this test owns the claim until recycle below.
    let heap = unsafe { &mut *heap_ptr };
    let layout = Layout::from_size_align(64 * 1024, 16).unwrap();
    let class = SegmentLayout::class_for(layout.size(), layout.align()).unwrap();
    let mut bases = Vec::new();
    let mut blocks = Vec::new();
    for _ in 0..256 {
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null());
        let base = heap.dbg_segment_base_of_ptr(ptr);
        if !bases.contains(&base) {
            bases.push(base);
        }
        blocks.push(ptr);
        if bases.len() == 3 {
            break;
        }
    }
    assert_eq!(bases.len(), 3, "need a non-primordial, non-current segment");
    let target_base = bases[1];
    let victim = *blocks
        .iter()
        .find(|&&p| heap.dbg_segment_base_of_ptr(p) == target_base)
        .unwrap();
    for ptr in blocks {
        if ptr != victim {
            // SAFETY: each other live block is freed exactly once with its layout.
            unsafe { heap.dealloc(ptr, layout) };
        }
    }
    #[cfg(feature = "fastbin")]
    heap.dbg_flush_all();
    assert_eq!(heap.dbg_live_count_for(victim), Some(1));

    // SAFETY: victim is still live; the returned closure retains no segment pointer.
    let notify = unsafe { HeapCore::dbg_resolve_dirty_notification(victim, layout) }.unwrap();
    // SAFETY: this is the victim's sole logical free, with its exact size class.
    assert!(unsafe { heap.dbg_push_to_ring(victim, class) });
    heap.dbg_drain_all_rings();
    heap.dbg_drain_small_pool();
    assert!(
        !heap.dbg_contains_base(target_base),
        "segment must be released before apply"
    );
    let applied = std::thread::spawn(notify)
        .join()
        .expect("delayed notifier panicked");
    assert!(
        applied,
        "the real helper must transition its dirty bit after release"
    );

    // SAFETY: no live user blocks remain and this test still owns the claim.
    unsafe { HeapRegistry::recycle(heap_ptr) };
}
