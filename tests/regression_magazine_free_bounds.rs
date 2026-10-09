#![cfg(all(feature = "internals", feature = "fastbin"))]

use core::alloc::Layout;
use sefer_alloc::alloc_core::SegmentLayout;
use sefer_alloc::registry::{bootstrap, HeapRegistry};

#[test]
fn magazine_free_rejects_metadata_and_uncarved_offsets() {
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim heap");
    let heap = lease.core();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let anchor = heap.alloc(layout);
    assert!(!anchor.is_null());
    let base = heap.dbg_segment_base_of_ptr(anchor);
    let payload_start = match heap.dbg_kind_at_tag(anchor) {
        0 => SegmentLayout::PRIMORDIAL_PAYLOAD_START_WORD,
        1 => SegmentLayout::SMALL_PAYLOAD_START_WORD,
        kind => panic!("expected small segment, got {kind}"),
    } * SegmentLayout::MIN_BLOCK
        * 64;
    let first = base.with_addr(base.addr() + payload_start);
    let mut issued = vec![anchor];
    while !issued.contains(&first) {
        let p = heap.alloc(layout);
        assert!(!p.is_null());
        assert_eq!(heap.dbg_segment_base_of_ptr(p), base);
        assert!(issued.len() < 64, "first payload block was not issued");
        issued.push(p);
    }
    heap.dbg_flush_all();
    assert_eq!(heap.dbg_tcache_count(0), 0);
    let metadata = base.with_addr(base.addr() + payload_start - 16);
    let uncarved = base.with_addr(base.addr() + SegmentLayout::SEGMENT - 16);
    for invalid in [metadata, uncarved] {
        // SAFETY: deliberate unsafe-contract misuse confined to this owned segment;
        // the defence-in-depth guard must reject it before mutating allocator state.
        unsafe { heap.dealloc(invalid, layout) };
        assert_eq!(heap.dbg_tcache_count(0), 0, "invalid free entered magazine");
    }
    // SAFETY: first is an issued, still-live block of this heap with matching layout.
    unsafe { heap.dealloc(first, layout) };
    assert_eq!(
        heap.dbg_tcache_count(0),
        1,
        "payload boundary free rejected"
    );
    let reused = heap.alloc(layout);
    assert_eq!(reused, first, "valid payload block was not reused");
    for p in issued {
        // SAFETY: every issued block is live exactly once, including the reissued first.
        unsafe { heap.dealloc(p, layout) };
    }
    heap.dbg_flush_all();
}

#[test]
fn small_magazine_free_rejects_metadata_and_uncarved_offsets() {
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim heap");
    let heap = lease.core();
    let layout = Layout::from_size_align(SegmentLayout::SMALL_MAX, 8).unwrap();
    let class = SegmentLayout::class_for(layout.size(), layout.align()).expect("small class");
    let block_size = SegmentLayout::SIZE_CLASS_TABLE[class];
    let payload_start = SegmentLayout::SMALL_PAYLOAD_START_WORD * SegmentLayout::MIN_BLOCK * 64;
    let first_offset = payload_start.div_ceil(block_size) * block_size;
    let limit = 2 * (SegmentLayout::SEGMENT / block_size + 1);
    let mut issued = Vec::new();
    let mut target = None;
    for _ in 0..limit {
        let p = heap.alloc(layout);
        assert!(!p.is_null());
        issued.push(p);
        if heap.dbg_kind_at_tag(p) == 1 {
            let base = heap.dbg_segment_base_of_ptr(p);
            let first = base.with_addr(base.addr() + first_offset);
            if issued.contains(&first) {
                target = Some((base, first));
                break;
            }
        }
    }
    let (base, first) = target.expect("geometry-bounded allocation must reach first Small block");
    heap.dbg_flush_all();
    assert_eq!(heap.dbg_tcache_count(class), 0);
    let metadata = base.with_addr(base.addr() + payload_start - SegmentLayout::MIN_BLOCK);
    let uncarved = base.with_addr(base.addr() + SegmentLayout::SEGMENT - block_size);
    for invalid in [metadata, uncarved] {
        // SAFETY: intentionally violates the deallocation contract in an owned segment;
        // this tests only defence-in-depth rejection, not supported caller behaviour.
        unsafe { heap.dealloc(invalid, layout) };
        assert_eq!(
            heap.dbg_tcache_count(class),
            0,
            "invalid Small free entered magazine"
        );
    }
    // SAFETY: first was issued with this layout and has not been freed.
    unsafe { heap.dealloc(first, layout) };
    assert_eq!(heap.dbg_tcache_count(class), 1, "valid Small free rejected");
    assert_eq!(
        heap.alloc(layout),
        first,
        "first Small block was not reused"
    );
    for p in issued {
        // SAFETY: all issued blocks remain live exactly once, including reissued first.
        unsafe { heap.dealloc(p, layout) };
    }
    heap.dbg_flush_all();
}
