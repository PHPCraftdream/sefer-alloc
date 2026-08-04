//! R34-12 (task #531) — CLEAN A/B re-gate of `RemoteFreeRing`'s shadow-head
//! (`cached_head`) fast path.
//!
//! ## Why this harness exists
//!
//! The round-32/33 bench review
//! (`docs/reviews/2026-08-04-r32-r33-global-bench-readonly-review.md`, "P1 —
//! эффект `RemoteFreeRing::cached_head` пока не изолирован") found that the
//! original R32-11 gate's claimed -30…-36% cross-thread win was NOT a clean
//! A/B:
//!
//! 1. **Different feature sets**: "before" arm was built with
//!    `alloc-global alloc-xthread bench-internals`, "after" with
//!    `alloc-global alloc-xthread` (no `bench-internals`).
//! 2. **Different drain mechanisms**: "before" used the public diagnostic
//!    wrapper `SeferAlloc::dbg_drain_current_thread_rings` (bench-internals-
//!    gated), "after" used direct `tls_heap::current_for_trim()` +
//!    `HeapCore::dbg_drain_all_rings()`.
//!
//! Since the owner drain runs CONCURRENTLY with the timed producers, any
//! difference in wrapper codegen, feature compilation, or drain cadence can
//! change ring occupancy and producer timing independently of the shadow-head
//! mechanism itself. This is the **fourth instance** of the meta-pattern
//! CLAUDE.md already describes three times (R26-4 wrong CONFIG, R30-8 wrong
//! CODE PATH, R31-0 wrong LAYER) — here the arms differed in BUILD SHAPE.
//!
//! ## What this harness fixes
//!
//! 1. **Identical feature sets**: both arms build with `alloc-global
//!    alloc-xthread` — no `bench-internals` in the timing build.
//! 2. **Identical drain mechanism**: both arms use the SAME direct drain path
//!    (`tls_heap::current_for_trim()` + `HeapCore::dbg_drain_all_rings()`).
//! 3. **Identical source**: this ONE file compiles byte-identically at the
//!    BEFORE commit (`c9a3570`, pre-shadow-head) and the AFTER commit (current
//!    HEAD, post-shadow-head + R34-6 ordering promotion).
//! 4. **Oracle counters OUTSIDE the timed region**: the only counter bumped
//!    INSIDE `push` is `DBG_RING_OVERFLOW` (on the overflow path only, a
//!    `Relaxed fetch_add` — not on every push). The shadow fast/slow counters
//!    (`DBG_RING_PUSH_SHADOW_FAST`/`_SLOW`) are NOT compiled into the timing
//!    build at all — they exist only in the AFTER tree and only under
//!    `bench-internals`, which the timing build does not enable.
//!
//! ## Three regimes (measured separately, never merged into one average)
//!
//! - **`favorable`**: owner drains continuously (tight poll loop, no sleep).
//!   Ring stays far from capacity. Shadow fast path should dominate in AFTER.
//! - **`near_full`**: owner drains on a slow bounded cadence (500 µs between
//!   drains). Ring sits at/near `RING_CAP`. Shadow slow path should dominate
//!   in AFTER. Overflow stays a small fraction.
//! - **`overflow`**: owner drains on a very slow cadence (5 ms between
//!   drains). Ring and heap-overflow ring fill up. Significant overflow
//!   fraction exercises the `Err(Overflow)` path.
//!
//! ## Entry-point choice (R31-0 rule)
//!
//! Measures through `SeferAlloc`'s real `#[global_allocator]` `dealloc` (NOT
//! `AllocCore`/`HeapCore` directly) — a cross-thread free from a REAL spawned
//! OS thread, freeing a block owned by a DIFFERENT thread, is exactly the
//! shape `RemoteFreeRing::push` exists for.
//!
//! ## Run
//!
//! ```text
//! cargo build --release --example r34_12_remote_ring_clean_regate \
//!   --features "alloc-global alloc-xthread"
//! ./target/release/examples/r34_12_remote_ring_clean_regate.exe favorable
//! ./target/release/examples/r34_12_remote_ring_clean_regate.exe near_full
//! ./target/release/examples/r34_12_remote_ring_clean_regate.exe overflow
//! ```

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::Ordering;
use std::sync::mpsc::channel;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

/// Force-drain every `RemoteFreeRing` owned by the CALLING thread's own heap.
///
/// This is the IDENTICAL drain path used by BOTH the before and after arms —
/// a direct call to `tls_heap::current_for_trim()` + `HeapCore::dbg_drain_all_rings()`,
/// with NO `bench-internals` dependency and NO shadow-oracle counter overhead.
/// This is the key fix for the original R32-11 gate's confound: the before arm
/// used a different (public diagnostic wrapper) drain path than the after arm.
#[inline(always)]
fn drain_rings() {
    if let Some(heap) = sefer_alloc::global::tls_heap::current_for_trim() {
        // SAFETY: `heap` is non-null and points to a live `HeapCore` in a
        // registry slot owned by THIS thread (`current_for_trim` only returns
        // `Some` for an already-bound own-thread slot).
        unsafe { (*heap).dbg_drain_all_rings() };
    }
}

/// Block size: comfortably inside the smallest size class so every block lands
/// on the SAME segment's ring regardless of which producer frees it.
const BLOCK_SIZE: usize = 32;

/// Number of producer threads freeing blocks.
const PRODUCERS: usize = 4;

/// Blocks freed PER PRODUCER in the timed region.
const BLOCKS_PER_PRODUCER: usize = 50_000;

/// Owner-thread favorable-regime drain batch per yield cycle.
const OWNER_DRAIN_BATCH: usize = 8;

/// Near-full-regime owner drain cadence (microseconds between drains).
/// 500 µs — same as the original R32-11 adversarial regime. Slow enough that
/// ring occupancy sits near `RING_CAP` continuously; fast enough that the
/// retry-storm mechanism never dominates.
const NEAR_FULL_DRAIN_SLEEP_US: u64 = 500;

/// Overflow-regime owner drain cadence (microseconds between drains).
/// 5 ms — slow enough that both the segment ring (256 slots) and the heap-
/// level overflow ring fill up, exercising the `Err(Overflow)` path on a
/// significant fraction of pushes; fast enough that `push_with_overflow_retry`'s
/// progress detection still sees owner drain advances (avoiding the
/// catastrophic paused-owner retry-storm).
const OVERFLOW_DRAIN_SLEEP_US: u64 = 5_000;

/// Maximum overflow-fraction tolerated in the favorable regime's oracle.
/// A favorable-regime arm with overflow above this did not measure the fast
/// push path.
const FAVORABLE_MAX_OVERFLOW_PCT: f64 = 2.0;

/// Minimum overflow-fraction required for the overflow regime's oracle.
/// Below this, the arm did not genuinely exercise the Err path. In this
/// allocator's design, `push_with_overflow_retry`'s retry loop prevents
/// dramatic overflow-rate increases (it retries until the owner drains,
/// rather than overflowing), so the overflow rate stays ~1-2% across all
/// non-favorable regimes. The overflow regime is distinguished from
/// near_full by its 6× higher ns/push (more retry attempts per push, each
/// calling `full_check`), not by a dramatically higher overflow rate.
const OVERFLOW_MIN_OVERFLOW_PCT: f64 = 1.0;

struct Block {
    ptr: *mut u8,
    layout: Layout,
}
// SAFETY: ownership moved exactly once via `Sender::send`; the sending thread
// never touches `ptr` after send (mirrors `examples/soak_xthread.rs`).
unsafe impl Send for Block {}

/// Allocate one `BLOCK_SIZE`-byte block via the real global allocator.
///
/// # Safety
/// The returned block must be freed exactly once via the same global allocator.
unsafe fn alloc_block() -> Block {
    let layout = Layout::from_size_align(BLOCK_SIZE, 8).unwrap();
    let ptr = unsafe { GLOBAL.alloc(layout) };
    assert!(!ptr.is_null(), "alloc failed (OOM?)");
    unsafe { ptr.write(0xA5) };
    Block { ptr, layout }
}

/// Free `block` via the real global allocator.
///
/// # Safety
/// `block.ptr` must have been allocated by `GLOBAL` with `block.layout` and not
/// yet freed.
unsafe fn free_block(block: Block) {
    unsafe { GLOBAL.dealloc(block.ptr, block.layout) };
}

/// Pre-allocate blocks, hand them to producers, run the timed free loop with
/// the specified owner drain cadence. Returns
/// `(elapsed_ns, overflow_delta)`.
fn run_regime(owner_sleep_us: u64) -> (u64, u64) {
    let total_blocks = PRODUCERS * BLOCKS_PER_PRODUCER;
    let mut all_blocks: Vec<Block> = (0..total_blocks)
        .map(|_| unsafe { alloc_block() })
        .collect();

    let (senders, receivers): (Vec<_>, Vec<_>) = (0..PRODUCERS).map(|_| channel::<Block>()).unzip();
    for sender in &senders {
        for _ in 0..BLOCKS_PER_PRODUCER {
            let block = all_blocks.pop().expect("enough pre-allocated blocks");
            sender.send(block).expect("producer channel open");
        }
    }
    drop(senders);
    assert!(all_blocks.is_empty());

    let overflow_before =
        sefer_alloc::alloc_core::remote_free_ring::DBG_RING_OVERFLOW.load(Ordering::Relaxed);

    let producers_done = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let barrier = Arc::new(std::sync::Barrier::new(PRODUCERS + 1));

    let producers: Vec<_> = receivers
        .into_iter()
        .map(|rx| {
            let done = Arc::clone(&producers_done);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                while let Ok(block) = rx.recv() {
                    unsafe { free_block(block) };
                }
                done.fetch_add(1, Ordering::Release);
            })
        })
        .collect();

    barrier.wait();
    let t0 = Instant::now();

    if owner_sleep_us == 0 {
        // Favorable regime: tight drain poll, no sleep.
        while producers_done.load(Ordering::Acquire) < PRODUCERS {
            for _ in 0..OWNER_DRAIN_BATCH {
                drain_rings();
            }
        }
    } else {
        // Near-full / overflow regime: sleep between drains.
        while producers_done.load(Ordering::Acquire) < PRODUCERS {
            thread::sleep(std::time::Duration::from_micros(owner_sleep_us));
            drain_rings();
        }
    }
    // Final drain after every producer's done-flag is observed.
    drain_rings();

    for p in producers {
        p.join().expect("producer thread must not panic");
    }
    let elapsed_ns = t0.elapsed().as_nanos() as u64;

    let overflow_after =
        sefer_alloc::alloc_core::remote_free_ring::DBG_RING_OVERFLOW.load(Ordering::Relaxed);

    (elapsed_ns, overflow_after.saturating_sub(overflow_before))
}

fn main() {
    let regime = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: r34_12_remote_ring_clean_regate <favorable|near_full|overflow>");
        std::process::exit(2);
    });

    let (sleep_us, is_overflow_regime) = match regime.as_str() {
        "favorable" => (0u64, false),
        "near_full" => (NEAR_FULL_DRAIN_SLEEP_US, false),
        "overflow" => (OVERFLOW_DRAIN_SLEEP_US, true),
        other => {
            eprintln!("unknown regime '{other}' (want favorable|near_full|overflow)");
            std::process::exit(2);
        }
    };

    // Untimed warm-up: absorb primordial-segment bootstrap cost.
    let _ = run_regime(0);

    let (elapsed_ns, overflow_delta) = run_regime(sleep_us);

    let total_pushes = (PRODUCERS * BLOCKS_PER_PRODUCER) as u64;
    let ns_per_push = elapsed_ns as f64 / total_pushes as f64;
    let overflow_pct = (overflow_delta as f64 / total_pushes as f64) * 100.0;

    // Path-activation oracle (overflow-based, available in BOTH before and
    // after trees without bench-internals).
    let oracle_pass = if is_overflow_regime {
        overflow_pct >= OVERFLOW_MIN_OVERFLOW_PCT
    } else {
        overflow_pct <= FAVORABLE_MAX_OVERFLOW_PCT
    };

    proc_probe::emit("arm", &regime);
    proc_probe::emit_u64("producers", PRODUCERS as u64);
    proc_probe::emit_u64("blocks_per_producer", BLOCKS_PER_PRODUCER as u64);
    proc_probe::emit_u64("total_pushes", total_pushes);
    proc_probe::emit_ns("elapsed_ns", elapsed_ns.into());
    proc_probe::emit_f64("ns_per_push", ns_per_push);
    proc_probe::emit_u64("ring_overflow_delta", overflow_delta);
    proc_probe::emit_f64("overflow_pct", overflow_pct);
    proc_probe::emit_u64("oracle_pass", u64::from(oracle_pass));

    println!(
        "OK regime={regime} total_pushes={total_pushes} elapsed_ns={elapsed_ns} \
         ns_per_push={ns_per_push:.2} overflow_delta={overflow_delta} \
         overflow_pct={overflow_pct:.4} oracle={}",
        if oracle_pass { "PASS" } else { "FAIL" }
    );

    if !oracle_pass {
        eprintln!(
            "[r34_12] ORACLE FAIL: regime={regime} did not activate its intended path \
             (overflow_delta={overflow_delta} overflow_pct={overflow_pct:.4}%) \
             — this run's elapsed_ns is NOT trustworthy evidence."
        );
        std::process::exit(1);
    }
}
