//! G1-K7: the floor/ring split — `PerClass::parked` counts the floor only,
//! the overflow ring its own records, and `PerClass::ovf` mirrors "ring
//! non-empty" through parks, reloads, decay and trim. Also pins that floor
//! parks keep a class active for the decay. 16 KiB budget: class 64 B parks
//! 16 records above its floor of 8.
#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-decommit"
))]

use sefer_alloc::alloc_core::LargeCacheConfig;
use sefer_alloc::registry::{bootstrap, HeapCore, HeapLease, HeapRegistry};
use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

const BUDGET: usize = 16 * 1024;
const C64: usize = 3;
const FLOOR64: usize = 8;
const OVF64: usize = BUDGET / (64 * 16);
const C16: usize = 0;
const FLOOR16: usize = 8;
const DECAY: usize = 5;

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

fn cycle(heap: &mut HeapCore, size: usize, n: usize) {
    let v: Vec<_> = (0..n).map(|_| heap.alloc(l(size))).collect();
    assert!(v.iter().all(|p| !p.is_null()));
    check(heap);
    for &p in &v {
        // SAFETY: each pointer is a distinct live allocation of `size`.
        unsafe { heap.dealloc(p, l(size)) };
    }
    check(heap);
}

/// The split's invariants for both classes in play.
fn check(heap: &HeapCore) {
    for (c, k) in [(C16, FLOOR16), (C64, FLOOR64)] {
        let (floor, ring, hint) = heap.dbg_depot_ring(c);
        assert!(floor <= k, "class {c}: floor {floor} over its K {k}");
        assert_eq!(
            hint,
            (ring > 0) as u8,
            "class {c}: hint {hint}, ring {ring}"
        );
        assert_eq!(heap.dbg_depot_len(c), floor + ring, "class {c}: depth");
    }
}

#[test]
fn the_hint_follows_the_ring_through_parks_reloads_decay_and_trim() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    heap.dbg_flush_all();
    let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
    let max = 16 * (1 + FLOOR64 + OVF64 + 2);
    for _ in 0..400 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        let size = if x & 1 == 0 { 16 } else { 64 };
        let n = 1 + (x >> 8) as usize % max;
        cycle(heap, size, n);
        if (x >> 40).is_multiple_of(97) {
            heap.dbg_flush_all();
            check(heap);
        }
    }
    // Leave class 64's ring full (the budget is shared: start from a trimmed
    // depot), then let class 16 drive epochs until the decay has emptied it:
    // the hint must follow the ring to zero.
    heap.dbg_flush_all();
    cycle(heap, 64, 16 * (1 + FLOOR64 + OVF64));
    assert_eq!(heap.dbg_depot_ring(C64).1, OVF64);
    let mut rounds = 0;
    while heap.dbg_depot_ring(C64).1 != 0 {
        cycle(heap, 16, 16 * (1 + FLOOR16 + 4));
        rounds += 1;
        assert!(rounds < 20_000, "the idle ring decays to empty");
    }
    assert_eq!(heap.dbg_depot_ring(C64), (FLOOR64, 0, 0));
    // An empty ring with an empty floor falls through to a segment refill.
    cycle(heap, 64, 16 * (2 + FLOOR64));
    heap.dbg_flush_all();
    check(heap);
    assert_eq!(heap.dbg_depot_ring(C64), (0, 0, 0));
    drop(lease);
}

#[test]
fn floor_parks_keep_a_class_active_for_the_decay() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    heap.dbg_flush_all();
    cycle(heap, 64, 16 * (1 + FLOOR64 + OVF64));
    assert_eq!(heap.dbg_depot_ring(C64), (FLOOR64, OVF64, 1));
    let d0 = heap.dbg_depot_overflow()[DECAY];
    for _ in 0..400 {
        // Class 16 drives epochs; class 64 cycles inside its floor only (the
        // floor empties first and refills first, the ring is not touched).
        cycle(heap, 16, 16 * (1 + FLOOR16 + 4));
        cycle(heap, 64, 16 * (1 + 4));
        assert_eq!(heap.dbg_depot_ring(C64).1, OVF64, "class 64 ring kept");
    }
    assert!(
        heap.dbg_depot_overflow()[DECAY] == d0 || heap.dbg_depot_ring(C16).1 < 4,
        "only class 16 may have decayed"
    );
    heap.dbg_flush_all();
    drop(lease);
}
