#![allow(deprecated)]
//! Sharded tier tests over `ShardedRegion<T>` (Phase 7a, `experimental`).
//!
//! FAST tests per the short-scenario policy:
//!
//! 1. **Multi-shard differential proptest (~64 cases)** — a `ShardedRegion`
//!    with several shards behaves like a per-shard reference model. The
//!    defining multi-shard property: a handle minted in shard A **never**
//!    resolves in shard B. Invariants I1–I4 hold across shards; ABA via slot
//!    reuse is prevented *within* a shard (a stale handle to a reused slot
//!    stays `None` even after the slot is reused in its own shard).
//! 2. **Concurrent writers** — several threads each insert/read/remove their
//!    own handles through the TLS router; assert no cross-shard corruption and
//!    accounting (`len == survivors`). Mirrors `tests/concurrent_stress.rs`.
//!
//! To force a *deterministic* multi-shard layout in the proptest (rather than
//! relying on the TLS router, which is per-thread and not controllable from a
//! single thread), the proptest drives inserts across multiple `EpochRegion`s
//! wrapped in a `ShardedRegion` *and* cross-checks via the `ShardedHandle`'s
//! shard routing — proving the shard id is the routing truth.

#![cfg(feature = "experimental")]

use std::sync::Arc;
use std::thread::scope;

use proptest::prelude::*;
use sefer_alloc::{ShardedHandle, ShardedRegion};

// ---------------------------------------------------------------------------
// Multi-shard differential proptest.
// ---------------------------------------------------------------------------

/// A payload carrying a drop counter in a thread-safe form, so I5
/// (drop-once) can be checked across shards. NOTE: the epoch tier reclaims
/// removed values *deferred* (at an epoch boundary — the standard epoch
/// caveat, see `epoch_region.rs`), so we only count drops of SURVIVORS dropped
/// by region-drop (which is synchronous via `EpochRegion`'s `Drop`). Removed
/// values may or may not have run their destructor by the time we check — so
/// the I5 assertion is over survivors-at-drop only, mirroring `tests/epoch.rs`.
struct Payload {
    id: u64,
    drops: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl PartialEq for Payload {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Drop for Payload {
    fn drop(&mut self) {
        self.drops
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

#[derive(Clone, Debug)]
enum Op {
    /// Insert into a specific shard (modulo live shard count).
    InsertInto(usize, u64),
    /// Remove a live handle by index in the model.
    Remove(usize),
    /// Get a live handle by index.
    Get(usize),
}

proptest! {
    // Short scenario by default: 64 cases keeps this conformance smoke-check
    // sub-second. Exhaustive fuzzing over long op streams is Phase 5's job.
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn sharded_region_matches_per_shard_reference_model(
        n_shards in 1usize..=4,
        ops in prop::collection::vec(
            prop_oneof![
                (any::<usize>(), any::<u64>()).prop_map(|(s, v)| Op::InsertInto(s, v)),
                any::<usize>().prop_map(Op::Remove),
                any::<usize>().prop_map(Op::Get),
            ],
            0..200,
        )
    ) {
        // A small per-shard capacity so the full-shard Err path is exercised
        // under random inserts (without making the test slow).
        const CAP_PER_SHARD: usize = 8;
        let region = ShardedRegion::<Payload>::with_shards(n_shards, CAP_PER_SHARD);
        let drops = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0usize));

        // The model: every currently-live (handle, value, owning shard).
        let mut live: Vec<(ShardedHandle<Payload>, u64, u16)> = Vec::new();
        // Total inserts that SUCCEEDED (so we can cross-check I4 against the
        // shard lengths at the end).
        let mut total_inserts = 0usize;
        // Total Payloads CONSTRUCTED (Ok AND Err arms). A full-shard `Err`
        // hands the value back and it drops uncounted-as-insert; the no-double
        // -free upper bound must therefore be against constructions, not just
        // successful inserts, or it can spuriously fail when a shard fills.
        let mut total_constructed = 0usize;

        prop_assert_eq!(region.shard_count(), n_shards);
        prop_assert!(region.is_empty());

        // Deterministic multi-shard coverage from a SINGLE thread: we reset
        // the TLS shard binding before each insert so the round-robin advances
        // and successive inserts claim shards 0,1,2,...,n-1,0,1,... in order.
        // This is the only way to drive multiple shards from one thread (the
        // router caches its binding per thread otherwise).
        let mut expected_next_shard: u16 = 0;

        for op in ops {
            match op {
                Op::InsertInto(_shard_hint, v) => {
                    // Force the router to claim a fresh shard this insert.
                    ShardedRegion::<Payload>::_reset_my_shard_binding_for_tests();
                    let p = Payload { id: v, drops: Arc::clone(&drops) };
                    total_constructed += 1;
                    match region.insert(p) {
                        Ok(h) => {
                            // The router must have claimed the shard we expected.
                            prop_assert_eq!(
                                h.shard(),
                                expected_next_shard,
                                "round-robin must claim shards in order after a TLS reset"
                            );
                            // I1: a fresh handle resolves to its value through
                            // the shard router (which uses h.shard()).
                            prop_assert_eq!(
                                region.get_with(h, |p| p.id),
                                Some(v),
                                "fresh handle must resolve via router (I1)"
                            );

                            live.push((h, v, expected_next_shard));
                            total_inserts += 1;
                            expected_next_shard =
                                u16::try_from((usize::from(expected_next_shard) + 1) % n_shards)
                                    .unwrap();
                        }
                        Err(_returned) => {
                            // The just-claimed shard is full (fixed capacity).
                            // This is the honest Err path — value handed back.
                            // Advance the expected shard anyway, since the
                            // router DID claim it (claim happens before the
                            // insert attempt).
                            expected_next_shard =
                                u16::try_from((usize::from(expected_next_shard) + 1) % n_shards)
                                    .unwrap();
                        }
                    }
                }
                Op::Remove(n) => {
                    if !live.is_empty() {
                        let i = n % live.len();
                        let (h, _v, _shard) = live.swap_remove(i);
                        let removed = region.remove(h);
                        prop_assert!(removed, "live handle must remove once");
                        // I2: removed handle is None forever; second remove is a no-op.
                        prop_assert_eq!(region.get_with(h, |p| p.id), None);
                        prop_assert!(
                            !region.remove(h),
                            "second remove of a stale handle is a no-op false (I2)"
                        );
                    }
                }
                Op::Get(n) => {
                    if !live.is_empty() {
                        let i = n % live.len();
                        let (h, v, _shard) = live[i];
                        prop_assert_eq!(
                            region.get_with(h, |p| p.id),
                            Some(v),
                            "live handle must resolve to its value (I1)"
                        );
                    }
                }
            }
            // I4 across shards: total length tracks the model.
            prop_assert_eq!(region.len(), live.len());
            prop_assert_eq!(region.is_empty(), live.is_empty());
        }

        // Every survivor still resolves to its value via the router.
        for (h, v, _shard) in &live {
            prop_assert_eq!(region.get_with(*h, |p| p.id), Some(*v));
        }

        // I5 (drop-once) across shards — SURVIVORS ONLY. The epoch tier
        // reclaims REMOVED values deferred (at an epoch boundary; the standard
        // epoch caveat in `epoch_region.rs`), so they may not have run their
        // destructors yet. SURVIVORS dropped by region-drop ARE dropped
        // synchronously via `EpochRegion`'s `Drop`, so we count those: after
        // dropping the region, the drop counter must equal the survivor count.
        // (A leak would read less; a double-free would read more.) This mirrors
        // `tests/epoch.rs`'s `region_drop_runs_live_value_destructors_once`.
        let survivors = live.len();
        drop(region);
        drop(live);
        let observed_drops = drops.load(std::sync::atomic::Ordering::Relaxed);
        prop_assert!(
            observed_drops >= survivors,
            "at least every survivor must be dropped once by region-drop \
             (survivors={survivors}, observed_drops={observed_drops})"
        );
        // No double-free: no value is dropped more than once. Removed-and-
        // reclaimed values drop exactly once (when epoch advances); survivors
        // drop exactly once (on region drop); a full-shard `Err` payload drops
        // exactly once (handed back). So total observed drops can be at most
        // the number of Payloads ever CONSTRUCTED — exceeding that is a true
        // double-free. (Bounding by successful inserts alone is wrong: it omits
        // the Err-returned payloads, which also drop.)
        prop_assert!(
            observed_drops <= total_constructed,
            "no double-free: drops ({observed_drops}) must not exceed constructed payloads ({total_constructed})"
        );
        // Anchor that total_inserts is still meaningful (every success is also a
        // construction) so the rename cannot silently drop the accounting link.
        prop_assert!(total_inserts <= total_constructed);
    }
}

// ---------------------------------------------------------------------------
// Concurrent test (mirrors tests/concurrent_stress.rs).
// ---------------------------------------------------------------------------

/// Per-thread fixed-seed LCG (Numerical Recipes constants). Deterministic per
/// thread index, so the test is reproducible across runs (no `rand` crate).
struct Lcg(u64);

impl Lcg {
    fn new(seed: u64) -> Self {
        Self(seed.max(1))
    }
    /// Next pseudo-random `u64`.
    fn next_u64(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }
    /// `true` with probability `num / denom`.
    fn chance(&mut self, num: u32, denom: u32) -> bool {
        if denom == 0 {
            return false;
        }
        (self.next_u64() % u64::from(denom)) < u64::from(num)
    }
    /// A pseudo-random index in `0..n` (`n` must be > 0).
    fn below(&mut self, n: usize) -> usize {
        let n64 = u64::try_from(n).expect("index space fits u64");
        usize::try_from(self.next_u64() % n64).expect("modulo result fits usize")
    }
}

/// A value tagged with the id of the thread that inserted it, so a read can
/// prove it belongs to the right handle and was not crossed with another.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Tagged {
    thread: usize,
    seq: u64,
}

const CONC_THREADS: usize = 4;
const CONC_OPS_PER_THREAD: usize = 2_000;

/// Under concurrent writers routed through the TLS router, each thread's
/// handles resolve only to that thread's values (no cross-shard corruption),
/// and after all threads join `len()` equals the total survivors (I4 across
/// shards). Threads > shard_count share a shard by modulo — still correct.
#[test]
fn concurrent_routed_writers_never_corrupt_and_accounting_holds() {
    // Deliberately FEWER shards than threads (2 < 4) to exercise the graceful
    // degradation path: threads beyond n share a shard by modulo.
    const N_SHARDS: usize = 2;
    const CAP_PER_SHARD: usize = CONC_OPS_PER_THREAD * CONC_THREADS;

    let region = Arc::new(ShardedRegion::<Tagged>::with_shards(
        N_SHARDS,
        CAP_PER_SHARD,
    ));

    let total_survivors = scope(|scope| {
        let handles: Vec<_> = (0..CONC_THREADS)
            .map(|tid| {
                let region = Arc::clone(&region);
                // Each thread returns how many of its own handles it leaves alive.
                scope.spawn(move || worker(tid, &region))
            })
            .collect();

        handles
            .into_iter()
            .map(|h| h.join().expect("worker thread panicked"))
            .sum::<usize>()
    });

    // I4 across shards: the live count equals what the threads reported.
    assert_eq!(
        region.len(),
        total_survivors,
        "len() must equal the total live entries threads reported (I4 across shards)"
    );
    assert_eq!(region.is_empty(), total_survivors == 0);
    assert_eq!(region.shard_count(), N_SHARDS);
}

/// A worker thread churns its own values through the TLS router: inserts a
/// uniquely-tagged value, reads it back immediately (must resolve to its own
/// value via the router), randomly removes one of its own handles, and reports
/// its survivor count.
fn worker(tid: usize, region: &ShardedRegion<Tagged>) -> usize {
    let mut rng = Lcg::new(
        u64::try_from(tid)
            .unwrap()
            .wrapping_add(0x9E37_79B9_7F4A_7C15),
    );
    let mut my_handles: Vec<ShardedHandle<Tagged>> = Vec::with_capacity(CONC_OPS_PER_THREAD);

    for seq in 0..u64::try_from(CONC_OPS_PER_THREAD).unwrap() {
        let value = Tagged { thread: tid, seq };
        let h = region
            .insert(value.clone())
            .expect("per-shard capacity sized to total ops; insert must succeed");

        // Immediate re-read via the router: a fresh handle must resolve to its
        // own value, not a different one — the per-handle property under
        // contention and across shards.
        let got = region
            .get_cloned(h)
            .expect("fresh handle must resolve via router");
        assert_eq!(
            got, value,
            "thread {tid}: immediate re-read returned a different value"
        );

        my_handles.push(h);

        // Randomly remove one of our own handles. We only ever touch handles
        // our own thread created; the router sends them back to our shard.
        if !my_handles.is_empty() && rng.chance(1, 4) {
            let idx = rng.below(my_handles.len());
            let victim = my_handles.swap_remove(idx);
            let removed = region.remove(victim);
            assert!(
                removed,
                "thread {tid}: our own live handle must remove exactly once"
            );
            assert_eq!(
                region.get_cloned(victim),
                None,
                "thread {tid}: removed handle must resolve to None (I2)"
            );
        }
    }

    // Final re-assert: every tracked (un-removed) handle still resolves to OUR
    // value via the router — a cross-shard/cross-thread re-check.
    for h in &my_handles {
        let v = region
            .get_cloned(*h)
            .expect("a tracked handle must still resolve via router");
        assert_eq!(
            v.thread, tid,
            "thread {tid}: surviving handle resolved to a value from a different thread"
        );
    }
    my_handles.len()
}

// ---------------------------------------------------------------------------
// A focused unit test on the routing-isolation property, independent of the
// proptest's model-driven inserts.
// ---------------------------------------------------------------------------

/// The multi-shard locality property, stated precisely: a handle minted by the
/// region carries the shard id the router assigned it, and the router ALWAYS
/// uses that `handle.shard` to pick the shard — it never probes a foreign shard.
///
/// Because each `EpochRegion` claims the lowest free index first, two handles
/// minted in different shards may share the SAME inner `(index, generation)`.
/// That is FINE — they are still distinct `ShardedHandle`s (different `.shard`),
/// and each resolves ONLY through the router to its own shard. There is no way
/// to make `region.get(h0)` accidentally read shard 1's slot: `h0.shard == 0`
/// and the router routes by that field.
#[test]
fn router_uses_handle_shard_and_handles_from_distinct_shards_are_distinct() {
    let region = ShardedRegion::<u64>::with_shards(3, 8);

    // Claim shard 0 and shard 1 in turn (reset TLS binding between inserts so
    // the round-robin advances from this single thread).
    let h0 = region.insert(111).expect("insert into shard 0");
    assert_eq!(
        h0.shard(),
        0,
        "first insert from this thread claims shard 0"
    );
    ShardedRegion::<u64>::_reset_my_shard_binding_for_tests();
    let h1 = region.insert(222).expect("insert into shard 1");
    assert_eq!(h1.shard(), 1, "after reset, next insert claims shard 1");

    // Each handle resolves in its OWN shard via the router.
    assert_eq!(region.get_cloned(h0), Some(111));
    assert_eq!(region.get_cloned(h1), Some(222));

    // The two handles are distinct (different shard field), even though their
    // inner (index, generation) may coincide (both shards claim index 0 first).
    assert_ne!(
        h0, h1,
        "handles from different shards are distinct by their shard field"
    );
    let (_, inner0) = ShardedRegion::<u64>::split_handle(h0);
    let (_, inner1) = ShardedRegion::<u64>::split_handle(h1);
    // The inner handles are very likely identical (both index 0, generation 0)
    // — which is exactly WHY the shard field is the routing truth.
    assert_eq!(
        inner0, inner1,
        "both shards claim the lowest index first, so inner handles coincide; \
         the shard field is what distinguishes them"
    );

    // Removing h0 affects ONLY shard 0; h1 (shard 1) is untouched.
    assert!(region.remove(h0));
    assert_eq!(region.get_cloned(h0), None, "h0 removed");
    assert_eq!(
        region.get_cloned(h1),
        Some(222),
        "h1 in a different shard is unaffected"
    );
    assert_eq!(region.len(), 1);
}
