//! The proptest front-end, driven against the always-correct `System`
//! allocator: proves the harness itself (model + M1–M4 oracles + strategy) is
//! sound — a correct allocator must pass every oracle. Requires the `proptest`
//! feature.

#![cfg(feature = "proptest")]

use std::alloc::System;

use globalalloc_model::{drive, op_strategy, Config};
use proptest::prelude::*;

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, failure_persistence: None, ..ProptestConfig::default() })]
    #[test]
    fn system_matches_reference_model(ops in op_strategy(Config::default(), 0..200)) {
        drive(&System, Config::default(), &ops);
    }
}
