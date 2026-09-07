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

/// Miri's per-byte cost is the dominant term for this crate: the driver
/// fills and verify-reads every modeled block byte by byte (at creation,
/// before each dealloc/realloc, and again per teardown free), so under
/// Miri the SIZE bounds shrink alongside cases/length — both arms stay
/// enabled (small + large), as do grow/shrink reallocs and the
/// zeroed/dealloc paths; only each block's byte cost is capped. The
/// native path keeps `Config::default()`'s full range unchanged.
fn test_config() -> Config {
    if cfg!(miri) {
        Config {
            small_max: 256,
            large_max: 4096,
            ..Config::default()
        }
    } else {
        Config::default()
    }
}

fn op_size(op: &Op) -> Option<usize> {
    match op {
        Op::Alloc { size, .. } => Some(*size),
        Op::AllocZeroed { size, .. } => Some(*size),
        Op::Realloc { new_size, .. } => Some(*new_size),
        Op::Dealloc(_) => None,
    }
}

/// The op's own alignment (`Alloc`/`AllocZeroed` only): `Realloc` is
/// size-bearing but borrows the old block's alignment, and `Dealloc` has
/// neither.
fn op_align(op: &Op) -> Option<usize> {
    match op {
        Op::Alloc { align, .. } | Op::AllocZeroed { align, .. } => Some(*align),
        Op::Realloc { .. } | Op::Dealloc(_) => None,
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: CASES, failure_persistence: None, ..ProptestConfig::default() })]
    // `failure_persistence: None` keeps runs hermetic (same rationale as the
    // root crate's `tests/heap_differential.rs`): no cross-run regressions
    // file replay, so every invocation is reproducible from the seed alone.
    #[test]
    fn system_matches_reference_model(ops in op_strategy(test_config(), 0..MAX_LEN)) {
        drive(&System, test_config(), &ops);
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

/// The scenario is deliberately minimal: a fixed 5-op stream (length range
/// `5..6` is fixed-length, so `collection::vec` cannot shrink the stream
/// empty) still contains size-bearing ops through the FULL shrink walk, and
/// a disabled small arm (sizes ≤ 8) must never appear.
#[test]
fn disabled_small_arm_stays_disabled_while_shrinking() {
    let config = Config {
        small_max: 8,
        large_max: 12,
        max_align: 16,
        small_weight: 0,
        large_weight: 1,
        ..Config::default()
    };
    let strategy = op_strategy(config, 5..6);
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

/// Every align the strategy can yield, collected over many deterministic
/// draws: each size-bearing op's align must be a power of two
/// `<= max_align`, and (with enough draws) every admissible power of two —
/// including 1 and `max_align` — must actually appear.
#[test]
fn align_strategy_yields_exactly_the_powers_of_two_up_to_max_align() {
    // Under miri, strategy-tree materialization is interpreted and this
    // sweep's cost scales with draws x stream length; shrink both on the
    // interpreter only (16 x 8 = 128 draws still covers all 13 admissible
    // aligns deterministically -- verified under miri), keep the full
    // 64 x 24 sweep native.
    const DRAWS: usize = if cfg!(miri) { 16 } else { 64 };
    const STREAM_LEN: usize = if cfg!(miri) { 8 } else { 24 };
    let max_align = 4096usize;
    let config = Config {
        max_align,
        ..Config::default()
    };
    let strategy = op_strategy(config, STREAM_LEN..STREAM_LEN + 1);
    let mut seen = std::collections::BTreeSet::new();
    for seed in 0..DRAWS as u64 {
        let mut runner = TestRunner::new(ProptestConfig {
            rng_seed: RngSeed::Fixed(seed),
            ..ProptestConfig::default()
        });
        let ops = strategy
            .new_tree(&mut runner)
            .expect("align sweep: generate a stream");
        for op in ops.current() {
            // `Realloc` is size-bearing but carries no align of its own; only
            // `Alloc`/`AllocZeroed` go through `align_strategy`.
            let align = match op {
                Op::Alloc { align, .. } | Op::AllocZeroed { align, .. } => Some(align),
                Op::Realloc { .. } | Op::Dealloc(_) => None,
            };
            if let Some(align) = align {
                assert!(
                    align.is_power_of_two() && align <= max_align,
                    "generated align {align} outside 1..={max_align} powers of two"
                );
                seen.insert(align);
            }
        }
    }
    let expected: std::collections::BTreeSet<usize> = (0..=max_align.trailing_zeros())
        .map(|e| 1usize << e)
        .collect();
    assert_eq!(
        seen, expected,
        "align sweep missed values; the strategy's reachable set changed"
    );
}

#[test]
fn align_strategy_shrinks_toward_one() {
    // The old `sample::select` over an ascending list shrank toward its
    // FIRST element (align 1); the exponent range must keep that direction:
    // a fully simplified stream has every align-bearing op at align 1.
    // Shorter stream under miri for the same reason as the sweep test
    // above: the walk's cost is (simplify steps) x (stream clone). The
    // property needs only at least one align-bearing op through the full
    // walk; the full 12-op walk stays native.
    let len = if cfg!(miri) { 6..7 } else { 12..13 };
    let strategy = op_strategy(
        Config {
            ..Config::default()
        },
        len,
    );
    let mut tree = strategy
        .new_tree(&mut TestRunner::deterministic())
        .expect("generate a stream to shrink");
    loop {
        if !tree.simplify() {
            break;
        }
    }
    let ops = tree.current();
    let aligns: Vec<usize> = ops.iter().filter_map(op_align).collect();
    assert!(
        !aligns.is_empty(),
        "fully simplified stream had no align-bearing op"
    );
    for align in aligns {
        assert_eq!(align, 1, "fully simplified op kept align {align}");
    }
}

#[test]
fn align_strategy_exponent_is_width_correct_at_the_isize_ceiling() {
    // `Config::validate`'s `<= isize::MAX` cap is what keeps the exponent
    // strictly below `usize::BITS` on EVERY pointer width, so `1usize << exp`
    // cannot overflow. Assert that with pointer-width-relative math (the
    // same way this suite's other 32/64-portable tests do), at the largest
    // power of two `isize::MAX` admits.
    let largest = 1usize << (usize::BITS - 2); // greatest power of two <= isize::MAX
    assert!(largest <= isize::MAX as usize);
    assert_eq!(largest.trailing_zeros(), usize::BITS - 2);
    // The mapping itself stays exact at the ceiling on this width.
    assert_eq!(1usize << largest.trailing_zeros(), largest);
    // And the default config's exponent is far from the width.
    assert_eq!(Config::default().max_align.trailing_zeros(), 12);

    // Production-mapping proof: drive the REAL generator at that ceiling and
    // confirm it actually reaches `largest` without overflowing/panicking —
    // the arithmetic above proves the shift is safe in isolation; this
    // confirms `op_strategy` really exercises it (review run 3, P4-2: this
    // test used to prove NOTHING about op_strategy/align_strategy
    // themselves).
    let config = Config {
        max_align: largest,
        ..Config::default()
    };
    let strategy = op_strategy(config, 8..9);
    let mut seen_ceiling = false;
    // The ceiling exponent is 1 of `usize::BITS - 1` exponents and ops are
    // only half align-bearing (32 single-op seeds measurably miss it), so
    // the sweep uses a fixed-length stream and enough fixed-seed draws to
    // reach the ceiling deterministically; same Fixed-seed approach as
    // `align_strategy_yields_exactly_the_powers_of_two_up_to_max_align`.
    for seed in 0..32u64 {
        let ops = strategy
            .new_tree(&mut TestRunner::new(ProptestConfig {
                rng_seed: RngSeed::Fixed(seed),
                ..ProptestConfig::default()
            }))
            .expect("ceiling sweep: generate a stream")
            .current();
        for op in ops {
            if let Op::Alloc { align, .. } | Op::AllocZeroed { align, .. } = op {
                assert!(
                    align.is_power_of_two() && align <= largest,
                    "generated align {align} outside 1..={largest} powers of two"
                );
                if align == largest {
                    seen_ceiling = true;
                }
            }
        }
    }
    assert!(
        seen_ceiling,
        "ceiling sweep never generated the maximal align {largest}"
    );
}
