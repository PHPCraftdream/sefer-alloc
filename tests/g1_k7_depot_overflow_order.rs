//! G1-K7: exact order oracles for the depot overflow — floor first, then
//! the overflow newest first, also across the ring's wrap, oldest-first decay of
//! ⌈h/2⌉ records per idle epoch with at most `DECAY_WORK` (4) per cold event,
//! and decay driven by virgin (`alloc_zeroed`) refills too. One config per
//! binary: every heap has a 16 KiB budget, so class 64 B (1 KiB per magazine)
//! has a ring of 16 records above its floor of 8.
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
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

const BUDGET: usize = 16 * 1024;
const C64: usize = 3;
const FLOOR: usize = 8;
const RING: usize = BUDGET / (64 * 16);
const MAG: usize = 16;
const DECAY: usize = 5;
const S_MAG: u8 = 1;
const S_FREE: u8 = 2;

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

fn free_all(heap: &mut HeapCore, size: usize, v: &[*mut u8]) {
    for &p in v {
        // SAFETY: each pointer is a distinct live allocation of `size`.
        unsafe { heap.dealloc(p, l(size)) };
    }
}

/// Allocate `mags` magazines' worth of class-64 blocks from an empty
/// magazine, then free them in order: chunk `k` becomes magazine `k + 1`
/// (a full magazine parks when the next block is freed), the last chunk
/// stays live.
fn fill(heap: &mut HeapCore, mags: usize) -> Vec<HashSet<usize>> {
    assert_eq!(
        heap.dbg_tcache_count(C64),
        0,
        "starts from an empty magazine"
    );
    let blocks = alloc_n(heap, 64, MAG * mags);
    assert_eq!(heap.dbg_tcache_count(C64), 0, "refills are whole magazines");
    free_all(heap, 64, &blocks);
    chunks(&blocks)
}

fn chunks(v: &[*mut u8]) -> Vec<HashSet<usize>> {
    v.chunks(MAG)
        .map(|c| c.iter().map(|p| *p as usize).collect())
        .collect()
}

/// Reissue the magazines `parked` (chunk `k` = magazine `k + 1`; the first
/// `FLOOR` went to the floor, the last stayed live, the rest to the
/// overflow) and check the order: the live magazine, then the floor top to
/// bottom (it empties first), then the overflow newest to oldest.
fn reissue_lifo(heap: &mut HeapCore, parked: &[HashSet<usize>]) -> Vec<*mut u8> {
    let n = parked.len();
    let order: Vec<usize> = std::iter::once(n - 1)
        .chain((0..FLOOR).rev())
        .chain((FLOOR..n - 1).rev())
        .collect();
    let again = alloc_n(heap, 64, MAG * n);
    for (g, got) in chunks(&again).iter().enumerate() {
        assert_eq!(
            got,
            &parked[order[g]],
            "reissue group {g} is magazine {}",
            order[g] + 1
        );
    }
    again
}

fn states(heap: &HeapCore, mag: &HashSet<usize>) -> HashSet<u8> {
    mag.iter()
        .map(|&p| {
            heap.dbg_state_byte_state_for(p as *mut u8)
                .expect("state byte")
        })
        .collect()
}

/// Drive class-16 segment refills (cold events; class 64 stays idle) until
/// the decay counter reaches `stop` above `d0`. Returns every distinct decay
/// level seen and the class-16 blocks to free afterwards.
fn drive_decay(heap: &mut HeapCore, d0: u64, stop: u64, zeroed: bool) -> (Vec<u64>, Vec<*mut u8>) {
    let mut seen = vec![heap.dbg_depot_overflow()[DECAY] - d0];
    let mut keep = Vec::new();
    for _ in 0..64 * 1024 {
        let before = heap.dbg_depot_overflow()[DECAY];
        let p = if zeroed {
            heap.alloc_zeroed(l(16))
        } else {
            heap.alloc(l(16))
        };
        assert!(!p.is_null());
        keep.push(p);
        let after = heap.dbg_depot_overflow()[DECAY];
        assert!(after - before <= 4, "at most DECAY_WORK records per event");
        if after - d0 != *seen.last().unwrap() {
            seen.push(after - d0);
        }
        if after - d0 >= stop {
            break;
        }
    }
    (seen, keep)
}

#[test]
fn reload_empties_the_floor_first_then_the_overflow_newest_first() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    heap.dbg_flush_all();
    let parked = fill(heap, 1 + FLOOR + 2);
    assert_eq!(heap.dbg_depot_len(C64), FLOOR + 2);
    let again = reissue_lifo(heap, &parked);
    free_all(heap, 64, &again);
    heap.dbg_flush_all();
    drop(lease);
}

#[test]
fn idle_overflow_decays_oldest_first_by_half_per_epoch() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    heap.dbg_flush_all();
    let m = fill(heap, 1 + FLOOR + RING);
    assert_eq!(heap.dbg_depot_len(C64), FLOOR + RING);
    let d0 = heap.dbg_depot_overflow()[DECAY];
    let (seen, keep) = drive_decay(heap, d0, (RING / 2) as u64, false);
    assert_eq!(
        *seen.last().unwrap(),
        (RING / 2) as u64,
        "first idle epoch: ⌈16/2⌉"
    );
    // The oldest half of the ring went back to its segments; the newest
    // half, the floor and the live magazine are untouched.
    for (k, mag) in m.iter().enumerate() {
        let want = if (FLOOR..FLOOR + RING / 2).contains(&k) {
            S_FREE
        } else {
            S_MAG
        };
        assert_eq!(
            states(heap, mag),
            HashSet::from([want]),
            "magazine {}",
            k + 1
        );
    }
    assert_eq!(
        usize::from(heap.dbg_tcache_count(C64)),
        MAG,
        "live magazine kept"
    );
    assert_eq!(heap.dbg_depot_len(C64), FLOOR + RING / 2);
    let (rest, keep2) = drive_decay(heap, d0, RING as u64, false);
    let mut all = seen.clone();
    all.extend(rest.into_iter().skip(1));
    assert_eq!(
        all,
        vec![0, 4, 8, 12, 14, 15, 16],
        "⌈h/2⌉ per epoch, 4 per event"
    );
    assert_eq!(
        heap.dbg_depot_len(C64),
        FLOOR,
        "decay never touches the floor"
    );
    free_all(heap, 16, &keep);
    free_all(heap, 16, &keep2);
    heap.dbg_flush_all();
    drop(lease);
}

#[test]
fn the_ring_wraps_after_decay_and_stays_lifo() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    heap.dbg_flush_all();
    fill(heap, 1 + FLOOR + RING);
    let d0 = heap.dbg_depot_overflow()[DECAY];
    // Decay moves the ring head past the 8 oldest records.
    let (seen, keep) = drive_decay(heap, d0, (RING / 2) as u64, false);
    assert_eq!(*seen.last().unwrap(), (RING / 2) as u64);
    // Take everything back plus 7 fresh magazines, then park 15 overflow
    // records from head 8: records 8..15 then 0..6 (the wrap).
    let live_and_parked = 1 + RING / 2 + FLOOR;
    let blocks = alloc_n(heap, 64, MAG * (live_and_parked + 7));
    assert_eq!(heap.dbg_depot_len(C64), 0);
    assert_eq!(heap.dbg_tcache_count(C64), 0);
    let d1 = heap.dbg_depot_overflow()[DECAY];
    free_all(heap, 64, &blocks);
    assert_eq!(heap.dbg_depot_len(C64), FLOOR + RING - 1, "wrapped ring");
    assert_eq!(heap.dbg_depot_overflow()[0] as usize, (RING - 1) * 64 * MAG);
    let again = reissue_lifo(heap, &chunks(&blocks));
    assert_eq!(heap.dbg_depot_overflow()[DECAY], d1, "no decay mid-check");
    free_all(heap, 64, &again);
    free_all(heap, 16, &keep);
    heap.dbg_flush_all();
    drop(lease);
}

/// Regression (rush code review of `ccbd676c`): the virgin refill path
/// counted no cold event, so `alloc_zeroed`-only work never decayed.
#[cfg(feature = "virgin-zero-skip")]
#[test]
fn virgin_refills_drive_decay_too() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    heap.dbg_flush_all();
    fill(heap, 1 + FLOOR + RING);
    let d0 = heap.dbg_depot_overflow()[DECAY];
    let (seen, keep) = drive_decay(heap, d0, (RING / 2) as u64, true);
    assert_eq!(
        *seen.last().unwrap(),
        (RING / 2) as u64,
        "decayed under alloc_zeroed"
    );
    free_all(heap, 16, &keep);
    heap.dbg_flush_all();
    drop(lease);
}

/// Floor first: with the overflow full, a cohort that fits the floor cycles
/// through the floor alone — no overflow park or reload (the K5 path).
#[test]
fn a_floor_sized_cycle_never_touches_a_full_overflow() {
    let _s = Serial::lock();
    let mut lease = claim();
    let heap = lease.core();
    heap.dbg_flush_all();
    fill(heap, 1 + FLOOR + RING);
    let s0 = heap.dbg_depot_overflow();
    for _ in 0..50 {
        let v = alloc_n(heap, 64, MAG * 5);
        free_all(heap, 64, &v);
    }
    let s1 = heap.dbg_depot_overflow();
    assert_eq!((s1[1], s1[2]), (s0[1], s0[2]), "no overflow park or reload");
    assert_eq!(heap.dbg_depot_len(C64), FLOOR + RING);
    heap.dbg_flush_all();
    drop(lease);
}
