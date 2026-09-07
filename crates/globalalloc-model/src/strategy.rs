//! The proptest front-end: a [`Strategy`] over `Vec<Op>` driven by [`Config`].
//!
//! This is the `cargo test` / miri front-end over the shared model. It mirrors
//! the size/align shape the in-tree differential tests used: a weighted small /
//! large size arm plus power-of-two aligns.

use alloc::vec::Vec;
use proptest::prelude::*;

use crate::{Config, Op};

/// A weighted size generator. A zero weight structurally removes that arm, so
/// shrinking cannot enter a disabled range. Non-zero weights retain their full
/// `u32` ratio; proptest's tuple union accumulates them in `u64`.
fn size_strategy(config: Config) -> BoxedStrategy<usize> {
    let small = 1usize..=config.small_max.max(1);
    let large = config.small_max.saturating_add(1)
        ..=config.large_max.max(config.small_max.saturating_add(1));
    match (config.small_weight, config.large_weight) {
        (0, _) => large.boxed(),
        (_, 0) => small.boxed(),
        (small_weight, large_weight) => {
            prop_oneof![small_weight => small, large_weight => large].boxed()
        }
    }
}

/// A power-of-two alignment generator up to `config.max_align`: the EXPONENT
/// is generated from the plain integer range `0..=max_align.trailing_zeros()`
/// and mapped to `1usize << exponent` — no `Vec` of candidates is
/// materialized (the previous implementation built one per call and handed
/// it to `proptest::sample::select`, twice per factory).
///
/// No shift can overflow: `op_strategy` calls `Config::validate` BEFORE any
/// strategy is built, and `validate` rejects a `max_align` that is zero, not
/// a power of two, or greater than `isize::MAX` — so the largest admissible
/// `max_align` is the greatest power of two `<= isize::MAX`, i.e.
/// `1 << (usize::BITS - 2)`, whose `trailing_zeros()` is `usize::BITS - 2`,
/// strictly below `usize::BITS` on every pointer width. (Same reasoning as
/// `bound_align` in the arbitrary front-end's exponent cap, one front-end
/// over: `min(2^21, max_align)` there, plain `max_align` here.)
///
/// Values and shrink direction are unchanged: the exponent range yields
/// exactly the powers of two `1..=max_align`, and proptest's range strategies
/// shrink toward their start, so shrinking drives the exponent to 0 and the
/// alignment to 1 — the same smallest-first direction
/// `proptest::sample::select` had over the ascending candidate list.
fn align_strategy(config: Config) -> impl Strategy<Value = usize> {
    debug_assert!(config.max_align.is_power_of_two());
    let max_exp = config.max_align.trailing_zeros();
    (0..=max_exp).prop_map(move |exponent| 1usize << exponent)
}

/// A proptest [`Strategy`] yielding a `Vec<Op>` whose length is drawn from
/// `len_range`, with sizes/aligns shaped by `config`.
///
/// Feed the result to [`crate::drive`]. The default `Config` reproduces the
/// historical in-tree shape (9:1 small:large, small ≤ 4 KiB, large ≤ 128 KiB,
/// aligns 1..=4096).
///
/// # Panics
///
/// Panics before generating anything if `config` violates a generator
/// precondition (see [`Config::validate`]), or if `len_range` is empty
/// (e.g. `4..4`): `proptest::collection::vec` rejects an empty length
/// range outright.
pub fn op_strategy(
    config: Config,
    len_range: core::ops::Range<usize>,
) -> impl Strategy<Value = Vec<Op>> {
    config.validate();
    let alloc = (size_strategy(config), align_strategy(config))
        .prop_map(|(size, align)| Op::Alloc { size, align });
    let alloc_zeroed = (size_strategy(config), align_strategy(config))
        .prop_map(|(size, align)| Op::AllocZeroed { size, align });
    let dealloc = any::<usize>().prop_map(Op::Dealloc);
    let realloc = (any::<usize>(), size_strategy(config))
        .prop_map(|(i, new_size)| Op::Realloc { i, new_size });

    let op = prop_oneof![alloc, alloc_zeroed, dealloc, realloc];
    prop::collection::vec(op, len_range)
}
