//! `Config::validate()`'s documented `# Panics` contract, pinned as behavior
//! (review run 3, P3-3 + P3-4): a well-formed config passes, each documented
//! precondition panics with the documented message, and BOTH front-ends
//! (`op_strategy` and `OpStream::arbitrary_with_config`) reject a degenerate
//! config BEFORE generating or decoding anything. As in
//! `tests/oracle_negative.rs`, the pins are message-level on purpose.

use globalalloc_model::Config;

#[test]
fn default_config_passes_validation() {
    Config::default().validate();
}

#[test]
#[should_panic(expected = "Config::max_align must be a non-zero power of two <= isize::MAX")]
fn zero_max_align_panics() {
    Config {
        max_align: 0,
        ..Config::default()
    }
    .validate();
}

#[test]
#[should_panic(expected = "Config::max_align must be a non-zero power of two <= isize::MAX")]
fn non_power_of_two_max_align_panics() {
    Config {
        max_align: 3000,
        ..Config::default()
    }
    .validate();
}

#[test]
#[should_panic(expected = "Config::small_weight and Config::large_weight must not both be zero")]
fn all_zero_weights_panics() {
    Config {
        small_weight: 0,
        large_weight: 0,
        ..Config::default()
    }
    .validate();
}

// Review run 3, P3-4: `usize::MAX`, the natural spelling of "no limit", is a
// precondition violation for the size bounds exactly as it already was for
// `max_align` — a bound that large only generates guaranteed M1 null reports
// (the harness does not model OOM).

#[test]
#[should_panic(expected = "Config::large_max must be <= isize::MAX")]
fn unbounded_large_max_panics() {
    Config {
        large_max: usize::MAX,
        ..Config::default()
    }
    .validate();
}

#[test]
#[should_panic(expected = "Config::small_max must be <= isize::MAX")]
fn unbounded_small_max_panics() {
    Config {
        small_max: usize::MAX,
        ..Config::default()
    }
    .validate();
}

#[test]
fn bounds_at_the_ceiling_pass_validation() {
    // The new bound is inclusive: `isize::MAX` itself passes `validate()`.
    // This pins the sanity ceiling, NOT Layout-admissibility: whether
    // `Layout::from_size_align(isize::MAX, align)` succeeds is
    // align-dependent (true only at align == 1). This config inherits
    // `Config::default()`'s `max_align: 4096`, for which the admissible
    // ceiling is `(isize::MAX / 4096) * 4096`; the front-ends would emit
    // `small_max + 1 = 2^63` sizes past it, and `drive` clamps those down
    // to the ceiling before the allocator ever sees them.
    Config {
        small_max: isize::MAX as usize,
        large_max: isize::MAX as usize,
        ..Config::default()
    }
    .validate();
}

// Both front-ends call `validate()` before generating/decoding; these two
// prove the rejection happens up front, not lazily mid-stream.

#[cfg(feature = "proptest")]
#[test]
#[should_panic(expected = "Config::small_weight and Config::large_weight must not both be zero")]
fn proptest_frontend_rejects_before_generating() {
    let _ = globalalloc_model::op_strategy(
        Config {
            small_weight: 0,
            large_weight: 0,
            ..Config::default()
        },
        0..4,
    );
}

#[cfg(feature = "arbitrary")]
#[test]
#[should_panic(expected = "Config::small_weight and Config::large_weight must not both be zero")]
fn arbitrary_frontend_rejects_before_decoding() {
    let mut u = arbitrary::Unstructured::new(&[]);
    let _ = globalalloc_model::OpStream::arbitrary_with_config(
        &mut u,
        Config {
            small_weight: 0,
            large_weight: 0,
            ..Config::default()
        },
    );
}
