#![cfg(all(
    feature = "internals",
    feature = "bench-internals",
    feature = "fastbin"
))]

use core::alloc::Layout;
use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::registry::{bootstrap, HeapRegistry};
use std::sync::Mutex;

static TEST_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn own_free_resolves_once_and_parks_every_block() {
    let _guard = TEST_LOCK.lock().unwrap();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim heap");
    let heap = lease.core();
    let layout = Layout::from_size_align(16, 8).unwrap();
    const N: usize = 16;
    let mut blocks = [core::ptr::null_mut(); N];
    for block in &mut blocks {
        *block = heap.alloc(layout);
        assert!(!block.is_null());
    }
    assert_eq!(heap.dbg_tcache_count(0), 0);
    let hits_before = AllocCore::dbg_contains_base_tier1_hits();
    let misses_before = AllocCore::dbg_contains_base_tier1_misses();
    for block in blocks {
        // SAFETY: each block is live, owned by this heap, and freed once with its allocation layout.
        unsafe { heap.dealloc(block, layout) };
    }
    let hits = AllocCore::dbg_contains_base_tier1_hits() - hits_before;
    let misses = AllocCore::dbg_contains_base_tier1_misses() - misses_before;
    let count = heap.dbg_tcache_count(0);
    println!(
        "N={N} hits={hits} misses={misses} lookups={} tcache_count={count}",
        hits + misses
    );
    assert_eq!(usize::from(count), N);
    assert_eq!(hits + misses, N as u64);
}

#[test]
fn primordial_lower_bound_and_payload_boundary() {
    use sefer_alloc::alloc_core::SegmentLayout;
    let _guard = TEST_LOCK.lock().unwrap();
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim heap");
    let heap = lease.core();
    let layout = Layout::from_size_align(16, 8).unwrap();
    let anchor = heap.alloc(layout);
    assert!(!anchor.is_null());
    assert_eq!(
        heap.dbg_kind_at_tag(anchor),
        0,
        "must exercise Primordial, not Small"
    );
    let base = heap.dbg_segment_base_of_ptr(anchor);
    let start = SegmentLayout::PRIMORDIAL_PAYLOAD_START_WORD * SegmentLayout::MIN_BLOCK * 64;
    let first = base.with_addr(base.addr() + start);
    let mut issued = vec![anchor];
    while !issued.contains(&first) {
        let block = heap.alloc(layout);
        assert!(!block.is_null());
        assert_eq!(heap.dbg_segment_base_of_ptr(block), base);
        issued.push(block);
        assert!(issued.len() <= 64);
    }
    heap.dbg_flush_all();
    // SAFETY: deliberate contract misuse inside the mapped owned Primordial
    // segment tests defence-in-depth rejection before metadata mutation.
    unsafe { heap.dealloc(base.with_addr(base.addr() + start - 16), layout) };
    assert_eq!(
        heap.dbg_tcache_count(0),
        0,
        "Primordial metadata entered magazine"
    );
    // SAFETY: first is an issued live block with matching layout.
    unsafe { heap.dealloc(first, layout) };
    assert_eq!(heap.dbg_tcache_count(0), 1, "payload boundary rejected");
    assert_eq!(heap.alloc(layout), first);
    for block in issued {
        // SAFETY: each issued block is live, including the reissued first.
        unsafe { heap.dealloc(block, layout) };
    }
}
