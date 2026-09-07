//! The `arbitrary` front-end (`OpStream`), decoded from bytes and driven against
//! the always-correct `System` allocator: proves the fuzz front-end decodes into
//! a valid op stream and that a correct allocator passes every oracle over it,
//! AND that decoding is non-vacuous — a decode change that silently empties
//! the streams (or drops a variant) fails the per-variant asserts below.
//! Requires the `arbitrary` feature.

#![cfg(feature = "arbitrary")]

use std::alloc::System;

use arbitrary::Unstructured;
use globalalloc_model::{drive, Config, Op, OpStream};

/// Decode a handful of deterministic byte buffers into `OpStream`s and drive
/// each against `System`. This is the crate-side smoke test for the same
/// front-end libFuzzer uses; libFuzzer supplies the fuzzed bytes, here we supply
/// fixed ones so the test is deterministic and cheap.
///
/// Under miri the byte-at-a-time fill/verify of default-sized blocks (up to
/// 128 KiB) makes this the dominant cost of the CI miri job — which runs it
/// TWICE (plain, then strict-provenance) — so the generator bounds shrink
/// under miri, mirroring tests/system_proptest.rs's CASES/MAX_LEN
/// reduction. The seed sweep and byte buffers stay IDENTICAL either way:
/// `Config` shapes sizes/aligns, not the decoded variant mix, so the
/// per-variant non-vacuity asserts below see the same op stream under both
/// configurations. Since the P3-3 generator fix every seed produces a
/// non-empty stream, so the sweep's miri cost is roughly double its pre-fix
/// size — the `cfg!(miri)` shrink above already bounds that.
#[test]
fn system_matches_arbitrary_stream() {
    let config = if cfg!(miri) {
        Config {
            small_max: 256,
            large_max: 4096,
            ..Config::default()
        }
    } else {
        Config::default()
    };
    // A spread of seeds — enough distinct bytes to decode non-trivial streams
    // (allocs, reallocs, deallocs) without a fuzzer.
    let mut total_ops = 0usize;
    let mut seen = [0usize; 4];
    for seed in 0u8..32 {
        let mut bytes: Vec<u8> = (0u16..512)
            .map(|i| (i as u8).wrapping_add(seed).wrapping_mul(31))
            .collect();
        // Review run 7, P3-3: `arbitrary`'s iterator reads its continue-bit
        // from bytes[0]'s LOW bit (`bool::arbitrary` = `u8::arbitrary(u)? & 1
        // == 1`, front byte first), and 31 is odd — so the low bit used to
        // equal the seed's parity and every EVEN seed decoded to an empty
        // stream, silently halving this sweep. Force bytes[0] odd for every
        // seed; the per-seed assert below keeps it that way.
        bytes[0] |= 1;
        let mut u = Unstructured::new(&bytes);
        let stream = OpStream::arbitrary_with_config(&mut u, config).expect("decode op stream");
        assert!(
            !stream.ops.is_empty(),
            "seed {seed} decoded to an empty stream (arbitrary's continue-bit is the \
             low bit of bytes[0]; keep it odd for every seed)"
        );
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
        drive(&System, config, &stream.ops);
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
