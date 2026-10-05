#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals"
))]

use core::alloc::Layout;
use sefer_alloc::registry::segment_route::RouteDirectory;
use sefer_alloc::registry::{HeapCore, HeapRegistry};
use sefer_alloc::SegmentLayout;

static TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn miss_and_empty(heap: &mut HeapCore, class: usize, out: &mut Vec<*mut u8>) {
    let layout = Layout::from_size_align(64, 8).unwrap();
    assert_eq!(heap.dbg_tcache_count(class), 0);
    let first = heap.alloc(layout);
    assert!(!first.is_null());
    out.push(first);
    let retained = heap.dbg_tcache_count(class);
    assert!(retained > 0, "the measured path must refill a magazine");
    for _ in 0..retained {
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null());
        out.push(ptr);
    }
    assert_eq!(heap.dbg_tcache_count(class), 0);
}

#[test]
fn real_heap_refills_charge_fixed_large_probe_budget_at_l8_and_l64() {
    let _lock = TEST_LOCK.lock().unwrap();
    let large_layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let small_layout = Layout::from_size_align(64, 8).unwrap();
    let class = SegmentLayout::class_for(64, 8).unwrap();
    const MISSES: usize = 6;
    let budget = HeapCore::dbg_large_hot_budget();
    assert!(budget > 0 && budget < 8);
    for large_count in [8, 64] {
        let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
        let heap = lease.core();
        let mut large = Vec::with_capacity(large_count);
        for _ in 0..large_count {
            let ptr = heap.alloc(large_layout);
            assert!(!ptr.is_null());
            large.push(ptr);
        }
        assert_eq!(heap.dbg_active_kind_census(), (1, large_count, true));
        let before = HeapCore::dbg_large_sidecar_slot_inspections();
        let rescues = HeapCore::dbg_large_sidecar_full_rescues();
        let mut small = Vec::new();
        for _ in 0..MISSES {
            miss_and_empty(heap, class, &mut small);
        }
        let probes = HeapCore::dbg_large_sidecar_slot_inspections() - before;
        assert_eq!(probes, (MISSES * budget) as u64, "L={large_count}");
        println!("L={large_count} K={MISSES} B={budget} probes={probes}");
        assert_eq!(HeapCore::dbg_large_sidecar_full_rescues(), rescues);
        assert_eq!(heap.dbg_active_kind_census(), (1, large_count, true));
        // SAFETY: every pointer is a distinct live issue of the matching layout.
        unsafe {
            for ptr in small {
                heap.dealloc(ptr, small_layout);
            }
            for ptr in large {
                heap.dealloc(ptr, large_layout);
            }
        }
    }
}

#[test]
fn recycled_table_slot_behind_cursor_is_revisited() {
    let _lock = TEST_LOCK.lock().unwrap();
    let large_layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let small_layout = Layout::from_size_align(64, 8).unwrap();
    let class = SegmentLayout::class_for(64, 8).unwrap();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let mut large = Vec::new();
    for _ in 0..8 {
        let ptr = heap.alloc(large_layout);
        assert!(!ptr.is_null());
        large.push(ptr);
    }
    let table_count = heap.dbg_table_count();
    let victim = large.remove(2);
    let old = RouteDirectory::global().lookup(victim).unwrap();
    let old_incarnation = old.incarnation();
    // SAFETY: victim is one unique current Large issue.
    assert!(unsafe { old.publish_large() });
    let mut small = Vec::new();
    let max_misses = (table_count as usize).div_ceil(HeapCore::dbg_large_hot_budget()) + 1;
    let mut first_misses = 0;
    while heap.dbg_active_kind_census().1 == 8 {
        assert!(first_misses < max_misses);
        miss_and_empty(heap, class, &mut small);
        first_misses += 1;
    }
    assert_eq!(heap.dbg_active_kind_census(), (1, 7, true));
    let reissued = heap.alloc(large_layout);
    assert!(!reissued.is_null());
    assert_eq!(heap.dbg_table_count(), table_count);
    let next = RouteDirectory::global().lookup(reissued).unwrap();
    assert_ne!(next.incarnation(), old_incarnation);
    // SAFETY: reissued is a new unique Large issue.
    assert!(unsafe { next.publish_large() });
    let mut next_misses = 0;
    while heap.dbg_active_kind_census().1 == 8 {
        assert!(next_misses < max_misses);
        miss_and_empty(heap, class, &mut small);
        next_misses += 1;
    }
    assert_eq!(heap.dbg_active_kind_census(), (1, 7, true));
    println!("reused slot: first misses={first_misses}, next misses={next_misses}");
    // SAFETY: transferred issues are excluded; the rest remain live.
    unsafe {
        for ptr in small {
            heap.dealloc(ptr, small_layout);
        }
        for ptr in large {
            heap.dealloc(ptr, large_layout);
        }
    }
}

#[test]
fn strict_drain_still_retires_far_terminal_large_in_one_pass() {
    let _lock = TEST_LOCK.lock().unwrap();
    let layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let mut live = Vec::new();
    for _ in 0..9 {
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null());
        live.push(ptr);
    }
    let victim = live.pop().unwrap();
    let pin = RouteDirectory::global().lookup(victim).unwrap();
    // SAFETY: victim is a current unique issue transferred once to its route.
    assert!(unsafe { pin.publish_large() });
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 1);
    assert_eq!(heap.dbg_drain_sidecar_ingress(), 0);
    assert_eq!(heap.dbg_active_kind_census(), (1, 8, true));
    // SAFETY: only these eight issues remain caller-owned.
    unsafe {
        for ptr in live {
            heap.dealloc(ptr, layout);
        }
    }
}

#[test]
fn logical_capacity_failure_gets_one_full_rescue_and_one_retry() {
    let _lock = TEST_LOCK.lock().unwrap();
    let layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let mut live = Vec::new();
    for _ in 0..9 {
        let ptr = heap.alloc(layout);
        assert!(!ptr.is_null());
        live.push(ptr);
    }
    let victim = live.pop().unwrap();
    let pin = RouteDirectory::global().lookup(victim).unwrap();
    // SAFETY: victim is a current unique issue transferred once to its route.
    assert!(unsafe { pin.publish_large() });
    let probes = HeapCore::dbg_large_sidecar_slot_inspections();
    let rescues = HeapCore::dbg_large_sidecar_full_rescues();
    assert_eq!(heap.dbg_large_rescue_refill_model(), (1, 2));
    assert_eq!(HeapCore::dbg_large_sidecar_full_rescues() - rescues, 1);
    assert_eq!(HeapCore::dbg_large_sidecar_slot_inspections() - probes, 9);
    assert_eq!(heap.dbg_active_kind_census(), (1, 8, true));
    // SAFETY: only these eight issues remain caller-owned.
    unsafe {
        for ptr in live {
            heap.dealloc(ptr, layout);
        }
    }
}

#[cfg(feature = "virgin-zero-skip")]
#[test]
fn virgin_zeroed_magazine_refills_share_the_large_hot_budget() {
    let _lock = TEST_LOCK.lock().unwrap();
    let large_layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let small_layout = Layout::from_size_align(64, 8).unwrap();
    let class = SegmentLayout::class_for(64, 8).unwrap();
    let mut lease = HeapRegistry::dbg_claim_lease().expect("claim");
    let heap = lease.core();
    let mut large = Vec::new();
    for _ in 0..8 {
        let ptr = heap.alloc(large_layout);
        assert!(!ptr.is_null());
        large.push(ptr);
    }
    let before = HeapCore::dbg_large_sidecar_slot_inspections();
    let rescues = HeapCore::dbg_large_sidecar_full_rescues();
    let mut small = Vec::new();
    const MISSES: usize = 4;
    for _ in 0..MISSES {
        assert_eq!(heap.dbg_tcache_count(class), 0);
        let ptr = heap.alloc_zeroed(small_layout);
        assert!(!ptr.is_null());
        small.push(ptr);
        let retained = heap.dbg_tcache_count(class);
        assert!(retained > 0);
        for _ in 0..retained {
            let ptr = heap.alloc_zeroed(small_layout);
            assert!(!ptr.is_null());
            small.push(ptr);
        }
        assert_eq!(heap.dbg_tcache_count(class), 0);
    }
    let probes = HeapCore::dbg_large_sidecar_slot_inspections() - before;
    assert_eq!(probes, (MISSES * HeapCore::dbg_large_hot_budget()) as u64);
    assert_eq!(HeapCore::dbg_large_sidecar_full_rescues(), rescues);
    println!("virgin-zeroed K={MISSES} probes={probes}");
    // SAFETY: every pointer is a distinct live issue of its matching layout.
    unsafe {
        for ptr in small {
            heap.dealloc(ptr, small_layout);
        }
        for ptr in large {
            heap.dealloc(ptr, large_layout);
        }
    }
}
