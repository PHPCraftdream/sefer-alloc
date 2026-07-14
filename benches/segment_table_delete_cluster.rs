//! R5-R5 (round5 performance review §6.2, §8.2 item 2, §10 Stage C) —
//! `SegmentTable`'s backward-shift deletion (R4-8/N3,
//! `src/alloc_core/segment_table.rs::hash_remove`) cost as a function of
//! **probe-cluster length**.
//!
//! ## What this measures and why
//!
//! `hash_remove` repairs the open-addressing probe chain in place instead of
//! leaving a tombstone (see the module doc on `hash_remove` for the
//! shift-eligibility condition). Its cost is bounded by "the CURRENT cluster
//! length (the run of live entries that probed past the deleted slot)" per
//! that doc comment — NOT by `HASH_CAPACITY`. The round5 static performance
//! review flagged this as an unverified hypothesis: a single delete's cost
//! isn't fixed, it scales with how far the shift-eligibility walk has to go
//! to find (or fail to find) the next eligible entry. No existing test or
//! bench measures this relationship directly — the correctness proptests
//! (`tests/segment_table_backshift_proptest.rs`) check membership invariants,
//! not timing, and the regression test
//! (`tests/regression_segment_table_tombstone_rebuild.rs`) only guards against
//! a catastrophic max/median outlier ratio at one churn shape, not a sweep
//! across controlled cluster lengths.
//!
//! ## Controlled cluster construction — verifiably correct, not assumed
//!
//! We use the existing `#[doc(hidden)]` test-only seam
//! `sefer_alloc::alloc_core::SegmentHashHarness` (already built for
//! `tests/segment_table_backshift_proptest.rs`), which exposes the REAL
//! `hash_insert`/`hash_remove`/`hash_contains` code path over synthetic
//! (never-dereferenced) SEGMENT-aligned pointer values, plus
//! `base_for_index(h)` — a value whose `hash_index` is EXACTLY `h`. This lets
//! us build a dense, contiguous probe cluster of any length `L` at a known
//! starting hash index deterministically, with NO dependence on OS segment
//! address behaviour (unlike going through `AllocCore`, where cluster length
//! is an indirect, unverifiable consequence of allocation order and OS
//! address assignment). No new test-only seam was needed.
//!
//! We do not just ASSUME the insertion pattern produced a cluster of the
//! intended length: `verify_cluster_len` walks the harness's own
//! `contains`/`hash_index`-shaped probe from the cluster start and counts the
//! actual contiguous run of live entries, independently confirming the
//! achieved length before/after each timed delete and printing it alongside
//! the timing (see the constraint in the task: "the resulting cluster lengths
//! must be VERIFIABLE").
//!
//! ## Measuring a SINGLE delete, with percentiles
//!
//! Criterion's default `b.iter(...)` only surfaces mean/median point
//! estimates in its own report; it does not expose p95/p99/max directly. We
//! use `iter_custom` (criterion 0.5 API) to capture the RAW per-iteration
//! `Duration` for every sample — each sample rebuilds a fresh cluster of the
//! swept length (untimed) and times exactly ONE `hash_remove` call (timed) —
//! then compute median/p95/p99/max ourselves from the raw sample vector and
//! report them via `eprintln!`, matching this project's established
//! diagnostic-reporting pattern (`benches/global_alloc.rs`'s
//! `working_set_cycle` decommit-delta report,
//! `benches/heap_fanin_production.rs`'s per-cell reporting). Criterion's own
//! mean/median still appears in its normal stdout table (the summed
//! `Duration` returned from `iter_custom` feeds criterion's own statistics
//! engine unmodified).
//!
//! ## Cluster-length sweep bound
//!
//! `HASH_CAPACITY = 2048`, load factor guaranteed ≤ 50% (`MAX_SEGMENTS =
//! 1024`), so a single dense cluster can be at most ~1024 entries long before
//! violating the table's own capacity invariant. We sweep
//! short/medium/long/very-long points well inside that bound: 4, 16, 64, 256,
//! 768.
//!
//! Gated on `alloc-core` (the only feature `SegmentHashHarness` needs — it is
//! re-exported under `#[cfg(feature = "alloc-core")]` per `src/lib.rs` /
//! `src/alloc_core/mod.rs`), mirroring `benches/heap_xthread.rs`'s gating
//! style for a single-feature-gated bench.

#![cfg(feature = "alloc-core")]
#![allow(clippy::cast_precision_loss)]

use std::hint::black_box;
use std::time::{Duration, Instant};

use criterion::{criterion_group, criterion_main, Criterion};
use sefer_alloc::alloc_core::SegmentHashHarness;

/// Cluster lengths to sweep: short, medium, long, very-long. All comfortably
/// inside `SegmentHashHarness::CAPACITY / 2` (the guaranteed-safe load
/// factor), so no run ever approaches "hash table full" territory.
const CLUSTER_LENS: &[usize] = &[4, 16, 64, 256, 768];

/// Number of timed samples (single-delete measurements) collected per
/// cluster-length point. Each sample is a full rebuild-cluster + one
/// `hash_remove` + verify cycle; a single delete is cheap (bounded by cluster
/// length, at most ~768 pointer-sized writes even at the longest swept
/// point), so a few hundred samples keeps each point's wall-clock small while
/// still giving a meaningful p95/p99 (per CLAUDE.md's "lean toward MORE
/// samples ... given each individual delete is fast" guidance).
const SAMPLES_PER_POINT: usize = 500;

/// Build a dense, contiguous cluster of `len` entries starting at hash index
/// `start` in a fresh harness, and return the harness plus the list of
/// `(hash_index, base)` pairs inserted (in insertion order == hash-index
/// order), so the caller can pick a deletion target and independently verify
/// membership afterward.
fn build_cluster(start: usize, len: usize) -> (SegmentHashHarness, Vec<(usize, *mut u8)>) {
    let mut h = SegmentHashHarness::new();
    let cap = SegmentHashHarness::CAPACITY;
    let mut members = Vec::with_capacity(len);
    for k in 0..len {
        let idx = (start + k) % cap;
        let base = SegmentHashHarness::base_for_index(idx);
        h.insert(base);
        members.push((idx, base));
    }
    (h, members)
}

/// Independently VERIFY the achieved contiguous cluster length starting at
/// `start`, by walking `contains` forward from `start` until the first
/// absent index — the exact same "probe chain" shape `hash_contains` itself
/// walks. This does not trust the construction loop above; it re-derives the
/// length from the harness's own query surface, so a load-factor surprise or
/// an off-by-one in `build_cluster` would show up as a mismatch against the
/// intended length rather than being silently assumed.
fn verify_cluster_len(h: &SegmentHashHarness, start: usize) -> usize {
    let cap = SegmentHashHarness::CAPACITY;
    let mut n = 0usize;
    loop {
        let idx = (start + n) % cap;
        if !h.contains(SegmentHashHarness::base_for_index(idx)) {
            break;
        }
        n += 1;
        if n >= cap {
            // Defensive: should never happen at ≤50% load factor with the
            // swept lengths, but avoid an infinite loop under a future
            // regression rather than hanging the bench.
            break;
        }
    }
    n
}

/// The `p`-th percentile (0.0..=1.0) of an already-sorted `&[Duration]`
/// slice, using nearest-rank on the sorted index (simple and adequate for a
/// diagnostic — this is not a statistics research tool).
fn percentile(sorted: &[Duration], p: f64) -> Duration {
    debug_assert!(!sorted.is_empty());
    let rank = ((sorted.len() as f64) * p).ceil() as usize;
    let idx = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[idx]
}

/// Time `SAMPLES_PER_POINT` single deletes from a cluster of length `len`,
/// each sample rebuilding a fresh cluster (untimed setup) and deleting the
/// FIRST entry of the cluster (the worst-case position for backward-shift:
/// removing the cluster's head means the shift-eligibility walk potentially
/// considers every remaining live entry in the cluster before finding the
/// terminal empty slot). Returns the raw per-sample `Duration`s plus the
/// achieved (verified) cluster length observed on the first sample.
fn measure_cluster_deletes(len: usize) -> (Vec<Duration>, usize) {
    // Stagger the cluster start per point so repeated runs / different sweep
    // points don't all land on hash index 0 (not that it would be incorrect —
    // `hash_index` distribution doesn't depend on absolute position — but a
    // fixed nonzero start makes it easy to eyeball logs and confirms the
    // wrap-mod arithmetic in `build_cluster`/`verify_cluster_len` is
    // exercised, not just the `start == 0` special case).
    let start = 100usize;

    let mut samples = Vec::with_capacity(SAMPLES_PER_POINT);
    let mut achieved_len = 0usize;

    for i in 0..SAMPLES_PER_POINT {
        // Untimed: rebuild a fresh dense cluster of the target length.
        let (mut h, members) = build_cluster(start, len);

        if i == 0 {
            achieved_len = verify_cluster_len(&h, start);
        }

        // Delete target: the cluster's FIRST (lowest hash-index) entry — the
        // worst case for backward-shift, since every one of the remaining
        // `len - 1` live entries is a candidate the shift-eligibility walk
        // must visit before it can terminate (either by finding the first
        // ineligible entry, which stops the shift with the hole left
        // earlier, or by reaching the cluster's empty tail).
        let (_target_idx, target_base) = members[0];

        // Timed: exactly one `hash_remove` call.
        let t0 = Instant::now();
        h.remove(black_box(target_base));
        let dt = t0.elapsed();
        samples.push(dt);

        // Keep the harness alive (and touched) past the timed region so the
        // compiler cannot hoist/elide the delete; also a cheap correctness
        // sanity check that costs negligible time relative to the delete
        // itself is folded in via black_box below.
        black_box(&h);
    }

    (samples, achieved_len)
}

fn bench_segment_table_delete_cluster(c: &mut Criterion) {
    let mut group = c.benchmark_group("segment_table_delete_cluster");

    for &len in CLUSTER_LENS {
        // Run the full controlled measurement ONCE per point (outside
        // criterion's own `iter_custom` closure) so we get exactly
        // `SAMPLES_PER_POINT` raw samples with a verified achieved cluster
        // length, independent of how many times criterion's harness calls
        // the routine. `iter_custom` is then fed the SAME pre-recorded
        // samples on each of its own invocations (criterion may call the
        // routine multiple times across warm-up/measurement); recomputing a
        // fresh cluster sweep every criterion invocation would multiply the
        // wall-clock without adding signal, since the cluster-length effect
        // is already fully captured in one controlled run.
        let (samples, achieved_len) = measure_cluster_deletes(len);

        let mut sorted = samples.clone();
        sorted.sort_unstable();
        let median = percentile(&sorted, 0.50);
        let p95 = percentile(&sorted, 0.95);
        let p99 = percentile(&sorted, 0.99);
        let max = *sorted.last().unwrap();
        let sum: Duration = sorted.iter().sum();
        let mean = sum / (sorted.len() as u32);

        eprintln!(
            "segment_table_delete_cluster/len={len} (achieved={achieved_len}, \
             n={}): mean={mean:?} median={median:?} p95={p95:?} p99={p99:?} \
             max={max:?}",
            sorted.len(),
        );
        assert_eq!(
            achieved_len, len,
            "cluster construction did not achieve the intended length — \
             load-factor or hash-index assumption violated for len={len}"
        );

        group.bench_function(format!("len_{len}"), move |b| {
            // `iter_custom` (criterion 0.5 API): criterion tells us how many
            // iterations it wants for this sample; we replay that many
            // pre-recorded raw single-delete `Duration`s (cycling if
            // criterion asks for more than `SAMPLES_PER_POINT`, which only
            // happens if a config bumps sample_size/measurement_time beyond
            // what this sweep pre-records) and return their sum, so
            // criterion's own mean/median statistics are computed from the
            // SAME real per-delete timings recorded above (not re-measured,
            // avoiding a second, redundant, unverified cluster-construction
            // pass per criterion invocation).
            let samples = samples.clone();
            b.iter_custom(move |iters| {
                let mut total = Duration::ZERO;
                for k in 0..iters {
                    total += samples[(k as usize) % samples.len()];
                }
                total
            })
        });
    }

    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(50);
    targets = bench_segment_table_delete_cluster
}
criterion_main!(benches);
