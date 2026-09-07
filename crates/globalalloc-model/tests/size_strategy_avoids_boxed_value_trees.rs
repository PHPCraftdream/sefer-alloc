//! Regression guard for Sol-codex review P4-5 (rounds 2-4): `size_strategy`'s
//! `SizeStrategy`/`SizeValueTree` enum must not silently regress back to
//! `BoxedStrategy`, which reintroduces a heap allocation for every drawn
//! `Alloc`/`AllocZeroed`/`Realloc` size.
//!
//! Measures the MARGINAL allocation cost of one additional op in a stream
//! drawn from the crate's own `op_strategy`, averaged over many seeds —
//! comparing allocation counts between two DIFFERENTLY-SHAPED generators
//! (e.g. `op_strategy` vs. a hand-written non-boxed baseline) from "the same"
//! seed is unreliable: each strategy consumes a different amount of RNG
//! entropy per size draw (a plain range vs. a weighted union both drawing an
//! extra arm-selection bit), so their resulting op-kind sequences diverge
//! after the first differing draw, confounding a single-seed comparison.
//! Measuring the SAME generator at two lengths sidesteps that: the marginal
//! allocs/op no longer depends on matching a separate generator's RNG
//! consumption. The guard now runs with `failure_persistence: None` pinned
//! explicitly in its `ProptestConfig` (the same convention
//! `tests/system_proptest.rs` adopts): the old calibration instead inherited
//! proptest's env-var-dependent default persistence
//! (`PROPTEST_DISABLE_FAILURE_PERSISTENCE` unset ⇒ `SourceParallel`), worth
//! ~1.64 allocs/op of `Box<dyn FailurePersistence>` clone noise that masked
//! the real gap and produced the old ~1.585 vs ~2.336 readings. The current
//! threshold was re-derived from a PAIRED A/B — a faithful boxed counterpart
//! of the same generator shape, run at the same fixed seeds and the same
//! `Config`, asserted to produce byte-identical `Vec<Op>` streams per seed
//! (see `examples/perf_probe_p4_measurements.rs`): 0.015 marginal allocs/op
//! for the enum vs. 0.766 for the boxed form.
//!
//!
//! Not run under Miri (`#![cfg(not(miri))]`, the same convention
//! `tests/system_proptest.rs`'s `cfg!(miri)`-scaled constants already
//! establish in this crate): a `#[global_allocator]` wrapping `System`
//! crashes under Miri's Windows target with a Stacked Borrows violation
//! INSIDE `std::sys::alloc::windows`'s own `Header`-lookback pointer
//! arithmetic, entirely unrelated to this file's own code (the actual test
//! body runs and passes before the crash, which happens later during the
//! `test` harness's own internal mpmc-channel teardown). This crate's Miri
//! CI job runs on `ubuntu-latest`, where `System` has no such
//! Header-lookback backend, so the crash is Windows-Miri-specific — but
//! Miri's own allocator instrumentation makes host-allocator-call counting
//! meaningless under Miri regardless of platform, so skipping here loses no
//! real coverage; the native (non-Miri) run is the only environment this
//! counting technique is meant to observe.

#![cfg(all(feature = "proptest", not(miri)))]

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

use globalalloc_model::{op_strategy, Config, Op};
use proptest::prelude::ProptestConfig;
use proptest::strategy::{BoxedStrategy, Strategy, ValueTree};
use proptest::test_runner::{RngSeed, TestRunner};

struct CountingAlloc;
static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);

/// Serializes the allocation-counting tests (1 and 3): the default test
/// harness runs them on parallel threads, and a concurrent test's own
/// allocations would pollute this binary's global counter.
static MEASURE_LOCK: Mutex<()> = Mutex::new(());

// SAFETY: forwards every call unchanged to `System`, the same allocator this
// process would otherwise use; the counter is a side-effect-free read/write
// of a dedicated atomic, ordering relaxed since only the total count matters.
unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

/// Average total allocation count of ONE `new_tree()` draw of a `len`-op
/// stream from the given strategy, over `seeds` independent fixed seeds.
fn mean_allocs<S: Strategy<Value = Vec<Op>>>(
    build: impl Fn(core::ops::Range<usize>) -> S,
    len: usize,
    seeds: u64,
) -> f64 {
    let strategy = build(len..len + 1);
    let mut total = 0usize;
    for seed in 0..seeds {
        let mut runner = TestRunner::new(guard_config(seed));
        let before = ALLOC_CALLS.load(Ordering::Relaxed);
        let tree = strategy.new_tree(&mut runner).expect("generate a tree");
        std::hint::black_box(tree.current());
        let after = ALLOC_CALLS.load(Ordering::Relaxed);
        total += after - before;
    }
    total as f64 / seeds as f64
}

/// Fixed-seed config with `failure_persistence: None` pinned explicitly, so
/// the counts below no longer depend on the ambient
/// `PROPTEST_DISABLE_FAILURE_PERSISTENCE` env var (see
/// `tests/system_proptest.rs` for the same convention).
fn guard_config(seed: u64) -> ProptestConfig {
    ProptestConfig {
        rng_seed: RngSeed::Fixed(seed),
        failure_persistence: None,
        ..ProptestConfig::default()
    }
}

// Test/example-only: a FAITHFUL boxed counterpart of the crate's private
// `SizeStrategy` — identical ranges, weights, arm order, prop_map closures
// and collection::vec shape, differing ONLY in the final `.boxed()`. The
// old P4-5 baseline used a different generator shape entirely (plain
// 1..=8 ranges), so its deltas were confounded; this one is not.
// Deliberately NOT added to src/strategy.rs — production stays unboxed.
// Duplicated verbatim in examples/perf_probe_p4_measurements.rs.
fn size_strategy_boxed(config: Config) -> BoxedStrategy<usize> {
    let small = 1usize..=config.small_max.max(1);
    let large = config.small_max.saturating_add(1)
        ..=config.large_max.max(config.small_max.saturating_add(1));
    match (config.small_weight, config.large_weight) {
        (0, _) => large.boxed(),
        (_, 0) => small.boxed(),
        (small_weight, large_weight) => proptest::strategy::TupleUnion::new((
            (small_weight, std::sync::Arc::new(small)),
            (large_weight, std::sync::Arc::new(large)),
        ))
        .boxed(),
    }
}

fn op_strategy_boxed(config: Config, len_range: core::ops::Range<usize>) -> BoxedStrategy<Vec<Op>> {
    config.validate();
    let alloc = (size_strategy_boxed(config), align_strategy(config))
        .prop_map(|(size, align)| Op::Alloc { size, align });
    let alloc_zeroed = (size_strategy_boxed(config), align_strategy(config))
        .prop_map(|(size, align)| Op::AllocZeroed { size, align });
    let dealloc = proptest::prelude::any::<usize>().prop_map(Op::Dealloc);
    let realloc = (
        proptest::prelude::any::<usize>(),
        size_strategy_boxed(config),
    )
        .prop_map(|(i, new_size)| Op::Realloc { i, new_size });

    let op = proptest::prelude::prop_oneof![alloc, alloc_zeroed, dealloc, realloc];
    proptest::collection::vec(op, len_range).boxed()
}

fn align_strategy(config: Config) -> impl Strategy<Value = usize> {
    let max_exp = config.max_align.trailing_zeros();
    (0..=max_exp).prop_map(move |exponent| 1usize << exponent)
}

#[test]
fn op_strategy_marginal_allocation_cost_stays_below_boxed_threshold() {
    let _serial = MEASURE_LOCK.lock().unwrap();
    const SEEDS: u64 = 64;
    let short = mean_allocs(|r| op_strategy(Config::default(), r), 20, SEEDS);
    let long = mean_allocs(|r| op_strategy(Config::default(), r), 220, SEEDS);
    let marginal_allocs_per_op = (long - short) / 200.0;

    // Re-derived from the paired A/B (tests 2/3 and the probe): measured
    // marginal allocs/op is 0.015 for the enum and 0.766 for the boxed
    // counterpart under this fixed config (`failure_persistence: None`,
    // 64 fixed seeds), so the threshold sits strictly between them — ~23x
    // above the enum side, ~2.2x below the boxed side — failing hard if
    // boxing is reintroduced. It would only need recalibration if a future
    // proptest changes the NON-boxing path's own allocation behavior (an
    // over-threshold enum reading false-positives loudly rather than
    // silently passing a regression).
    const THRESHOLD: f64 = 0.35;
    assert!(
        marginal_allocs_per_op < THRESHOLD,
        "op_strategy's marginal allocation cost is {marginal_allocs_per_op:.4} allocs/op \
         (short-stream mean {short:.2}, long-stream mean {long:.2}), at or above the \
         {THRESHOLD} threshold that separates the enum-based size_strategy fix from a \
         regression back to BoxedStrategy (Sol-codex review P4-5)"
    );
}

#[test]
fn paired_ab_enum_and_boxed_counterpart_produce_identical_streams() {
    // Not a counting test itself, but its draws would pollute the global
    // allocation counter while a counting test measures — serialize too.
    let _serial = MEASURE_LOCK.lock().unwrap();
    const SEEDS: u64 = 64;
    const LEN: core::ops::Range<usize> = 200..201;
    let configs = [
        ("default", Config::default()),
        (
            "single",
            Config {
                large_weight: 0,
                ..Config::default()
            },
        ),
    ];
    for (name, config) in configs {
        for seed in 0..SEEDS {
            let enum_tree = op_strategy(config, LEN)
                .new_tree(&mut TestRunner::new(guard_config(seed)))
                .expect("generate a tree");
            let boxed_tree = op_strategy_boxed(config, LEN)
                .new_tree(&mut TestRunner::new(guard_config(seed)))
                .expect("generate a tree");
            assert_eq!(
                enum_tree.current(),
                boxed_tree.current(),
                "seed {seed}, config {name}: paired enum/boxed trees diverged"
            );
        }
    }

    // Full-shrink walks agree too (default config): same step count and the
    // same final simplified value.
    let boxed_strategy = op_strategy_boxed(Config::default(), LEN);
    for seed in 0..4u64 {
        let mut enum_tree = op_strategy(Config::default(), LEN)
            .new_tree(&mut TestRunner::new(guard_config(seed)))
            .expect("generate a tree");
        let mut boxed_tree = boxed_strategy
            .new_tree(&mut TestRunner::new(guard_config(seed)))
            .expect("generate a tree");
        let mut enum_steps = 0usize;
        let mut boxed_steps = 0usize;
        while enum_tree.simplify() {
            enum_steps += 1;
        }
        while boxed_tree.simplify() {
            boxed_steps += 1;
        }
        assert_eq!(
            enum_tree.current(),
            boxed_tree.current(),
            "seed {seed}: fully-shrunk values diverged"
        );
        assert_eq!(
            enum_steps, boxed_steps,
            "seed {seed}: shrink trajectories diverged"
        );
    }
}

#[test]
fn threshold_still_separates_enum_from_boxed_paired_ab() {
    let _serial = MEASURE_LOCK.lock().unwrap();
    const SEEDS: u64 = 64;
    // `.boxed()` on the enum arm wraps the strategy object once at build
    // time, and its `new_tree` boxes the returned stream tree once per draw
    // — the same +1 alloc at BOTH measured lengths, so it cancels in the
    // (long − short) / 200 marginal and the per-op behavior measured is the
    // enum's own.
    let build_enum = |r: core::ops::Range<usize>| op_strategy(Config::default(), r).boxed();
    let build_boxed = |r: core::ops::Range<usize>| op_strategy_boxed(Config::default(), r);

    let marginal = |build: &dyn Fn(core::ops::Range<usize>) -> BoxedStrategy<Vec<Op>>| {
        (mean_allocs(build, 220, SEEDS) - mean_allocs(build, 20, SEEDS)) / 200.0
    };
    let enum_marginal = marginal(&build_enum);
    let boxed_marginal = marginal(&build_boxed);

    const THRESHOLD: f64 = 0.35;
    println!(
        "paired A/B marginal allocs/op: enum {enum_marginal:.4} < THRESHOLD {THRESHOLD} < \
         boxed {boxed_marginal:.4}"
    );
    assert!(
        enum_marginal < THRESHOLD && THRESHOLD < boxed_marginal,
        "the {THRESHOLD} threshold no longer separates enum from boxed: enum {enum_marginal:.4}, \
         boxed {boxed_marginal:.4} marginal allocs/op — recalibrate \
         op_strategy_marginal_allocation_cost_stays_below_boxed_threshold before its guard \
         silently stops discriminating (e.g. after a proptest version bump)"
    );
}
