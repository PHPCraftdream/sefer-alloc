//! WB-3: the depot fast lanes (`free_magazine_park` / `refill_magazine_miss`
//! with `DepotOverflow::park_simple` / `reload_simple`) keep the overflow's
//! decay, ingress and latch bookkeeping exactly as the generic
//! `park_overflow` / `reload_overflow_tick` chain does.
//!
//! Workload: class 64 B (class 3; magazine 16, floor 8, ring of 16 records
//! under a 16 KiB budget), repeated bursts of `16 * (1 + FLOOR + RING)`
//! allocations followed by freeing all of them. Each burst's frees park 8
//! magazines into the floor and `RING` into the overflow; the next burst's
//! allocations reload the floor first, then exactly the `RING` overflow
//! records, so the ring drains to empty on the burst's last reload and no
//! segment refill runs. Every overflow park counts one decay event, every
//! overflow reload that leaves bytes parked counts one, and every 8th
//! overflow reload runs one ingress check — a fast lane that skipped or
//! doubled any of these, or took an event the generic path would have
//! spent on decay or an ingress check, breaks the exact counts below.

#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-decommit"
))]

use sefer_alloc::alloc_core::LargeCacheConfig;
use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};
use std::alloc::Layout;

const BUDGET: usize = 16 * 1024;
const C64: usize = 3;
const FLOOR64: usize = 8;
/// Overflow records parked per burst: below the ring capacity (16) minus
/// one, so no park latches the class.
const RING: usize = 10;
const BURST: usize = 16 * (1 + FLOOR64 + RING);
/// Bursts: total decay events stay below `EPOCH_MIN` (256), so no epoch
/// ends and no decay runs during the test.
const BURSTS: usize = 12;

const BYTES: usize = 0;
const PARKS: usize = 1;
const RELOADS: usize = 2;
const DECAY: usize = 5;
const INGRESS: usize = 6;

fn l() -> Layout {
    Layout::from_size_align(64, 8).unwrap()
}

fn burst(heap: &mut HeapCore) -> Vec<*mut u8> {
    let v: Vec<_> = (0..BURST).map(|_| heap.alloc(l())).collect();
    assert!(v.iter().all(|p| !p.is_null()));
    v
}

fn free_all(heap: &mut HeapCore, v: &[*mut u8]) {
    for &p in v {
        // SAFETY: each pointer is a distinct live 64 B allocation of `heap`.
        unsafe { heap.dealloc(p, l()) };
    }
}

#[test]
fn fast_lanes_keep_decay_ingress_and_latch_bookkeeping_exact() {
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease_with_config(
        LargeCacheConfig::new().depot_budget_bytes(BUDGET),
    )
    .expect("claim");
    let heap = lease.core();
    heap.dbg_flush_all();
    let start = heap.dbg_depot_overflow();
    let (events0, epoch_len0, debt0, starved0, rsi0, _, _) =
        heap.dbg_depot_epoch(C64).expect("bound heap");
    assert_eq!(
        (events0, debt0, starved0, rsi0),
        (0, 0, 0, 0),
        "a drained depot starts clean"
    );
    assert_eq!(
        epoch_len0, 256,
        "a drained depot starts with the minimum epoch"
    );

    let mut ring_parks = 0u64;
    let mut ring_reloads = 0u64;
    let mut emptying_reloads = 0u64;
    let mut live = burst(heap);
    for b in 0..BURSTS {
        free_all(heap, &live);
        ring_parks += RING as u64;
        let s = heap.dbg_depot_overflow();
        let (floor, ring, hint) = heap.dbg_depot_ring(C64);
        assert_eq!((floor, ring, hint), (FLOOR64, RING, 1), "burst {b}: parked");
        assert_eq!(
            s[BYTES] as usize,
            RING * 64 * 16,
            "burst {b}: bytes == len * magazine"
        );
        assert_eq!(
            s[PARKS] - start[PARKS],
            ring_parks,
            "burst {b}: every ring park counted"
        );

        let (events, epoch_len, debt, starved, rsi, low, parked_in_epoch) =
            heap.dbg_depot_epoch(C64).unwrap();
        assert_eq!(
            events as u64,
            ring_parks + ring_reloads - emptying_reloads,
            "burst {b}: one decay event per ring park and per ring reload leaving bytes"
        );
        assert_eq!(epoch_len, 256, "burst {b}: no epoch ended");
        assert_eq!((debt, starved), (0, 0), "burst {b}: no debt, no latch");
        assert_eq!(s[DECAY], start[DECAY], "burst {b}: no decay ran");
        assert_eq!(rsi as u64, ring_reloads % 8, "burst {b}: ingress counter");
        assert_eq!(
            s[INGRESS] - start[INGRESS],
            ring_reloads / 8,
            "burst {b}: one ingress check per 8 ring reloads"
        );
        assert!(low as usize <= ring, "burst {b}: low <= len");
        assert_ne!(
            parked_in_epoch & (1 << C64),
            0,
            "burst {b}: class parked this epoch"
        );

        if b + 1 == BURSTS {
            break;
        }
        live = burst(heap);
        ring_reloads += RING as u64;
        emptying_reloads += 1;
        let s = heap.dbg_depot_overflow();
        assert_eq!(
            heap.dbg_depot_ring(C64),
            (0, 0, 0),
            "burst {b}: drained, hint cleared"
        );
        assert_eq!(s[BYTES], 0, "burst {b}: no bytes left parked");
        assert_eq!(
            s[RELOADS] - start[RELOADS],
            ring_reloads,
            "burst {b}: every ring reload counted"
        );
        let (events, _, _, _, rsi, low, _) = heap.dbg_depot_epoch(C64).unwrap();
        assert_eq!(
            events as u64,
            ring_parks + ring_reloads - emptying_reloads,
            "burst {b}: the emptying reload counts no event"
        );
        assert_eq!(
            rsi as u64,
            ring_reloads % 8,
            "burst {b}: ingress counter after reloads"
        );
        assert_eq!(low, 0, "burst {b}: low tracks the drained ring");
    }
    heap.dbg_flush_all();
    drop(lease);
}
