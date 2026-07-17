//! The `arbitrary` front-end (`OpStream`), decoded from bytes and driven against
//! the always-correct `System` allocator: proves the fuzz front-end decodes into
//! a valid op stream and that a correct allocator passes every oracle over it.
//! Requires the `arbitrary` feature.

#![cfg(feature = "arbitrary")]

use std::alloc::System;

use arbitrary::{Arbitrary, Unstructured};
use globalalloc_model::{drive, Config, OpStream};

/// Decode a handful of deterministic byte buffers into `OpStream`s and drive
/// each against `System`. This is the crate-side smoke test for the same
/// front-end libFuzzer uses; libFuzzer supplies the fuzzed bytes, here we supply
/// fixed ones so the test is deterministic and cheap.
#[test]
fn system_matches_arbitrary_stream() {
    // A spread of seeds — enough distinct bytes to decode non-trivial streams
    // (allocs, reallocs, deallocs) without a fuzzer.
    for seed in 0u8..32 {
        let bytes: Vec<u8> = (0u16..512)
            .map(|i| (i as u8).wrapping_add(seed).wrapping_mul(31))
            .collect();
        let mut u = Unstructured::new(&bytes);
        let stream = OpStream::arbitrary(&mut u).expect("decode op stream");
        drive(&System, Config::default(), &stream.ops);
    }
}
