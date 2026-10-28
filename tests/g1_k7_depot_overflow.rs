//! G1-K7: the depot overflow — full magazines parked above the depot floor
//! within the heap's byte budget. One config per binary (a recycled slot keeps
//! the config it was created with): every heap here has a 16 KiB budget, so
//! class 64 B (class 3, 1 KiB per magazine) parks 16 overflow records above
//! its floor of 8.
#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-decommit"
))]

use sefer_alloc::alloc_core::LargeCacheConfig;
use sefer_alloc::registry::{bootstrap, HeapLease, HeapRegistry};
use std::alloc::Layout;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

const BUDGET: usize = 16 * 1024;
const C64: usize = 3;
const FLOOR64: usize = 8;
const OVF64: usize = BUDGET / (64 * 16);
const C16: usize = 0;

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

fn claim() -> HeapLease {
    let _ = bootstrap::ensure();
    HeapRegistry::dbg_claim_lease_with_config(LargeCacheConfig::new().depot_budget_bytes(BUDGET))
        .expect("claim")
}

fn l(size: usize) -> Layout {
    Layout::from_size_align(size, 8).unwrap()
}

const BYTES: usize = 0;
const PARKS: usize = 1;
const RELOADS: usize = 2;
const BUDGET_REFUSALS: usize = 3;
const DECAY: usize = 5;
const DECOMMITS: usize = 7;

/// Allocate `n` blocks of `size`, all distinct and non-null.
fn alloc_n(heap: &mut sefer_alloc::registry::HeapCore, size: usize, n: usize) -> Vec<*mut u8> {
    let v: Vec<_> = (0..n).map(|_| heap.alloc(l(size))).collect();
    assert!(v.iter().all(|p| !p.is_null()));
    v
}

fn free_all(heap: &mut sefer_alloc::registry::HeapCore, size: usize, v: &[*mut u8]) {
    for &p in v {
        // SAFETY: each pointer is a distinct live allocation of `size`.
        unsafe { heap.dealloc(p, l(size)) };
    }
}

#[test]
fn budget_resolves_and_reads_back() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    assert_eq!(heap.dbg_depot_budget(), (BUDGET, BUDGET));
    assert_eq!(
        LargeCacheConfig::new()
            .depot_budget_bytes(1000)
            .dbg_resolved_depot_budget(),
        768,
        "rounded down to 256 B"
    );
    assert_eq!(
        LargeCacheConfig::new()
            .depot_budget_bytes(usize::MAX)
            .dbg_resolved_depot_budget(),
        (u16::MAX as usize - 8) * 256,
        "capped so PerClass::parked (u16) cannot wrap"
    );
    assert_eq!(
        LargeCacheConfig::new().dbg_resolved_depot_budget(),
        1024 * 1024
    );
    drop(lease);
}

#[test]
fn overflow_parks_up_to_budget_then_flushes_and_reloads_only_freed_blocks() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    heap.dbg_flush_all();
    let before = heap.dbg_depot_overflow();
    // Live magazine + floor + overflow + 3 more magazines that must flush.
    let n = 16 * (1 + FLOOR64 + OVF64 + 3);
    let blocks = alloc_n(heap, 64, n);
    free_all(heap, 64, &blocks);
    let after = heap.dbg_depot_overflow();
    assert_eq!(
        heap.dbg_depot_len(C64),
        FLOOR64 + OVF64,
        "floor + full overflow"
    );
    assert_eq!(
        after[BYTES] as usize, BUDGET,
        "overflow holds exactly the budget"
    );
    assert_eq!(after[PARKS] - before[PARKS], OVF64 as u64);
    assert!(
        after[BUDGET_REFUSALS] > before[BUDGET_REFUSALS],
        "over budget -> flush"
    );
    // Reissue: every block comes back exactly once, only freed ones.
    let freed: HashSet<usize> = blocks.iter().map(|p| *p as usize).collect();
    let again = alloc_n(heap, 64, n);
    let got: HashSet<usize> = again.iter().map(|p| *p as usize).collect();
    assert_eq!(got.len(), n, "no duplicate address");
    assert_eq!(got, freed, "reissued exactly the freed blocks");
    let reloaded = heap.dbg_depot_overflow();
    assert_eq!(reloaded[RELOADS] - after[RELOADS], OVF64 as u64);
    assert_eq!(reloaded[BYTES], 0);
    free_all(heap, 64, &again);
    heap.dbg_flush_all();
    drop(lease);
}

#[test]
fn double_free_of_an_overflow_parked_block_is_rejected() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    heap.dbg_flush_all();
    let n = 16 * (1 + FLOOR64 + 2);
    let blocks = alloc_n(heap, 64, n);
    free_all(heap, 64, &blocks);
    assert_eq!(heap.dbg_depot_len(C64), FLOOR64 + 2, "two overflow records");
    let depth = heap.dbg_depot_len(C64);
    let mag = heap.dbg_tcache_count(C64);
    // Magazine k (blocks 16(k-1)..16k-1) parks at free 16k: k = 1..8 fill the
    // floor, k = 9, 10 the overflow; block 16 * 9 is in overflow record 2.
    let parked = blocks[16 * (1 + FLOOR64)];
    assert_eq!(
        heap.dbg_state_byte_state_for(parked),
        Some(1),
        "parked block is MAG"
    );
    // SAFETY: deliberate double free of a parked block; the allocator must
    // reject it as a no-op (state byte is MAG, not LIVE).
    unsafe { heap.dealloc(parked, l(64)) };
    assert_eq!(heap.dbg_depot_len(C64), depth);
    assert_eq!(heap.dbg_tcache_count(C64), mag);
    let again = alloc_n(heap, 64, n);
    let got: HashSet<usize> = again.iter().map(|p| *p as usize).collect();
    assert_eq!(got.len(), n, "the double free did not duplicate a block");
    free_all(heap, 64, &again);
    heap.dbg_flush_all();
    drop(lease);
}

#[test]
fn trim_drains_the_overflow_and_returns_its_pages() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    heap.dbg_flush_all();
    let n = 16 * (1 + FLOOR64 + OVF64);
    let blocks = alloc_n(heap, 64, n);
    free_all(heap, 64, &blocks);
    assert!(heap.dbg_depot_overflow()[BYTES] > 0);
    let before = heap.dbg_depot_overflow();
    heap.dbg_flush_all();
    let after = heap.dbg_depot_overflow();
    assert_eq!(heap.dbg_depot_len(C64), 0);
    assert_eq!(heap.dbg_tcache_count(C64), 0);
    assert_eq!(after[BYTES], 0);
    assert!(
        after[DECOMMITS] > before[DECOMMITS],
        "committed record pages returned"
    );
    // Reuse after trim: the overflow parks again.
    let again = alloc_n(heap, 64, n);
    free_all(heap, 64, &again);
    assert!(heap.dbg_depot_overflow()[PARKS] > after[PARKS]);
    heap.dbg_flush_all();
    drop(lease);
}

#[test]
fn idle_class_overflow_decays_and_a_cycling_class_does_not() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    heap.dbg_flush_all();
    // Fill class 64's overflow, then leave it idle.
    let n = 16 * (1 + FLOOR64 + OVF64);
    let parked = alloc_n(heap, 64, n);
    free_all(heap, 64, &parked);
    assert_eq!(heap.dbg_depot_len(C64), FLOOR64 + OVF64);
    let d0 = heap.dbg_depot_overflow()[DECAY];
    // Cold events on class 16: cohorts larger than its floor park and reload
    // repeatedly (each cycle is a fill-and-empty, so class 16 never decays).
    let cohort = 16 * (1 + 8 + 4);
    for _ in 0..200 {
        let v = alloc_n(heap, 16, cohort);
        free_all(heap, 16, &v);
    }
    let d1 = heap.dbg_depot_overflow()[DECAY];
    assert!(d1 > d0, "idle class 64 overflow decayed");
    assert!(heap.dbg_depot_len(C64) < FLOOR64 + OVF64);
    assert!(
        heap.dbg_depot_len(C64) >= FLOOR64,
        "decay never touches the floor"
    );
    // Class 16 kept cycling through its overflow: its records are still there.
    assert!(
        heap.dbg_depot_len(C16) > 8,
        "cycling class not decayed away"
    );
    heap.dbg_flush_all();
    drop(lease);
}

#[test]
fn a_cycle_inside_the_budget_never_decays() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    heap.dbg_flush_all();
    let d0 = heap.dbg_depot_overflow()[DECAY];
    let n = 16 * (1 + FLOOR64 + OVF64);
    for _ in 0..300 {
        let v = alloc_n(heap, 64, n);
        free_all(heap, 64, &v);
    }
    assert_eq!(
        heap.dbg_depot_overflow()[DECAY],
        d0,
        "fill/empty cycles keep the overflow"
    );
    heap.dbg_flush_all();
    drop(lease);
}
