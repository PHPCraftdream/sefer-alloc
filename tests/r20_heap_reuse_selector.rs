//! Deterministic selector checks using the production selector and bitmap type.
#![cfg(all(feature = "alloc-global", feature = "internals"))]
#![forbid(unsafe_code)]

use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use sefer_alloc::registry::heap_registry::{pick_with_hint_spill, SaturationHint};

const MAX_HEAPS: usize = 4096;
#[allow(dead_code, unused_imports)]
#[path = "../src/registry/bootstrap/free_slot_hint.rs"]
mod real_free_slot_hint;
use real_free_slot_hint::FreeSlotHint;

const LIVE: u8 = 1;
const FREE: u8 = 2;
const MAINTENANCE: u8 = 3;

fn select(
    hint: &AtomicU32,
    count: &AtomicU32,
    saturation: &SaturationHint,
    free: &FreeSlotHint,
    states: &[AtomicU8],
    probes: &AtomicU32,
) -> Option<usize> {
    let cap = states.len();
    pick_with_hint_spill(
        hint,
        count,
        saturation,
        cap,
        || free.summary_nonempty(),
        || free.take_nonempty(),
        || {
            for (index, state) in states
                .iter()
                .enumerate()
                .take(count.load(Ordering::Acquire) as usize)
            {
                probes.fetch_add(1, Ordering::Relaxed);
                if matches!(state.load(Ordering::Acquire), FREE | 0) {
                    return Some(index);
                }
            }
            None
        },
        || {
            count
                .fetch_update(Ordering::AcqRel, Ordering::Acquire, |old| {
                    (old as usize != cap).then_some(old + 1)
                })
                .ok()
                .map(|old| old as usize)
        },
    )
}

#[test]
fn valid_lifo_hint_precedes_lowest_bitmap_bit_before_bump() {
    let free = FreeSlotHint::new();
    let states = [
        AtomicU8::new(FREE),
        AtomicU8::new(FREE),
        AtomicU8::new(FREE),
        AtomicU8::new(0),
    ];
    free.publish(0);
    free.publish(1);
    let hint = AtomicU32::new(2);
    let count = AtomicU32::new(3);
    let saturation = SaturationHint::new();
    let probes = AtomicU32::new(0);

    assert_eq!(
        select(&hint, &count, &saturation, &free, &states, &probes),
        Some(2)
    );
    states[2].store(LIVE, Ordering::Release);
    assert_eq!(
        select(&hint, &count, &saturation, &free, &states, &probes),
        Some(0)
    );
    assert_eq!(count.load(Ordering::Acquire), 3, "bitmap precedes bump");
    states[0].store(LIVE, Ordering::Release);
    assert_eq!(
        select(&hint, &count, &saturation, &free, &states, &probes),
        Some(1)
    );
}

#[test]
fn first_touch_mints_without_slot_probes_then_full_live_scans() {
    for n in [32_usize, 64] {
        let free = FreeSlotHint::new();
        let hint = AtomicU32::new(n as u32);
        let count = AtomicU32::new(0);
        let saturation = SaturationHint::new();
        let probes = AtomicU32::new(0);
        let states: Vec<_> = (0..n).map(|_| AtomicU8::new(0)).collect();
        for expected in 0..n {
            let index =
                select(&hint, &count, &saturation, &free, &states, &probes).expect("fresh slot");
            assert_eq!(index, expected);
            assert_eq!(states[index].swap(LIVE, Ordering::AcqRel), 0);
        }
        assert_eq!(probes.load(Ordering::Relaxed), 0);
        assert_eq!(
            select(&hint, &count, &saturation, &free, &states, &probes),
            None
        );
        assert_eq!(probes.load(Ordering::Relaxed) as usize, n);
    }
}

#[test]
fn bitmap_candidate_requires_free_to_live_cas() {
    let free = FreeSlotHint::new();
    let states = [
        AtomicU8::new(MAINTENANCE),
        AtomicU8::new(FREE),
        AtomicU8::new(FREE),
    ];
    free.publish(0);
    free.publish(1);
    let hint = AtomicU32::new(3);
    let count = AtomicU32::new(3);
    let saturation = SaturationHint::new();
    let probes = AtomicU32::new(0);

    let selected =
        select(&hint, &count, &saturation, &free, &states, &probes).expect("bitmap candidate");
    assert_eq!(selected, 0);
    assert!(states[selected]
        .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
        .is_err());
    // The failed CAS does not claim ownership or reinsert the consumed hint.
    let available = select(&hint, &count, &saturation, &free, &states, &probes)
        .expect("second bitmap candidate");
    assert_eq!(available, 1);
    assert!(states[available]
        .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
        .is_ok());
    assert_eq!(count.load(Ordering::Acquire), 3);
}
