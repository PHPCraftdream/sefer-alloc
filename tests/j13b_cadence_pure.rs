//! Actual-source pure tests of the adaptive Auto control cadence: identity
//! with the fixed selector at level 0, the subset and exact-rate properties
//! at every level, the gap bound, decorrelation, wrap totality and the
//! run/level/agreement arithmetic.
#[path = "support/j13b_cadence_math.rs"]
mod math;
pub(crate) use math::alloc_core;
use math::large_thp_observation::large_thp_control_selector::LargeThpControlSelector;
use math::{LargeThpEstimator, LargeThpGradient, LargeThpShare};

#[allow(dead_code)]
#[path = "../src/alloc_core/config/policy/large_thp_runtime/state/large_thp_cadence.rs"]
mod large_thp_cadence;
use large_thp_cadence::LargeThpCadence;

const SEEDS: [u64; 6] = [
    0,
    1,
    u64::MAX,
    0x4a13_b7d2_96c5_e80f,
    0x9E37_79B9_7F4A_7C15,
    0xDEAD_BEEF_0BAD_F00D,
];
const COHORT: u64 = LargeThpControlSelector::COHORT_LEN;

fn window(level: u32) -> u64 {
    COHORT << level
}

/// Controls of `decide` in `[start, start + len)` (wrapping ordinals).
fn controls(decide: impl Fn(u64) -> bool, start: u64, len: u64) -> u64 {
    (0..len)
        .filter(|offset| decide(start.wrapping_add(*offset)))
        .count() as u64
}

/// Next xorshift value; a deterministic schedule source, not a statistic.
fn next(state: &mut u64) -> u64 {
    *state ^= *state << 13;
    *state ^= *state >> 7;
    *state ^= *state << 17;
    *state
}

#[test]
fn constants_match_the_design() {
    assert_eq!(LargeThpCadence::LEVEL_MAX, 5);
    assert_eq!(LargeThpCadence::RUNS_PER_LEVEL, 4);
    assert_eq!(
        LargeThpCadence::RUN_MAX,
        LargeThpCadence::LEVEL_MAX * LargeThpCadence::RUNS_PER_LEVEL
    );
    assert_eq!(LargeThpCadence::TOLERANCE_RAW, 1 << 28);
    assert_eq!(LargeThpCadence::TOLERANCE_RAW, LargeThpShare::SCALE / 8);
    assert_eq!(window(LargeThpCadence::LEVEL_MAX), 256);
}

#[test]
fn level_zero_is_the_fixed_selector_for_every_ordinal() {
    for seed in SEEDS {
        for ordinal in (0..4096u64).chain(u64::MAX - 4095..=u64::MAX) {
            assert_eq!(
                LargeThpCadence::select(seed, ordinal, 0),
                LargeThpControlSelector::decision_at(seed, ordinal),
                "level 0 must equal decision_at: seed {seed:#x} ordinal {ordinal}"
            );
            assert!(LargeThpCadence::live(seed, ordinal, 0));
        }
    }
}

#[test]
fn every_level_selects_a_subset_of_the_fixed_candidates() {
    for seed in SEEDS {
        for level in 0..=LargeThpCadence::LEVEL_MAX {
            for ordinal in 0..8192u64 {
                if LargeThpCadence::select(seed, ordinal, level) {
                    assert!(LargeThpControlSelector::decision_at(seed, ordinal));
                }
            }
        }
    }
}

#[test]
fn any_level_schedule_keeps_at_most_one_control_per_aligned_cohort() {
    for seed in SEEDS {
        let mut state = seed | 1;
        let mut per_cohort = vec![0u32; 2048];
        for ordinal in 0..(2048 * COHORT) {
            // The level may change between any two requests.
            let level = (next(&mut state) % 8) as u32;
            if LargeThpCadence::select(seed, ordinal, level) {
                per_cohort[(ordinal / COHORT) as usize] += 1;
            }
        }
        assert!(per_cohort.iter().all(|count| *count <= 1));
        assert!(per_cohort.contains(&1));
    }
}

#[test]
fn constant_level_has_exactly_one_control_per_super_cohort() {
    for seed in SEEDS {
        for level in 0..=LargeThpCadence::LEVEL_MAX {
            let len = window(level);
            for super_cohort in 0..64u64 {
                assert_eq!(
                    controls(
                        |o| LargeThpCadence::select(seed, o, level),
                        super_cohort * len,
                        len
                    ),
                    1,
                    "seed {seed:#x} level {level} super-cohort {super_cohort}"
                );
            }
        }
    }
}

#[test]
fn the_window_oracle_rejects_the_unthinned_mutant() {
    let seed = SEEDS[3];
    for level in 1..=LargeThpCadence::LEVEL_MAX {
        let unthinned = controls(
            |o| LargeThpControlSelector::decision_at(seed, o),
            0,
            window(level),
        );
        assert_eq!(unthinned, 1 << level);
        assert_ne!(unthinned, 1, "a mutant without thinning must fail H2");
    }
}

#[test]
fn consecutive_controls_are_at_most_two_windows_minus_one_apart() {
    for seed in SEEDS {
        for level in 0..=LargeThpCadence::LEVEL_MAX {
            let len = window(level);
            let mut previous = None;
            let mut longest = 0u64;
            for ordinal in 0..(256 * len) {
                if LargeThpCadence::select(seed, ordinal, level) {
                    if let Some(last) = previous {
                        longest = longest.max(ordinal - last);
                    }
                    previous = Some(ordinal);
                }
            }
            assert!(longest > 0);
            assert!(
                longest < 2 * len,
                "seed {seed:#x} level {level} gap {longest} exceeds {}",
                2 * len - 1
            );
        }
    }
}

#[test]
fn the_live_cohort_index_is_spread_over_every_slot() {
    const SUPER_COHORTS: u64 = 2048;
    for seed in SEEDS {
        for level in 1..=LargeThpCadence::LEVEL_MAX {
            let slots = 1usize << level;
            let mut counts = vec![0u64; slots];
            for super_cohort in 0..SUPER_COHORTS {
                let base = super_cohort << level;
                let live: Vec<u64> = (0..slots as u64)
                    .filter(|offset| LargeThpCadence::live(seed, (base + offset) * COHORT, level))
                    .collect();
                assert_eq!(live.len(), 1, "exactly one live cohort per super-cohort");
                counts[live[0] as usize] += 1;
            }
            let expected = SUPER_COHORTS / slots as u64;
            // A uniform hash leaves each slot within a few standard
            // deviations of `expected`; a constant-index mutant puts every
            // super-cohort in one slot and fails both bounds.
            for (slot, count) in counts.iter().enumerate() {
                assert!(
                    *count >= expected / 4 && *count <= expected * 3,
                    "seed {seed:#x} level {level} slot {slot}: {count} of {SUPER_COHORTS} (expected ~{expected})"
                );
            }
        }
    }
}

#[test]
fn levels_above_the_maximum_clamp_and_the_wrap_is_total() {
    for seed in SEEDS {
        for ordinal in [0, 7, 8, 255, 256, u64::MAX - 255, u64::MAX - 7, u64::MAX] {
            for level in 0..=LargeThpCadence::LEVEL_MAX {
                let _ = LargeThpCadence::select(seed, ordinal, level);
            }
            assert_eq!(
                LargeThpCadence::select(seed, ordinal, 6),
                LargeThpCadence::select(seed, ordinal, LargeThpCadence::LEVEL_MAX)
            );
            assert_eq!(
                LargeThpCadence::select(seed, ordinal, u32::MAX),
                LargeThpCadence::select(seed, ordinal, LargeThpCadence::LEVEL_MAX)
            );
        }
        for level in 0..=LargeThpCadence::LEVEL_MAX {
            let len = window(level);
            // 2^64 is divisible by every window, so this window is aligned
            // across the u64 wrap and still holds exactly one control.
            assert_eq!(
                controls(
                    |o| LargeThpCadence::select(seed, o, level),
                    0u64.wrapping_sub(len),
                    len
                ),
                1
            );
            assert_eq!(
                controls(|o| LargeThpCadence::select(seed, o, level), 0, len),
                1
            );
        }
    }
}

#[test]
fn level_of_is_the_run_over_four_saturating_at_the_maximum() {
    for run in 0..=31u32 {
        assert_eq!(LargeThpCadence::level_of(run), run.min(20) / 4, "run {run}");
    }
    assert_eq!(LargeThpCadence::level_of(u32::MAX), 5);
    assert_eq!(LargeThpCadence::level_of(19), 4);
    assert_eq!(LargeThpCadence::level_of(20), 5);
}

#[test]
fn agreement_band_is_inclusive_symmetric_and_total() {
    let tol = LargeThpCadence::TOLERANCE_RAW;
    let scale = LargeThpShare::SCALE;
    let share = LargeThpShare::from_raw;
    assert!(LargeThpCadence::agrees(share(0), share(tol)));
    assert!(!LargeThpCadence::agrees(share(0), share(tol + 1)));
    assert!(LargeThpCadence::agrees(share(tol), share(0)));
    assert!(!LargeThpCadence::agrees(share(tol + 1), share(0)));
    assert!(LargeThpCadence::agrees(share(scale), share(scale - tol)));
    assert!(!LargeThpCadence::agrees(
        share(scale),
        share(scale - tol - 1)
    ));
    assert!(!LargeThpCadence::agrees(
        LargeThpShare::ZERO,
        LargeThpShare::ONE
    ));
    assert!(!LargeThpCadence::agrees(
        LargeThpShare::ONE,
        LargeThpShare::ZERO
    ));
    assert!(LargeThpCadence::agrees(
        LargeThpShare::ONE,
        LargeThpShare::ONE
    ));
    for (a, b) in [(0, 1 << 20), (1 << 29, 1 << 30), (7, scale), (scale, 3)] {
        assert_eq!(
            LargeThpCadence::agrees(share(a), share(b)),
            LargeThpCadence::agrees(share(b), share(a))
        );
    }
}

#[test]
fn run_saturates_resets_and_twenty_agreements_reach_the_floor() {
    assert_eq!(LargeThpCadence::next_run(0, true), 1);
    assert_eq!(LargeThpCadence::next_run(19, true), 20);
    assert_eq!(LargeThpCadence::next_run(20, true), 20);
    assert_eq!(LargeThpCadence::next_run(u32::MAX, true), 20);
    for run in [0, 1, 4, 19, 20, 99] {
        assert_eq!(LargeThpCadence::next_run(run, false), 0);
    }
    let mut run = 0;
    for step in 1..=20 {
        run = LargeThpCadence::next_run(run, true);
        assert_eq!(run, step);
        assert_eq!(
            LargeThpCadence::level_of(run),
            step / 4,
            "level after {step} agreements"
        );
    }
    assert_eq!(LargeThpCadence::level_of(run), LargeThpCadence::LEVEL_MAX);
    // A disagreement at any position returns to level 0 at once.
    for at in 0..20 {
        let mut run = 0;
        for _ in 0..at {
            run = LargeThpCadence::next_run(run, true);
        }
        run = LargeThpCadence::next_run(run, false);
        assert_eq!(LargeThpCadence::level_of(run), 0, "disagreement at {at}");
    }
}

#[test]
fn estimator_driven_runs_reach_the_floor_on_steady_data_and_reset_on_a_step_change() {
    let dense = LargeThpShare::ONE;
    let sparse = LargeThpShare::from_raw(LargeThpShare::SCALE / 20);
    for gradient in [
        LargeThpGradient::CAUTIOUS,
        LargeThpGradient::EAGER,
        LargeThpGradient::STEADY,
    ] {
        // Steady dense from the optimistic prior agrees from the first
        // observation, so exactly twenty observations reach the floor.
        let mut estimator = LargeThpEstimator::new();
        let mut run = 0;
        for step in 1..=20 {
            let prior = estimator.estimate(&gradient);
            let agreed = LargeThpCadence::agrees(prior, dense);
            assert!(agreed, "dense prior must agree at step {step}");
            run = LargeThpCadence::next_run(run, agreed);
            estimator.observe(&gradient, dense);
        }
        assert_eq!(LargeThpCadence::level_of(run), LargeThpCadence::LEVEL_MAX);
        // The step to sparse disagrees at the very first observation.
        let prior = estimator.estimate(&gradient);
        assert!(!LargeThpCadence::agrees(prior, sparse));
        run = LargeThpCadence::next_run(run, false);
        assert_eq!(LargeThpCadence::level_of(run), 0);
        estimator.observe(&gradient, sparse);
        // Steady sparse data converges, then climbs back to the floor.
        let mut observations = 1;
        while LargeThpCadence::level_of(run) < LargeThpCadence::LEVEL_MAX {
            let prior = estimator.estimate(&gradient);
            let agreed = LargeThpCadence::agrees(prior, sparse);
            run = LargeThpCadence::next_run(run, agreed);
            estimator.observe(&gradient, sparse);
            observations += 1;
            assert!(
                observations < 2000,
                "steady sparse data must settle: {gradient:?}"
            );
        }
        assert_eq!(run, LargeThpCadence::RUN_MAX);
    }
}
