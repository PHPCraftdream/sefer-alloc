//! R6-REGRESSION: fast wall-clock regression guard for the
//! `push_with_overflow_retry` sustained-double-saturation pathology
//! introduced by `345fa9b` (task R6-OPT-P0-4, "overflow-first remote-free
//! retry inversion") and fixed by this task's commit.
//!
//! ## What broke
//!
//! `345fa9b` scaled the bounded spin-retry loop in
//! `HeapCore::push_with_overflow_retry` (`src/registry/heap_core_xthread.rs`)
//! from `RING_PUSH_RETRY_SPINS` (8,192 native) to `RETRY_LOOP_ITERATIONS`
//! (2,097,152 = 8,192 × 256), as a flat, uninterrupted
//! `core::hint::spin_loop()`-paced busy-spin — a pure CPU-level hint, never
//! an OS-level yield or block. Under the "double-saturation" condition (both
//! the per-segment `RemoteFreeRing` — `RING_CAP = 256` — AND the heap-level
//! `HeapOverflow` second-chance ring — `HEAP_OVERFLOW_CAP = 2048` — are full)
//! WITH a "live but never draining" owner (`owner_slot_is_live` reports
//! `true` — the heap is claimed — but the owning thread never calls
//! `alloc()`/`dealloc()` to run the opportunistic ring/overflow drain), every
//! push past the combined capacity (2,304 blocks) had to burn through most or
//! all of its 2,097,152-iteration budget before conceding to the documented-
//! sound bounded leak. Multiplied across many contending producer threads,
//! this made `benches/heap_fanin_persistent.rs --reduced`'s `T=32,
//! burst=100_000, owner=paused` matrix cell burn thousands of CPU-seconds
//! over minutes of wall-clock with zero throughput — effectively
//! indistinguishable from a hang for any real caller, even though each
//! individual call is mathematically bounded (not a true infinite
//! loop/deadlock).
//!
//! ## What this test proves, fast
//!
//! This harness reproduces the qualitative pathology (many producers,
//! sustained double-saturation, a claimed-but-never-draining "paused" owner)
//! at a SMALL enough scale to run in under a couple of seconds even under the
//! FIXED code, while still measuring many SECONDS under the pre-fix flat
//! 2,097,152-iteration spin (confirmed by hand: N=2,500/T=32 measured
//! 15.4s-20.8s pre-fix vs. a stable 0.7-1.9s post-fix across many runs on
//! this project's dev hardware — see this task's final report for the full
//! RED/GREEN numbers).
//!
//! **Best-of-`ATTEMPTS` retry, not a single-shot timing.** This project's dev
//! box is a genuinely noisy SHARED machine (multiple concurrent agent
//! processes observed consuming 100% host CPU during this test's own
//! development, including minutes-long `find`/`node`/`rustc` bursts entirely
//! unrelated to this workload) — a single-shot wall-clock assertion measured
//! anywhere from 0.7s to 11.3s for the IDENTICAL fixed-code workload
//! depending on what else the host was doing at that instant. A fixed
//! bound tight enough to catch the ~15-20x pre-fix regression was
//! simultaneously too tight to survive this host's own noise. The fix:
//! run the timed burst up to [`ATTEMPTS`] times and pass as soon as ONE
//! attempt lands under [`MAX_ELAPSED`] — this is sound specifically because
//! the pre-fix pathology is not a one-off slow outlier, it is a SYSTEMIC
//! per-push cost (every push past the combined ring+overflow capacity pays
//! up to 2,097,152 spin iterations); the pre-fix code was independently
//! measured to exceed `MAX_ELAPSED` on every one of several separate runs,
//! never landing fast by chance, so requiring only ONE fast attempt out of
//! several does not let the genuine regression slip through — it only
//! absorbs transient host-noise spikes on the fixed code's side.
//!
//! Mirrors `tests/remote_fanin.rs::remote_fanin_owner_starved_residual_is_bounded`'s
//! "claim, pre-allocate N, spawn PRODUCERS remote-freeing threads, owner does
//! ZERO work for the whole burst" shape (the established pattern for
//! "saturated ring + non-draining owner" in this codebase's test
//! infrastructure) — this file adds ONLY the wall-clock assertion that
//! `remote_fanin.rs` does not make (that file asserts on loss/recovery
//! counts, not on latency).
//!
//! **Native-only** (`#[cfg(not(miri))]`): a genuine timing-sensitive stress
//! test (thousands of ops across many threads); miri's interpreter overhead
//! makes wall-clock assertions meaningless there, and the retry loop's own
//! `#[cfg(miri)]`-narrowed budget (`RETRY_ROUND_MAX_ROUNDS = 1`) is already
//! covered for UB (not timing) by
//! `tests/remote_fanin.rs::remote_fanin_miri_minimal_retry_ub_check`.

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread", not(miri)))]

use std::alloc::Layout;
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

/// A small-class size well under `SMALL_MAX`, so every block is routed
/// through the ring (never the Large/A1 path) — mirrors
/// `tests/remote_fanin.rs::BLOCK_SIZE`.
const BLOCK_SIZE: usize = 64;

/// Just past `RING_CAP (256) + HEAP_OVERFLOW_CAP (2048) = 2304` — large
/// enough that a meaningful number of pushes (a couple hundred) must fall
/// through BOTH tiers and into the bounded spin-retry, sustaining the
/// double-saturation condition (rather than a single transient blip), but
/// otherwise as small as this project's `RING_CAP`/`HEAP_OVERFLOW_CAP`
/// constants (not runtime-configurable) allow.
const N: usize = 2_500;

/// 32 producers: the same upper-bound producer count
/// `tests/remote_fanin.rs::remote_fanin_high_contention_budget_is_sufficient`
/// (the task #99/#136 calibration judge) uses, and the count this task's
/// investigation measured as the largest gap between pre-fix (15.4s) and
/// post-fix (0.7s) wall-clock at this burst size.
const PRODUCERS: usize = 32;

/// Per-attempt upper bound for the FIXED code's wall-clock cost of this
/// workload (measured stably at 0.7-1.9s across most runs, with an outlier
/// up to ~11s observed during a period of extreme concurrent host load — see
/// the module doc's "best-of-`ATTEMPTS` retry" section for why a single-shot
/// assertion at any fixed bound was unworkable on this project's noisy dev
/// box). 10s leaves ample headroom over the common-case fixed-code cost while
/// staying far below the pre-fix broken code's measured ~15-21s for the
/// identical workload — every independently observed pre-fix run exceeded
/// this bound.
const MAX_ELAPSED: Duration = Duration::from_secs(10);

/// Number of best-of-N attempts — see the module doc's "best-of-`ATTEMPTS`
/// retry" section. 3 is enough to absorb a single transient host-noise spike
/// (the worst observed during this task's development was one outlier in a
/// run of several) without meaningfully weakening the assertion: the pre-fix
/// pathology is systemic (every push past ring+overflow capacity pays the
/// same inflated cost), so it fails ALL attempts, not just some.
const ATTEMPTS: u32 = 3;

/// Runs the timed "paused owner, sustained double-saturation" burst once and
/// returns its wall-clock duration. Claims a FRESH heap and producer slots
/// each call so repeated attempts (see [`ATTEMPTS`]) do not reuse
/// already-recycled state from a prior attempt.
fn run_burst_once() -> Duration {
    let layout = Layout::from_size_align(BLOCK_SIZE, 8).unwrap();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // Owner pre-allocates all N blocks up front, then does ZERO further
    // work (no alloc, no dealloc, no drain) until every producer below has
    // finished freeing them — the "owner=paused" shape
    // `benches/heap_fanin_persistent.rs` models, and the same "owner does
    // ABSOLUTELY NOTHING during the burst" shape
    // `tests/remote_fanin.rs::remote_fanin_owner_starved_residual_is_bounded`
    // already establishes as this codebase's pattern for a saturated-ring +
    // non-draining-owner harness.
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for _ in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "owner pre-alloc returned null");
        ptrs.push(p);
    }
    let addrs: Vec<usize> = ptrs.iter().map(|&p| p as usize).collect();

    let chunk = N.div_ceil(PRODUCERS);
    let mut handles = Vec::with_capacity(PRODUCERS);

    let t0 = Instant::now();
    for slice in addrs.chunks(chunk) {
        let slice = slice.to_vec();
        handles.push(thread::spawn(move || {
            let _ = bootstrap::ensure();
            let remote_heap = HeapRegistry::claim();
            assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
            for addr in slice {
                unsafe { (*remote_heap).dealloc(addr as *mut u8, layout) };
            }
            unsafe { HeapRegistry::recycle(remote_heap) };
        }));
    }
    // The owner does NOTHING here — no alloc, no drain — for the entire
    // producer burst, exactly like `remote_fanin_owner_starved_residual_is_bounded`.
    // This is the deliberately pathological "owner=paused" shape.
    for h in handles {
        h.join().expect("producer thread must not panic");
    }
    let elapsed = t0.elapsed();

    unsafe { HeapRegistry::recycle(heap) };
    elapsed
}

#[test]
fn paused_owner_sustained_saturation_completes_fast() {
    let _ = bootstrap::ensure();

    let mut samples = Vec::with_capacity(ATTEMPTS as usize);
    for attempt in 1..=ATTEMPTS {
        let elapsed = run_burst_once();
        eprintln!(
            "paused_owner_sustained_saturation_completes_fast: attempt {attempt}/{ATTEMPTS} \
             N={N} PRODUCERS={PRODUCERS} elapsed={elapsed:?} (bound={MAX_ELAPSED:?})"
        );
        if elapsed < MAX_ELAPSED {
            return; // Best-of-N: one fast attempt is sufficient — see module doc.
        }
        samples.push(elapsed);
    }

    panic!(
        "paused-owner sustained double-saturation burst exceeded {MAX_ELAPSED:?} on ALL \
         {ATTEMPTS} attempts (samples: {samples:?}) — this is the R6-REGRESSION pathology: \
         a bounded spin-retry loop that is mathematically bounded in ITERATION count but \
         pathologically slow in WALL-CLOCK time under sustained contention with a \
         live-but-never-draining owner. See `HeapCore::push_with_overflow_retry`'s doc \
         comment (`src/registry/heap_core_xthread.rs`) for the fix (capped probe rounds \
         with a real OS-level sleep between rounds, not one flat multi-million-iteration \
         busy-spin)."
    );
}
