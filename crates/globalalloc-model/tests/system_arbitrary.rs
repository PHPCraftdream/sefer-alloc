//! The `arbitrary` front-end (`OpStream`), decoded from bytes and driven against
//! the always-correct `System` allocator: proves the fuzz front-end decodes into
//! a valid op stream and that a correct allocator passes every oracle over it,
//! AND that decoding is non-vacuous — a decode change that silently empties
//! the streams (or drops a variant) fails the per-variant asserts below.
//! Requires the `arbitrary` feature.

#![cfg(feature = "arbitrary")]

use std::alloc::System;

use arbitrary::{Arbitrary, Unstructured};
use globalalloc_model::{drive, Config, Op, OpStream};

/// Decode a handful of deterministic byte buffers into `OpStream`s and drive
/// each against `System`. This is the crate-side smoke test for the same
/// front-end libFuzzer uses; libFuzzer supplies the fuzzed bytes, here we supply
/// fixed ones so the test is deterministic and cheap.
#[test]
fn system_matches_arbitrary_stream() {
    // A spread of seeds — enough distinct bytes to decode non-trivial streams
    // (allocs, reallocs, deallocs) without a fuzzer.
    let mut total_ops = 0usize;
    let mut seen = [0usize; 4];
    for seed in 0u8..32 {
        let bytes: Vec<u8> = (0u16..512)
            .map(|i| (i as u8).wrapping_add(seed).wrapping_mul(31))
            .collect();
        let mut u = Unstructured::new(&bytes);
        let stream = OpStream::arbitrary(&mut u).expect("decode op stream");
        total_ops += stream.ops.len();
        for op in &stream.ops {
            let idx = match op {
                Op::Alloc { .. } => 0,
                Op::AllocZeroed { .. } => 1,
                Op::Dealloc(_) => 2,
                Op::Realloc { .. } => 3,
            };
            seen[idx] += 1;
        }
        drive(&System, Config::default(), &stream.ops);
    }
    // Non-vacuity: every op variant must actually appear across the seeds.
    assert!(total_ops > 0, "no ops decoded across seeds 0..32");
    let variants = ["Alloc", "AllocZeroed", "Dealloc", "Realloc"];
    for (idx, &count) in seen.iter().enumerate() {
        assert!(
            count > 0,
            "variant {} never appeared in seeds 0..32 (all four verified present)",
            variants[idx]
        );
    }
}
