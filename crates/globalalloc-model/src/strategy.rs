//! The proptest front-end: a [`Strategy`] over `Vec<Op>` driven by [`Config`].
//!
//! This is the `cargo test` / miri front-end over the shared model. It mirrors
//! the size/align shape the in-tree differential tests used: a weighted small /
//! large size arm plus power-of-two aligns.

use proptest::prelude::*;

use crate::{Config, Op};

/// A weighted size generator: mostly small (the hot free-list path),
/// occasionally large (the dedicated-segment path), per `config`.
fn size_strategy(config: Config) -> impl Strategy<Value = usize> {
    let small = (1usize..=config.small_max.max(1)).prop_map(|s| s.max(1));
    let large = (config.small_max.saturating_add(1)..=config.large_max.max(config.small_max + 1))
        .prop_map(|s| s.max(1));
    prop_oneof![
        config.small_weight => small,
        config.large_weight => large,
    ]
}

/// A power-of-two alignment generator up to `config.max_align`.
fn align_strategy(config: Config) -> impl Strategy<Value = usize> {
    let mut aligns: Vec<usize> = Vec::new();
    let mut a = 1usize;
    while a <= config.max_align {
        aligns.push(a);
        a <<= 1;
    }
    proptest::sample::select(aligns)
}

/// A proptest [`Strategy`] yielding a `Vec<Op>` whose length is drawn from
/// `len_range`, with sizes/aligns shaped by `config`.
///
/// Feed the result to [`crate::drive`]. The default `Config` reproduces the
/// historical in-tree shape (9:1 small:large, small ≤ 4 KiB, large ≤ 128 KiB,
/// aligns 1..=4096).
pub fn op_strategy(
    config: Config,
    len_range: core::ops::Range<usize>,
) -> impl Strategy<Value = Vec<Op>> {
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
