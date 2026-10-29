#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use sefer_alloc::registry::HeapRegistry;
use sefer_alloc::SegmentLayout;

#[test]
fn outlined_overflow_keeps_exact_residency_state_and_credits() {
    let mut lease = HeapRegistry::dbg_claim_lease().expect("lease");
    let heap = lease.core();
    for size in [16, 16_384] {
        heap.dbg_flush_all();
        let layout = Layout::from_size_align(size, 16).unwrap();
        let class = SegmentLayout::class_for(size, 16).unwrap();
        let cap = heap.dbg_free_park_cap(class).unwrap();
        assert!((1..=16).contains(&cap));
        let blocks: Vec<_> = (0..cap + 2).map(|_| heap.alloc(layout)).collect();
        assert!(blocks.iter().all(|p| !p.is_null()));
        let anchor = blocks[cap + 1];
        heap.dbg_flush_all();
        assert_eq!(heap.dbg_tcache_count(class), 0);
        let live = heap.dbg_live_count_for(anchor).unwrap();
        for (i, &ptr) in blocks[..=cap].iter().enumerate() {
            // SAFETY: уникальный живой блок heap с исходным layout.
            unsafe { heap.dealloc(ptr, layout) };
            if i < cap {
                assert_eq!(usize::from(heap.dbg_tcache_count(class)), i + 1);
            }
        }
        let flushed = if cap == 16 { 8 } else { 1 };
        assert_eq!(usize::from(heap.dbg_tcache_count(class)), cap + 1 - flushed);
        assert_eq!(heap.dbg_live_count_for(anchor), Some(live - flushed as u32));
        for (i, &ptr) in blocks[..=cap].iter().enumerate() {
            let resident = if cap == 16 { i >= 8 } else { i != cap - 1 };
            assert_eq!(heap.dbg_tcache_contains(class, ptr), resident);
            assert_eq!(
                heap.dbg_state_byte_state_for(ptr),
                Some(if resident { 1 } else { 2 })
            );
        }
        assert!(heap.dbg_tcache_contains(class, blocks[cap]));
        #[cfg(feature = "virgin-zero-skip")]
        assert_eq!(heap.dbg_tcache_virgin_mask(class), 0);
        let reused = heap.alloc(layout);
        assert_eq!(reused, blocks[cap]);
        assert_eq!(heap.dbg_state_byte_state_for(reused), Some(0));
        // SAFETY: reused и anchor — различные живые блоки с исходным layout.
        unsafe {
            heap.dealloc(reused, layout);
            heap.dealloc(anchor, layout);
        }
        heap.dbg_flush_all();
    }
}
