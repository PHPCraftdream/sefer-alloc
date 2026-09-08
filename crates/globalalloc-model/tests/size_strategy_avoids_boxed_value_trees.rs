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
//! for the enum vs. 0.766 for the boxed form. Numerator convention (review
//! P4-2): this wrapper does NOT override `GlobalAlloc::realloc`, so each
//! realloc event passes through the default implementation's one `alloc`
//! call and lands in `ALLOC_CALLS` — these figures count alloc calls PLUS
//! realloc events, exactly the probe's ALLOC_CALLS + REALLOC_CALLS SUM
//! numerator (the probe also reports alloc-only and realloc-only
//! separately; its alloc-only enum figure is 0.000, and the previously
//! published enum-side 0.015 -> 0.000 change was that definitional split,
//! not drift).
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
//!
//! Beyond allocation counting, the paired test below also pins the shrink
//! TRAJECTORY: the enum tree and its boxed counterpart are driven in lockstep
//! by a fixed decision schedule, and after every `simplify()`/`complicate()`
//! call both the returned bool and the full `current()` stream must match
//! step-for-step (both `Single` delegation arms included, over a bounded
//! prefix of each walk), with `complicate()` guarded by a path-activation
//! oracle (Sol-codex round-5 review P4-1).

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

    // Re-derived from the paired A/B (tests 2/3 and the probe). Numerator:
    // this wrapper's ALLOC_CALLS, which — because `realloc` is NOT
    // overridden and the trait default routes the new block through the
    // wrapper's own `alloc` — counts realloc events too, i.e. the probe's
    // ALLOC_CALLS + REALLOC_CALLS SUM numerator (review P4-2). Measured
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

/// Verifies three things about the crate's enum-based `op_strategy` against
/// its faithful boxed counterpart (`op_strategy_boxed` above):
///
/// 1. Per-seed INITIAL stream equality: for 64 seeds x 2 configs, the
///    `current()` `Vec<Op>` of a fresh draw must be identical between the
///    enum arm and the boxed arm.
/// 2. LOCKSTEP shrink walks (Sol-codex round-5 review P4-1): the enum tree
///    and the boxed counterpart are driven by the same fixed decision
///    schedule, and after EVERY `simplify()` AND every `complicate()` call
///    the returned bool AND the full `current()` `Vec<Op>` must be identical
///    between the two arms — trajectory equivalence over the bounded prefix
///    of each walk, not merely matching step counts and endpoints.
///    It covers the weighted default `Config` AND both zero-weight `Single`
///    configs (`small_weight: 0` selects the `SizeValueTree::Single`
///    large-range arm, `large_weight: 0` the small-range arm), because those
///    select different delegation arms in the private `SizeValueTree`
///    `simplify`/`complicate` impls.
/// 3. A path-activation oracle: `complicate()` must actually be called
///    `MAX_COMPLICATES` times per walk and return true at least
///    `MIN_TRUE_COMPLICATES` times, so the backoff branch cannot silently
///    stop being exercised (proving a config is wrong is not enough — the
///    intended mechanism must be shown to fire).
#[test]
fn paired_ab_enum_and_boxed_counterpart_streams_and_shrink_steps_match_step_for_step() {
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

    // Cap on seeds per lockstep walk: fixed seeds + a fixed decision schedule
    // are fully deterministic, and 4 seeds x 3 configs already observe
    // complicate() behavior densely enough to calibrate the oracle.
    //
    // Cap on simplify steps per walk: full trajectories of a 200-op stream
    // are ~15.5k simplify steps per draw (S3 measurement in
    // examples/perf_probe_p4_measurements.rs), so STEP_CAP truncates
    // deliberately and the run stays bounded on a loaded machine.
    //
    // Drive a complicate() back-off every COMPLICATE_EVERY-th successful
    // simplify, capped at MAX_COMPLICATES backoffs per walk — enough
    // invocations that the backoff branch is well exercised per walk.
    //
    // Oracle floors: the walk must reach STEP_CAP, call complicate() EXACTLY
    // MAX_COMPLICATES times (fixed cadence + cap make this deterministic), and
    // see at least MIN_TRUE_COMPLICATES true returns (calibrated at ~3/4 of
    // the observed minimum across all config/seed walks) — the caps are NOT
    // so low that complicate never fires.
    const SEEDS_LOCKSTEP: u64 = 4;
    const STEP_CAP: usize = 1024;
    const COMPLICATE_EVERY: usize = 8;
    const MAX_COMPLICATES: usize = 32;
    const MIN_TRUE_COMPLICATES: usize = 24;

    let configs = [
        ("default", Config::default()),
        (
            "single-small-disabled",
            Config {
                small_weight: 0,
                ..Config::default()
            },
        ),
        (
            "single-large-disabled",
            Config {
                large_weight: 0,
                ..Config::default()
            },
        ),
    ];
    for (name, config) in configs {
        let enum_strategy = op_strategy(config, LEN);
        let boxed_strategy = op_strategy_boxed(config, LEN);
        for seed in 0..SEEDS_LOCKSTEP {
            let mut enum_tree = enum_strategy
                .new_tree(&mut TestRunner::new(guard_config(seed)))
                .expect("generate a tree");
            let mut boxed_tree = boxed_strategy
                .new_tree(&mut TestRunner::new(guard_config(seed)))
                .expect("generate a tree");
            assert_eq!(
                enum_tree.current(),
                boxed_tree.current(),
                "seed {seed}, config {name}: paired enum/boxed trees diverged"
            );
            let mut steps = 0usize;
            let mut complicate_calls = 0usize;
            let mut true_complicates = 0usize;
            loop {
                let enum_simplified = enum_tree.simplify();
                let boxed_simplified = boxed_tree.simplify();
                assert_eq!(
                    enum_simplified, boxed_simplified,
                    "seed {seed}, config {name}, step {steps}: simplify() return values \
                     diverged (enum {enum_simplified}, boxed {boxed_simplified})"
                );
                if !enum_simplified {
                    break;
                }
                steps += 1;
                assert_eq!(
                    enum_tree.current(),
                    boxed_tree.current(),
                    "seed {seed}, config {name}, step {steps}: current() diverged after \
                     simplify()"
                );
                // Accept/reject backoff in the shape a real TestRunner uses:
                // on every COMPLICATE_EVERY-th successful simplify, treat the
                // candidate as accepted and complicate() to back off one
                // step, until MAX_COMPLICATES backoffs have been exercised.
                if steps % COMPLICATE_EVERY == 0 && complicate_calls < MAX_COMPLICATES {
                    let enum_complicated = enum_tree.complicate();
                    let boxed_complicated = boxed_tree.complicate();
                    complicate_calls += 1;
                    true_complicates += usize::from(enum_complicated);
                    assert_eq!(
                        enum_complicated, boxed_complicated,
                        "seed {seed}, config {name}, step {steps}: complicate() return values \
                         diverged (enum {enum_complicated}, boxed {boxed_complicated})"
                    );
                    assert_eq!(
                        enum_tree.current(),
                        boxed_tree.current(),
                        "seed {seed}, config {name}, step {steps}: current() diverged after \
                         complicate()"
                    );
                }
                if steps >= STEP_CAP {
                    break;
                }
            }
            assert_eq!(
                steps, STEP_CAP,
                "seed {seed}, config {name}: walk ended after {steps} steps, short of the \
                 {STEP_CAP}-step promised prefix — trajectories are shorter than the cap, \
                 recalibrate instead of silently shrinking coverage"
            );
            assert_eq!(
                complicate_calls, MAX_COMPLICATES,
                "seed {seed}, config {name}: complicate() called {complicate_calls} times, expected \
                 exactly {MAX_COMPLICATES} (STEP_CAP/{COMPLICATE_EVERY} backoff slots, capped) — the \
                 backoff schedule changed, recalibrate"
            );
            assert!(
                true_complicates >= MIN_TRUE_COMPLICATES,
                "seed {seed}, config {name}: complicate() returned true only \
                 {true_complicates}/{complicate_calls} times (floor {MIN_TRUE_COMPLICATES}) — \
                 back-off is not really happening"
            );
        }
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
