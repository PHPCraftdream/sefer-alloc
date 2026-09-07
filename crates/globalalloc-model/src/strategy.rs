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
/// cost one avoidable heap allocation. Measured (P4-1 re-measurement,
/// `examples/perf_probe_p4_measurements.rs`): a PAIRED A/B against a faithful
/// boxed counterpart — identical generator shape, same per-seed RNG, asserted
/// byte-identical `Vec<Op>` streams per seed — finds +0.75 boxing-attributable
/// allocs per op (≈150 per 200-op stream draw) and ≈+67 KiB total/peak heap
/// bytes per 200-op draw at the default `Config`. The earlier "~2x
/// generation-time overhead" claim was retracted: that measurement compared
/// against a differently-shaped baseline (confounded); wall-time in the
/// paired run is mostly run-to-run noise; the one consistent signal across
/// repeated runs is that at the default `Config` the boxed form paid slower
/// full-shrink walks (~16-38% over three runs) and a 3-14x slower drop
/// (freeing the ~150 per-draw heap blocks) — the allocation counts above are
/// the durable part of the result.
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
/// Lifetime and memory reality (measured via
/// `size_strategy_repr_sizes`, see `examples/perf_probe_p4_measurements.rs`):
/// these trees are NOT short-lived transient stack values. proptest's
/// `VecValueTree` stores one tree per generated op in its
/// `elements: Vec<T>` for the op-stream tree's ENTIRE lifetime, including
/// shrinking — so this enum's inline representation grows every element's
/// stride in that Vec: O(N) memory with a large constant, replacing the
/// 16-byte (2-word) `Box<dyn ValueTree>` slot the `.boxed()` form used.
/// Measured `size_of`s: `SizeValueTree` 1152 B (the `Weighted` arm IS the
/// whole enum; it embeds two proptest `LazyValueTree`s, each of which can
/// hold a whole `TestRunner` inline), the zero-weight `Single` arm's
/// tree 24 B, and the per-op element tree of the stream 4240 B (the 4-arm
/// `TupleUnionValueTree` whose size-tree slots make up that 1152 B).
///
/// Measured trade-off (P4-5 paired, same-seed A/B against the faithful
/// boxed counterpart, 200-op stream draws) — BOTH config rows reported:
///
/// - At the DEFAULT `Config` (the shape this crate's own test suite and the
///   round-4 fix target) the enum measured FEWER total and peak-live heap
///   bytes than the boxed counterpart (848,124 B vs 915,594 B per 200-op
///   draw) AND far fewer allocations (0.015 vs 0.766 per op) — inline wins
///   on every measured axis.
/// - At a zero-weight/`Single` `Config` the enum still wins allocations
///   (0.015 vs 0.765 per op) and wall-time, but costs ~384 KiB MORE heap
///   per 200-op draw (848,124 B vs 464,204 B): every element's slot is
///   sized for the `Weighted` variant even when only the 24 B `Single`
///   tree is ever initialized — the enum's default-config and single-config
///   rows are byte-identical at 848,124 B, which is exactly this
///   layout-driven slot sizing. Accepted because it is bounded per drawn
///   op-stream case (a testing harness holds one stream per active
///   proptest case, not per unit of real work), and boxing only the
///   `Weighted` variant would reintroduce the per-draw heap allocation on
///   the DEFAULT config's common path — the exact cost the round-4 fix
///   removed.
///
/// `large_enum_variant` is silenced deliberately: the size gap is
/// `WeightedSizeTree` carrying an uninitialized `TupleUnion` branch's own
/// `LazyValueTree`, which embeds a whole (not-yet-generated) `TestRunner` —
/// a proptest-internal cost inherent to `TupleUnion` itself, not something
/// this enum introduces. Boxing this variant to silence the lint would
/// reintroduce a heap allocation on every draw that reaches the weighted
/// arm (the DEFAULT `Config`'s common case) — exactly the cost this enum
/// exists to avoid.
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
    prop::collection::vec(op_element_strategy(config), len_range)
}

/// One op: the per-element strategy [`op_strategy`] collects into a stream.
/// Same expression tree the previous inline body of `op_strategy` built, so
/// behavior is identical.
fn op_element_strategy(config: Config) -> impl Strategy<Value = Op> {
    let alloc = (size_strategy(config), align_strategy(config))
        .prop_map(|(size, align)| Op::Alloc { size, align });
    let alloc_zeroed = (size_strategy(config), align_strategy(config))
        .prop_map(|(size, align)| Op::AllocZeroed { size, align });
    let dealloc = any::<usize>().prop_map(Op::Dealloc);
    let realloc = (any::<usize>(), size_strategy(config))
        .prop_map(|(i, new_size)| Op::Realloc { i, new_size });

    prop_oneof![alloc, alloc_zeroed, dealloc, realloc]
}

// Second test-only export from this module, alongside `op_strategy` (the
// established test-only-forwarder category — same rationale as
// `crate::peak_live_count` in `src/lib.rs`): static size introspection for
// this module's private `SizeStrategy`/`SizeValueTree` types, used only by
// `examples/perf_probe_p4_measurements.rs`.

/// Doc-hidden, `internals`-gated static size introspection for this module's
/// private `SizeStrategy`/`SizeValueTree` types. Not stable public API; exists
/// solely so memory-footprint measurements in
/// `examples/perf_probe_p4_measurements.rs` can report exact compile-time
/// sizes they cannot otherwise name (the types are private behind
/// `impl Strategy`). Same test-only-export rationale as
/// `crate::peak_live_count` (see `src/lib.rs`).
#[cfg(feature = "internals")]
#[doc(hidden)]
pub struct SizeStrategyReprSizes {
    /// `size_of::<SizeStrategy>()` — the strategy object itself.
    pub size_strategy: usize,
    /// `size_of::<SizeValueTree>()` — one drawn size value tree, inline.
    pub size_value_tree: usize,
    /// `size_of` of the zero-weight `Single` arm's `RangeInclusive` tree.
    pub single_tree: usize,
    /// `size_of::<WeightedSizeTree>()` — the weighted-union tree (two
    /// `LazyValueTree`s, each able to embed a whole `TestRunner`).
    pub weighted_tree: usize,
    /// `size_of` of ONE element of the op-stream `VecValueTree` (the per-op
    /// 4-arm `TupleUnionValueTree` that embeds size-tree slots). This is the
    /// stride item whose growth the enum's inline representation pays on
    /// every element of a whole stream.
    pub op_element_tree: usize,
}

/// See [`SizeStrategyReprSizes`].
#[cfg(feature = "internals")]
#[doc(hidden)]
pub fn size_strategy_repr_sizes(config: Config) -> SizeStrategyReprSizes {
    fn tree_size<S: Strategy>(_: &S) -> usize {
        core::mem::size_of::<S::Tree>()
    }
    let element = op_element_strategy(config);
    SizeStrategyReprSizes {
        size_strategy: core::mem::size_of::<SizeStrategy>(),
        size_value_tree: core::mem::size_of::<SizeValueTree>(),
        single_tree: core::mem::size_of::<<RangeInclusive<usize> as Strategy>::Tree>(),
        weighted_tree: core::mem::size_of::<WeightedSizeTree>(),
        op_element_tree: tree_size(&element),
    }
}
