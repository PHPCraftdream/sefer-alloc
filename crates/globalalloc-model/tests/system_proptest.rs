//! The proptest front-end, driven against the always-correct `System`
//! allocator: proves the harness itself (model + M1–M4 oracles + strategy) is
//! sound — a correct allocator must pass every oracle. Requires the `proptest`
//! feature.

#![cfg(feature = "proptest")]

use std::alloc::System;

use globalalloc_model::{drive, op_strategy, Config};
use proptest::prelude::*;

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
