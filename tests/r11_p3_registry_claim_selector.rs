//! Deterministic selector costs and cold-census boundaries, without HeapCore
//! construction. Candidate probes are counted before reading model state.
#![cfg(all(feature = "alloc-global", feature = "internals"))]

use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use sefer_alloc::registry::heap_registry::{pick_with_saturation, SaturationHint};

const LIVE: u8 = 1;
const FREE: u8 = 2;
const MAINTENANCE: u8 = 3;

fn select(
    hint: &AtomicU32,
    count: &AtomicU32,
    saturation: &SaturationHint,
    states: &[AtomicU8],
    probes: &AtomicU32,
) -> Option<usize> {
    let cap = states.len();
    pick_with_saturation(
        hint,
        count,
        saturation,
        cap,
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
fn live_first_touch_is_linear_in_selector_probes() {
    for n in [16_usize, 32, 64] {
        let hint = AtomicU32::new(n as u32);
        let count = AtomicU32::new(0);
        let saturation = SaturationHint::new();
        let probes = AtomicU32::new(0);
        let states: Vec<_> = (0..n).map(|_| AtomicU8::new(0)).collect();
        for expected in 0..n {
            let index =
                select(&hint, &count, &saturation, &states, &probes).expect("fresh candidate");
            assert_eq!(index, expected);
            assert_eq!(states[index].swap(LIVE, Ordering::AcqRel), 0);
        }
        assert_eq!(count.load(Ordering::Acquire) as usize, n);
        assert_eq!(probes.load(Ordering::Relaxed), 0, "N={n}");
        assert_eq!(select(&hint, &count, &saturation, &states, &probes), None);
        assert_eq!(probes.load(Ordering::Relaxed) as usize, n);
    }
}

#[test]
fn overwritten_free_hint_is_deferred_until_cap_not_lost() {
    let states = [
        AtomicU8::new(FREE),
        AtomicU8::new(FREE),
        AtomicU8::new(LIVE),
        AtomicU8::new(0),
    ];
    let hint = AtomicU32::new(1);
    let count = AtomicU32::new(3);
    let saturation = SaturationHint::new();
    let probes = AtomicU32::new(0);
    assert_eq!(
        select(&hint, &count, &saturation, &states, &probes),
        Some(1)
    );
    states[1].store(LIVE, Ordering::Release);
    assert_eq!(
        select(&hint, &count, &saturation, &states, &probes),
        Some(3)
    );
    states[3].store(LIVE, Ordering::Release);
    assert_eq!(probes.load(Ordering::Relaxed), 0);
    assert_eq!(
        select(&hint, &count, &saturation, &states, &probes),
        Some(0)
    );
    assert_eq!(probes.load(Ordering::Relaxed), 1);
}

#[test]
fn logical_last_index_and_independent_cold_census() {
    const CAP: usize = 4096;
    let states: Vec<_> = (0..CAP).map(|_| AtomicU8::new(LIVE)).collect();
    let hint = AtomicU32::new(CAP as u32);
    let count = AtomicU32::new((CAP - 1) as u32);
    let saturation = SaturationHint::new();
    let probes = AtomicU32::new(0);
    assert_eq!(
        select(&hint, &count, &saturation, &states, &probes),
        Some(CAP - 1)
    );
    assert_eq!(count.load(Ordering::Acquire), CAP as u32);
    assert_eq!(probes.load(Ordering::Relaxed), 0);
    states[CAP - 1].store(LIVE, Ordering::Release);
    assert_eq!(select(&hint, &count, &saturation, &states, &probes), None);
    assert!(saturation.is_saturated());
    assert_eq!(count.load(Ordering::Acquire), CAP as u32);

    states[CAP - 1].store(FREE, Ordering::Release);
    saturation.publish_claimable();
    let census: Vec<_> = states
        .iter()
        .enumerate()
        .filter_map(|(index, state)| (state.load(Ordering::Acquire) == FREE).then_some(index))
        .collect();
    assert_eq!(census, [CAP - 1]);
    assert_eq!(
        select(&hint, &count, &saturation, &states, &probes),
        Some(CAP - 1)
    );
    assert_eq!(count.load(Ordering::Acquire), CAP as u32);
}

#[test]
fn delayed_stale_hint_cannot_grant_a_second_owner() {
    let states = [
        AtomicU8::new(LIVE),
        AtomicU8::new(MAINTENANCE),
        AtomicU8::new(FREE),
    ];
    let hint = AtomicU32::new(0);
    let count = AtomicU32::new(3);
    let saturation = SaturationHint::new();
    let probes = AtomicU32::new(0);
    let stale = select(&hint, &count, &saturation, &states, &probes).unwrap();
    assert_eq!(stale, 0);
    assert!(states[stale]
        .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
        .is_err());
    hint.store(1, Ordering::Relaxed); // delayed older publication
    let stale_maintenance = select(&hint, &count, &saturation, &states, &probes).unwrap();
    assert_eq!(stale_maintenance, 1);
    assert!(states[stale_maintenance]
        .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
        .is_err());
    let recovered = select(&hint, &count, &saturation, &states, &probes).unwrap();
    assert_eq!(recovered, 2);
    assert!(states[recovered]
        .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
        .is_ok());
    assert_eq!(states[0].load(Ordering::Acquire), LIVE);
    assert_eq!(states[1].load(Ordering::Acquire), MAINTENANCE);
}

#[test]
fn overwritten_unmaterialized_retry_is_cold_recoverable() {
    let states = [
        AtomicU8::new(LIVE),
        AtomicU8::new(0),
        AtomicU8::new(0),
        AtomicU8::new(0),
    ];
    let hint = AtomicU32::new(2);
    let count = AtomicU32::new(3);
    let saturation = SaturationHint::new();
    let probes = AtomicU32::new(0);
    assert_eq!(
        select(&hint, &count, &saturation, &states, &probes),
        Some(2)
    );
    states[2].store(LIVE, Ordering::Release);
    assert_eq!(
        select(&hint, &count, &saturation, &states, &probes),
        Some(3)
    );
    states[3].store(LIVE, Ordering::Release);
    assert_eq!(probes.load(Ordering::Relaxed), 0);
    assert_eq!(
        select(&hint, &count, &saturation, &states, &probes),
        Some(1)
    );
    assert_eq!(count.load(Ordering::Acquire), 4);
}
