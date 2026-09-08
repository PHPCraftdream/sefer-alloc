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
/// `examples/perf_probe_p4_measurements.rs`, corrected after review findings
/// P3-1/P3-2/P4-3): a PAIRED A/B against a faithful boxed counterpart —
/// identical generator shape, same per-seed RNG, asserted byte-identical
/// `Vec<Op>` streams per seed — finds +0.75 boxing-attributable allocs per op
/// (≈150 per 200-op stream draw; a fresh run measured 0.000 vs 0.751/0.750 —
/// small-count run-to-run drift on the enum arm; the ~+0.75 paired gap is the
/// durable signal). The heap-byte picture is regime-dependent, which is
/// exactly why the probe's single mislabeled "draw" scenario was retired into
/// three separately-labeled memory scenarios (probe-internal labels S1/S2/S3,
/// described below and in the example): in the simplify-only regime the enum
/// still measures +67,470 B total/peak per 200-op draw at the default
/// `Config`, but at a plain successful draw the BOXED form uses LESS memory
/// (-221,196 B at default, -390,009 B at single), and the shrink-protocol
/// regime is +62,448 B enum-side at default. The earlier "~2x
/// generation-time overhead" claim was retracted: that measurement compared
/// against a differently-shaped baseline (confounded). Wall-time in the
/// paired run is NOT consistent across configs on this loaded host: at the
/// default `Config` the boxed form measured slower full-shrink walks (~24%,
/// 529,661 vs 428,006 ns/draw) and a ~13x slower drop, but at the single
/// config the boxed form measured FASTER on both — only the allocation
/// counts are the durable part of the result.
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
/// 2-word (`2 x size_of::<usize>()`, 16 B on 64-bit targets) slot the
/// `.boxed()` form used. Measured `size_of`s (re-confirmed by the corrected
/// run): `SizeValueTree` 1152 B (the `Weighted` arm IS the
/// whole enum; it embeds two proptest `LazyValueTree`s, each of which can
/// hold a whole `TestRunner` inline), the zero-weight `Single` arm's
/// tree 24 B, the per-op element tree of the stream 4240 B (the 4-arm
/// `TupleUnionValueTree` whose size-tree slots make up that 1152 B), and
/// `SizeStrategy` itself 40 B. These are measurements of THIS
/// target/toolchain/feature set (x86_64-pc-windows, 64-bit, proptest 1.11.0,
/// `internals` enabled), NOT portable contracts of the types.
///
/// Measured trade-off (P4-5 paired, same-seed A/B against the faithful
/// boxed counterpart, 200-op stream draws; the probe's three scenarios:
/// S1 = a successful draw from `new_tree` through `current()` to drop with
/// NO shrinking, S2 = a full simplify-only shrink walk without per-step
/// `current()`, S3 = the REAL shrink protocol — proptest's own
/// `TestRunner::run_one` accept/reject/complicate walk under an
/// explicitly CUSTOM 65,536-iteration budget (NOT the default runner,
/// whose resolved limit is cases * 4 = 1024); "peak" = trajectory
/// peak-live; per-window realloc counts from the corrected realloc-honest
/// counters) — BOTH config rows reported, totals/peaks in heap bytes:
///
/// - S1, default `Config` (n=64 draws): enum 860,316/854,268; boxed
///   639,120/633,072; delta (boxed - enum) -221,196 on both axes; 6.0
///   reallocs/window. S1, single `Config`: enum 860,316/854,268; boxed
///   470,307/464,259; delta -390,009 on both; 6.0 reallocs/window.
/// - S2, default (n=64): enum 848,124/848,124; boxed 915,594/915,594;
///   delta +67,470 on both; 0.0 reallocs/window (boxed post-construct
///   626,928 B, window max 951,036 B). S2, single: enum 848,124/848,124;
///   boxed 464,204/464,204; delta -383,920 on both; 0.0 reallocs/window.
/// - S3, default (n=8 draws; REAL `TestRunner::run_one` walk under the
///   CUSTOM 65,536-iteration budget, 0 seeds cap-truncated; 15,547.0
///   evals/draw, 15,545.5 accepts, 1.5 complicate attempts): enum
///   190,409,340 total/854,268 peak (dominated by ~15.5k per-eval
///   `Vec<Op>` materializations); boxed 190,471,788/916,716; delta
///   (boxed - enum) +62,448 on both — enum smaller; 93,288.0
///   reallocs/window. S3, single (15,886.6 evals/draw; 1.6 complicate
///   attempts): enum 194,550,048/854,268; boxed 194,166,234/470,454;
///   delta -383,814 on both — boxed smaller; 95,325.8 reallocs/window.
///
/// Three findings follow. (1) The previously published figures (848,124
/// vs 915,594 default; 848,124 vs 464,204 single) reproduce EXACTLY as
/// the S2 totals under the corrected counter, and S2 windows contain
/// 0.0 reallocs — so the old counter bug (which only fired on realloc)
/// did not numerically affect them; what was wrong was the label: those
/// numbers are construction PLUS a full simplify-only walk, not a plain
/// draw. (2) The regime ordering FLIPS: at a plain successful draw (S1)
/// the boxed form uses LESS memory at BOTH configs, because proptest's
/// `TupleUnion` only materializes the boxed arms' extra size-Boxes while
/// simplifying; the enum's inline stride is paid from construction;
/// "the enum uses less memory" is true ONLY in the shrink/simplify
/// regimes (S2, and S3 peak). (3) The enum's total/peak bytes are
/// byte-identical across default and single configs in EVERY scenario
/// (860,316/854,268 in S1, 848,124 in S2, 854,268 peak in S3) — the
/// `Weighted` slot dominates regardless of config — while the boxed
/// counterpart varies by config.
///
/// Acceptance rationale, stated honestly: the enum does NOT win on every
/// measured axis. It wins allocations at every config (~+0.75/op boxed;
/// the regression-test 0.35 threshold stays valid), and it wins memory in
/// the shrink regimes; but a plain successful draw costs it MORE heap than
/// boxed at both configs (-221,196 B default, -390,009 B single), bounded
/// per drawn op-stream case (a testing harness holds one stream per active
/// proptest case, not per unit of real work). Boxing only the `Weighted`
/// variant would still reintroduce the per-draw heap allocation on the
/// DEFAULT config's common path — the exact cost the round-4 fix removed.
/// This is measurement, not marketing, in either direction.
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

/// See [`SizeStrategyReprSizes`]. Takes no arguments: the result is a pure
/// compile-time quantity over this module's tree TYPES, independent of any
/// `Config`. The previous form built a whole element strategy just to read
/// `size_of` off its tree type, allocating the outer `Arc` union branches
/// (and, at default weights, an `Arc` per range branch of every size
/// generator) for nothing. Passing the never-called factory keeps the
/// concrete type opaque to the caller. Cold-path allocation hygiene, NOT a
/// claimed speedup (this runs once per probe invocation).
#[cfg(feature = "internals")]
#[doc(hidden)]
pub fn size_strategy_repr_sizes() -> SizeStrategyReprSizes {
    fn element_tree_size<S: Strategy<Value = Op>>(_: &impl Fn(Config) -> S) -> usize {
        core::mem::size_of::<S::Tree>()
    }
    SizeStrategyReprSizes {
        size_strategy: core::mem::size_of::<SizeStrategy>(),
        size_value_tree: core::mem::size_of::<SizeValueTree>(),
        single_tree: core::mem::size_of::<<RangeInclusive<usize> as Strategy>::Tree>(),
        weighted_tree: core::mem::size_of::<WeightedSizeTree>(),
        op_element_tree: element_tree_size(&op_element_strategy),
    }
}
