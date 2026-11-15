//! Actual-source model of one group's adaptive control-cadence cell: the
//! production `thp_cadence_cell.rs` and the real pure cadence arithmetic.
//! Native rows pin the run/level transitions; the Loom rows (`RUSTFLAGS="--cfg
//! loom" cargo test --release --features "alloc-core alloc-xthread
//! tagged-index-stack/loom" --test loom_thp_cadence_cell`) execute the
//! interleaving oracles on model atomics.
#[path = "support/j13b_cadence_math.rs"]
mod math;
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

#[cfg(not(loom))]
pub(crate) use core::sync::atomic::{AtomicU64, Ordering};
#[cfg(loom)]
pub(crate) use loom::sync::atomic::{AtomicU64, Ordering};

#[allow(dead_code)]
#[path = "../crates/core/sefer-policy/src/policy/large_thp_runtime/state/large_thp_cadence.rs"]
mod large_thp_cadence;
#[path = "../crates/core/sefer-policy/src/policy/large_thp_runtime/state/thp_cadence_cell.rs"]
mod thp_cadence_cell;
use thp_cadence_cell::ThpCadenceCell;

#[cfg(not(loom))]
fn cell_with(word: u64) -> ThpCadenceCell {
    ThpCadenceCell {
        state: AtomicU64::new(word),
    }
}

#[cfg(not(loom))]
#[test]
fn a_fresh_cell_is_level_zero_with_an_empty_run() {
    let cell = ThpCadenceCell::new();
    assert_eq!(cell.run(), 0);
    assert_eq!(cell.level(), 0);
    assert_eq!(cell.state.load(Ordering::Relaxed), ThpCadenceCell::INITIAL);
    assert_eq!(ThpCadenceCell::INITIAL, 0);
}

#[cfg(not(loom))]
#[test]
fn agreements_raise_the_level_every_four_and_saturate_at_twenty() {
    let cell = ThpCadenceCell::new();
    for step in 1..=25u32 {
        assert!(cell.note_agree());
        assert_eq!(cell.run(), step.min(20), "run after {step} agreements");
        assert_eq!(cell.level(), step.min(20) / 4, "level after {step}");
    }
    assert_eq!(cell.level(), 5);
    assert_eq!(cell.state.load(Ordering::Relaxed), 20);
}

#[cfg(not(loom))]
#[test]
fn disagreement_clears_the_run_returns_the_previous_run_and_is_idempotent() {
    for previous in [0u32, 1, 3, 4, 7, 19, 20] {
        let cell = ThpCadenceCell::new();
        for _ in 0..previous {
            assert!(cell.note_agree());
        }
        assert_eq!(cell.note_disagree(), previous);
        assert_eq!(cell.run(), 0);
        assert_eq!(cell.level(), 0);
        assert_eq!(cell.note_disagree(), 0);
    }
    // After a reset the climb restarts from one.
    let cell = ThpCadenceCell::new();
    for _ in 0..20 {
        assert!(cell.note_agree());
    }
    cell.note_disagree();
    assert!(cell.note_agree());
    assert_eq!((cell.run(), cell.level()), (1, 0));
}

#[cfg(not(loom))]
#[test]
fn out_of_range_run_bits_decode_as_the_saturated_run() {
    for word in [21u64, 25, 31] {
        let cell = cell_with(word);
        assert_eq!((cell.run(), cell.level()), (20, 5));
        assert!(cell.note_agree(), "saturated agreement is a no-op success");
        assert_eq!(cell.state.load(Ordering::Relaxed), word);
        assert_eq!(cell.note_disagree(), 20);
        assert_eq!(cell.state.load(Ordering::Relaxed), 0);
    }
}

#[cfg(not(loom))]
#[test]
fn updates_touch_only_the_run_bits() {
    let marker = 1u64 << 40;
    let cell = cell_with(marker | 3);
    assert!(cell.note_agree());
    assert_eq!(cell.state.load(Ordering::Relaxed), marker | 4);
    assert_eq!(cell.note_disagree(), 4);
    assert_eq!(cell.state.load(Ordering::Relaxed), marker);
}

#[cfg(not(loom))]
#[test]
fn groups_are_independent_words() {
    let groups: [ThpCadenceCell; 6] = core::array::from_fn(|_| ThpCadenceCell::new());
    for _ in 0..9 {
        assert!(groups[2].note_agree());
    }
    groups[4].note_agree();
    assert_eq!((groups[2].run(), groups[2].level()), (9, 2));
    assert_eq!((groups[4].run(), groups[4].level()), (1, 0));
    groups[2].note_disagree();
    assert_eq!(groups[4].run(), 1);
    for group in [0, 1, 3, 5] {
        assert_eq!(groups[group].run(), 0);
    }
}

#[cfg(not(loom))]
#[test]
fn competing_agreements_never_lose_an_accepted_update() {
    use std::sync::{Arc, Barrier};
    let cell = Arc::new(ThpCadenceCell::new());
    let start = Arc::new(Barrier::new(4));
    let threads: Vec<_> = (0..4)
        .map(|_| {
            let cell = Arc::clone(&cell);
            let start = Arc::clone(&start);
            std::thread::spawn(move || {
                start.wait();
                (0..4).filter(|_| cell.note_agree()).count()
            })
        })
        .collect();
    let accepted: usize = threads
        .into_iter()
        .map(|thread| thread.join().expect("worker must finish"))
        .sum();
    // Sixteen attempts never reach the saturation point, so every accepted
    // agreement is exactly one increment and a dropped one adds nothing.
    assert!(accepted > 0 && accepted <= 16);
    assert_eq!(cell.run() as usize, accepted);
}

#[cfg(all(not(loom), feature = "bench-internals"))]
#[test]
fn a_pin_overrides_the_level_but_never_the_run_or_the_updates() {
    let cell = ThpCadenceCell::new();
    cell.pin(Some(3));
    assert_eq!(cell.level(), 3);
    assert_eq!(cell.run(), 0);
    for _ in 0..8 {
        assert!(cell.note_agree());
    }
    assert_eq!((cell.run(), cell.level()), (8, 3), "pin hides level 2");
    cell.note_disagree();
    assert_eq!((cell.run(), cell.level()), (0, 3), "reset keeps the pin");
    cell.pin(Some(0));
    for _ in 0..20 {
        assert!(cell.note_agree());
    }
    assert_eq!((cell.run(), cell.level()), (20, 0), "level 0 pin is exact");
    cell.pin(Some(9));
    assert_eq!(cell.level(), 5, "a pin above the maximum clamps");
    cell.pin(None);
    assert_eq!((cell.run(), cell.level()), (20, 5), "release shows the run");
    cell.pin(None);
    assert_eq!(cell.state.load(Ordering::Relaxed), 20);
}

#[cfg(loom)]
#[test]
fn two_racing_agreements_end_at_exactly_run_two() {
    use loom::{sync::Arc, thread};
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let cell = Arc::new(ThpCadenceCell::new());
        let other = Arc::clone(&cell);
        let worker = thread::spawn(move || other.note_agree());
        let mine = cell.note_agree();
        let theirs = worker.join().expect("worker must finish");
        assert!(mine && theirs, "two attempts fit the eight-attempt bound");
        assert_eq!(cell.run(), 2, "a lost agreement would leave run one");
    });
}

#[cfg(loom)]
#[test]
fn an_agreement_racing_a_disagreement_ends_at_zero_or_one_never_above() {
    use loom::{sync::Arc, thread};
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let cell = Arc::new(ThpCadenceCell {
            state: AtomicU64::new(3),
        });
        let other = Arc::clone(&cell);
        let worker = thread::spawn(move || other.note_agree());
        let previous = cell.note_disagree();
        assert!(worker.join().expect("worker must finish"));
        let run = cell.run();
        assert!(
            (previous == 4 && run == 0) || (previous == 3 && run == 1),
            "disagreement lost or run above one: previous {previous}, run {run}"
        );
    });
}

#[cfg(loom)]
#[test]
fn a_level_reader_racing_a_writer_sees_only_valid_levels() {
    use loom::{sync::Arc, thread};
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(3);
    builder.check(|| {
        let cell = Arc::new(ThpCadenceCell {
            state: AtomicU64::new(3),
        });
        let other = Arc::clone(&cell);
        let worker = thread::spawn(move || other.note_agree());
        let seen = cell.level();
        assert!(seen <= 1, "level {seen} is neither the old nor the new one");
        assert!(worker.join().expect("worker must finish"));
        assert_eq!(cell.level(), 1);
        let other = Arc::clone(&cell);
        let resetter = thread::spawn(move || other.note_disagree());
        let seen = cell.level();
        assert!(seen <= 1);
        resetter.join().expect("worker must finish");
        assert_eq!(cell.level(), 0);
    });
}
