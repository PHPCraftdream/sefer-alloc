//! Deterministic selector costs and cold-census boundaries, without HeapCore
//! construction. Candidate probes are counted before reading model state.
#![cfg(all(feature = "alloc-global", feature = "internals"))]

use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use sefer_alloc::registry::bootstrap::FreeSlotHint;
use sefer_alloc::registry::heap_registry::{pick_with_saturation, SaturationHint};

const LIVE: u8 = 1;
const FREE: u8 = 2;
const MAINTENANCE: u8 = 3;

#[allow(deprecated)] // fetch_update is deprecated for try_update, which needs a newer MSRV.
fn select(
    hint: &AtomicU32,
    count: &AtomicU32,
    saturation: &SaturationHint,
    states: &[AtomicU8],
    probes: &AtomicU32,
    reuse: impl FnOnce() -> Option<usize>,
) -> Option<usize> {
    let cap = states.len();
    pick_with_saturation(
        hint,
        count,
        saturation,
        cap,
        reuse,
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
        let free_set = FreeSlotHint::new();
        let states: Vec<_> = (0..n).map(|_| AtomicU8::new(0)).collect();
        for expected in 0..n {
            let index = select(&hint, &count, &saturation, &states, &probes, || {
                free_set.take()
            })
            .expect("fresh candidate");
            assert_eq!(index, expected);
            assert_eq!(states[index].swap(LIVE, Ordering::AcqRel), 0);
        }
        assert_eq!(count.load(Ordering::Acquire) as usize, n);
        assert_eq!(probes.load(Ordering::Relaxed), 0, "N={n}");
        assert_eq!(
            select(&hint, &count, &saturation, &states, &probes, || free_set
                .take()),
            None
        );
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
        select(&hint, &count, &saturation, &states, &probes, || None),
        Some(1)
    );
    states[1].store(LIVE, Ordering::Release);
    assert_eq!(
        select(&hint, &count, &saturation, &states, &probes, || None),
        Some(3)
    );
    states[3].store(LIVE, Ordering::Release);
    assert_eq!(probes.load(Ordering::Relaxed), 0);
    assert_eq!(
        select(&hint, &count, &saturation, &states, &probes, || None),
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
        select(&hint, &count, &saturation, &states, &probes, || None),
        Some(CAP - 1)
    );
    assert_eq!(count.load(Ordering::Acquire), CAP as u32);
    assert_eq!(probes.load(Ordering::Relaxed), 0);
    states[CAP - 1].store(LIVE, Ordering::Release);
    assert_eq!(
        select(&hint, &count, &saturation, &states, &probes, || None),
        None
    );
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
        select(&hint, &count, &saturation, &states, &probes, || None),
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
    let stale = select(&hint, &count, &saturation, &states, &probes, || None).unwrap();
    assert_eq!(stale, 0);
    assert!(states[stale]
        .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
        .is_err());
    hint.store(1, Ordering::Relaxed); // delayed older publication
    let stale_maintenance = select(&hint, &count, &saturation, &states, &probes, || None).unwrap();
    assert_eq!(stale_maintenance, 1);
    assert!(states[stale_maintenance]
        .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
        .is_err());
    let recovered = select(&hint, &count, &saturation, &states, &probes, || None).unwrap();
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
        select(&hint, &count, &saturation, &states, &probes, || None),
        Some(2)
    );
    states[2].store(LIVE, Ordering::Release);
    assert_eq!(
        select(&hint, &count, &saturation, &states, &probes, || None),
        Some(3)
    );
    states[3].store(LIVE, Ordering::Release);
    assert_eq!(probes.load(Ordering::Relaxed), 0);
    assert_eq!(
        select(&hint, &count, &saturation, &states, &probes, || None),
        Some(1)
    );
    assert_eq!(count.load(Ordering::Acquire), 4);
}

#[test]
fn free_set_reuses_overwritten_free_before_bump() {
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
    let free_set = FreeSlotHint::new();
    free_set.publish(0);
    free_set.publish(1);
    for expected in [1, 0] {
        let index = select(&hint, &count, &saturation, &states, &probes, || {
            free_set.take()
        })
        .unwrap();
        assert_eq!(index, expected);
        assert!(states[index]
            .compare_exchange(FREE, LIVE, Ordering::AcqRel, Ordering::Acquire)
            .is_ok());
        free_set.forget(index);
    }
    assert_eq!(count.load(Ordering::Acquire), 3);
    assert_eq!(probes.load(Ordering::Relaxed), 0);
    assert_eq!(free_set.take(), None);
}

#[test]
fn invalid_reuse_index_is_rejected() {
    for invalid in [3, 4, usize::MAX] {
        let hint = AtomicU32::new(4);
        let count = AtomicU32::new(3);
        let saturation = SaturationHint::new();
        let states = [const { AtomicU8::new(LIVE) }; 4];
        let probes = AtomicU32::new(0);
        assert_eq!(
            select(&hint, &count, &saturation, &states, &probes, || Some(
                invalid
            )),
            Some(3)
        );
        assert_eq!(count.load(Ordering::Acquire), 4);
        assert_eq!(probes.load(Ordering::Relaxed), 0);
    }
}

#[test]
fn free_set_precedes_saturation() {
    let hint = AtomicU32::new(4);
    let count = AtomicU32::new(4);
    let saturation = SaturationHint::new();
    saturation.try_mark_saturated(saturation.snapshot());
    let free_set = FreeSlotHint::new();
    free_set.publish(2);
    assert_eq!(
        pick_with_saturation(
            &hint,
            &count,
            &saturation,
            4,
            || free_set.take(),
            || panic!("scan"),
            || panic!("bump")
        ),
        Some(2)
    );
}

#[test]
fn reuse_candidate_minted_after_the_count_snapshot_is_not_discarded() {
    let states: Vec<AtomicU8> = (0..8).map(|_| AtomicU8::new(0)).collect();
    states[5].store(FREE, Ordering::Relaxed);
    let hint = AtomicU32::new(8);
    let count = AtomicU32::new(5);
    let saturation = SaturationHint::new();
    let probes = AtomicU32::new(0);
    // The selector snapshots count = 5; slot 5 is then minted, freed and
    // published (count = 6) before the free-set take returns it.
    let picked = select(&hint, &count, &saturation, &states, &probes, || {
        count.store(6, Ordering::Release);
        Some(5)
    });
    assert_eq!(picked, Some(5));
    assert_eq!(count.load(Ordering::Relaxed), 6, "no extra slot was minted");
    assert_eq!(probes.load(Ordering::Relaxed), 0);
}
