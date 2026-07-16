//! R6-REVIEW-F2: fast wall-clock regression guard for the MULTI-SEGMENT
//! paused-owner shape — the gap the round-6 single-entry fast-concede memo
//! left open and the [`STALL_CONCESSION_WAYS`]-way concession cache in
//! `HeapCore::push_with_overflow_retry` (`src/registry/heap_core_xthread.rs`,
//! `LAST_STALL_CONCESSIONS`) closes.
//!
//! ## What the single-entry memo missed
//!
//! The R6-REGRESSION-2 fast-concede memo bounds the paused-owner burst's
//! wall-clock by memoizing, per thread, the `(segment base, ring head,
//! overflow head)` snapshot of the most recent retry CONCESSION: the next
//! push into the same unchanged stall concedes after ONE probe round instead
//! of re-paying the full `RETRY_STALLED_ROUNDS_GIVE_UP` (128-round, ~0.3–2s)
//! patience. But the original memo held exactly ONE entry. A paused owner
//! whose saturated blocks span TWO OR MORE 4 MiB segments, freed by a
//! producer whose frees interleave across them (A, B, A, B, …), overwrote
//! the memo with the OTHER segment's snapshot on every concession — the memo
//! never matched, every push past the combined ring+overflow capacity
//! re-paid the full patience, and the linear-in-push-count wall-clock wall
//! the memo exists to bound came back (in polite sleep form, not CPU burn —
//! but a wall all the same). `tests/regression_paused_owner_wallclock.rs`
//! never caught this because its whole burst fits ONE segment.
//!
//! ## What this test drives
//!
//! An owner claims a heap and pre-allocates enough 4 KiB small-class blocks
//! to span SEVERAL (asserted ≥ 4) distinct 4 MiB segments, then does ZERO
//! work (no alloc, no dealloc, no drain — the "owner=paused" shape) while
//! producer threads free every block. Each producer's free list is an exact
//! A, B, A, B interleave of TWO of the owner's segments (producers are
//! partitioned over segment PAIRS), so with a single-entry memo every
//! retry-tier push thrashes the memo, while the N-way cache (N = 4 ≥ 2
//! distinct segments per thread) reaches its steady state after ONE
//! full-patience payment per (thread, segment) and fast-concedes everything
//! after that. `N` is sized so a meaningful number of pushes per producer
//! (hundreds) overflow BOTH tiers (per-segment `RemoteFreeRing`, `RING_CAP` =
//! 256 each, AND the owner-heap-level `HeapOverflow`, `HEAP_OVERFLOW_CAP` =
//! 2048, shared across all its segments) and reach the retry loop.
//!
//! **RED counterfactual** (verified by hand for this task): with
//! `STALL_CONCESSION_WAYS` temporarily reverted to 1 (the single-entry
//! memo, arity being the ONLY variable), every one of those retry pushes
//! pays the full patience and the burst blows [`MAX_ELAPSED`] on every
//! attempt; with the 4-way cache restored it completes in a couple of
//! seconds. See the task's final report for the measured numbers.
//!
//! **Best-of-[`ATTEMPTS`] retry, not a single-shot timing** — same rationale,
//! verbatim, as `tests/regression_paused_owner_wallclock.rs`'s module doc:
//! this project's dev box is a noisy shared machine, and the pathology being
//! guarded is SYSTEMIC (every retry-tier push pays the same inflated cost),
//! so it fails ALL attempts — one fast attempt is sufficient evidence of the
//! fix, and the retries only absorb transient host-noise spikes on the fixed
//! code's side.
//!
//! **Native-only** (`#[cfg(not(miri))]`): a timing-sensitive stress test
//! (thousands of ops across dozens of threads); wall-clock assertions are
//! meaningless under miri's interpreter, and the retry loop's miri-narrowed
//! budget is already UB-covered by
//! `tests/remote_fanin.rs::remote_fanin_miri_minimal_retry_ub_check`.

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread", not(miri)))]

use std::alloc::Layout;
use std::collections::BTreeMap;
use std::thread;
use std::time::{Duration, Instant};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

/// 4 KiB: a small-class size (well under `SMALL_MAX` ≈ 253 KiB, so every
/// block is routed through the per-segment ring, never the Large/A1 path)
/// chosen LARGE — unlike `regression_paused_owner_wallclock.rs`'s 64 B —
/// specifically so a few thousand blocks span MULTIPLE 4 MiB segments
/// (~1,000 blocks per segment), which is this test's entire point.
const BLOCK_SIZE: usize = 4096;

/// Segment size/alignment (4 MiB — `SEGMENT_SHIFT` = 22 in
/// `src/alloc_core/segment_table.rs`; not exported, so restated here the
/// same way the allocator's own `segment_base_of_ptr` mask works). Used only
/// to GROUP the owner's blocks by owning segment.
const SEG_ALIGN: usize = 1 << 22;

/// Sized so the owner's blocks span ≥ [`MIN_SEGMENTS`] segments (~1,000
/// 4 KiB blocks per 4 MiB segment → ~7 segments) AND so a large excess past
/// the combined absorb capacity — `HEAP_OVERFLOW_CAP` (2048) + `RING_CAP`
/// (256) × segments ≈ 3,800 — reaches the retry tier: ~3,000 retry-tier
/// pushes, ~200 per producer, EACH of which re-pays the full ~0.3–2s stall
/// patience under the single-entry memo (RED: a minute-plus even on a calm
/// host) but only 2 per producer under the N-way cache (one per distinct
/// segment in its pair; GREEN: sub-second calm). The excess is deliberately
/// LARGE relative to the [`MAX_ELAPSED`] bound so RED-on-a-calm-host and
/// GREEN-under-a-hostile-host cannot overlap — see `MAX_ELAPSED`'s doc.
const N: usize = 7_000;

/// Fewer than the distinct segments this burst spans MUST interleave per
/// producer for the cache to win: each producer works a PAIR — see the
/// module doc. 2 < `STALL_CONCESSION_WAYS` (4), deliberately: the guard is
/// "multi-segment interleave no longer thrashes", not "exactly at capacity".
const SEGMENTS_PER_PRODUCER: usize = 2;

/// Lower bound asserted on the number of distinct segments the owner's
/// pre-allocation actually spans — the non-vacuousness check (a burst that
/// fit one segment would silently re-test what
/// `regression_paused_owner_wallclock.rs` already covers).
const MIN_SEGMENTS: usize = 4;

/// Target producer-thread count (the exact count can end up a couple higher
/// after per-pair work splitting — see `run_burst_once`). 16 — HALF the
/// existing paused-owner test's 32 — deliberately: this test's assertion is
/// a wall-clock bound whose fixed-code cost is dominated by each producer's
/// TWO serial full-patience payments, and on a fully-hogged 16-core host the
/// scheduler starvation of 32+ producer threads (plus the hog) inflated
/// exactly those payments; 16 producers halve that inflation while DOUBLING
/// the retry-tier pushes per producer, which only widens the RED/GREEN gap
/// (the thrash mechanism needs interleave, not any particular thread count —
/// contention realism at 32 producers is the #136 judge's job, not this
/// test's).
const PRODUCERS: usize = 16;

/// Per-attempt upper bound for the FIXED code's wall-clock. GREEN cost is
/// dominated by TWO full-patience payments per producer (one per segment in
/// its pair, ~0.3–2s each, paid concurrently across producers): measured
/// ~0.7s on a calm host and ~17s under a deliberate sustained 16-thread
/// CPU hog (starvation inflates each payment's 128 sleeps). The RED
/// single-entry-memo counterfactual pays the full patience on ~200 pushes
/// PER PRODUCER: measured 55-101s on a CALM host (see the task report),
/// worse under load. 30s sits between GREEN's hostile-host worst case and
/// RED's calm-host best case with a ~1.8x margin BOTH ways — wider than the
/// single-segment test's 10s bound precisely because this harness was
/// measured under a sustained full-host hog, where the fixed code's
/// two-payments-per-producer floor is genuinely ~15s.
const MAX_ELAPSED: Duration = Duration::from_secs(30);

/// Best-of-N attempts — see the module doc (same discipline as
/// `regression_paused_owner_wallclock.rs::ATTEMPTS`).
const ATTEMPTS: u32 = 3;

/// Exact A, B, A, B, … interleave of two address lists (tail of the longer
/// list appended once the shorter runs out).
fn interleave(a: &[usize], b: &[usize]) -> Vec<usize> {
    let mut out = Vec::with_capacity(a.len() + b.len());
    let mut ia = a.iter();
    let mut ib = b.iter();
    loop {
        match (ia.next(), ib.next()) {
            (None, None) => break,
            (x, y) => {
                if let Some(&v) = x {
                    out.push(v);
                }
                if let Some(&v) = y {
                    out.push(v);
                }
            }
        }
    }
    out
}

/// Runs the timed "paused owner, multi-segment sustained double-saturation"
/// burst once and returns its wall-clock duration. Claims a FRESH heap and
/// fresh producer threads each call (so repeated attempts start with clean
/// per-thread concession caches and no recycled-slot state).
fn run_burst_once() -> Duration {
    let layout = Layout::from_size_align(BLOCK_SIZE, 8).unwrap();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // Owner pre-allocates all N blocks up front, then does ZERO further work
    // until every producer has finished — the "owner=paused" shape (see
    // `regression_paused_owner_wallclock.rs` for the pattern's pedigree).
    let mut by_seg: BTreeMap<usize, Vec<usize>> = BTreeMap::new();
    for i in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "owner pre-alloc[{i}] returned null");
        by_seg
            .entry(p as usize & !(SEG_ALIGN - 1))
            .or_default()
            .push(p as usize);
    }
    assert!(
        by_seg.len() >= MIN_SEGMENTS,
        "owner pre-allocation spans only {} distinct segments (need >= {MIN_SEGMENTS}) — \
         the multi-segment thrash shape is not being exercised; raise N or BLOCK_SIZE",
        by_seg.len(),
    );

    // Partition the segments into PAIRS and each producer onto one pair,
    // its free list an exact interleave of the pair's two segments — the
    // A, B, A, B pattern that thrashes a single-entry concession memo while
    // staying within the N-way cache's arity (2 <= 4). A trailing unpaired
    // segment (odd count) is worked alone: harmless (no thrash, no payment
    // difference between RED and GREEN for those producers).
    let segs: Vec<Vec<usize>> = by_seg.into_values().collect();
    let pairs: Vec<Vec<usize>> = segs
        .chunks(SEGMENTS_PER_PRODUCER)
        .map(|ch| {
            if ch.len() == 2 {
                interleave(&ch[0], &ch[1])
            } else {
                ch[0].clone()
            }
        })
        .collect();
    let per_pair = PRODUCERS.div_ceil(pairs.len());
    let mut worklists: Vec<Vec<usize>> = Vec::new();
    for pair in &pairs {
        // Chunking an interleaved list preserves the alternation within
        // every producer's slice.
        let chunk = pair.len().div_ceil(per_pair);
        for slice in pair.chunks(chunk) {
            worklists.push(slice.to_vec());
        }
    }

    let mut handles = Vec::with_capacity(worklists.len());
    let t0 = Instant::now();
    for slice in worklists {
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
    // producer burst (the deliberately pathological "owner=paused" shape).
    for h in handles {
        h.join().expect("producer thread must not panic");
    }
    let elapsed = t0.elapsed();

    unsafe { HeapRegistry::recycle(heap) };
    elapsed
}

#[test]
fn paused_owner_multisegment_interleave_completes_fast() {
    let _ = bootstrap::ensure();

    let mut samples = Vec::with_capacity(ATTEMPTS as usize);
    for attempt in 1..=ATTEMPTS {
        let elapsed = run_burst_once();
        eprintln!(
            "paused_owner_multisegment_interleave_completes_fast: attempt {attempt}/{ATTEMPTS} \
             N={N} PRODUCERS~{PRODUCERS} elapsed={elapsed:?} (bound={MAX_ELAPSED:?})"
        );
        if elapsed < MAX_ELAPSED {
            return; // Best-of-N: one fast attempt is sufficient — see module doc.
        }
        samples.push(elapsed);
    }

    panic!(
        "paused-owner MULTI-SEGMENT interleaved burst exceeded {MAX_ELAPSED:?} on ALL \
         {ATTEMPTS} attempts (samples: {samples:?}) — the R6-REVIEW-F2 pathology: a \
         fast-concede stall memo that cannot hold one snapshot per concurrently-stalled \
         segment thrashes under frees interleaved across 2+ saturated segments of a \
         paused owner, re-paying the full ~128-round stall patience on every push. See \
         `LAST_STALL_CONCESSIONS` / `STALL_CONCESSION_WAYS` in \
         `src/registry/heap_core_xthread.rs`."
    );
}
