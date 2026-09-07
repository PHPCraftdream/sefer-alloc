//! The proptest front-end, driven against the always-correct `System`
//! allocator: proves the harness itself (model + M1–M4 oracles + strategy) is
//! sound — a correct allocator must pass every oracle. Requires the `proptest`
//! feature. (One test below,
//! `two_random_test_runners_seed_independently`, drives nothing: it pins a
//! build-configuration property — that proptest's std feature resolved ON,
//! so seeding is OS-entropy rather than a hardcoded constant.)

#![cfg(feature = "proptest")]

use std::alloc::System;

use globalalloc_model::{drive, op_strategy, Config, Op};
use proptest::prelude::*;
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::{RngSeed, TestRunner};

/// Miri's interpreter makes the full 64x200 run minutes-long; the exhaustive
/// shape is covered natively.
const CASES: u32 = if cfg!(miri) { 4 } else { 64 };
const MAX_LEN: usize = if cfg!(miri) { 24 } else { 200 };

fn op_size(op: &Op) -> Option<usize> {
    match op {
        Op::Alloc { size, .. } => Some(*size),
        Op::AllocZeroed { size, .. } => Some(*size),
        Op::Realloc { new_size, .. } => Some(*new_size),
        Op::Dealloc(_) => None,
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: CASES, failure_persistence: None, ..ProptestConfig::default() })]
    // `failure_persistence: None` keeps runs hermetic (same rationale as the
    // root crate's `tests/heap_differential.rs`): no cross-run regressions
    // file replay, so every invocation is reproducible from the seed alone.
    #[test]
    fn system_matches_reference_model(ops in op_strategy(Config::default(), 0..MAX_LEN)) {
        drive(&System, Config::default(), &ops);
    }
}

#[test]
fn accepted_weight_configs_construct_and_generate() {
    let configs = [
        Config {
            small_weight: 0,
            large_weight: 1,
            ..Config::default()
        },
        Config {
            small_weight: 1,
            large_weight: 0,
            ..Config::default()
        },
        Config {
            small_weight: u32::MAX,
            large_weight: u32::MAX,
            ..Config::default()
        },
        Config {
            small_max: 0,
            large_max: 0,
            small_weight: 1,
            large_weight: 0,
            ..Config::default()
        },
        Config {
            small_max: 8,
            large_max: 7,
            small_weight: 0,
            large_weight: 1,
            ..Config::default()
        },
    ];

    for config in configs {
        let strategy = op_strategy(config, 4..5);
        let ops = strategy
            .new_tree(&mut TestRunner::deterministic())
            .expect("accepted config generates a tree")
            .current();
        assert_eq!(ops.len(), 4);
    }
}

#[test]
fn disabled_small_arm_stays_disabled_while_shrinking() {
    let config = Config {
        small_max: 8,
        large_max: 12,
        small_weight: 0,
        large_weight: 1,
        ..Config::default()
    };
    let strategy = op_strategy(config, 32..33);
    let mut tree = strategy
        .new_tree(&mut TestRunner::deterministic())
        .expect("generate a fixed-length op stream");
    let mut saw_size = false;

    loop {
        for op in tree.current() {
            if let Some(size) = op_size(&op) {
                saw_size = true;
                assert!(
                    (config.small_max + 1..=config.large_max).contains(&size),
                    "shrinking entered the disabled small arm: {size}"
                );
            }
        }
        if !tree.simplify() {
            break;
        }
    }
    assert!(
        saw_size,
        "the shrink walk never contained a size-bearing op"
    );
}

#[test]
fn proptest_matches_degenerate_size_ranges() {
    let configs = [
        (
            Config {
                small_max: 0,
                large_max: 0,
                small_weight: 1,
                large_weight: 0,
                ..Config::default()
            },
            1,
        ),
        (
            Config {
                small_max: 8,
                large_max: 7,
                small_weight: 0,
                large_weight: 1,
                ..Config::default()
            },
            9,
        ),
    ];

    for (config, expected) in configs {
        let ops = op_strategy(config, 32..33)
            .new_tree(&mut TestRunner::deterministic())
            .expect("degenerate config generates a tree")
            .current();
        let sizes: Vec<_> = ops.iter().filter_map(op_size).collect();
        assert!(!sizes.is_empty(), "stream had no size-bearing op");
        assert!(sizes.iter().all(|&size| size == expected));
    }
}

/// Pins OS-entropy seeding even when `PROPTEST_RNG_SEED` requests ordinary
/// property-test reproduction. Only this feature check overrides that setting.
#[test]
fn two_random_test_runners_seed_independently() {
    let strategy = op_strategy(Config::default(), 8..9);
    let random_config = || ProptestConfig {
        rng_seed: RngSeed::Random,
        ..ProptestConfig::default()
    };
    let ops_a = strategy
        .new_tree(&mut TestRunner::new(random_config()))
        .expect("generate an 8-op stream from runner A")
        .current();
    let ops_b = strategy
        .new_tree(&mut TestRunner::new(random_config()))
        .expect("generate an 8-op stream from runner B")
        .current();
    assert_eq!(ops_a.len(), 8, "strategy drew the requested length");
    assert_eq!(ops_b.len(), 8, "strategy drew the requested length");
    assert_ne!(
        ops_a, ops_b,
        "two explicitly random TestRunners produced identical op \
         streams — proptest's std feature (OS-entropy seeding) has regressed, \
         and OS-entropy seeding is unavailable"
    );
}
