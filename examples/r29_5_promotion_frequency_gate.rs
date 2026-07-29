//! R29-5 (task #436) — medium→Large promotion frequency / copied-byte
//! distribution gate.
//!
//! MEASUREMENT-ONLY. Answers `docs/perf/R22_16_PROMOTION_REMAP_DESIGN.md`
//! §5.1's Stage-1 workload-shape question for the MediumExtent
//! (remap-instead-of-copy) design's precondition — before ANYONE designs or
//! prototypes an OS-level `mremap` for the medium→Large realloc-promotion
//! memcpy (`HeapCore::try_promote_to_large`,
//! `src/registry/heap_core_free.rs`), does the promotion path actually have
//! a material VICTIM? Per the independent review's explicit rule ("No
//! victim, no implementation"): if promotions are RARE relative to total
//! allocation activity, OR the copied-byte counts are SMALL, the asymptotic
//! win (`O(bytes)` memcpy → `O(1)` VM op) has no victim to rescue and the
//! design stays deferred — that verdict is fixed BEFORE this probe runs, not
//! rationalized after seeing the numbers.
//!
//! ## Workload shape — realistic, not synthetic
//!
//! `Vec`-growth is the most common real-world trigger of the promotion path:
//! repeated `push` past capacity issues a growing `realloc`, and the vast
//! majority of real `Vec<T>` populations in an application never grow past a
//! few hundred bytes to a few KiB. Only a small minority (large buffers,
//! big collections, string-building) ever cross the 256 KiB
//! `MEDIUM_REALLOC_PROMOTION_THRESHOLD`. This probe drives `HeapCore::alloc`
//! and `HeapCore::realloc` directly. No `std::vec::Vec` indirection is
//! needed: the promotion path is entirely a property of the
//! `(old_size, new_size)` sequence a growing realloc presents, which is
//! exactly what `Vec::push`'s amortized-doubling growth produces. The probe
//! uses a **mixed population**:
//!
//! - **Majority (small-object population):** starts at 64 B, doubles on each
//!   growth step, capped at a per-object ceiling drawn from a distribution
//!   that is overwhelmingly small (most objects cap out well under 256 KiB —
//!   the common case: a `Vec<u32>` of a few hundred elements, a small
//!   `String`, etc.).
//! - **Minority (large-object population):** same doubling growth pattern,
//!   but allowed to grow past 256 KiB and further (simulating the real but
//!   less common large buffer / big collection case).
//!
//! Every step is a real `HeapCore::realloc` call through the SAME dispatch
//! `Vec::push`'s reallocation would take (medium-classes routes the same
//! `GlobalAlloc::realloc` → `HeapCore::realloc` path).
//!
//! ## Run
//!
//! ```text
//! cargo run --release --example r29_5_promotion_frequency_gate --features "production medium-classes bench-internals"
//! ```

#![cfg(all(feature = "medium-classes", feature = "bench-internals"))]
#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::alloc::Layout;

use sefer_alloc::registry::{HeapCore, HeapRegistry};
use sefer_alloc::AllocCore;

// ---------------------------------------------------------------------------
// Workload parameters.
// ---------------------------------------------------------------------------

/// Number of independent simulated `Vec`-growth objects in the SMALL
/// (majority) population — objects that grow but overwhelmingly stay well
/// under the 256 KiB promotion threshold, matching the real-world shape
/// where most collections in an application are small.
const SMALL_POPULATION: usize = 4000;

/// Number of independent simulated `Vec`-growth objects in the LARGE
/// (minority) population — objects that grow past the promotion threshold,
/// matching the real but less common "big buffer / big collection" case.
const LARGE_POPULATION: usize = 40;

/// Realistic population ratio: 100 small objects per 1 large object (a
/// conservative, not cherry-picked, estimate of "most collections stay
/// small" — deliberately not tuned to make promotions artificially rare OR
/// artificially common).
const _RATIO_CHECK: () = assert!(SMALL_POPULATION / LARGE_POPULATION == 100);

/// Initial capacity for every simulated object (mirrors `Vec::push`'s first
/// real growth step for a small starting capacity).
const INITIAL_SIZE: usize = 64;

/// Growth factor per step (mirrors `Vec`'s amortized-doubling growth
/// strategy exactly).
const GROWTH_FACTOR: usize = 2;

/// Small-population ceiling: caps each small object's growth so the
/// population's sizes span a realistic small-`Vec` range (64 B .. ~64 KiB)
/// — well short of the 256 KiB promotion threshold for the overwhelming
/// majority, with a handful of the largest small-population objects landing
/// close to but still under the threshold (realistic tail, not a cliff).
const SMALL_CEILING: usize = 64 * 1024;

/// Large-population ceiling: caps each large object's growth well past the
/// 256 KiB promotion threshold (up to 2 MiB), so the large population
/// actually exercises multiple promotion events per object as it keeps
/// growing past the threshold (mirrors a big buffer that keeps growing).
const LARGE_CEILING: usize = 2 * 1024 * 1024;

const ALIGN: usize = 8;

// ---------------------------------------------------------------------------
// Deterministic small PRNG (xorshift64) — same family used by this round's
// other gates (R29-4) for reproducibility without an external dependency.
// ---------------------------------------------------------------------------

struct XorShift64(u64);

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self(seed)
    }

    #[inline]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
}

/// Simulate one object's `Vec`-push-shaped growth lifecycle: allocate at
/// `INITIAL_SIZE`, then repeatedly double via `HeapCore::realloc` until
/// hitting `ceiling`, then free. Returns the number of realloc (growth)
/// steps performed (for the frequency denominator).
fn run_growth_object(heap: &mut HeapCore, ceiling: usize) -> u64 {
    let layout = Layout::from_size_align(INITIAL_SIZE, ALIGN).unwrap();
    let mut ptr = heap.alloc(layout);
    assert!(!ptr.is_null(), "initial alloc failed");
    let mut cur_size = INITIAL_SIZE;
    let mut cur_layout = layout;
    let mut steps = 0u64;

    while cur_size < ceiling {
        let new_size = (cur_size * GROWTH_FACTOR).min(ceiling);
        // SAFETY: `ptr` was allocated by `heap` with `cur_layout`, both live.
        let new_ptr = unsafe { heap.realloc(ptr, cur_layout, new_size) };
        assert!(!new_ptr.is_null(), "realloc failed at size {new_size}");
        ptr = new_ptr;
        cur_layout = Layout::from_size_align(new_size, ALIGN).unwrap();
        cur_size = new_size;
        steps += 1;
    }

    // SAFETY: `ptr` is the final live allocation for this object.
    unsafe { heap.dealloc(ptr, cur_layout) };
    steps
}

// ---------------------------------------------------------------------------
// Fresh-allocation background activity — a realistic workload does not ONLY
// grow objects; it also allocates plenty of never-grown short-lived objects
// (function-local buffers, small temporaries). Included so the frequency
// denominator ("promotions / total allocation activity") reflects a
// realistic mix, not a workload hand-tuned to inflate the promotion ratio by
// omitting all the allocation activity that never touches this path.
// ---------------------------------------------------------------------------

const BACKGROUND_ALLOCS: usize = 20_000;
const BACKGROUND_SIZE: usize = 48;

fn run_background_allocs(heap: &mut HeapCore) -> u64 {
    let layout = Layout::from_size_align(BACKGROUND_SIZE, ALIGN).unwrap();
    let mut count = 0u64;
    for _ in 0..BACKGROUND_ALLOCS {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "background alloc failed");
        // SAFETY: `p` was just allocated by `heap` with `layout`.
        unsafe { heap.dealloc(p, layout) };
        count += 1;
    }
    count
}

fn main() {
    println!("=== R29-5 promotion-frequency / copied-byte-distribution gate ===");
    println!();
    println!(
        "workload: {SMALL_POPULATION} small-population objects (ceiling {SMALL_CEILING} B) + \
         {LARGE_POPULATION} large-population objects (ceiling {LARGE_CEILING} B), \
         {BACKGROUND_ALLOCS} background never-grown allocations ({BACKGROUND_SIZE} B each)"
    );
    println!();

    let heap_ptr = HeapRegistry::claim();
    assert!(!heap_ptr.is_null(), "HeapRegistry::claim returned null");
    // SAFETY: `heap_ptr` was just returned by `claim`.
    let heap: &mut HeapCore = unsafe { &mut *heap_ptr };

    // Baseline: promotion counters should read 0 before any growth activity
    // (self-verification that we are reading a fresh process's counters,
    // matching this round's established "resolved value read back, not
    // assumed" discipline).
    let promo_count_before = AllocCore::dbg_promotion_count();
    let promo_bytes_before = AllocCore::dbg_promotion_bytes_sum();

    let mut total_growth_steps: u64 = 0;
    let mut total_fresh_allocs: u64 = 0;

    // Small (majority) population — deterministic per-object ceiling drawn
    // from a realistic small-Vec distribution (mostly well under 256 KiB,
    // spanning up to SMALL_CEILING).
    let mut rng = XorShift64::new(0x0295);
    for i in 0..SMALL_POPULATION {
        // Vary the ceiling across the population (realistic: not every small
        // Vec grows to exactly the same size) — uniform in
        // [INITIAL_SIZE, SMALL_CEILING].
        let span = SMALL_CEILING - INITIAL_SIZE;
        let ceiling = INITIAL_SIZE + (rng.next_u64() as usize % span);
        let _ = i;
        total_growth_steps += run_growth_object(heap, ceiling);
        total_fresh_allocs += 1;
    }

    // Large (minority) population — grows well past the promotion threshold.
    for _ in 0..LARGE_POPULATION {
        let span = LARGE_CEILING - INITIAL_SIZE;
        let ceiling = INITIAL_SIZE + (rng.next_u64() as usize % span);
        total_growth_steps += run_growth_object(heap, ceiling);
        total_fresh_allocs += 1;
    }

    // Background never-grown allocation activity.
    total_fresh_allocs += run_background_allocs(heap);

    let promo_count = AllocCore::dbg_promotion_count() - promo_count_before;
    let promo_bytes_sum = AllocCore::dbg_promotion_bytes_sum() - promo_bytes_before;
    let promo_bytes_min = AllocCore::dbg_promotion_bytes_min();
    let promo_bytes_max = AllocCore::dbg_promotion_bytes_max();
    let promo_hist = AllocCore::dbg_promotion_bytes_hist();

    // Total allocation-activity denominator: every fresh alloc PLUS every
    // realloc (growth step) — the full population of "an allocator
    // operation happened" events, matching §5.1's
    // `promotions_triggered / medium_allocations_made`-shaped ratio (here
    // widened to ALL allocation activity, not just medium-class allocs, to
    // report the promotion path's share of overall allocator traffic too).
    let total_alloc_activity = total_fresh_allocs + total_growth_steps;

    println!("--- raw counts ---");
    println!("RESULT total_fresh_allocs={total_fresh_allocs}");
    println!("RESULT total_growth_realloc_steps={total_growth_steps}");
    println!("RESULT total_alloc_activity={total_alloc_activity}");
    println!("RESULT promotion_count={promo_count}");
    println!("RESULT promotion_bytes_sum={promo_bytes_sum}");
    println!("RESULT promotion_bytes_min={promo_bytes_min}");
    println!("RESULT promotion_bytes_max={promo_bytes_max}");
    let promo_bytes_mean = if promo_count > 0 {
        promo_bytes_sum as f64 / promo_count as f64
    } else {
        0.0
    };
    println!("RESULT promotion_bytes_mean={promo_bytes_mean:.1}");

    println!();
    println!("--- ratios (the Stage-1 gating question) ---");
    let ratio_vs_activity = if total_alloc_activity > 0 {
        promo_count as f64 / total_alloc_activity as f64
    } else {
        0.0
    };
    let ratio_vs_growth_steps = if total_growth_steps > 0 {
        promo_count as f64 / total_growth_steps as f64
    } else {
        0.0
    };
    let ratio_vs_objects = if (SMALL_POPULATION + LARGE_POPULATION) > 0 {
        promo_count as f64 / (SMALL_POPULATION + LARGE_POPULATION) as f64
    } else {
        0.0
    };
    println!(
        "RESULT ratio_promotions_over_total_alloc_activity={ratio_vs_activity:.6} \
         ({promo_count}/{total_alloc_activity})"
    );
    println!(
        "RESULT ratio_promotions_over_growth_steps={ratio_vs_growth_steps:.6} \
         ({promo_count}/{total_growth_steps})"
    );
    println!(
        "RESULT ratio_promotions_over_objects={ratio_vs_objects:.6} \
         ({promo_count}/{})",
        SMALL_POPULATION + LARGE_POPULATION
    );

    println!();
    println!("--- copied-byte histogram (the distribution, not just the aggregate) ---");
    let bucket_labels = [
        "[0,4KiB)",
        "[4KiB,16KiB)",
        "[16KiB,64KiB)",
        "[64KiB,128KiB)",
        "[128KiB,256KiB)",
        "[256KiB,512KiB)",
        "[512KiB,1MiB)",
        "[1MiB,inf)",
    ];
    for (i, (label, count)) in bucket_labels.iter().zip(promo_hist.iter()).enumerate() {
        println!("RESULT promotion_hist_bucket{i}_{label}={count}");
    }

    println!();
    println!(
        "OK promotion_count={promo_count} total_alloc_activity={total_alloc_activity} \
         ratio_vs_activity={ratio_vs_activity:.6} bytes_mean={promo_bytes_mean:.1} \
         bytes_min={promo_bytes_min} bytes_max={promo_bytes_max}"
    );

    // SAFETY: `heap_ptr` was returned by `claim` above.
    unsafe { HeapRegistry::recycle(heap_ptr) };
}
