//! Force the actual cadence cell's CAS seam to reject, independent of
//! scheduling: agreements are bounded and droppable, a disagreement is not.
#![cfg(not(loom))]

#[path = "support/j13b_cadence_math.rs"]
mod math;
pub(crate) use core::sync::atomic::Ordering;
// P1.3: the moved production sources resolve their imports at the
// sefer-policy crate root (crate::policy::...); alias the shim tree under
// those paths.
#[allow(dead_code)]
mod policy {
    pub(crate) mod large_thp {
        pub(crate) use super::super::math::alloc_core::config::policy::large_thp::LargeThpShare;
    }
    pub(crate) mod large_thp_observation {
        pub(crate) mod bounded_mix {
            pub(crate) use super::super::super::math::large_thp_observation::bounded_mix::bounded_mix;
        }
        pub(crate) mod large_thp_control_selector {
            pub(crate) use super::super::super::math::large_thp_observation::large_thp_control_selector::LargeThpControlSelector;
        }
    }
}

pub(crate) struct AtomicU64 {
    value: core::sync::atomic::AtomicU64,
    attempts: core::sync::atomic::AtomicUsize,
}

#[allow(dead_code)]
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

    fn fetch_and(&self, mask: u64, ordering: Ordering) -> u64 {
        self.value.fetch_and(mask, ordering)
    }

    fn fetch_or(&self, bits: u64, ordering: Ordering) -> u64 {
        self.value.fetch_or(bits, ordering)
    }
}

#[allow(dead_code)]
#[path = "../crates/core/sefer-policy/src/policy/large_thp_runtime/state/large_thp_cadence.rs"]
mod large_thp_cadence;
#[path = "../crates/core/sefer-policy/src/policy/large_thp_runtime/state/thp_cadence_cell.rs"]
mod thp_cadence_cell;
use thp_cadence_cell::ThpCadenceCell;

fn cell_with(word: u64) -> ThpCadenceCell {
    ThpCadenceCell {
        state: AtomicU64::new(word),
    }
}

fn attempts(cell: &ThpCadenceCell) -> usize {
    cell.state.attempts.load(Ordering::Relaxed)
}

fn word(cell: &ThpCadenceCell) -> u64 {
    cell.state.value.load(Ordering::Relaxed)
}

#[test]
fn exhausted_cas_budget_drops_the_agreement_after_exactly_eight_attempts() {
    let cell = cell_with(3);
    assert!(!cell.note_agree());
    assert_eq!(attempts(&cell), 8);
    assert_eq!(
        word(&cell),
        3,
        "a dropped agreement leaves the run unchanged"
    );
    assert_eq!(cell.run(), 3);
}

#[test]
fn a_disagreement_still_clears_the_run_when_every_cas_is_rejected() {
    let cell = cell_with(7);
    // Counterfactual: a bounded-CAS reset would fail here and keep run seven.
    assert_eq!(cell.note_disagree(), 7);
    assert_eq!(word(&cell), 0);
    assert_eq!(cell.level(), 0);
    assert_eq!(attempts(&cell), 0, "the reset is one wait-free fetch_and");
}

#[test]
fn a_saturated_agreement_performs_no_read_modify_write() {
    let cell = cell_with(20);
    assert!(cell.note_agree());
    assert_eq!(attempts(&cell), 0);
    assert_eq!(word(&cell), 20);
}

#[test]
fn the_level_and_run_reads_are_single_loads_without_attempts() {
    let cell = cell_with(13);
    assert_eq!((cell.run(), cell.level()), (13, 3));
    assert_eq!(attempts(&cell), 0);
}
