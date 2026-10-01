//! Small deterministic model of the hint-first, fresh-first selector and
//! full-capacity publication hint.
//! No HeapCore or OS segment is created by these saturation cases.

#![cfg(all(feature = "alloc-global", feature = "internals"))]

use core::sync::atomic::{AtomicU32, AtomicU8, Ordering};
use sefer_alloc::registry::heap_registry::{pick_with_saturation, SaturationHint};

const CAP: usize = 8;
const BUSY: u8 = 0;
const FREE: u8 = 1;

fn pick(
    hint: &AtomicU32,
    count: &AtomicU32,
    saturation: &SaturationHint,
    states: &[AtomicU8; CAP],
    scans: &AtomicU32,
) -> Option<usize> {
    pick_with_saturation(
        hint,
        count,
        saturation,
        CAP,
        || {
            scans.fetch_add(1, Ordering::Relaxed);
            let n = count.load(Ordering::Acquire) as usize;
            (0..n).find(|&idx| states[idx].load(Ordering::Acquire) == FREE)
        },
        || None,
    )
}

#[test]
fn pre_cap_mints_without_scanning_an_older_free_slot() {
    let hint = AtomicU32::new(CAP as u32);
    let count = AtomicU32::new(3);
    let saturation = SaturationHint::new();
    let states = [const { AtomicU8::new(BUSY) }; CAP];
    states[1].store(FREE, Ordering::Release);
    let scans = AtomicU32::new(0);
    let minted = pick_with_saturation(
        &hint,
        &count,
        &saturation,
        CAP,
        || {
            scans.fetch_add(1, Ordering::Relaxed);
            Some(1)
        },
        || {
            count.store(4, Ordering::Release);
            Some(3)
        },
    );
    assert_eq!(minted, Some(3));
    assert_eq!(scans.load(Ordering::Relaxed), 0);
    assert_eq!(states[1].load(Ordering::Acquire), FREE);
    assert!(!saturation.is_saturated());
}

#[test]
fn full_registry_scans_once_until_availability_changes() {
    let hint = AtomicU32::new(CAP as u32);
    let count = AtomicU32::new(CAP as u32);
    let saturation = SaturationHint::new();
    let states = [const { AtomicU8::new(BUSY) }; CAP];
    let scans = AtomicU32::new(0);

    for _ in 0..64 {
        assert_eq!(pick(&hint, &count, &saturation, &states, &scans), None);
    }
    assert_eq!(scans.load(Ordering::Relaxed), 1);

    // The same publication is used by recycle, failed construction, chunk
    // OOM, and maintenance release after their respective state transitions.
    for slot in [3, 6, 1, 7] {
        states[slot].store(FREE, Ordering::Release);
        saturation.publish_claimable();
        assert_eq!(
            pick(&hint, &count, &saturation, &states, &scans),
            Some(slot)
        );
        states[slot].store(BUSY, Ordering::Release);
        assert_eq!(pick(&hint, &count, &saturation, &states, &scans), None);
    }
    assert_eq!(scans.load(Ordering::Relaxed), 9);
    for _ in 0..64 {
        assert_eq!(pick(&hint, &count, &saturation, &states, &scans), None);
    }
    assert_eq!(scans.load(Ordering::Relaxed), 9);
}

#[test]
fn publication_during_negative_scan_prevents_stale_saturation() {
    let hint = AtomicU32::new(CAP as u32);
    let count = AtomicU32::new(CAP as u32);
    let saturation = SaturationHint::new();
    let states = [const { AtomicU8::new(BUSY) }; CAP];
    let scans = AtomicU32::new(0);

    let first = pick_with_saturation(
        &hint,
        &count,
        &saturation,
        CAP,
        || {
            scans.fetch_add(1, Ordering::Relaxed);
            assert!(states
                .iter()
                .all(|state| state.load(Ordering::Acquire) == BUSY));
            states[5].store(FREE, Ordering::Release);
            saturation.publish_claimable();
            None
        },
        || None,
    );
    assert_eq!(first, None, "one racing fallback is allowed");
    assert!(!saturation.is_saturated());
    assert_eq!(pick(&hint, &count, &saturation, &states, &scans), Some(5));
    assert_eq!(scans.load(Ordering::Relaxed), 2);
}

#[test]
fn newly_minted_last_index_cannot_be_hidden_by_older_scan() {
    let hint = AtomicU32::new(CAP as u32);
    let count = AtomicU32::new((CAP - 1) as u32);
    let saturation = SaturationHint::new();
    let states = [const { AtomicU8::new(BUSY) }; CAP];
    let scans = AtomicU32::new(0);

    let first = pick_with_saturation(
        &hint,
        &count,
        &saturation,
        CAP,
        || {
            scans.fetch_add(1, Ordering::Relaxed);
            None
        },
        || {
            count.store(CAP as u32, Ordering::Release);
            None
        },
    );
    assert_eq!(first, None);
    assert!(!saturation.is_saturated());
    states[CAP - 1].store(FREE, Ordering::Release);
    assert_eq!(
        pick(&hint, &count, &saturation, &states, &scans),
        Some(CAP - 1)
    );
    assert_eq!(scans.load(Ordering::Relaxed), 2);
}

#[test]
fn consumed_reuse_hint_does_not_repeat_full_scan_after_resaturation() {
    let hint = AtomicU32::new(CAP as u32);
    let count = AtomicU32::new(CAP as u32);
    let saturation = SaturationHint::new();
    let states = [const { AtomicU8::new(BUSY) }; CAP];
    let scans = AtomicU32::new(0);

    assert_eq!(pick(&hint, &count, &saturation, &states, &scans), None);
    states[2].store(FREE, Ordering::Release);
    hint.store(2, Ordering::Relaxed);
    saturation.publish_claimable();
    assert_eq!(pick(&hint, &count, &saturation, &states, &scans), Some(2));
    states[2].store(BUSY, Ordering::Release);

    assert_eq!(pick(&hint, &count, &saturation, &states, &scans), None);
    for _ in 0..64 {
        assert_eq!(pick(&hint, &count, &saturation, &states, &scans), None);
    }
    assert_eq!(scans.load(Ordering::Relaxed), 2);
}

#[test]
fn version_exhaustion_disables_hint_and_keeps_scan_backstop() {
    let hint = AtomicU32::new(CAP as u32);
    let count = AtomicU32::new(CAP as u32);
    let saturation = SaturationHint::dbg_with_word_for_test(u64::MAX - 5);
    let states = [const { AtomicU8::new(BUSY) }; CAP];
    let scans = AtomicU32::new(0);

    saturation.publish_claimable();
    assert_eq!(saturation.snapshot(), u64::MAX - 3);
    assert_eq!(pick(&hint, &count, &saturation, &states, &scans), None);
    assert_eq!(saturation.snapshot(), u64::MAX - 2);
    assert!(saturation.is_saturated());

    saturation.publish_claimable();
    assert_eq!(saturation.snapshot(), u64::MAX);
    assert!(!saturation.is_saturated());
    saturation.publish_claimable();
    saturation.try_mark_saturated(u64::MAX);
    saturation.try_mark_saturated(u64::MAX - 3);
    assert_eq!(saturation.snapshot(), u64::MAX);

    for expected_scans in 2..=4 {
        assert_eq!(pick(&hint, &count, &saturation, &states, &scans), None);
        assert_eq!(scans.load(Ordering::Relaxed), expected_scans);
    }
    states[4].store(FREE, Ordering::Release);
    assert_eq!(pick(&hint, &count, &saturation, &states, &scans), Some(4));
    assert_eq!(scans.load(Ordering::Relaxed), 5);
}

#[test]
fn near_max_publication_during_scan_cannot_restore_saturation() {
    let hint = AtomicU32::new(CAP as u32);
    let count = AtomicU32::new(CAP as u32);
    let saturation = SaturationHint::dbg_with_word_for_test(u64::MAX - 3);
    let states = [const { AtomicU8::new(BUSY) }; CAP];
    let scans = AtomicU32::new(0);

    let first = pick_with_saturation(
        &hint,
        &count,
        &saturation,
        CAP,
        || {
            scans.fetch_add(1, Ordering::Relaxed);
            states[5].store(FREE, Ordering::Release);
            saturation.publish_claimable();
            None
        },
        || None,
    );
    assert_eq!(first, None, "the overlapping claimant may fall back once");
    assert_eq!(saturation.snapshot(), u64::MAX);
    assert!(!saturation.is_saturated());
    assert_eq!(pick(&hint, &count, &saturation, &states, &scans), Some(5));
    assert_eq!(scans.load(Ordering::Relaxed), 2);
}
