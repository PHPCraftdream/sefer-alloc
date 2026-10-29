//! Depot correctness, owner-lifecycle, and regression coverage.
#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-decommit"
))]

use sefer_alloc::registry::{bootstrap, HeapRegistry};
use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

const CAP: usize = 16;
static SERIAL: AtomicBool = AtomicBool::new(false);
struct Serial;
impl Serial {
    fn lock() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        Self
    }
}
impl Drop for Serial {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}
/// Every heap in this binary is created with the depot overflow off
/// (G1-K7 budget 0), so these tests pin the K5 floor exactly; the overflow
/// has its own binaries (`tests/g1_k7_depot_overflow*.rs`). One config per
/// binary: a recycled slot keeps the config it was created with.
fn claim_floor_only() -> Option<sefer_alloc::registry::HeapLease> {
    HeapRegistry::dbg_claim_lease_with_config(
        sefer_alloc::alloc_core::LargeCacheConfig::new().depot_budget_bytes(0),
    )
}
fn l16() -> Layout {
    Layout::from_size_align(16, 8).unwrap()
}

#[test]
fn depot_budget_and_real_class_caps() {
    let _s = Serial::lock();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();
    let sizes = [16, 32, 48, 64, 80, 112, 144, 192];
    let k = [8usize, 8, 8, 8, 8, 8, 7, 1];
    let mut budget = 0;
    for (c, size) in sizes.into_iter().enumerate() {
        assert_eq!(
            heap.dbg_class_for(Layout::from_size_align(size, 8).unwrap()),
            Some(c)
        );
        assert_eq!(heap.dbg_free_park_cap(c), Some(CAP));
        budget += size * CAP * k[c];
    }
    assert_eq!(budget, 64_256);
    assert!(budget <= 65_536);
    drop(lease);
}

#[test]
fn depot_roundtrip_preserves_all_parked_addresses() {
    let _s = Serial::lock();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();
    heap.dbg_flush_all();
    let layout = l16();
    let mut blocks = Vec::with_capacity(144);
    for _ in 0..144 {
        let p = heap.alloc(layout);
        assert!(!p.is_null());
        blocks.push(p);
    }
    assert_eq!(
        heap.dbg_tcache_count(0),
        0,
        "144 allocations leave tcache empty"
    );
    let freed = &blocks[..129];
    for &p in freed {
        // SAFETY: distinct live allocations, freed exactly once.
        unsafe { heap.dealloc(p, layout) };
    }
    assert_eq!(heap.dbg_depot_len(0), 8, "precondition before reload");
    assert_eq!(heap.dbg_tcache_count(0), 1);
    let mut reissued = Vec::with_capacity(129);
    for _ in 0..129 {
        let p = heap.alloc(layout);
        assert!(!p.is_null());
        reissued.push(p);
    }
    let mut expected: Vec<_> = freed.iter().map(|p| *p as usize).collect();
    let mut actual: Vec<_> = reissued.iter().map(|p| *p as usize).collect();
    expected.sort_unstable();
    actual.sort_unstable();
    let multiset_ok = actual == expected;
    let unique_ok = actual.windows(2).all(|w| w[0] != w[1]);
    let untouched_live = blocks[129..].iter().all(|p| !reissued.contains(p));
    // SAFETY: all reissued addresses are currently live and are returned once.
    for &p in &reissued {
        unsafe { heap.dealloc(p, layout) };
    }
    drop(lease);
    assert!(
        multiset_ok,
        "D2: sorted multiset of all 129 addresses conserved"
    );
    assert!(unique_ok, "D2: reissued set contains no duplicate address");
    assert!(
        untouched_live,
        "the other 15 live allocations were not issued"
    );
}

#[test]
fn depot_overflow_flush_returns_exactly_one_magazine() {
    let _s = Serial::lock();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();
    heap.dbg_flush_all();
    let layout = l16();
    let mut blocks = Vec::with_capacity(160);
    for _ in 0..160 {
        let p = heap.alloc(layout);
        assert!(!p.is_null());
        blocks.push(p);
    }
    let initial_live = heap.dbg_live_count_for(blocks[0]).unwrap();
    for &p in &blocks[..145] {
        // SAFETY: each selected pointer is live and freed exactly once.
        unsafe { heap.dealloc(p, layout) };
    }
    assert!(
        heap.dbg_depot_len(0) > 0,
        "precondition before overflow oracle"
    );
    let free_count = blocks[..145]
        .iter()
        .filter(|&&p| heap.dbg_state_byte_state_for(p) == Some(2))
        .count();
    assert_eq!(free_count, 8, "overflow flushes half the full magazine");
    assert_eq!(
        heap.dbg_live_count_for(blocks[0]).unwrap(),
        initial_live - 8
    );
    for &p in &blocks[145..] {
        // SAFETY: these allocations remain live and each is freed exactly once.
        unsafe { heap.dealloc(p, layout) };
    }
    assert!(heap.dbg_depot_len(0) > 0, "precondition before D1 flush");
    let live_before = heap.dbg_live_count_for(blocks[145]).unwrap();
    heap.dbg_flush_all();
    assert_eq!(heap.dbg_depot_len(0), 0);
    assert_eq!(
        heap.dbg_live_count_for(blocks[145]).unwrap(),
        live_before - (CAP * 9) as u32
    );
    let free_prefix = blocks[..145]
        .iter()
        .filter(|&&p| heap.dbg_state_byte_state_for(p) == Some(2))
        .count();
    assert_eq!(
        free_prefix, 145,
        "every freed prefix block is FREE after flush"
    );
    drop(lease);
}

#[test]
fn depot_full_flush_returns_parked_blocks_and_empties_store() {
    let _s = Serial::lock();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();
    heap.dbg_flush_all();
    let layout = l16();
    let mut ps = Vec::new();
    for _ in 0..CAP * 10 {
        let p = heap.alloc(layout);
        assert!(!p.is_null());
        ps.push(p);
    }
    for &p in &ps {
        // SAFETY: each pointer is live and freed exactly once.
        unsafe { heap.dealloc(p, layout) };
    }
    assert_eq!(heap.dbg_depot_len(0), 8, "precondition before flush");
    let before = heap.dbg_live_count_for(ps[0]).unwrap();
    heap.dbg_flush_all();
    assert_eq!(heap.dbg_depot_len(0), 0);
    assert_eq!(heap.dbg_tcache_count(0), 0);
    assert_eq!(
        heap.dbg_live_count_for(ps[0]).unwrap(),
        before - (CAP * 9) as u32
    );
    for &p in &ps {
        assert_eq!(heap.dbg_state_byte_state_for(p), Some(2));
    }
    drop(lease);
}

#[test]
fn parked_double_free_is_rejected_by_magazine_oracle() {
    let _s = Serial::lock();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();
    heap.dbg_flush_all();
    let layout = l16();
    let mut ps = Vec::new();
    for _ in 0..CAP * 10 {
        let p = heap.alloc(layout);
        assert!(!p.is_null());
        ps.push(p);
    }
    for &p in &ps {
        // SAFETY: each pointer is live and freed exactly once.
        unsafe { heap.dealloc(p, layout) };
    }
    assert_eq!(heap.dbg_depot_len(0), 8, "precondition before double-free");
    let parked = ps[0];
    assert_eq!(heap.dbg_state_byte_state_for(parked), Some(1));
    assert!(!heap.dbg_tcache_contains(0, parked));
    let live = heap.dbg_live_count_for(parked).unwrap();
    // SAFETY: deliberate duplicate free invokes the MAG oracle.
    unsafe { heap.dealloc(parked, layout) };
    assert_eq!(heap.dbg_state_byte_state_for(parked), Some(1));
    assert_eq!(heap.dbg_live_count_for(parked).unwrap(), live);
    assert_eq!(heap.dbg_depot_len(0), 8);
    drop(lease);
}

// `exact-object-proto` serves narrow layouts from System, bypassing the heap.
#[cfg(not(feature = "exact-object-proto"))]
#[test]
fn explicit_current_thread_trim_drains_depot() {
    let _s = Serial::lock();
    let _ = bootstrap::ensure();
    let alloc = sefer_alloc::SeferAlloc::new();
    let layout = l16();
    let mut held = Vec::new();
    for _ in 0..CAP * 10 {
        // SAFETY: valid non-zero layout.
        let p = unsafe { std::alloc::GlobalAlloc::alloc(&alloc, layout) };
        assert!(!p.is_null());
        held.push(p);
    }
    for &p in &held {
        // SAFETY: p was allocated above and is freed exactly once.
        unsafe { std::alloc::GlobalAlloc::dealloc(&alloc, p, layout) };
    }
    use sefer_alloc::global::tls_heap;
    let owner = match tls_heap::current_for_alloc() {
        tls_heap::CurrentHeap::Own(p) => p,
        tls_heap::CurrentHeap::Fallback => panic!("expected bound current-thread heap"),
    };
    // SAFETY: owner is this thread's own live heap while its TLS lease is held.
    let occupied = unsafe { (*owner).dbg_depot_len(0) };
    assert!(occupied > 0, "precondition before trim_current_thread");
    alloc.trim_current_thread();
    // SAFETY: this thread's owner heap remains bound after trim.
    assert_eq!(unsafe { (*owner).dbg_depot_len(0) }, 0);
}

#[test]
fn lease_recycle_clears_depot_and_reuses_same_slot() {
    let _s = Serial::lock();
    let _ = bootstrap::ensure();
    let child = thread::spawn(|| {
        let mut lease = claim_floor_only().expect("claim");
        let slot = lease.slot_index();
        let heap = lease.core();
        heap.dbg_flush_all();
        let layout = l16();
        let mut blocks = Vec::new();
        for _ in 0..CAP * 10 {
            let p = heap.alloc(layout);
            assert!(!p.is_null());
            blocks.push(p);
        }
        for &p in &blocks {
            // SAFETY: each allocation is live and is freed exactly once.
            unsafe { heap.dealloc(p, layout) };
        }
        assert!(
            heap.dbg_depot_len(0) > 0,
            "precondition before lease recycle"
        );
        let live_before = heap.dbg_live_count_for(blocks[0]).unwrap();
        drop(lease);
        let mut reclaimed = claim_floor_only().expect("reclaim");
        assert_eq!(reclaimed.slot_index(), slot, "same slot must be reused");
        assert_eq!(reclaimed.core().dbg_depot_len(0), 0);
        assert_eq!(reclaimed.slot_index(), slot, "same slot must be reused");
        let recycled_live = reclaimed.core().dbg_live_count_for(blocks[0]).unwrap();
        let all_free = blocks
            .iter()
            .all(|&p| reclaimed.core().dbg_state_byte_state_for(p) == Some(2));
        drop(reclaimed);
        assert!(all_free, "every recycled allocation becomes FREE");
        assert!(
            recycled_live <= live_before,
            "live count does not increase on recycle"
        );
    });
    child.join().unwrap();
}

#[cfg(feature = "virgin-zero-skip")]
#[test]
fn depot_reload_is_conservatively_non_virgin() {
    let _s = Serial::lock();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();
    heap.dbg_flush_all();
    let layout = l16();
    let mut dirty = Vec::new();
    for _ in 0..CAP * 10 {
        let p = heap.alloc(layout);
        assert!(!p.is_null());
        // SAFETY: p points to a live 16-byte allocation.
        unsafe { std::ptr::write_bytes(p, 0xA5, 16) };
        dirty.push(p);
    }
    for &p in &dirty {
        // SAFETY: p is live and freed exactly once.
        unsafe { heap.dealloc(p, layout) };
    }
    assert!(heap.dbg_depot_len(0) > 0, "precondition before reload");
    while heap.dbg_tcache_count(0) > 0 {
        let p = heap.alloc(layout);
        assert!(!p.is_null());
    }
    assert!(
        heap.dbg_depot_len(0) > 0,
        "precondition before zeroed reload"
    );
    let issued = heap.alloc_zeroed(layout);
    assert!(!issued.is_null());
    let reload_count = heap.dbg_tcache_count(0);
    let reload_occupancy = heap.dbg_depot_len(0);
    let virgin_mask = heap.dbg_tcache_virgin_mask(0);
    let mut bytes = [1u8; 16];
    // SAFETY: issued points to a live allocation with at least 16 bytes.
    unsafe { std::ptr::copy_nonoverlapping(issued, bytes.as_mut_ptr(), bytes.len()) };
    // SAFETY: issued is live and freed exactly once.
    unsafe { heap.dealloc(issued, layout) };
    drop(lease);
    assert_eq!(reload_count, CAP as u16 - 1);
    assert_eq!(reload_occupancy, 7);
    assert_eq!(virgin_mask, 0);
    assert_eq!(bytes, [0; 16]);
}

/// G1-K5: classes 240..1024 B have a depot of two magazines; the parked-block
/// bound per thread stays within the const-asserted budget.
#[test]
fn depot_mid_classes_budget_and_caps() {
    let _s = Serial::lock();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();
    let sizes = [240, 256, 304, 384, 480, 512, 608, 768, 960, 1024];
    let mut budget = 64_256;
    for (i, size) in sizes.into_iter().enumerate() {
        let c = 8 + i;
        assert_eq!(
            heap.dbg_class_for(Layout::from_size_align(size, 8).unwrap()),
            Some(c)
        );
        assert_eq!(heap.dbg_free_park_cap(c), Some(CAP));
        budget += size * CAP * 2;
    }
    assert_eq!(budget, 241_408);
    assert!(budget <= 240 * 1024);
    drop(lease);
}

/// G1-K5: a mid class parks exactly two full magazines, the next overflow
/// takes the flush arm, and reloads hand back only blocks this test freed.
/// A class above 1024 B never parks.
#[test]
fn depot_mid_class_parks_two_and_reloads() {
    let _s = Serial::lock();
    let _ = bootstrap::ensure();
    let mut lease = claim_floor_only().expect("claim");
    let heap = lease.core();
    for size in [256usize, 1024] {
        heap.dbg_flush_all();
        let layout = Layout::from_size_align(size, 8).unwrap();
        let c = heap.dbg_class_for(layout).unwrap();
        let blocks: Vec<*mut u8> = (0..3 * CAP + 1).map(|_| heap.alloc(layout)).collect();
        assert!(blocks.iter().all(|p| !p.is_null()));
        // The last refill left blocks in the magazine; start the frees empty.
        heap.dbg_flush_all();
        assert_eq!(heap.dbg_tcache_count(c), 0);
        assert_eq!(heap.dbg_depot_len(c), 0);
        for &p in &blocks {
            // SAFETY: distinct live allocations, freed exactly once.
            unsafe { heap.dealloc(p, layout) };
        }
        assert_eq!(
            heap.dbg_depot_len(c),
            2,
            "size {size}: two parked magazines"
        );
        let resident = heap.dbg_tcache_count(c) as usize;
        let take = resident + 2 * CAP;
        let reissued: Vec<*mut u8> = (0..take).map(|_| heap.alloc(layout)).collect();
        assert_eq!(
            heap.dbg_depot_len(c),
            0,
            "size {size}: both magazines reloaded"
        );
        let mut uniq: Vec<usize> = reissued.iter().map(|p| *p as usize).collect();
        uniq.sort_unstable();
        uniq.dedup();
        assert_eq!(uniq.len(), take, "size {size}: no duplicate reissue");
        assert!(
            reissued.iter().all(|p| blocks.contains(p)),
            "size {size}: magazine and depot hand back only freed blocks"
        );
        for &p in &reissued {
            // SAFETY: each reissued block is live and freed once.
            unsafe { heap.dealloc(p, layout) };
        }
    }
    let big = Layout::from_size_align(1200, 8).unwrap();
    let c_big = heap.dbg_class_for(big).unwrap();
    let blocks: Vec<*mut u8> = (0..3 * CAP + 1).map(|_| heap.alloc(big)).collect();
    for &p in &blocks {
        // SAFETY: distinct live allocations, freed exactly once.
        unsafe { heap.dealloc(p, big) };
    }
    assert_eq!(heap.dbg_depot_len(c_big), 0, "1200 B has no depot");
    heap.dbg_flush_all();
    drop(lease);
}
