//! G1-K7 v5: a segment refill ticks the overflow decay only while its class
//! carries `MARK_TICK`. An overflow park arms every small class, a class's
//! own segment refill disarms it once the overflow is empty, the teardown
//! drain disarms all — and the decay still reaches records parked by a class
//! that no longer refills (pure segment refills of another class drive it).
//! 16 KiB budget: class 64 B parks 16 records above its floor of 8.
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
const ARMS: usize = 9;
const DISARMS: usize = 10;
/// A small class above the depot classes (> 1024 B): every refill is a
/// segment refill.
const NO_DEPOT: usize = 2048;

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

fn alloc_n(heap: &mut HeapCore, size: usize, n: usize) -> Vec<*mut u8> {
    let v: Vec<_> = (0..n).map(|_| heap.alloc(l(size))).collect();
    assert!(v.iter().all(|p| !p.is_null()));
    v
}

fn free_all(heap: &mut HeapCore, size: usize, v: Vec<*mut u8>) {
    for p in v {
        // SAFETY: each pointer is a distinct live allocation of `size`.
        unsafe { heap.dealloc(p, l(size)) };
    }
}

fn cycle(heap: &mut HeapCore, size: usize, n: usize) {
    let v = alloc_n(heap, size, n);
    free_all(heap, size, v);
}

/// The mirror: the per-class flags equal the overflow's armed set.
fn mirrored(heap: &HeapCore) -> u64 {
    let (flags, armed) = heap.dbg_depot_ticking();
    assert_eq!(flags, armed, "MARK_TICK flags mirror the armed set");
    armed
}

fn all_small(armed: u64) -> bool {
    armed != 0 && armed & armed.wrapping_add(1) == 0 && armed & 1 != 0
}

#[test]
fn a_park_arms_every_class_and_an_empty_overflow_disarms_lazily() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    heap.dbg_flush_all();
    assert_eq!(mirrored(heap), 0, "nothing parked: no class ticks");
    let s0 = heap.dbg_depot_overflow();

    // Fill class 64's floor and ring: the first overflow park arms all.
    cycle(heap, 64, 16 * (1 + FLOOR64 + OVF64));
    assert_eq!(heap.dbg_depot_ring(C64).1, OVF64);
    let full = mirrored(heap);
    assert!(all_small(full), "armed {full:#x} covers every small class");
    let s1 = heap.dbg_depot_overflow();
    assert_eq!(s1[ARMS] - s0[ARMS], 1, "one arming for the whole series");

    // Drain class 64's magazine, floor and ring into live blocks: the
    // overflow is empty, but no class has refilled from segments yet —
    // still armed.
    let held = alloc_n(heap, 64, 16 * (1 + FLOOR64 + OVF64));
    assert_eq!(heap.dbg_depot_ring(C64), (0, 0, 0));
    assert_eq!(heap.dbg_depot_overflow()[0], 0, "no bytes parked");
    assert_eq!(mirrored(heap), full, "disarm is lazy");

    // One more class-64 block needs a segment refill: class 64 disarms, and
    // only it.
    let extra = alloc_n(heap, 64, 1);
    let after = mirrored(heap);
    assert_eq!(full & !after, 1 << C64, "only the refilling class disarms");
    let s2 = heap.dbg_depot_overflow();
    assert_eq!(s2[DISARMS] - s1[DISARMS], 1);

    // A segment refill of a class with no depot disarms that class too.
    let big = alloc_n(heap, NO_DEPOT, 1);
    let after2 = mirrored(heap);
    assert_eq!(
        (after & !after2).count_ones(),
        1,
        "the 2048 B class disarms"
    );
    free_all(heap, NO_DEPOT, big);

    // Parking into the overflow again re-arms every class.
    free_all(heap, 64, extra);
    free_all(heap, 64, held);
    assert!(heap.dbg_depot_ring(C64).1 > 0);
    assert_eq!(mirrored(heap), full, "a park re-arms every class");
    assert_eq!(heap.dbg_depot_overflow()[ARMS] - s2[ARMS], 1);

    heap.dbg_flush_all();
    assert_eq!(mirrored(heap), 0, "the teardown drain disarms every class");
    drop(lease);
}

#[test]
fn segment_refills_of_another_class_decay_an_idle_ring() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    heap.dbg_flush_all();
    cycle(heap, 64, 16 * (1 + FLOOR64 + OVF64));
    assert_eq!(heap.dbg_depot_ring(C64).1, OVF64);
    let d0 = heap.dbg_depot_overflow()[5];
    // Only the 2048 B class runs: no depot, so every refill is a segment
    // refill and the only decay events are its ticks.
    let mut rounds = 0;
    while heap.dbg_depot_ring(C64).1 != 0 {
        cycle(heap, NO_DEPOT, 64);
        rounds += 1;
        assert!(rounds < 20_000, "segment refills decay the idle ring");
    }
    assert_eq!(heap.dbg_depot_ring(C64), (FLOOR64, 0, 0));
    assert_eq!(heap.dbg_depot_overflow()[5] - d0, OVF64 as u64);
    // Empty overflow: the next refill of the 2048 B class disarms it, and it
    // stops ticking.
    cycle(heap, NO_DEPOT, 64);
    let armed = mirrored(heap);
    cycle(heap, NO_DEPOT, 64);
    assert_eq!(mirrored(heap), armed, "a disarmed class stays disarmed");
    heap.dbg_flush_all();
    drop(lease);
}

#[test]
fn a_recycled_slot_starts_disarmed() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    heap.dbg_flush_all();
    cycle(heap, 64, 16 * (1 + FLOOR64 + OVF64));
    assert!(all_small(mirrored(heap)));
    drop(lease);
    let mut lease = claim();
    let heap = lease.core();
    assert_eq!(heap.dbg_depot_ticking(), (0, 0), "recycle disarms");
    assert_eq!(heap.dbg_depot_overflow()[0], 0);
    drop(lease);
}
