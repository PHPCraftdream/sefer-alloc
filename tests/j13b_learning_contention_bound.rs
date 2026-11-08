//! Force the actual cell's CAS seam to reject, independent of scheduling.
#![cfg(not(loom))]

#[path = "support/j13b_learning_math.rs"]
mod math;
pub(crate) use core::sync::atomic::Ordering;
pub(crate) use math::alloc_core;
use math::{LargeThpEstimator, LargeThpGradient};

pub(crate) struct AtomicU64 {
    value: core::sync::atomic::AtomicU64,
    attempts: core::sync::atomic::AtomicUsize,
}

impl AtomicU64 {
    fn new(value: u64) -> Self {
        Self {
            value: core::sync::atomic::AtomicU64::new(value),
            attempts: core::sync::atomic::AtomicUsize::new(0),
        }
    }

    fn load(&self, ordering: Ordering) -> u64 {
        self.value.load(ordering)
    }

    fn compare_exchange(
        &self,
        _current: u64,
        _next: u64,
        _success: Ordering,
        failure: Ordering,
    ) -> Result<u64, u64> {
        self.attempts.fetch_add(1, Ordering::Relaxed);
        Err(self.value.load(failure))
    }
}

#[path = "../src/alloc_core/config/policy/large_thp_runtime/state/thp_learning_cell.rs"]
mod thp_learning_cell;
use thp_learning_cell::ThpLearningCell;

#[test]
fn exhausted_cas_budget_reports_loss_without_mutating_or_retrying_forever() {
    let cell = ThpLearningCell::new();
    let observation = ThpLearningCell::observation(0, 1).expect("valid observation reaches CAS");
    assert!(!cell.try_observe(&LargeThpGradient::CAUTIOUS, observation));
    assert_eq!(cell.state.attempts.load(Ordering::Relaxed), 8);
    assert_eq!(cell.snapshot(), LargeThpEstimator::new());
}
