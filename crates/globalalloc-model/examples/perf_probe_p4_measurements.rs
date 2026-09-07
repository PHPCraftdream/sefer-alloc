//! Exploratory measurement for the 3 P4 performance follow-ups deferred by
//! Sol-codex review rounds 2-4 (per-byte fill/verify cost, O(M^2) overlap
//! scan, `BoxedStrategy` heap-allocation overhead). Not a shipped
//! benchmark — a one-off probe to decide whether any of the three is worth
//! a structural fix, per this project's measurement-before-restructuring
//! discipline. Requires `--features proptest` for the P4-5 section.
//!
//! Scope of the overlap-scan verdict: it is a NO-GO for the regimes actually
//! measured below — K=2048 (the `arbitrary` front-end's per-`OpStream`
//! decode-attempt ceiling) and K~200 (the native integration tests' own
//! stream length) — NOT a claim that those K values bound the crate. The
//! driver's real cost is O(M + M*K + B), worst case O(M^2 + B), where M is
//! the op count, K the peak live count, and B the total oracle byte work
//! (fill + verify passes over block contents): every block-creating op is
//! compared against every currently-live block. `op_strategy`'s `len_range`
//! is caller-supplied and `Config` permits a frequent (not merely rare)
//! large arm, so a hand-built or long generated stream can reach a far
//! larger peak live count than either measured regime; the linear scan
//! stays because no measured regime approached a cost that would pay for an
//! ordered interval index.
//!
//! `pattern_byte` is duplicated here verbatim from `src/drive.rs` (it is
//! private) — same "independent copy" rationale `tests/oracle_negative.rs`
//! already uses.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use globalalloc_model::{drive, Config, Op};

// --- P4-5: a counting global allocator, active for the whole process ---
struct CountingAlloc;
static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static TOTAL_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_LIVE: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        TOTAL_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
        let live = LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
        PEAK_LIVE.fetch_max(live, Ordering::Relaxed);
        // SAFETY: forwarding to the System allocator, as before.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: forwarding to the System allocator, as before.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        let live = LIVE_BYTES.fetch_add(new_size, Ordering::Relaxed) + new_size - layout.size();
        PEAK_LIVE.fetch_max(live, Ordering::Relaxed);
        // SAFETY: forwarding to the System allocator, as before.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

fn pattern_byte(fill: u8, offset: usize) -> u8 {
    if offset == 0 {
        return fill;
    }
    let mut x = (fill as u32) ^ (offset as u32).wrapping_mul(0x9E37_79B1);
    x = x.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 13;
    x = x.wrapping_mul(0xC2B2_AE35);
    x ^= x >> 16;
    x as u8
}

fn pattern_byte_old_additive(fill: u8, offset: usize) -> u8 {
    fill.wrapping_add(offset as u8)
}

fn main() {
    println!("=== P4-3: pattern_byte marginal cost (isolated, not through drive) ===");
    {
        const N: u64 = 200_000_000;
        let start = Instant::now();
        let mut acc: u64 = 0;
        for i in 0..N {
            let fill = (i % 255 + 1) as u8;
            let offset = (i % 65536) as usize;
            acc = acc.wrapping_add(std::hint::black_box(pattern_byte(fill, offset)) as u64);
        }
        let elapsed = start.elapsed();
        println!(
            "  new (hash-mixed, offset-0 raw): {N} calls in {elapsed:?} ({:.3} ns/call) [acc={acc}]",
            elapsed.as_secs_f64() * 1e9 / N as f64
        );

        let start = Instant::now();
        let mut acc: u64 = 0;
        for i in 0..N {
            let fill = (i % 255 + 1) as u8;
            let offset = (i % 65536) as usize;
            acc = acc
                .wrapping_add(std::hint::black_box(pattern_byte_old_additive(fill, offset)) as u64);
        }
        let elapsed = start.elapsed();
        println!(
            "  old (additive):                 {N} calls in {elapsed:?} ({:.3} ns/call) [acc={acc}]",
            elapsed.as_secs_f64() * 1e9 / N as f64
        );
    }

    println!("\n=== P4-3/P4-4: end-to-end drive() cost by regime ===");
    {
        // Regime A: byte-oracle-heavy — few large blocks, minimal live count.
        let large_ops: Vec<Op> = (0..50)
            .map(|_| Op::Alloc {
                size: 128 * 1024,
                align: 8,
            })
            .collect();
        let start = Instant::now();
        drive(&System, Config::default(), &large_ops);
        let elapsed = start.elapsed();
        let total_bytes: usize = 50 * 128 * 1024;
        println!(
            "  Regime A (50 x 128 KiB blocks, K=50 peak live, ~{} MiB touched): {elapsed:?} ({:.3} ns/byte-op-equivalent)",
            total_bytes / 1024 / 1024,
            elapsed.as_secs_f64() * 1e9 / total_bytes as f64
        );

        // Regime B: overlap-scan-heavy — many tiny blocks, high live count.
        // K=2048 is the `arbitrary` front-end's per-`OpStream`
        // decode-attempt ceiling (`MAX_OPS` in src/arbitrary_stream.rs), NOT
        // a limit on the public driver or on `op_strategy` (whose
        // `len_range` is caller-supplied): a hand-built or long generated
        // stream can reach a far larger peak live count, so this is one
        // measured regime, not the crate's most adversarial reachable case.
        let small_ops: Vec<Op> = (0..2048).map(|_| Op::Alloc { size: 8, align: 8 }).collect();
        let start = Instant::now();
        drive(&System, Config::default(), &small_ops);
        let elapsed = start.elapsed();
        println!(
            "  Regime B (2048 x 8 B blocks, K=2048 peak live, O(K^2/2)~2.1M overlap compares): {elapsed:?} ({:.1} ns/op)",
            elapsed.as_secs_f64() * 1e9 / 2048.0
        );

        // Regime B at the native integration tests' own stream length
        // (tests/system_proptest.rs MAX_LEN, default 200) — a test-suite
        // constant, not a Config or driver limit.
        let small_ops_200: Vec<Op> = (0..200).map(|_| Op::Alloc { size: 8, align: 8 }).collect();
        let start = Instant::now();
        drive(&System, Config::default(), &small_ops_200);
        let elapsed = start.elapsed();
        println!(
            "  Regime B at typical K=200: {elapsed:?} ({:.1} ns/op)",
            elapsed.as_secs_f64() * 1e9 / 200.0
        );
    }

    #[cfg(feature = "proptest")]
    {
        #[cfg(feature = "internals")]
        {
            let sizes = globalalloc_model::size_strategy_repr_sizes(Config::default());
            println!(
                "\n=== P4-5: static sizes (size_of, compile-time) ===\n  SizeStrategy: {} B; SizeValueTree (inline, replaces a 2-word Box<dyn ValueTree> = 16 B slot): {} B; Single tree: {} B; WeightedSizeTree: {} B; per-op element tree in the stream VecValueTree: {} B",
                sizes.size_strategy,
                sizes.size_value_tree,
                sizes.single_tree,
                sizes.weighted_tree,
                sizes.op_element_tree
            );
        }
        #[cfg(not(feature = "internals"))]
        {
            println!(
                "\n=== P4-5: static sizes need --features internals (the heap-byte proxies below do not) ==="
            );
        }
        p4_paired_ab::run();
    }
}

#[cfg(feature = "proptest")]
mod p4_paired_ab {
    use super::{ALLOC_CALLS, LIVE_BYTES, PEAK_LIVE, TOTAL_BYTES};
    use globalalloc_model::{op_strategy, Config, Op};
    use proptest::prelude::ProptestConfig;
    use proptest::strategy::{BoxedStrategy, Strategy, ValueTree};
    use proptest::test_runner::{FileFailurePersistence, RngSeed, TestRunner};
    use std::sync::atomic::Ordering;
    use std::time::Instant;

    // Test/example-only: a FAITHFUL boxed counterpart of the crate's private
    // `SizeStrategy` — identical ranges, weights, arm order, prop_map closures
    // and collection::vec shape, differing ONLY in the final `.boxed()`. The
    // old P4-5 baseline used a different generator shape entirely (plain
    // 1..=8 ranges), so its deltas were confounded; this one is not.
    // Deliberately NOT added to src/strategy.rs — production stays unboxed.
    // Duplicated verbatim in tests/size_strategy_avoids_boxed_value_trees.rs.
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

    fn op_strategy_boxed(
        config: Config,
        len_range: core::ops::Range<usize>,
    ) -> BoxedStrategy<Vec<Op>> {
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

    fn runner(
        seed: u64,
        persistence: Option<Box<dyn proptest::test_runner::FailurePersistence>>,
    ) -> TestRunner {
        TestRunner::new(ProptestConfig {
            rng_seed: RngSeed::Fixed(seed),
            failure_persistence: persistence,
            ..ProptestConfig::default()
        })
    }

    fn draw<S: Strategy<Value = Vec<Op>>>(strat: &S, seed: u64) -> Vec<Op> {
        strat
            .new_tree(&mut runner(seed, None))
            .expect("generate a tree")
            .current()
    }

    fn mean_allocs<S: Strategy<Value = Vec<Op>>>(
        build: impl Fn(core::ops::Range<usize>) -> S,
        len: usize,
        seeds: u64,
    ) -> f64 {
        let strat = build(len..len + 1);
        let mut total = 0usize;
        for seed in 0..seeds {
            let before = ALLOC_CALLS.load(Ordering::Relaxed);
            let tree = strat.new_tree(&mut runner(seed, None)).unwrap();
            std::hint::black_box(tree.current());
            total += ALLOC_CALLS.load(Ordering::Relaxed) - before;
        }
        total as f64 / seeds as f64
    }

    struct DrawMetrics {
        total_bytes: usize,
        peak_bytes: usize,
        new_ns: f64,
        shrink_ns: f64,
        drop_ns: f64,
        steps: usize,
    }

    fn draw_metrics<S: Strategy<Value = Vec<Op>>>(strat: &S, seed: u64) -> DrawMetrics {
        let mut runner = runner(seed, None);
        let before_total = TOTAL_BYTES.load(Ordering::Relaxed);
        let start_live = LIVE_BYTES.load(Ordering::Relaxed);
        PEAK_LIVE.store(start_live, Ordering::Relaxed);

        let t = Instant::now();
        let mut tree = strat.new_tree(&mut runner).expect("generate a tree");
        let new_ns = t.elapsed().as_nanos() as f64;

        let mut steps = 0;
        let t = Instant::now();
        while tree.simplify() {
            steps += 1;
        }
        let shrink_ns = t.elapsed().as_nanos() as f64;

        let t = Instant::now();
        drop(tree);
        let drop_ns = t.elapsed().as_nanos() as f64;

        DrawMetrics {
            total_bytes: TOTAL_BYTES.load(Ordering::Relaxed) - before_total,
            peak_bytes: PEAK_LIVE.load(Ordering::Relaxed) - start_live,
            new_ns,
            shrink_ns,
            drop_ns,
            steps,
        }
    }

    /// Full-simplify walk; returns (step count, final shrunk value).
    fn shrink_steps<S: Strategy<Value = Vec<Op>>>(strat: &S, seed: u64) -> (usize, Vec<Op>) {
        let mut tree = strat
            .new_tree(&mut runner(seed, None))
            .expect("generate a tree");
        let mut steps = 0;
        while tree.simplify() {
            steps += 1;
        }
        (steps, tree.current())
    }

    fn measure_arm<S: Strategy<Value = Vec<Op>>>(
        name: &str,
        strat: &S,
        build: impl Fn(core::ops::Range<usize>) -> S,
        seeds: u64,
    ) -> (f64, f64, f64) {
        // Marginal allocs/op: (mean allocs for len=220) - (mean for len=20)
        // divided by 200 — the same len-20-vs-len-220 technique the previous
        // calibration module used.
        let a20 = mean_allocs(&build, 20, seeds);
        let a220 = mean_allocs(&build, 220, seeds);
        let marginal = (a220 - a20) / 200.0;

        let mut total = 0u64;
        let mut peak = 0u64;
        let mut peak_max = 0usize;
        let mut new_ns = 0f64;
        let mut shrink_ns = 0f64;
        let mut drop_ns = 0f64;
        let mut steps_sum = 0u64;
        for seed in 0..seeds {
            let m = draw_metrics(strat, seed);
            total += m.total_bytes as u64;
            peak += m.peak_bytes as u64;
            peak_max = peak_max.max(m.peak_bytes);
            new_ns += m.new_ns;
            shrink_ns += m.shrink_ns;
            drop_ns += m.drop_ns;
            steps_sum += m.steps as u64;
        }
        let n = seeds as f64;
        println!(
            "  {name:14} marginal {marginal:6.3} allocs/op | draw: total {:7.0} B, peak-live {:6.0} B (max {peak_max} B) | mean ns/draw (release profile, {} draws): new_tree {:7.0}, full-shrink walk {:8.0} ({} steps avg), drop {:5.0}",
            total as f64 / n,
            peak as f64 / n,
            seeds,
            new_ns / n,
            shrink_ns / n,
            steps_sum as f64 / n,
            drop_ns / n
        );
        (marginal, total as f64 / n, peak as f64 / n)
    }

    pub fn run() {
        const SEEDS: u64 = 64;
        const SHRINK_SEEDS: u64 = 8;
        const STREAM: usize = 200;

        println!("\n=== P4-5: paired A/B, enum SizeStrategy vs faithful boxed counterpart ===");
        println!("  ({SEEDS} seeds per arm, one {STREAM}-op stream draw per seed; paired identity asserted per seed)");

        let arms: [(&str, Config, bool); 4] = [
            ("enum/default", Config::default(), false),
            ("boxed/default", Config::default(), true),
            (
                "enum/single",
                Config {
                    large_weight: 0,
                    ..Config::default()
                },
                false,
            ),
            (
                "boxed/single",
                Config {
                    large_weight: 0,
                    ..Config::default()
                },
                true,
            ),
        ];

        // Gate: per-seed REAL pairwise comparison — same seed + same Config
        // must yield byte-identical Vec<Op> streams from the enum strategy
        // and its boxed counterpart. Two real pairs per seed (default and
        // single configs); the 4 measurement arms reuse these 2 configs.
        for seed in 0..SEEDS {
            for (name, config) in [
                ("default", Config::default()),
                (
                    "single",
                    Config {
                        large_weight: 0,
                        ..Config::default()
                    },
                ),
            ] {
                let expect = draw(&op_strategy(config, STREAM..STREAM + 1), seed);
                let got = draw(&op_strategy_boxed(config, STREAM..STREAM + 1), seed);
                assert_eq!(
                    expect, got,
                    "paired identity broke ({name} config), seed {seed}"
                );
            }
        }
        println!("  paired identity (Vec<Op> equality, all {SEEDS} seeds x 2 configs): OK");

        // Full-shrink equality on a small seed subset.
        for seed in 0..SHRINK_SEEDS {
            let (enum_steps, enum_final) =
                shrink_steps(&op_strategy(Config::default(), STREAM..STREAM + 1), seed);
            let (boxed_steps, boxed_final) = shrink_steps(
                &op_strategy_boxed(Config::default(), STREAM..STREAM + 1),
                seed,
            );
            assert_eq!(enum_steps, boxed_steps, "shrink step count, seed {seed}");
            assert_eq!(enum_final, boxed_final, "shrunk final value, seed {seed}");
        }
        println!(
            "  full-shrink equality (steps + final value, {SHRINK_SEEDS} seeds, default config): OK"
        );

        let mut stats = Vec::new();
        for (name, config, boxed) in arms {
            let build = move |len: core::ops::Range<usize>| {
                if boxed {
                    op_strategy_boxed(config, len)
                } else {
                    op_strategy(config, len).boxed()
                }
            };
            let strat = build(STREAM..STREAM + 1);
            stats.push((name, measure_arm(name, &strat, build, SEEDS)));
        }

        let get = |key: &str, field: usize| {
            let (_, t) = stats.iter().find(|(n, _)| *n == key).unwrap();
            match field {
                0 => t.0,
                1 => t.1,
                _ => t.2,
            }
        };
        println!(
            "  boxing-attributable delta (boxed/default - enum/default): {:+.3} allocs/op, {:+.0} total B/draw, {:+.0} peak-live B/draw",
            get("boxed/default", 0) - get("enum/default", 0),
            get("boxed/default", 1) - get("enum/default", 1),
            get("boxed/default", 2) - get("enum/default", 2),
        );
        println!(
            "  boxing-attributable delta (boxed/single - enum/single): {:+.3} allocs/op, {:+.0} total B/draw, {:+.0} peak-live B/draw",
            get("boxed/single", 0) - get("enum/single", 0),
            get("boxed/single", 1) - get("enum/single", 1),
            get("boxed/single", 2) - get("enum/single", 2),
        );

        // Legacy uncontrolled calibration sensitivity row: NOT a decision
        // number. This replicates the env-unset default
        // (failure_persistence = Some(FileFailurePersistence::SourceParallel)).
        // The extra allocs are NOT file I/O: they come from the per-
        // `LazyValueTree`-init `partial_clone` of `TestRunner` cloning
        // `Config`, which clones the `Box<dyn FailurePersistence>` — one
        // extra heap alloc per non-selected union arm. File I/O only happens
        // when a failure is actually saved (never in this probe); the setting
        // exists to persist failures to the filesystem.
        let strat = op_strategy(Config::default(), STREAM..STREAM + 1);
        let mut total = 0usize;
        for seed in 0..SEEDS {
            let before = ALLOC_CALLS.load(Ordering::Relaxed);
            let tree = strat
                .new_tree(&mut runner(
                    seed,
                    Some(Box::new(FileFailurePersistence::SourceParallel(
                        "proptest-regressions",
                    ))),
                ))
                .unwrap();
            std::hint::black_box(tree.current());
            total += ALLOC_CALLS.load(Ordering::Relaxed) - before;
        }
        println!(
            "  [legacy sensitivity, NOT a decision number] failure_persistence = Some(SourceParallel) (env-unset default): {:.1} allocs/draw over {SEEDS} seeds — extra allocs from the TestRunner partial_clone cloning Box<dyn FailurePersistence> (setting exists to persist failures to the filesystem; no I/O occurs here)",
            total as f64 / SEEDS as f64
        );
    }
}
