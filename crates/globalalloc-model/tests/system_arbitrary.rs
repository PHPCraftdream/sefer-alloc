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

fn decode(bytes: &[u8], config: Config) -> Vec<Op> {
    let mut u = Unstructured::new(bytes);
    OpStream::arbitrary_with_config(&mut u, config)
        .expect("decode op stream")
        .ops
}

fn one_op(variant: u8, payload: &[u8]) -> Vec<u8> {
    // Current arbitrary-1.4 derive encoding only; this is not a stable corpus
    // format promised by `OpStream`.
    let mut bytes = vec![1, 0, 0, 0, variant << 6];
    bytes.extend_from_slice(payload);
    bytes.push(0);
    bytes
}

fn alloc_size(bytes: &[u8], config: Config) -> usize {
    match decode(bytes, config).as_slice() {
        [Op::Alloc { size, .. }] => *size,
        ops => panic!("expected one Alloc, got {ops:?}"),
    }
}

#[test]
fn arbitrary_size_draws_cover_both_arm_endpoints() {
    let config = Config {
        small_max: 3,
        large_max: 5,
        small_weight: 1,
        large_weight: 1,
        ..Config::default()
    };

    assert_eq!(alloc_size(&one_op(0, &[0, 0, 0]), config), 1);
    assert_eq!(alloc_size(&one_op(0, &[0, 0, 2]), config), 3);
    assert_eq!(alloc_size(&one_op(0, &[0, 1, 0]), config), 4);
    assert_eq!(alloc_size(&one_op(0, &[0, 1, 1]), config), 5);
}

#[test]
fn arbitrary_size_draws_honor_zero_and_max_weights() {
    let small_only = Config {
        small_max: 3,
        large_max: 5,
        small_weight: 1,
        large_weight: 0,
        ..Config::default()
    };
    let large_only = Config {
        small_weight: 0,
        large_weight: 1,
        ..small_only
    };
    assert_eq!(alloc_size(&one_op(0, &[0, 2]), small_only), 3);
    assert_eq!(alloc_size(&one_op(0, &[0, 1]), large_only), 5);

    let max_weights = Config {
        small_weight: u32::MAX,
        large_weight: u32::MAX,
        ..small_only
    };
    let small = one_op(0, &[0, 0, 0, 0, 0, 0, 2]);
    let large = one_op(0, &[0, 0, 0xff, 0xff, 0xff, 0xff, 1]);
    assert_eq!(alloc_size(&small, max_weights), 3);
    assert_eq!(alloc_size(&large, max_weights), 5);
}

#[test]
fn arbitrary_size_draws_match_degenerate_ranges() {
    let small = Config {
        small_max: 0,
        large_max: 0,
        small_weight: 1,
        large_weight: 0,
        ..Config::default()
    };
    let large = Config {
        small_max: 3,
        large_max: 2,
        small_weight: 0,
        large_weight: 1,
        ..Config::default()
    };
    assert_eq!(alloc_size(&one_op(0, &[0]), small), 1);
    assert_eq!(alloc_size(&one_op(0, &[0]), large), 4);
}

#[cfg(target_pointer_width = "64")]
#[test]
fn arbitrary_size_draw_reaches_above_u32_without_allocating() {
    let small_max = u32::MAX as usize;
    let config = Config {
        small_max,
        large_max: small_max + 256,
        ..Config::default()
    };
    let size = alloc_size(&one_op(0, &[0, 9, 0xff]), config);
    assert_eq!(size, small_max + 256);
    assert!(size > u32::MAX as usize);
}

#[test]
fn arbitrary_stream_decodes_every_variant() {
    let config = Config {
        small_max: 0,
        large_max: 0,
        small_weight: 1,
        large_weight: 0,
        ..Config::default()
    };
    let cases = [
        (one_op(0, &[0]), Op::Alloc { size: 1, align: 1 }),
        (one_op(1, &[0]), Op::AllocZeroed { size: 1, align: 1 }),
        (one_op(2, &[0x34, 0x12]), Op::Dealloc(0x1234)),
        (
            one_op(3, &[0x34, 0x12]),
            Op::Realloc {
                i: 0x1234,
                new_size: 1,
            },
        ),
    ];

    for (bytes, expected) in cases {
        assert_eq!(decode(&bytes, config), [expected]);
    }
}

#[test]
fn arbitrary_stream_caps_attempted_ops() {
    const MAX_OPS: usize = 2048;
    const ATTEMPTS: u16 = 2056;
    let mut bytes = Vec::with_capacity((MAX_OPS + 8) * 7 + 1);
    for i in 0..ATTEMPTS {
        let [lo, hi] = i.to_le_bytes();
        bytes.extend_from_slice(&[1, 0, 0, 0, 0x80, lo, hi]);
    }
    bytes.push(0);

    let ops = decode(&bytes, Config::default());
    assert_eq!(ops.len(), MAX_OPS);
    assert!(ops.iter().all(|op| matches!(op, Op::Dealloc(_))));
}

/// Decode a handful of deterministic byte buffers into `OpStream`s and drive
/// each against `System`. This is the crate-side smoke test for the same
/// front-end libFuzzer uses; libFuzzer supplies the fuzzed bytes, here we supply
/// fixed ones so the test is deterministic and cheap.
///
/// Under miri the byte-at-a-time fill/verify of default-sized blocks dominates
/// this test, so the size bounds shrink while the seed corpus remains fixed.
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
        // `arbitrary` reads its continue-bit
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
