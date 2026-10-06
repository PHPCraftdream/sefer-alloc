#![cfg(all(
    feature = "batch-api",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::registry::{HeapCore, HeapRegistry};

static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

const LARGE: usize = 2 * 1024 * 1024;

fn large_layout() -> Layout {
    Layout::from_size_align(LARGE, 8).unwrap()
}

fn small_layout() -> Layout {
    Layout::from_size_align(64, 8).unwrap()
}

/// One batch of one block with an empty magazine: always a magazine miss.
fn batch_small_miss(heap: &mut HeapCore) -> *mut u8 {
    let mut out = [core::ptr::null_mut(); 1];
    assert_eq!(heap.alloc_batch(small_layout(), &mut out), 1);
    assert!(!out[0].is_null());
    out[0]
}

fn alloc_large_live(heap: &mut HeapCore, count: usize) -> Vec<*mut u8> {
    (0..count)
        .map(|_| {
            let ptr = heap.alloc(large_layout());
            assert!(!ptr.is_null());
            ptr
        })
        .collect()
}

#[test]
fn small_batch_miss_probes_at_most_the_hot_budget() {
    let _lock = TEST_LOCK.lock().unwrap();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim heap");
    let heap = lease.core();
    let large_live = alloc_large_live(heap, 64);
    assert_eq!(heap.dbg_active_kind_census(), (1, 64, true));
    let budget = HeapCore::dbg_large_hot_budget();
    let before = HeapCore::dbg_large_sidecar_slot_inspections();
    let rescues = HeapCore::dbg_large_sidecar_full_rescues();
    let small_ptr = batch_small_miss(heap);
    let probes = HeapCore::dbg_large_sidecar_slot_inspections() - before;
    assert!(probes > 0, "active Large routes must activate the oracle");
    assert!(
        probes <= budget as u64,
        "inspected {probes}; budget={budget}"
    );
    assert_eq!(HeapCore::dbg_large_sidecar_full_rescues(), rescues);
    println!("R12-05 small L=64 K=1 B={budget} inspections={probes} rescues=0");
    // SAFETY: every pointer is a distinct live issue of the matching layout.
    unsafe {
        heap.dealloc(small_ptr, small_layout());
        for ptr in large_live {
            heap.dealloc(ptr, large_layout());
        }
    }
}

#[test]
fn large_batch_probes_at_most_the_hot_budget() {
    let _lock = TEST_LOCK.lock().unwrap();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim heap");
    let heap = lease.core();
    let live = alloc_large_live(heap, 9);
    assert_eq!(heap.dbg_active_kind_census(), (1, 9, true));
    let budget = HeapCore::dbg_large_hot_budget();
    let before = HeapCore::dbg_large_sidecar_slot_inspections();
    let rescues = HeapCore::dbg_large_sidecar_full_rescues();
    let mut out = [core::ptr::null_mut(); 1];
    assert_eq!(heap.alloc_batch(large_layout(), &mut out), 1);
    assert!(!out[0].is_null());
    let probes = HeapCore::dbg_large_sidecar_slot_inspections() - before;
    assert!(
        probes > 0 && probes <= budget as u64,
        "inspected {probes}; budget={budget}"
    );
    assert_eq!(HeapCore::dbg_large_sidecar_full_rescues(), rescues);
    println!("R12-05 large L=9 K=1 B={budget} inspections={probes} rescues=0");
    // SAFETY: every pointer is a distinct live issue of the matching layout.
    unsafe {
        heap.dealloc(out[0], large_layout());
        for ptr in live {
            heap.dealloc(ptr, large_layout());
        }
    }
}

#[test]
fn late_large_publication_is_retired_by_rotating_small_batch_probes() {
    let _lock = TEST_LOCK.lock().unwrap();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim heap");
    let heap = lease.core();
    let mut large = alloc_large_live(heap, 9);
    let table_count = heap.dbg_table_count() as usize;
    let victim = large.pop().unwrap();
    let route = RouteDirectory::global().lookup(victim).unwrap();
    // SAFETY: victim is one unique live Large issue transferred once to its route.
    assert!(unsafe { route.publish_large() });
    assert_eq!(heap.dbg_active_kind_census(), (1, 9, true));
    let budget = HeapCore::dbg_large_hot_budget();
    let max_misses = table_count.div_ceil(budget) + 1;
    let rescues = HeapCore::dbg_large_sidecar_full_rescues();
    let mut small = Vec::new();
    let mut misses = 0;
    while heap.dbg_active_kind_census().1 == 9 {
        assert!(misses < max_misses, "not retired after {misses} misses");
        let before = HeapCore::dbg_large_sidecar_slot_inspections();
        small.push(batch_small_miss(heap));
        let probes = HeapCore::dbg_large_sidecar_slot_inspections() - before;
        assert!(probes <= budget as u64, "miss {misses}: inspected {probes}");
        misses += 1;
    }
    assert_eq!(heap.dbg_active_kind_census(), (1, 8, true));
    assert_eq!(HeapCore::dbg_large_sidecar_full_rescues(), rescues);
    println!(
        "R12-05 late publication retired after {misses} misses (table={table_count} B={budget})"
    );
    // SAFETY: the victim was transferred to its route; the rest are live.
    unsafe {
        for ptr in small {
            heap.dealloc(ptr, small_layout());
        }
        for ptr in large {
            heap.dealloc(ptr, large_layout());
        }
    }
}
