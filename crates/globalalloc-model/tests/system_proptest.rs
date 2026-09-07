//! The proptest front-end, driven against the always-correct `System`
//! allocator: proves the harness itself (model + M1–M4 oracles + strategy) is
//! sound — a correct allocator must pass every oracle. Requires the `proptest`
//! feature.

#![cfg(feature = "proptest")]

use std::alloc::System;

use globalalloc_model::{drive, op_strategy, Config};
use proptest::prelude::*;
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::TestRunner;

/// Miri's interpreter makes the full 64x200 run minutes-long; the exhaustive
/// shape is covered natively.
const CASES: u32 = if cfg!(miri) { 4 } else { 64 };
const MAX_LEN: usize = if cfg!(miri) { 24 } else { 200 };

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

/// Pins the random seeding the property above depends on (review run 3,
/// P3-8; the bug it guards is round-2 P2-1): with proptest's `std` feature
/// resolved OFF, `TestRunner::default()` seeds its RNG from a hardcoded
/// constant and the property silently degenerates into a single fixed
/// 8-op stream on every run. Two independently constructed default runners
/// must therefore produce DIFFERENT streams. Under the regressed
/// (std-less dev-dependency) resolution this test compiles but FAILS on
/// `assert_ne!`, which is exactly the signal the round-2 bug lacked.
#[test]
fn two_default_test_runners_seed_independently() {
    let strategy = op_strategy(Config::default(), 8..9);
    let ops_a = strategy
        .new_tree(&mut TestRunner::default())
        .expect("generate an 8-op stream from runner A")
        .current();
    let ops_b = strategy
        .new_tree(&mut TestRunner::default())
        .expect("generate an 8-op stream from runner B")
        .current();
    assert_eq!(ops_a.len(), 8, "strategy drew the requested length");
    assert_eq!(ops_b.len(), 8, "strategy drew the requested length");
    assert_ne!(
        ops_a, ops_b,
        "two independently constructed TestRunners produced identical op \
         streams — proptest's std feature (OS-entropy seeding) has regressed, \
         and the property above is again a fixed-seed suite"
    );
}
