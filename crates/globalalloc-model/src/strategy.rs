//! The proptest front-end: a [`Strategy`] over `Vec<Op>` driven by [`Config`].
//!
//! This is the `cargo test` / miri front-end over the shared model. It mirrors
//! the size/align shape the in-tree differential tests used: a weighted small /
//! large size arm plus power-of-two aligns.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::RangeInclusive;
use proptest::prelude::*;
use proptest::strategy::{NewTree, TupleUnion, ValueTree, WA};
use proptest::test_runner::TestRunner;

use crate::{Config, Op};

/// The union arm's exact strategy and value-tree types, factored out to
/// avoid clippy's `type_complexity` lint on the spots that would otherwise
/// repeat them.
type WeightedSizeStrategy = TupleUnion<(WA<RangeInclusive<usize>>, WA<RangeInclusive<usize>>)>;
type WeightedSizeTree = <WeightedSizeStrategy as Strategy>::Tree;

/// The two concrete shapes [`size_strategy`] can produce: one range (an arm
/// disabled by a zero weight) or a weighted union of both ranges. A hand-rolled
/// enum instead of `.boxed()`: `TupleUnion` is already a non-boxing union
/// (its own doc comment: "allows better performance than vanilla `Union`
/// since one does not need to resort to boxing and dynamic dispatch"), so the
/// ONLY boxing `size_strategy` ever needed was to unify a bare
/// `RangeInclusive<usize>` with a `TupleUnion<...>` into one return type.
/// Delegating instead of boxing removes a heap allocation for the strategy
/// object AND — the larger effect — for every drawn value tree:
/// `BoxedStrategy::new_tree` always returns `Box<dyn ValueTree>`, so under the
/// old `.boxed()` form every generated `Alloc`/`AllocZeroed`/`Realloc` size
/// cost one avoidable heap allocation. Measured (Sol-codex review runs 2-4,
/// P4-5): `examples/perf_probe_p4_measurements.rs` isolates this against a
/// same-shape non-boxed baseline and finds ~154 avoidable allocations and
/// roughly a 2x generation-time overhead per 200-op stream draw, entirely
/// attributable to this boxing.
///
/// Both variants delegate to proptest's own, already-correct
/// `RangeInclusive`/`TupleUnion` implementations — this enum adds no shrink
/// logic of its own, so it carries no correctness risk to the zero-weight-arm-
/// stays-excluded-during-shrinking invariant `tests/system_proptest.rs`'s
/// `disabled_small_arm_stays_disabled_while_shrinking` pins.
#[derive(Debug)]
enum SizeStrategy {
    Single(RangeInclusive<usize>),
    Weighted(WeightedSizeStrategy),
}

/// [`SizeStrategy`]'s value tree: an enum, not `Box<dyn ValueTree>`, for the
/// same reason. Every method is a pure delegation to the active variant's own
/// tree.
///
/// `large_enum_variant` is silenced deliberately: the size gap is
/// `WeightedSizeTree` carrying an uninitialized `TupleUnion` branch's own
/// `LazyValueTree`, which embeds a whole (not-yet-generated) `TestRunner` —
/// a proptest-internal cost inherent to `TupleUnion` itself, not something
/// this enum introduces. Boxing this variant to silence the lint would
/// reintroduce a heap allocation on every draw that reaches the weighted
/// arm (the DEFAULT `Config`'s common case) — exactly the cost this enum
/// exists to avoid. A short-lived, transient stack value is the correct
/// trade against a per-draw heap allocation.
#[allow(clippy::large_enum_variant)]
enum SizeValueTree {
    Single(<RangeInclusive<usize> as Strategy>::Tree),
    Weighted(WeightedSizeTree),
}

impl Strategy for SizeStrategy {
    type Tree = SizeValueTree;
    type Value = usize;

    fn new_tree(&self, runner: &mut TestRunner) -> NewTree<Self> {
        match self {
            SizeStrategy::Single(s) => s.new_tree(runner).map(SizeValueTree::Single),
            SizeStrategy::Weighted(s) => s.new_tree(runner).map(SizeValueTree::Weighted),
        }
    }
}

impl ValueTree for SizeValueTree {
    type Value = usize;

    fn current(&self) -> usize {
        match self {
            SizeValueTree::Single(t) => t.current(),
            SizeValueTree::Weighted(t) => t.current(),
        }
    }

    fn simplify(&mut self) -> bool {
        match self {
            SizeValueTree::Single(t) => t.simplify(),
            SizeValueTree::Weighted(t) => t.simplify(),
        }
    }

    fn complicate(&mut self) -> bool {
        match self {
            SizeValueTree::Single(t) => t.complicate(),
            SizeValueTree::Weighted(t) => t.complicate(),
        }
    }
}

/// A weighted size generator. A zero weight structurally removes that arm, so
/// shrinking cannot enter a disabled range. Non-zero weights retain their full
/// `u32` ratio; proptest's tuple union accumulates them in `u64`.
fn size_strategy(config: Config) -> SizeStrategy {
    let small = 1usize..=config.small_max.max(1);
    let large = config.small_max.saturating_add(1)
        ..=config.large_max.max(config.small_max.saturating_add(1));
    match (config.small_weight, config.large_weight) {
        (0, _) => SizeStrategy::Single(large),
        (_, 0) => SizeStrategy::Single(small),
        (small_weight, large_weight) => SizeStrategy::Weighted(TupleUnion::new((
            (small_weight, Arc::new(small)),
            (large_weight, Arc::new(large)),
        ))),
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
