//! J13b C4: pure advice geometry — extent, eligibility and share selection.
#![cfg(all(
    feature = "internals",
    feature = "alloc-decommit",
    not(feature = "numa-aware")
))]

use sefer_alloc::alloc_core::policy::large_thp_runtime::{ThpAdvice, ThpTuple};
use sefer_alloc::{LargeThpMode, LargeThpShare};

const MIB: usize = 1024 * 1024;
const PS: usize = 4096;

fn half() -> LargeThpShare {
    LargeThpShare::from_raw(LargeThpShare::SCALE / 2)
}

fn ext(span: usize, share: LargeThpShare, left: Option<usize>) -> usize {
    ThpAdvice::extent(span, share, left, PS)
}

#[test]
fn granule_is_two_mib() {
    assert_eq!(ThpAdvice::GRANULE, 2 * MIB);
}

#[test]
fn floors_to_granule() {
    assert_eq!(ext(5 * MIB, LargeThpShare::ONE, None), 4 * MIB);
    assert_eq!(ext(6 * MIB + 4096, LargeThpShare::ONE, None), 6 * MIB);
    assert_eq!(ext(2 * MIB - 4096, LargeThpShare::ONE, None), 0);
    assert_eq!(ext(2 * MIB, LargeThpShare::ONE, None), 2 * MIB);
    assert_eq!(ext(MIB, LargeThpShare::ONE, None), 0);
    assert_eq!(ext(0, LargeThpShare::ONE, None), 0);
}

#[test]
fn share_scales_span() {
    assert_eq!(ext(8 * MIB, half(), None), 4 * MIB);
    assert_eq!(ext(5 * MIB, half(), None), 2 * MIB);
    assert_eq!(ext(8 * MIB, LargeThpShare::ZERO, None), 0);
    assert_eq!(ext(3 * MIB, half(), None), 0);
}

#[test]
fn budget_left_caps() {
    assert_eq!(ext(8 * MIB, LargeThpShare::ONE, Some(3 * MIB)), 2 * MIB);
    assert_eq!(ext(8 * MIB, LargeThpShare::ONE, Some(0)), 0);
    assert_eq!(ext(8 * MIB, LargeThpShare::ONE, Some(100 * MIB)), 8 * MIB);
    assert_eq!(ext(8 * MIB, LargeThpShare::ONE, None), 8 * MIB);
    assert_eq!(ext(8 * MIB, half(), Some(2 * MIB)), 2 * MIB);
}

#[test]
fn huge_spans_do_not_overflow() {
    let g = ThpAdvice::GRANULE;
    let r = ext(usize::MAX, LargeThpShare::ONE, None);
    assert_eq!(r, usize::MAX / g * g);
    let h = ext(usize::MAX, half(), None);
    assert_eq!(h, (usize::MAX / 2) / g * g);
    assert_eq!(ext(usize::MAX, LargeThpShare::ZERO, None), 0);
    assert_eq!(ext(usize::MAX, LargeThpShare::ONE, Some(usize::MAX)), r);
    assert_eq!(ext(usize::MAX, LargeThpShare::ONE, Some(g)), g);
}

#[test]
fn page_geometry_guard() {
    assert_eq!(ThpAdvice::extent(8 * MIB, LargeThpShare::ONE, None, 0), 0);
    assert_eq!(
        ThpAdvice::extent(8 * MIB, LargeThpShare::ONE, None, 3 * MIB),
        0
    );
    assert_eq!(
        ThpAdvice::extent(8 * MIB, LargeThpShare::ONE, None, 3 * 4096),
        0
    );
    assert_eq!(
        ThpAdvice::extent(8 * MIB, LargeThpShare::ONE, None, 64 * 1024),
        8 * MIB
    );
    assert_eq!(
        ThpAdvice::extent(8 * MIB, LargeThpShare::ONE, None, 2 * MIB),
        8 * MIB
    );
    assert_eq!(
        ThpAdvice::extent(8 * MIB, LargeThpShare::ONE, None, 4 * MIB),
        0
    );
}

#[test]
fn monotone_and_never_widens() {
    let mut prev = 0;
    for raw in (0..=LargeThpShare::SCALE).step_by((LargeThpShare::SCALE / 64) as usize) {
        let span = 37 * MIB + 12345;
        let v = ext(span, LargeThpShare::from_raw(raw), None);
        assert!(v >= prev);
        assert!(v <= span);
        assert_eq!(v & (ThpAdvice::GRANULE - 1), 0);
        prev = v;
    }
    assert_eq!(prev, 36 * MIB);
    let mut prev = 0;
    for left in (0..48 * MIB).step_by(512 * 1024) {
        let v = ext(40 * MIB, LargeThpShare::ONE, Some(left));
        assert!(v >= prev);
        assert!(v <= left);
        assert!(v <= 40 * MIB);
        prev = v;
    }
}

#[test]
fn eligibility() {
    assert!(!ThpAdvice::eligible(LargeThpMode::Off, 0, 0));
    assert!(!ThpAdvice::eligible(LargeThpMode::Off, 0, usize::MAX));
    for m in [LargeThpMode::Always, LargeThpMode::Auto] {
        assert!(ThpAdvice::eligible(m, 0, 0));
        assert!(ThpAdvice::eligible(m, 0, 12345));
        assert!(ThpAdvice::eligible(m, 8 * MIB, 8 * MIB));
        assert!(!ThpAdvice::eligible(m, 8 * MIB, 8 * MIB - 1));
        assert!(ThpAdvice::eligible(m, 8 * MIB, 8 * MIB + 1));
    }
}

#[test]
fn share_selection() {
    let mut t = ThpTuple::DEFAULT;
    t.mode = LargeThpMode::Always;
    assert_eq!(ThpAdvice::share_for(&t, 10 * MIB), LargeThpShare::ONE);
    t.mode = LargeThpMode::Off;
    assert_eq!(ThpAdvice::share_for(&t, 10 * MIB), LargeThpShare::ZERO);
    t.mode = LargeThpMode::Auto;
    for size in [0, MIB, 10 * MIB, 40 * MIB, 200 * MIB, 600 * MIB] {
        assert_eq!(ThpAdvice::share_for(&t, size), LargeThpShare::ZERO);
    }
}
