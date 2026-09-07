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
//! consumption. Calibrated empirically (examples/perf_probe_p4_measurements.rs's
//! `marginal_cost_calibration` module) at ~1.585 allocs/op with this fix and
//! ~2.336 allocs/op reverted to `BoxedStrategy` — clearly separated by the
//! 2.0 threshold below.

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

use globalalloc_model::{op_strategy, Config};
use proptest::prelude::ProptestConfig;
use proptest::strategy::{Strategy, ValueTree};
use proptest::test_runner::{RngSeed, TestRunner};

struct CountingAlloc;
static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);

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
/// stream from the default `Config`, over `seeds` independent fixed seeds.
fn mean_allocs_for_len(len: usize, seeds: u64) -> f64 {
    let strategy = op_strategy(Config::default(), len..len + 1);
    let mut total = 0usize;
    for seed in 0..seeds {
        let mut runner = TestRunner::new(ProptestConfig {
            rng_seed: RngSeed::Fixed(seed),
            ..ProptestConfig::default()
        });
        let before = ALLOC_CALLS.load(Ordering::Relaxed);
        let tree = strategy.new_tree(&mut runner).expect("generate a tree");
        std::hint::black_box(tree.current());
        let after = ALLOC_CALLS.load(Ordering::Relaxed);
        total += after - before;
    }
    total as f64 / seeds as f64
}

#[test]
fn op_strategy_marginal_allocation_cost_stays_below_boxed_threshold() {
    const SEEDS: u64 = 64;
    let short = mean_allocs_for_len(20, SEEDS);
    let long = mean_allocs_for_len(220, SEEDS);
    let marginal_allocs_per_op = (long - short) / 200.0;

    // Threshold sits strictly between the two calibrated values (~1.585 with
    // this fix, ~2.336 reverted to `BoxedStrategy`): a large enough margin on
    // both sides to absorb ordinary proptest bookkeeping variance across
    // patch/minor versions (Cargo.toml pins only `proptest = "1"`), while
    // still failing hard if boxing is reintroduced.
    const THRESHOLD: f64 = 2.0;
    assert!(
        marginal_allocs_per_op < THRESHOLD,
        "op_strategy's marginal allocation cost is {marginal_allocs_per_op:.4} allocs/op \
         (short-stream mean {short:.2}, long-stream mean {long:.2}), at or above the \
         {THRESHOLD} threshold that separates the enum-based size_strategy fix from a \
         regression back to BoxedStrategy (Sol-codex review P4-5)"
    );
}
