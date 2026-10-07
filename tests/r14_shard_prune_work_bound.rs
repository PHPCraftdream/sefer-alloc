#![allow(deprecated)]
//! R14-02: the cold-point dead-claim sweep must not pay for live regions.
//!
//! Before the fix, every cold claim/bind ran a full `Vec::retain` over ALL
//! TLS claims (`Weak::strong_count` each), even when no token backing could
//! have died — binding N live one-shard regions performed N(N−1)/2
//! unrelated strong-count loads. The fix gates the sweep on a global
//! death-generation counter (`TOKEN_BACKING_DEATHS`), snapshotted BEFORE
//! each sweep so a concurrent death re-triggers the next cold point.
//!
//! Deterministic work-bound oracles (no timing, no RSS, no mocks): the
//! internals-gated `_prune_claim_checks_for_tests` counter counts the
//! ACTUAL per-claim `strong_count` checks `prune_dead_claims` performs.
//!
//! 1. `cold_binds_without_deaths_are_sweep_free` — one stable live claim
//!    plus 12 (> MAX_REMEMBERED_REGIONS = 8, so FIFO eviction forces
//!    repeated cold re-claims) concurrently live regions: ZERO prune checks.
//!    Under the old always-sweep behavior the very first cold re-bind is
//!    already > 0, so this test is its negative control.
//! 2. `death_triggers_one_bounded_sweep_and_keeps_live_claims` — each
//!    transient region drop triggers EXACTLY one sweep at the next cold
//!    point, checking exactly the claims present (2+3+4+5 = 14 checks for 4
//!    deaths, the last including the one dead transient); cold points
//!    without a new death again cost zero; the stable LIVE claim survives
//!    every sweep and still releases at thread exit (a fresh writer
//!    reclaims shard 0 after the join).
//!
//! The checks counter is process-global, so the tests serialize on a
//! file-local mutex and run their work on a single spawned worker each
//! (fresh per-thread TLS state). Every worker is joined with `expect`; no
//! test leaves a parked or panicking worker behind.

#![cfg(all(feature = "experimental", feature = "internals"))]

use std::sync::{Arc, Mutex};
use std::thread;

use sefer_alloc::{ShardedHandle, ShardedRegion};

fn gate() -> std::sync::MutexGuard<'static, ()> {
    static GATE: Mutex<()> = Mutex::new(());
    GATE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn checks() -> usize {
    ShardedRegion::<u32>::_prune_claim_checks_for_tests()
}

fn claims() -> usize {
    ShardedRegion::<u32>::_tls_claim_count_for_tests()
}

fn shard_of(h: ShardedHandle<u32>) -> u16 {
    ShardedRegion::<u32>::split_handle(h).0
}

#[test]
fn cold_binds_without_deaths_are_sweep_free() {
    let _gate = gate();
    thread::spawn(|| {
        let baseline = checks();

        // Cold point #1 (stable claim): brand-new guard, no death since its
        // death-generation snapshot — no sweep.
        let stable: ShardedRegion<u32> = ShardedRegion::with_shards(2, 4);
        let sh = stable.insert(1).expect("stable insert");
        assert_eq!(shard_of(sh), 0);
        assert_eq!(checks() - baseline, 0, "the first cold bind must not sweep");

        // 12 concurrently LIVE one-shard regions: each bind is a cold point
        // (and beyond 8 they FIFO-evict earlier bindings), but no backing
        // has died — the sweep must stay gated off entirely.
        let regions: Vec<ShardedRegion<u32>> =
            (0..12).map(|_| ShardedRegion::with_shards(1, 4)).collect();
        let handles: Vec<ShardedHandle<u32>> = regions
            .iter()
            .enumerate()
            .map(|(i, r)| r.insert(i as u32).expect("live insert"))
            .collect();
        assert_eq!(
            checks() - baseline,
            0,
            "binding live regions must perform ZERO prune checks (was \
             quadratic before R14-02)"
        );
        assert_eq!(claims(), 13, "stable claim + 12 live region claims");
        for (i, r) in regions.iter().enumerate() {
            assert_eq!(r.get_cloned(handles[i]), Some(i as u32), "region {i}");
        }
        assert_eq!(stable.get_cloned(sh), Some(1));

        // Revisit every one-shard region ROUND-ROBIN: their bindings were
        // FIFO-evicted long ago, so each revisit is a COLD point whose scan
        // finds the region's only shard already occupied BY THIS THREAD —
        // the scan wins nothing and the call degrades to modulo sharing on
        // the same shard. Two passes = 24 cold points, all sweep-free.
        for pass in 0..2 {
            for (i, r) in regions.iter().enumerate() {
                let h2 = r
                    .insert((100 + pass * 12 + i) as u32)
                    .expect("revisit insert");
                assert_eq!(shard_of(h2), 0, "shared shard of region {i}");
                assert_eq!(r.get_cloned(h2), Some((100 + pass * 12 + i) as u32));
            }
        }
        assert_eq!(
            checks() - baseline,
            0,
            "repeated FIFO-cache-miss cold points with all backings alive \
             must stay sweep-free"
        );
        assert_eq!(claims(), 13, "no new claim: modulo sharing wins no token");

        // Explicit bind on a cold region whose only shard's token this very
        // thread already holds: valid bind, NO exclusive win (CAS fails on
        // our own `true` token), and still zero sweep work.
        let evicted = &regions[0];
        assert!(
            evicted.bind_current_thread_to_shard(0),
            "bind records the routing binding even without an exclusive win"
        );
        assert_eq!(
            checks() - baseline,
            0,
            "explicit-bind cold point with a live backing must stay \
             sweep-free"
        );
        assert_eq!(claims(), 13, "bind over an already-held token wins nothing");
    })
    .join()
    .expect("worker panicked");
}

#[test]
fn death_triggers_one_bounded_sweep_and_keeps_live_claims() {
    let _gate = gate();
    let region = Arc::new(ShardedRegion::<u32>::with_shards(2, 4));

    let w_region = Arc::clone(&region);
    let worker = thread::spawn(move || {
        let baseline = checks();
        let h = w_region.insert(1).expect("stable insert");
        assert_eq!(shard_of(h), 0);

        let mut fresh: Vec<ShardedRegion<u32>> = Vec::new();
        for k in 0..4usize {
            // A transient region's drop kills one token backing (the only
            // death in this cycle).
            let transient: ShardedRegion<u32> = ShardedRegion::with_shards(1, 4);
            transient.insert(9).expect("transient insert");
            drop(transient);
            // The next cold point runs EXACTLY one sweep, over exactly the
            // claims recorded at that moment: stable (1) + fresh regions
            // from earlier cycles (k) + the one dead transient (1).
            let r: ShardedRegion<u32> = ShardedRegion::with_shards(1, 4);
            r.insert(8).expect("fresh insert");
            fresh.push(r);
            assert_eq!(
                checks() - baseline,
                (2..=5).take(k + 1).sum::<usize>(),
                "cycle {k}: exactly one bounded sweep since the last death"
            );
        }

        // Exactly 5 live claims before the extra cold point: stable + 4 fresh
        // regions; the last dead transient was pruned by its cycle's sweep.
        assert_eq!(
            claims(),
            5,
            "stable + 4 fresh live claims; dead transient pruned"
        );

        // A further cold point WITHOUT a new death performs no sweep, and its
        // new exclusive claim is live (6th).
        let extra: ShardedRegion<u32> = ShardedRegion::with_shards(1, 4);
        extra.insert(8).expect("extra insert");
        assert_eq!(
            checks() - baseline,
            2 + 3 + 4 + 5,
            "no sweep may run after the last death's sweep"
        );
        assert_eq!(claims(), 6, "the extra region adds a sixth live claim");

        assert_eq!(w_region.get_cloned(h), Some(1), "live claim still resolves");
        // The worker exits here; its guard releases shard 0 of `region`.
    });

    worker.join().expect("worker panicked");
    // The exit release survived the sweeps: a fresh writer reclaims shard 0.
    let h = region.insert(3).expect("post-join insert");
    assert_eq!(
        shard_of(h),
        0,
        "the worker's live claim must still be released at thread exit"
    );
    assert!(region.remove(h));
}
