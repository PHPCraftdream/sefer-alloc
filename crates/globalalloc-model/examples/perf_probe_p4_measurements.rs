//! Exploratory measurement for the 3 P4 performance follow-ups deferred by
//! Sol-codex review rounds 2-4 (per-byte fill/verify cost, O(M^2) overlap
//! scan, `BoxedStrategy` heap-allocation overhead). Not a shipped
//! benchmark — a one-off probe to decide whether any of the three is worth
//! a structural fix, per this project's measurement-before-restructuring
//! discipline. Requires `--features proptest` for the P4-5 section.
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

        // Regime B: overlap-scan-heavy — many tiny blocks, high live count,
        // matching the crate's own OpStream bounded-decoder ceiling (2048).
        let small_ops: Vec<Op> = (0..2048).map(|_| Op::Alloc { size: 8, align: 8 }).collect();
        let start = Instant::now();
        drive(&System, Config::default(), &small_ops);
        let elapsed = start.elapsed();
        println!(
            "  Regime B (2048 x 8 B blocks, K=2048 peak live, O(K^2/2)~2.1M overlap compares): {elapsed:?} ({:.1} ns/op)",
            elapsed.as_secs_f64() * 1e9 / 2048.0
        );

        // Regime B at the crate's typical native default MAX_LEN (200).
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
        use globalalloc_model::op_strategy;
        use proptest::strategy::{Strategy, ValueTree};
        use proptest::test_runner::TestRunner;

        println!("\n=== P4-5: BoxedStrategy heap-allocation overhead ===");
        let config = Config::default();
        let strategy = op_strategy(config, 200..201);

        let before = ALLOC_CALLS.load(Ordering::Relaxed);
        let start = Instant::now();
        const DRAWS: usize = 64; // matches this crate's own CASES default
        for _ in 0..DRAWS {
            let mut runner = TestRunner::deterministic();
            let tree = strategy.new_tree(&mut runner).expect("generate a tree");
            std::hint::black_box(tree.current());
        }
        let elapsed = start.elapsed();
        let after = ALLOC_CALLS.load(Ordering::Relaxed);
        println!(
            "  {DRAWS} draws of a 200-op stream: {elapsed:?}, {} alloc() calls total ({:.1} allocs/draw)",
            after - before,
            (after - before) as f64 / DRAWS as f64
        );

        // Isolate shrink-time allocation cost: one tree, walked to exhaustion.
        let before = ALLOC_CALLS.load(Ordering::Relaxed);
        let mut runner = TestRunner::deterministic();
        let mut tree = strategy.new_tree(&mut runner).expect("generate a tree");
        let mut steps = 0;
        while tree.simplify() {
            steps += 1;
        }
        let after = ALLOC_CALLS.load(Ordering::Relaxed);
        println!(
            "  full shrink walk of one 200-op stream: {steps} simplify() steps, {} alloc() calls ({:.1} allocs/step)",
            after - before,
            (after - before) as f64 / steps.max(1) as f64
        );

        // Baseline: an analogous 4-arm prop_oneof! generator with a trivial
        // NON-boxed size strategy (plain RangeInclusive, no config-dependent
        // branching), same op shapes. Isolates how much of the ~470
        // allocs/draw above is size_strategy's OWN boxing vs. proptest's
        // inherent per-op machinery (collection::vec, tuple wrapping, the
        // prop_oneof! union itself) that no fix here could remove anyway.
        use proptest::prelude::*;
        let baseline = prop::collection::vec(
            prop_oneof![
                (1usize..=8, 1usize..=8).prop_map(|(size, align)| Op::Alloc { size, align }),
                (1usize..=8, 1usize..=8).prop_map(|(size, align)| Op::AllocZeroed { size, align }),
                any::<usize>().prop_map(Op::Dealloc),
                (any::<usize>(), 1usize..=8).prop_map(|(i, new_size)| Op::Realloc { i, new_size }),
            ],
            200..201,
        );
        let before = ALLOC_CALLS.load(Ordering::Relaxed);
        let start = Instant::now();
        for _ in 0..DRAWS {
            let mut runner = TestRunner::deterministic();
            let tree = baseline.new_tree(&mut runner).expect("generate a tree");
            std::hint::black_box(tree.current());
        }
        let elapsed = start.elapsed();
        let after = ALLOC_CALLS.load(Ordering::Relaxed);
        println!(
            "  BASELINE (no BoxedStrategy, same 4-arm shape) {DRAWS} draws: {elapsed:?}, {} alloc() calls ({:.1} allocs/draw)",
            after - before,
            (after - before) as f64 / DRAWS as f64
        );
    }

    #[cfg(feature = "proptest")]
    marginal_cost_calibration::run();
}

#[cfg(feature = "proptest")]
mod marginal_cost_calibration {
    use super::ALLOC_CALLS;
    use globalalloc_model::{op_strategy, Config};
    use proptest::prelude::ProptestConfig;
    use proptest::strategy::{Strategy, ValueTree};
    use proptest::test_runner::{RngSeed, TestRunner};
    use std::sync::atomic::Ordering;

    fn total_allocs_for_len(len: usize, seeds: u64) -> f64 {
        let strategy = op_strategy(Config::default(), len..len + 1);
        let mut total = 0usize;
        for seed in 0..seeds {
            let mut runner = TestRunner::new(ProptestConfig {
                rng_seed: RngSeed::Fixed(seed),
                ..ProptestConfig::default()
            });
            let before = ALLOC_CALLS.load(Ordering::Relaxed);
            let tree = strategy.new_tree(&mut runner).unwrap();
            std::hint::black_box(tree.current());
            let after = ALLOC_CALLS.load(Ordering::Relaxed);
            total += after - before;
        }
        total as f64 / seeds as f64
    }

    pub fn run() {
        println!("\n=== P4-5 calibration: marginal allocs per additional op ===");
        const SEEDS: u64 = 64;
        let small = total_allocs_for_len(20, SEEDS);
        let large = total_allocs_for_len(220, SEEDS);
        let marginal = (large - small) / 200.0;
        println!(
            "  len=20: {small:.2} allocs avg; len=220: {large:.2} allocs avg; marginal = {marginal:.4} allocs/op"
        );
    }
}
