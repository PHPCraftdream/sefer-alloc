//! G1-K7: OS refusals of the depot-overflow storage. A refused reservation
//! turns the overflow off until trim; a refused page commit refuses one park
//! and leaves the records already parked intact. Either way the full
//! magazine is flushed as before, every block comes back exactly once, and
//! the overflow works again afterwards. One test, own binary: the fault
//! hooks are process-global and a reservation can fail only on a slot's
//! first use.
#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "internals",
    feature = "bench-internals",
    feature = "alloc-decommit",
    feature = "lazy-commit-fault-injection"
))]

use sefer_alloc::alloc_core::LargeCacheConfig;
use sefer_alloc::registry::{bootstrap, HeapCore, HeapRegistry};
use std::alloc::Layout;
use std::collections::HashSet;

const BUDGET: usize = 16 * 1024;
const C16: usize = 0;
const C64: usize = 3;
const FLOOR: usize = 8;
const MAG: usize = 16;
const BYTES: usize = 0;
const PARKS: usize = 1;
const OS_REFUSALS: usize = 4;
const COMMITTED: usize = 8;
const S_MAG: u8 = 1;
const S_FREE: u8 = 2;

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

/// Every freed block of class `c` is accounted for: parked (MAG — in the
/// live magazine or the depot) or flushed (FREE), none left LIVE; the
/// magazine and the depot hold exactly the parked ones, and those come back
/// first, each once. Then free them again.
fn account_and_reissue(heap: &mut HeapCore, size: usize, c: usize, freed: &[*mut u8]) {
    let mut parked = HashSet::new();
    for &p in freed {
        match heap.dbg_state_byte_state_for(p) {
            Some(S_MAG) => {
                parked.insert(p as usize);
            }
            Some(S_FREE) => {}
            s => panic!("freed block {p:?} reads {s:?}"),
        }
    }
    let resident = MAG * heap.dbg_depot_len(c) + usize::from(heap.dbg_tcache_count(c));
    assert_eq!(
        parked.len(),
        resident,
        "magazine + depot hold exactly the parked blocks"
    );
    let again = alloc_n(heap, size, resident);
    let got: HashSet<usize> = again.iter().map(|p| *p as usize).collect();
    assert_eq!(got, parked, "the parked blocks come back, each once");
    free_all(heap, size, &again);
}

#[test]
fn refused_reserve_and_commit_keep_blocks_and_recover() {
    let _ = bootstrap::ensure();
    let mut lease = HeapRegistry::dbg_claim_lease_with_config(
        LargeCacheConfig::new().depot_budget_bytes(BUDGET),
    )
    .expect("claim");
    let heap = lease.core();
    heap.dbg_flush_all();
    let s0 = heap.dbg_depot_overflow();
    assert_eq!(s0[COMMITTED], 0, "fresh slot: nothing reserved yet");

    // 1. Reservation refused: the overflow stays off, the floor still works.
    HeapCore::dbg_arm_depot_overflow_faults(1, 0);
    let n = MAG * (1 + FLOOR + 2);
    let blocks = alloc_n(heap, 64, n);
    free_all(heap, 64, &blocks);
    let s1 = heap.dbg_depot_overflow();
    assert!(s1[OS_REFUSALS] > s0[OS_REFUSALS], "reserve refusal counted");
    assert_eq!(s1[PARKS], s0[PARKS], "nothing parked in the overflow");
    assert_eq!((s1[BYTES], s1[COMMITTED]), (0, 0));
    assert_eq!(heap.dbg_depot_len(C64), FLOOR, "the floor still parks");
    account_and_reissue(heap, 64, C64, &blocks);

    // 2. Trim clears the refusal; the next park reserves and commits.
    heap.dbg_flush_all();
    let blocks = alloc_n(heap, 64, n);
    free_all(heap, 64, &blocks);
    let s2 = heap.dbg_depot_overflow();
    assert_eq!(s2[PARKS] - s1[PARKS], 2, "the overflow works after trim");
    assert!(s2[COMMITTED] > 0);
    account_and_reissue(heap, 64, C64, &blocks);
    heap.dbg_flush_all();

    // 3. Commit refused for class 16's first record page (trim decommitted
    //    every page): one park refused, then the next one commits.
    let s3 = heap.dbg_depot_overflow();
    assert_eq!(s3[COMMITTED], 0, "trim returned every record page");
    let n16 = MAG * (1 + FLOOR + 2);
    let b16 = alloc_n(heap, 16, n16);
    HeapCore::dbg_arm_depot_overflow_faults(0, 1);
    free_all(heap, 16, &b16);
    HeapCore::dbg_arm_depot_overflow_faults(0, 0);
    let s4 = heap.dbg_depot_overflow();
    assert_eq!(s4[OS_REFUSALS] - s3[OS_REFUSALS], 1, "one commit refused");
    assert!(s4[PARKS] > s3[PARKS], "a later park committed and parked");
    let page = s4[COMMITTED] as usize;
    assert!(page >= 4096 && page.is_power_of_two(), "one page committed");
    account_and_reissue(heap, 16, C16, &b16);
    heap.dbg_flush_all();

    // 4. Commit refused with records already parked: fill class 16's first
    //    record page, then refuse the next page's commit.
    let per_page = page / (MAG * core::mem::size_of::<usize>());
    let ring16 = BUDGET / (16 * MAG);
    if per_page < ring16 {
        let n_fill = MAG * (1 + FLOOR + per_page);
        let all = alloc_n(heap, 16, n_fill + MAG);
        free_all(heap, 16, &all[..n_fill]);
        let s5 = heap.dbg_depot_overflow();
        assert_eq!(heap.dbg_depot_len(C16), FLOOR + per_page);
        assert_eq!(s5[COMMITTED] as usize, page);
        // The live magazine is full: the next free parks record `per_page`,
        // the first one on the second page.
        HeapCore::dbg_arm_depot_overflow_faults(0, 1);
        free_all(heap, 16, &all[n_fill..n_fill + 1]);
        HeapCore::dbg_arm_depot_overflow_faults(0, 0);
        let s6 = heap.dbg_depot_overflow();
        assert_eq!(s6[OS_REFUSALS] - s5[OS_REFUSALS], 1, "second page refused");
        assert_eq!(s6[COMMITTED] as usize, page, "no page added");
        assert_eq!(
            heap.dbg_depot_len(C16),
            FLOOR + per_page,
            "parked records intact"
        );
        assert_eq!(s6[BYTES] as usize, per_page * 16 * MAG);
        free_all(heap, 16, &all[n_fill + 1..]);
        account_and_reissue(heap, 16, C16, &all);
        heap.dbg_flush_all();
    }
    assert_eq!(heap.dbg_depot_overflow()[BYTES], 0);
    drop(lease);
}
