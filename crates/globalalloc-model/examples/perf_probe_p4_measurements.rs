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
//!
//! # Three separately labeled scenarios (P3-2)
//!
//! The previous single `draw:` label was wrong: its one measurement window
//! opened before `new_tree` and then ALWAYS ran a full `while tree.simplify()`
//! walk before dropping, so it measured construction PLUS a mandatory
//! simplify-only walk, not a plain successful draw. The published
//! 848,124/915,594 B figures came from that combined protocol. The
//! distinction is not cosmetic: proptest's `TupleUnion` initializes
//! previously-unselected variants while simplifying, so on the boxed arm the
//! full-simplify regime pays extra size-`Box` allocations that a successful
//! case (which never triggers shrinking) never creates. Arm ordering by total
//! or peak memory is therefore NOT necessarily the same in the two regimes.
//! This probe now reports three separately labeled scenarios per arm:
//!
//! - `S1 successful-draw (new_tree -> current -> drop; NO shrinking)` — the
//!   successful property case.
//! - `S2 simplify-only walk (while simplify(); no per-step current(), no
//!   complicate)` — the old microbenchmark, kept because it isolates
//!   TupleUnion's lazy-branch materialization mechanics; at 64 seeds it is
//!   the apples-to-apples corrected number for the old (mislabeled)
//!   protocol.
//! - `S3 shrink-protocol (simplify; current() per step; accept/reject with
//!   complicate backoff)` — a faithful mirror of `TestRunner::shrink`'s
//!   accept/reject loop, which the old loop omitted entirely (no per-step
//!   `current()` materialization, no `complicate`). The per-step
//!   materialization is a substantial part of the Vec<Op> alloc+copy cost of
//!   real shrinking, so S3 runs on fewer seeds (8 vs 64).
//!
//! # Counter accounting convention (P3-1)
//!
//! `CountingAlloc` separates ATTEMPTED-call counters (`ALLOC_CALLS` for
//! `alloc()`, `REALLOC_CALLS` for `realloc()`, incremented regardless of
//! outcome) from byte accounting for SUCCESSFUL ranges (`LIVE_BYTES`,
//! `TOTAL_BYTES`, `PEAK_LIVE`), which update ONLY after a non-null backend
//! result. On a successful realloc, the OLD size is REPLACED by the NEW size
//! in the live state (one CAS loop); on a null realloc the live state is left
//! completely untouched (per the `GlobalAlloc` contract the old allocation
//! stays valid). `TOTAL_BYTES` counts the cumulative FULL requested size of
//! every SUCCESSFUL call — a successful realloc counts the full new request,
//! not the growth delta. Every measurement window reports its
//! `REALLOC_CALLS` delta so a reader can tell whether the window could have
//! been affected by the pre-fix accounting bug.
//!
//! # Honest build/target markers (P4-3)
//!
//! The output never claims the binary is a release build from inside the
//! binary: `cfg!(debug_assertions) == false` does not prove an optimized
//! profile, and the example runs fine without `--release`. The header prints
//! the `debug_assertions` state as the build claim plus the verbatim
//! canonical command (whose arguments are the invoker's business) and the
//! resolved dependency versions read from the nearest `Cargo.lock`.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use globalalloc_model::{drive, Config, Op};

// --- P4-5: a counting global allocator, active for the whole process ---
//
// Contract (review P3-1 fix):
// - ATTEMPTED-call counters: ALLOC_CALLS (alloc()) and REALLOC_CALLS
//   (realloc()) are incremented on EVERY call, before and regardless of the
//   backend result. The published "allocs/op" figures depend on ALLOC_CALLS
//   remaining attempted-call semantics.
// - Byte accounting (TOTAL_BYTES, LIVE_BYTES, PEAK_LIVE) is updated ONLY on
//   a SUCCESSFUL backend call. A null alloc or null realloc leaves
//   live/total/peak completely unchanged; per the GlobalAlloc contract a
//   failed realloc leaves the old allocation intact.
// - A SUCCESSFUL realloc REPLACES the old size with the new size in the live
//   state (CAS loop), and adds the FULL new_size to TOTAL_BYTES. Convention:
//   TOTAL_BYTES counts the cumulative FULL requested size of every
//   successful call; a successful realloc counts the full new request, not
//   the growth delta.
struct CountingAlloc;
static ALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static REALLOC_CALLS: AtomicUsize = AtomicUsize::new(0);
static TOTAL_BYTES: AtomicUsize = AtomicUsize::new(0);
static LIVE_BYTES: AtomicUsize = AtomicUsize::new(0);
static PEAK_LIVE: AtomicUsize = AtomicUsize::new(0);

unsafe impl GlobalAlloc for CountingAlloc {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        // Forward FIRST; count bytes only on success.
        // SAFETY: forwarding to the System allocator, as before.
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            TOTAL_BYTES.fetch_add(layout.size(), Ordering::Relaxed);
            let live = LIVE_BYTES.fetch_add(layout.size(), Ordering::Relaxed) + layout.size();
            PEAK_LIVE.fetch_max(live, Ordering::Relaxed);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // dealloc is infallible: accounting before the forward is fine.
        LIVE_BYTES.fetch_sub(layout.size(), Ordering::Relaxed);
        // SAFETY: forwarding to the System allocator, as before.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Attempted-call counter, like ALLOC_CALLS above.
        REALLOC_CALLS.fetch_add(1, Ordering::Relaxed);
        // Forward FIRST; per the GlobalAlloc contract a null result leaves
        // the old allocation intact, so live/total/peak must not move.
        // SAFETY: forwarding to the System allocator, as before.
        let out = unsafe { System.realloc(ptr, layout, new_size) };
        if !out.is_null() {
            // Full-new-request convention, documented above and in the
            // printed legend.
            TOTAL_BYTES.fetch_add(new_size, Ordering::Relaxed);
            // Replace old size with new size in the live state: one CAS loop
            // against concurrent updates (other threads may alloc/free).
            let mut prev = LIVE_BYTES.load(Ordering::Relaxed);
            loop {
                let next = prev - layout.size() + new_size;
                match LIVE_BYTES.compare_exchange_weak(
                    prev,
                    next,
                    Ordering::Relaxed,
                    Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        PEAK_LIVE.fetch_max(next, Ordering::Relaxed);
                        break;
                    }
                    Err(observed) => prev = observed,
                }
            }
        }
        out
    }
}

#[global_allocator]
static GLOBAL: CountingAlloc = CountingAlloc;

// --- P3-1 self-check: exercised unconditionally at process start ---
//
// Direct calls via the GlobalAlloc trait on CountingAlloc itself (NOT
// std::alloc::alloc), so the check is independent of the global-allocator
// wiring. Each step hard-asserts the EXACT expected stored LIVE_BYTES; the
// process is single-threaded at this point, so no concurrent counter updates
// can race the asserts.
fn count_alloc_self_check() {
    fn live() -> usize {
        LIVE_BYTES.load(Ordering::Relaxed)
    }
    let baseline = live();

    // Grow: alloc 64 B, then realloc to 128 B — live must REPLACE, not add.
    let layout64 = Layout::from_size_align(64, 8).unwrap();
    let p64 = unsafe { CountingAlloc.alloc(layout64) };
    assert!(!p64.is_null(), "self-check alloc failed");
    assert_eq!(live(), baseline + 64, "alloc did not add exactly 64");
    let p128 = unsafe { CountingAlloc.realloc(p64, layout64, 128) };
    assert!(!p128.is_null(), "self-check grow realloc failed");
    assert_eq!(
        live(),
        baseline + 128,
        "grow realloc did not replace 64 with 128"
    );

    // Shrink: 128 -> 32 B.
    let layout128 = Layout::from_size_align(128, 8).unwrap();
    let layout32 = Layout::from_size_align(32, 8).unwrap();
    let p32 = unsafe { CountingAlloc.realloc(p128, layout128, 32) };
    assert!(!p32.is_null(), "self-check shrink realloc failed");
    assert_eq!(
        live(),
        baseline + 32,
        "shrink realloc did not replace 128 with 32"
    );

    // Failing alloc: a valid Layout System cannot satisfy. Byte accounting
    // must not move.
    let huge = Layout::from_size_align(isize::MAX as usize, 1).unwrap();
    let null = unsafe { CountingAlloc.alloc(huge) };
    assert!(null.is_null(), "self-check expected the huge alloc to fail");
    assert_eq!(live(), baseline + 32, "failed alloc changed live bytes");

    // Failing realloc: the old 32 B block must remain intact and live.
    let null = unsafe { CountingAlloc.realloc(p32, layout32, isize::MAX as usize) };
    assert!(
        null.is_null(),
        "self-check expected the huge realloc to fail"
    );
    assert_eq!(live(), baseline + 32, "failed realloc changed live bytes");
    // Prove the old block is really still valid: write to it.
    unsafe { core::ptr::write_bytes(p32, 0, 32) };

    // Free: exactly back to the starting baseline.
    unsafe { CountingAlloc.dealloc(p32, layout32) };
    assert_eq!(live(), baseline, "dealloc did not return live to baseline");

    // The grow step must have raised the process peak at least once.
    assert!(
        PEAK_LIVE.load(Ordering::Relaxed) >= baseline + 128,
        "PEAK_LIVE never observed the grown live value"
    );
    println!("counter self-check: grow/shrink/null-alloc/null-realloc/free OK — live returned to baseline (P3-1 fix verified)");
}

// --- P4-3: resolved dependency versions from the nearest Cargo.lock ---
//
// The example's cwd depends on the invoker (crate dir, workspace root, ...),
// so candidate lockfile paths are tried outward. Minimal text scan of
// `[[package]]` / `name = "X"` / `version = "Y"` records; graceful fallback
// when no lockfile is reachable.
fn resolved_front_ends_line() -> String {
    const WANTED: [&str; 2] = ["proptest", "arbitrary"];
    let mut found: [(Option<String>, Option<String>); 2] = [(None, None), (None, None)];
    let mut source: Option<&'static str> = None;
    'candidates: for (depth, path) in [
        "Cargo.lock",
        "../Cargo.lock",
        "../../Cargo.lock",
        "../../../Cargo.lock",
    ]
    .iter()
    .enumerate()
    {
        let Ok(text) = std::fs::read_to_string(path) else {
            continue;
        };
        let mut in_package = false;
        let mut name: Option<String> = None;
        let mut version: Option<String> = None;
        for line in text.lines() {
            let line = line.trim();
            if line == "[[package]]" {
                if let (Some(n), Some(v)) = (name.as_deref(), version.as_deref()) {
                    if let Some(slot) = WANTED.iter().position(|w| *w == n) {
                        found[slot] = (Some(n.to_string()), Some(v.to_string()));
                    }
                }
                in_package = true;
                name = None;
                version = None;
                continue;
            }
            if !in_package {
                continue;
            }
            if let Some(rest) = line.strip_prefix("name = \"") {
                name = rest.strip_suffix('"').map(str::to_string);
            } else if let Some(rest) = line.strip_prefix("version = \"") {
                version = rest.strip_suffix('"').map(str::to_string);
            }
        }
        if let (Some(n), Some(v)) = (name.as_deref(), version.as_deref()) {
            if let Some(slot) = WANTED.iter().position(|w| *w == n) {
                found[slot] = (Some(n.to_string()), Some(v.to_string()));
            }
        }
        if found.iter().all(|(_, v)| v.is_some()) {
            source = Some(match depth {
                0 => "Cargo.lock (cwd)",
                1 => "../Cargo.lock",
                2 => "../../Cargo.lock",
                _ => "../../../Cargo.lock",
            });
            break 'candidates;
        }
    }
    if let Some(source) = source {
        format!(
            "resolved front-ends: proptest {}, arbitrary {}, globalalloc-model {} ({})",
            found[0].1.as_deref().unwrap_or("?"),
            found[1].1.as_deref().unwrap_or("?"),
            env!("CARGO_PKG_VERSION"),
            source
        )
    } else {
        "resolved front-ends: proptest ?, arbitrary ? (Cargo.lock not found from cwd — record versions from the invoking environment)".to_string()
    }
}

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
    count_alloc_self_check();

    // Honest build/target markers (P4-3). debug_assertions is the ONLY build
    // claim made here; the canonical command is printed verbatim as a
    // command string, not as a claim about this binary's profile.
    let debug_assertions = cfg!(debug_assertions);
    let build_note = if debug_assertions {
        "asserting build — timings not comparable to an optimized run"
    } else {
        "(assertions compiled out)"
    };
    println!(
        "probe identity: globalalloc-model v{}, {}-bit ptr (size_of::<usize>()), arch {}, os {}, debug_assertions={} {}",
        env!("CARGO_PKG_VERSION"),
        core::mem::size_of::<usize>() * 8,
        std::env::consts::ARCH,
        std::env::consts::OS,
        debug_assertions,
        build_note,
    );
    println!(
        "canonical command: cargo run --release -p globalalloc-model --example perf_probe_p4_measurements --features proptest,internals"
    );
    println!("{}", resolved_front_ends_line());

    println!("\n=== P4-3: pattern_byte marginal cost (isolated, not through drive) ===");
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
            let sizes = globalalloc_model::size_strategy_repr_sizes();
            println!(
                "\n=== P4-5: static sizes (size_of, compile-time) ===\n  SizeStrategy: {} B; SizeValueTree (inline, replaces a 2-word Box<dyn ValueTree> = {} B slot): {} B; Single tree: {} B; WeightedSizeTree: {} B; per-op element tree in the stream VecValueTree: {} B\n  (all size_of values are specific to this target/toolchain/feature set, NOT portable contracts of the types)",
                sizes.size_strategy,
                2 * core::mem::size_of::<usize>(),
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
    use super::{ALLOC_CALLS, LIVE_BYTES, PEAK_LIVE, REALLOC_CALLS, TOTAL_BYTES};
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

    // --- P3-2: three separately labeled scenarios replace the old single,
    // mislabeled "draw" (which was construction + a mandatory full
    // simplify-only walk). See the file-header doc comment for why the
    // distinction matters (TupleUnion initializes previously-unselected
    // variants while simplifying, so the boxed arm pays extra size-Box
    // allocations in the simplify regimes that a successful case never
    // creates).
    #[derive(Clone, Copy)]
    enum Scenario {
        SuccessfulDraw,
        SimplifyOnly,
        ShrinkProtocol,
    }

    impl Scenario {
        fn label(self) -> &'static str {
            match self {
                Scenario::SuccessfulDraw => {
                    "S1 successful-draw (new_tree -> current -> drop; NO shrinking)"
                }
                Scenario::SimplifyOnly => {
                    "S2 simplify-only walk (while simplify(); no per-step current(), no complicate)"
                }
                Scenario::ShrinkProtocol => {
                    "S3 shrink-protocol (simplify; current() per step; accept/reject with complicate backoff)"
                }
            }
        }
    }

    struct ScenarioMetrics {
        total_b: usize,
        total_b_post_construct: usize,
        peak_post_construct: usize,
        peak_traj: usize,
        new_ns: f64,
        walk_ns: f64,
        drop_ns: f64,
        steps: usize,
        accepts: usize,
        complicates: usize,
        reallocs: usize,
    }

    // proptest's default max_shrink_iters.
    const MAX_SHRINK_STEPS: usize = 16_384;
    // Guard against accept/complicate ping-pong; stricter than the real
    // runner (which has no such consecutive-reject bound).
    const MAX_CONSECUTIVE_REJECTS: usize = 16;

    /// The S3 "property under test": a stream FAILS the property (candidate
    /// accepted) while any op still carries a parameter above its minimum
    /// (size/new_size > 1, align > 1); only the fully-minimal stream passes.
    /// Predicate over ELEMENT VALUES because the strategies under test are
    /// built with `collection::vec(op, STREAM..STREAM + 1)`: proptest's
    /// VecValueTree pins the minimum length at the range start, so candidate
    /// length is constant (200) and only element values ever move — the
    /// earlier length-based predicates (non-empty, `> KEEP_OPS`) could never
    /// cross their boundary and left `complicates` at 0, a mechanism-dead
    /// scenario. As the element walk exhausts toward full minimality, the
    /// predicate flips and the `complicate` backoff genuinely fires; the
    /// printed accepts/complicates counts are the mechanism-activation
    /// evidence. `Op::Dealloc`'s index is ignored (shrinking it to 0 does
    /// not make a stream "pass").
    fn property_fails(ops: &[Op]) -> bool {
        ops.iter().any(|op| match op {
            Op::Alloc { size, align } | Op::AllocZeroed { size, align } => *size > 1 || *align > 1,
            Op::Realloc { new_size, .. } => *new_size > 1,
            Op::Dealloc(_) => false,
        })
    }

    fn scenario_metrics<S: Strategy<Value = Vec<Op>>>(
        strat: &S,
        seed: u64,
        scenario: Scenario,
    ) -> ScenarioMetrics {
        let mut runner = runner(seed, None);
        let before_total = TOTAL_BYTES.load(Ordering::Relaxed);
        let start_live = LIVE_BYTES.load(Ordering::Relaxed);
        // Re-base the window-local peak so constant pre-window offsets
        // cancel.
        PEAK_LIVE.store(start_live, Ordering::Relaxed);
        let before_reallocs = REALLOC_CALLS.load(Ordering::Relaxed);

        let t = Instant::now();
        let mut tree = strat.new_tree(&mut runner).expect("generate a tree");
        let new_ns = t.elapsed().as_nanos() as f64;

        // Post-construction boundary. S1's "construction" is new_tree +
        // current(): the successful case materializes its value.
        if matches!(scenario, Scenario::SuccessfulDraw) {
            std::hint::black_box(tree.current());
        }
        let total_b_post_construct = TOTAL_BYTES.load(Ordering::Relaxed) - before_total;
        let peak_post_construct = PEAK_LIVE.load(Ordering::Relaxed) - start_live;

        let mut steps = 0usize;
        let mut accepts = 0usize;
        let mut complicates = 0usize;
        let walk_t = Instant::now();
        match scenario {
            Scenario::SuccessfulDraw => {
                // Body intentionally empty: no shrinking.
            }
            Scenario::SimplifyOnly => {
                // Isolates TupleUnion's lazy-branch materialization
                // mechanics; the old (mislabeled "draw") microbenchmark.
                while tree.simplify() {
                    steps += 1;
                }
            }
            Scenario::ShrinkProtocol => {
                // Faithful mirror of TestRunner::shrink's accept/reject
                // protocol — the piece the old loop never had: per-step
                // current() materialization and complicate backoff.
                // Predicate (see `property_fails`): a stream fails the
                // property while any op still has a non-minimal parameter;
                // only the fully-minimal stream passes. The element-value
                // shrink walk crosses this boundary in its tail, so the
                // complicate backoff genuinely fires (length-based
                // predicates never could — candidate length is pinned at
                // STREAM by `vec(op, STREAM..STREAM + 1)`).
                // accepts/complicates are the per-window mechanism-
                // activation evidence (this repo's rule: a scenario must
                // prove it exercised its claimed mechanism).
                let mut result = tree.current(); // initial materialization
                accepts = 1; // the initial case "fails" the property
                let mut consecutive_rejects = 0usize;
                while steps < MAX_SHRINK_STEPS {
                    if !tree.simplify() {
                        break;
                    }
                    steps += 1;
                    // Per-step materialization: the Vec<Op> alloc+copy cost
                    // the old loop skipped.
                    let candidate = tree.current();
                    if property_fails(&candidate) {
                        // property FAILS (a non-minimal parameter remains)
                        // -> candidate accepted, keep shrinking from here
                        result = candidate;
                        accepts += 1;
                        consecutive_rejects = 0;
                    } else {
                        // property PASSES (fully minimal stream) ->
                        // reject; back off via complicate. This fires in
                        // the tail of the walk as full minimality is
                        // approached.
                        consecutive_rejects += 1;
                        if consecutive_rejects > MAX_CONSECUTIVE_REJECTS {
                            break;
                        }
                        if tree.complicate() {
                            complicates += 1;
                        }
                        // If complicate() returns false the tree stays at
                        // the simplified position — same as the real runner.
                    }
                }
                std::hint::black_box(&result);
            }
        }
        let walk_ns = walk_t.elapsed().as_nanos() as f64;

        let t = Instant::now();
        drop(tree);
        let drop_ns = t.elapsed().as_nanos() as f64;

        ScenarioMetrics {
            total_b: TOTAL_BYTES.load(Ordering::Relaxed) - before_total,
            total_b_post_construct,
            peak_post_construct,
            peak_traj: PEAK_LIVE.load(Ordering::Relaxed) - start_live,
            new_ns,
            walk_ns,
            drop_ns,
            steps,
            accepts,
            complicates,
            reallocs: REALLOC_CALLS.load(Ordering::Relaxed) - before_reallocs,
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

    /// Per-scenario (S1, S2, S3) (mean total B, mean trajectory peak B).
    type ScenarioMeans = [(f64, f64); 3];

    /// Per-scenario reporter. Returns, per scenario (S1, S2, S3 order),
    /// the (mean total B, mean trajectory peak B) means used for the
    /// boxing-attributable delta lines.
    fn measure_arm<S: Strategy<Value = Vec<Op>>>(
        name: &str,
        strat: &S,
        seeds_s12: u64,
        seeds_s3: u64,
    ) -> ScenarioMeans {
        let scenarios = [
            (Scenario::SuccessfulDraw, seeds_s12),
            (Scenario::SimplifyOnly, seeds_s12),
            (Scenario::ShrinkProtocol, seeds_s3),
        ];
        let mut means = [(0f64, 0f64); 3];
        for (index, (scenario, seeds)) in scenarios.into_iter().enumerate() {
            let mut total = 0u64;
            let mut total_post = 0u64;
            let mut peak_post = 0u64;
            let mut peak_traj = 0u64;
            let mut peak_traj_max = 0usize;
            let mut new_ns = 0f64;
            let mut walk_ns = 0f64;
            let mut drop_ns = 0f64;
            let mut steps_sum = 0u64;
            let mut accepts_sum = 0u64;
            let mut complicates_sum = 0u64;
            let mut reallocs_sum = 0u64;
            for seed in 0..seeds {
                let m = scenario_metrics(strat, seed, scenario);
                total += m.total_b as u64;
                total_post += m.total_b_post_construct as u64;
                peak_post += m.peak_post_construct as u64;
                peak_traj += m.peak_traj as u64;
                peak_traj_max = peak_traj_max.max(m.peak_traj);
                new_ns += m.new_ns;
                walk_ns += m.walk_ns;
                drop_ns += m.drop_ns;
                steps_sum += m.steps as u64;
                accepts_sum += m.accepts as u64;
                complicates_sum += m.complicates as u64;
                reallocs_sum += m.reallocs as u64;
            }
            let n = seeds as f64;
            let reallocs = reallocs_sum as f64 / n;
            println!(
                "  {name:14} {}: n={seeds} draws | total {:.0} B (post-construct {:.0} B) | peak post-construct {:.0} B, trajectory {:.0} B (max {peak_traj_max} B) | reallocs in window {reallocs:.1} | ns/draw: new_tree {:.0}, walk {:.0}, drop {:.0} | steps avg {:.1}{}",
                scenario.label(),
                total as f64 / n,
                total_post as f64 / n,
                peak_post as f64 / n,
                peak_traj as f64 / n,
                new_ns / n,
                walk_ns / n,
                drop_ns / n,
                steps_sum as f64 / n,
                if matches!(scenario, Scenario::ShrinkProtocol) {
                    format!(
                        ", accepts avg {:.1}, complicates avg {:.1}",
                        accepts_sum as f64 / n,
                        complicates_sum as f64 / n
                    )
                } else {
                    String::new()
                },
            );
            means[index] = (total as f64 / n, peak_traj as f64 / n);
        }
        means
    }

    pub fn run() {
        const SEEDS: u64 = 64;
        // Per-step current() materialization makes S3 the expensive
        // scenario, so it runs on fewer seeds.
        const SHRINK_SEEDS: u64 = 8;
        const STREAM: usize = 200;

        println!("\n=== P4-5: paired A/B, enum SizeStrategy vs faithful boxed counterpart ===");
        println!("  (S1/S2: {SEEDS} seeds per arm; S3: {SHRINK_SEEDS} seeds per arm — per-step current() materialization makes it the expensive scenario; one {STREAM}-op stream draw per seed; paired identity asserted per seed)");

        // Counter accounting legend (P3-1). Attempted vs successful calls;
        // realloc replace semantics; full-new-request TOTAL_BYTES
        // convention; window-local re-based peaks; P3-1 exposure via the
        // per-window REALLOC_CALLS delta.
        println!("legend: ATTEMPTED calls (ALLOC_CALLS, REALLOC_CALLS) are counted separately from byte accounting; LIVE_BYTES/TOTAL_BYTES/PEAK_LIVE update ONLY on successful backend calls (a null alloc or realloc leaves live unchanged; a successful realloc REPLACES the old size with the new one in live state). TOTAL_BYTES counts the cumulative FULL requested size of every successful call — a successful realloc counts the full new request, not the growth delta. peak = window-local max of LIVE_BYTES, re-based at window entry (start_live subtracted), so constant pre-window offsets cancel.");
        println!("P3-1 exposure: each window reports its REALLOC_CALLS delta. A window with 0 reallocs cannot have been affected by the pre-fix bug (it only fired on realloc) beyond a constant pre-window offset, which the re-basing cancels; a window with reallocs > 0 WAS affected (stored live previously grew by new_size instead of new_size - old_size per realloc, and null reallocs over-counted).");

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

        // Full-shrink equality on a small seed subset. Correctness gate,
        // not a measurement; uses the S2-style walk.
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

        // stats: (arm name, marginal allocs/op, per-scenario (mean total B,
        // mean trajectory peak B) in S1, S2, S3 order).
        let mut stats: Vec<(&str, f64, ScenarioMeans)> = Vec::new();
        for (name, config, boxed) in arms {
            let build = move |len: core::ops::Range<usize>| {
                if boxed {
                    op_strategy_boxed(config, len)
                } else {
                    op_strategy(config, len).boxed()
                }
            };
            let strat = build(STREAM..STREAM + 1);
            // Marginal allocs/op: (mean allocs for len=220) - (mean for
            // len=20) divided by 200 — unchanged attempted-ALLOC_CALLS
            // semantics.
            let a20 = mean_allocs(build, 20, SEEDS);
            let a220 = mean_allocs(build, 220, SEEDS);
            let marginal = (a220 - a20) / 200.0;
            println!("  {name:14} marginal {marginal:6.3} allocs/op (attempted ALLOC_CALLS)");
            let means = measure_arm(name, &strat, SEEDS, SHRINK_SEEDS);
            stats.push((name, marginal, means));
        }

        let scenario_names = ["S1", "S2", "S3"];
        for (boxed_arm, enum_arm) in [
            ("boxed/default", "enum/default"),
            ("boxed/single", "enum/single"),
        ] {
            let find = |key: &str| {
                let (_, marginal, means) = stats.iter().find(|(n, _, _)| *n == key).unwrap();
                (*marginal, *means)
            };
            let (b_marginal, b_means) = find(boxed_arm);
            let (e_marginal, e_means) = find(enum_arm);
            println!(
                "  boxing-attributable delta ({boxed_arm} - {enum_arm}): {:+.3} allocs/op",
                b_marginal - e_marginal
            );
            for (index, scenario_name) in scenario_names.iter().enumerate() {
                println!(
                    "    {scenario_name}: total B {:+.0}, trajectory peak B {:+.0}",
                    b_means[index].0 - e_means[index].0,
                    b_means[index].1 - e_means[index].1,
                );
            }
        }

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
