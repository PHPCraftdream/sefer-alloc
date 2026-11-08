//! Actual-source pair updates, coherent serializations and group isolation.
#[path = "support/j13b_learning_math.rs"]
mod math;
pub(crate) use math::alloc_core;
use math::{LargeThpEstimator, LargeThpGradient, LargeThpShare};

#[cfg(not(loom))]
pub(crate) use core::sync::atomic::{AtomicU64, Ordering};
#[cfg(loom)]
pub(crate) use loom::sync::atomic::{AtomicU64, Ordering};

#[path = "../src/alloc_core/config/policy/large_thp_runtime/state/thp_learning_cell.rs"]
mod thp_learning_cell;
use thp_learning_cell::ThpLearningCell;

#[cfg(not(loom))]
#[test]
fn every_update_matches_the_accepted_estimator_including_recovery() {
    for gradient in [
        LargeThpGradient::CAUTIOUS,
        LargeThpGradient::EAGER,
        LargeThpGradient::STEADY,
    ] {
        let cell = ThpLearningCell::new();
        let mut expected = LargeThpEstimator::new();
        for observation in [
            LargeThpShare::ZERO,
            LargeThpShare::ONE,
            LargeThpShare::from_raw(123_456_789),
        ]
        .into_iter()
        .cycle()
        .take(96)
        {
            expected.observe(&gradient, observation);
            assert!(cell.try_observe(&gradient, observation));
            assert_eq!(cell.snapshot(), expected);
        }
    }
}

#[cfg(not(loom))]
#[test]
fn updating_one_group_does_not_change_the_other_five() {
    let groups: [ThpLearningCell; 6] = core::array::from_fn(|_| ThpLearningCell::new());
    for group in [2, 2, 4] {
        assert!(groups[group].try_observe(&LargeThpGradient::CAUTIOUS, LargeThpShare::ZERO));
    }
    assert!(groups[2].snapshot().e_raw() < groups[4].snapshot().e_raw());
    assert!(groups[4].snapshot().e_raw() < LargeThpShare::SCALE);
    for group in [0, 1, 3, 5] {
        assert_eq!(groups[group].snapshot(), LargeThpEstimator::new());
    }
}

#[cfg(not(loom))]
#[test]
fn competing_updates_conserve_exactly_the_accepted_observations() {
    use std::sync::{Arc, Barrier};
    let cell = Arc::new(ThpLearningCell::new());
    let start = Arc::new(Barrier::new(4));
    let gradient = LargeThpGradient::custom(
        31,
        31,
        31,
        0,
        0,
        1,
        1,
        math::large_thp_curve::LargeThpCurve::Linear,
    );
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let cell = Arc::clone(&cell);
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                (0..16)
                    .filter(|_| cell.try_observe(&gradient, LargeThpShare::ZERO))
                    .count()
            })
        })
        .collect();
    let accepted: usize = threads
        .into_iter()
        .map(|thread| thread.join().expect("worker must finish"))
        .sum();
    assert!(accepted > 0 && accepted <= 64);
    // Shift 31 rounds each of these first 64 negative steps to exactly -1.
    let expected = LargeThpShare::SCALE - u32::try_from(accepted).expect("bounded count");
    assert_eq!(
        (cell.snapshot().e_raw(), cell.snapshot().m_raw()),
        (expected, expected)
    );
}

#[cfg(loom)]
#[test]
fn competing_observations_form_one_complete_serial_order_not_a_torn_pair() {
    use loom::{sync::Arc, thread};
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let gradient = LargeThpGradient::STEADY;
        let low = LargeThpShare::ZERO;
        let high = LargeThpShare::from_raw(LargeThpShare::SCALE / 3);
        let cell = Arc::new(ThpLearningCell::new());
        let other = Arc::clone(&cell);
        let worker = thread::spawn(move || other.try_observe(&gradient, low));
        let concurrent_snapshot = cell.snapshot();
        let mut low_only = LargeThpEstimator::new();
        low_only.observe(&gradient, low);
        assert!(
            concurrent_snapshot == LargeThpEstimator::new() || concurrent_snapshot == low_only,
            "snapshot racing a writer must not tear: {concurrent_snapshot:?}"
        );
        assert!(cell.try_observe(&gradient, high));
        assert!(worker.join().expect("worker must finish"));
        let mut low_high = LargeThpEstimator::new();
        low_high.observe(&gradient, low);
        low_high.observe(&gradient, high);
        let mut high_low = LargeThpEstimator::new();
        high_low.observe(&gradient, high);
        high_low.observe(&gradient, low);
        assert_ne!(
            low_high, high_low,
            "oracle must discriminate serialization order"
        );
        let actual = cell.snapshot();
        assert!(
            actual == low_high || actual == high_low,
            "lost update or torn pair: {actual:?}"
        );
    });
}

#[cfg(not(loom))]
#[test]
fn packed_pair_preserves_q31_endpoints_and_distinct_components() {
    for (e, m) in [
        (0, 0),
        (LargeThpShare::SCALE, LargeThpShare::SCALE),
        (1, LargeThpShare::SCALE - 1),
        (LargeThpShare::SCALE, 1),
    ] {
        let cell = ThpLearningCell {
            state: AtomicU64::new(((e as u64) << 32) | m as u64),
        };
        assert_eq!(cell.snapshot(), LargeThpEstimator::with_state(e, m));
        let mut expected = LargeThpEstimator::with_state(e, m);
        expected.observe(&LargeThpGradient::STEADY, LargeThpShare::ZERO);
        assert!(cell.try_observe(&LargeThpGradient::STEADY, LargeThpShare::ZERO));
        assert_eq!(cell.snapshot(), expected);
    }
}
