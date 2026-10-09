#![allow(deprecated)]
//! R13-02: dead-region shard-token retention in the `ShardedRegion` TLS
//! router.
//!
//! Before the fix, each exclusive claim the TLS `ErasedGuard` recorded held a
//! STRONG `Arc<[AtomicBool]>`, so a long-lived worker that created, bound and
//! dropped N transient regions pinned N token arrays (and N claim records)
//! until thread exit. The fix: a claim is a `Weak` to an out-of-line token
//! backing (`Arc<TokenBlock>` whose boxed slot array dies with the region —
//! NOT an inline `Arc<[AtomicBool]>`, whose backing allocation would survive
//! until the last `Weak` drops), and dead claims are pruned at the bounded
//! cold claim/bind points.
//!
//! Oracles (deterministic: bounded `sync_channel(1)` hand-offs and joins —
//! no sleeps, no RSS, no fake load; every waiter exits on channel
//! disconnect, so a worker panic can never strand its counterpart):
//!
//! 1. `dead_region_token_backing_dies_while_worker_alive` — the worker owns
//!    one stable claimed region, churns 12 (> 8) transient regions, samples
//!    the live-backing count, exercises and drops the stable region,
//!    samples again, sends BOTH samples to main and only then parks on a
//!    release receiver; main samples the counter while the worker is
//!    provably still alive. The count is maintained by a `Drop` probe on
//!    the token backing, so it observes the storage's actual destruction,
//!    not merely a failed `Weak::upgrade`.
//! 2. `live_region_claims_survive_fifo_churn` — 12 concurrently live regions
//!    (> the 8-entry FIFO cache) keep every claim and every handle
//!    resolvable; a cold re-insert into the cache-evicted region claims a
//!    SECOND shard of that same live region; routing for the cached-out
//!    first claim takes the documented remote path.
//! 3. `thread_exit_release_and_reclaim` — X binds shards 0 and 1 of a live
//!    3-shard region, removes its values, exits and is joined; E claims
//!    shard 0, reports it and PARKS while holding it; main (as G) claims
//!    shard 1, then releases E. Assertions run only after the release and
//!    the join: under a non-releasing mutant E lands on shard 2 and G on
//!    fallback shard 0 — a visible bound failure, never a hang.
//!
//! Tests 1–2 read the process-global live-block counter and are
//! `internals`-gated; all three serialize on a file-local mutex because the
//! counter is process-global and a test binary's tests run on parallel
//! threads by default. The whole target requires `experimental internals`.

#![cfg(all(feature = "experimental", feature = "internals"))]

use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use sefer_alloc::{ShardedHandle, ShardedRegion};

/// Serializes the tests in THIS binary (other `tests/*.rs` files are
/// separate processes and cannot interfere). Poison-tolerant: a failed test
/// must not cascade-fail the rest of the file.
fn gate() -> std::sync::MutexGuard<'static, ()> {
    static GATE: OnceLock<Mutex<()>> = OnceLock::new();
    GATE.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Creates, binds and drops `n` transient 1-shard regions on the CURRENT
/// thread. Each drop leaves the just-dropped region's dead claim behind and
/// the NEXT cold claim prunes it, so beside any stable live claim at most
/// the last dead transient can be recorded — a no-prune regression trips
/// the bound below before thread exit.
#[cfg(feature = "internals")]
fn churn_transient_regions(n: usize, cap_per_shard: usize) {
    for _ in 0..n {
        let region: ShardedRegion<u32> = ShardedRegion::with_shards(1, cap_per_shard);
        let h = region.insert(7).expect("transient insert");
        assert_eq!(region.get_cloned(h), Some(7));
        drop(region);
        let claims = ShardedRegion::<u32>::_tls_claim_count_for_tests();
        assert!(
            claims <= 2,
            "cold-claim prune must keep at most the stable live claim plus \
             the last dead transient: got {claims}"
        );
    }
}

fn shard_of<T>(h: ShardedHandle<T>) -> u16 {
    ShardedRegion::<T>::split_handle(h).0
}

#[cfg(feature = "internals")]
fn queue_len(r: &ShardedRegion<u32>, shard: u16) -> usize {
    r._remote_free_queue_buffer_identity_for_tests(shard)
        .expect("shard in range")
        .1
}

#[cfg(feature = "internals")]
#[test]
fn dead_region_token_backing_dies_while_worker_alive() {
    let _gate = gate();
    let baseline = ShardedRegion::<u32>::_live_token_blocks_for_tests();

    // samples: worker -> main ((with stable alive, after stable drop));
    // release: main -> worker ("main has sampled; finish").
    // Bounded(1): the worker's sample send buffers without a waiting
    // receiver, and its park exits when main drops the release sender.
    let (sample_tx, sample_rx) = sync_channel::<(usize, usize)>(1);
    let (release_tx, release_rx) = sync_channel::<()>(1);

    let worker = thread::spawn(move || {
        // The stable region stays alive (and claimed) across the churn.
        let stable: ShardedRegion<u32> = ShardedRegion::with_shards(2, 4);
        let sh = stable.insert(1).expect("stable insert");

        churn_transient_regions(12, 4); // more than MAX_REMEMBERED_REGIONS (8)
        let claims_after_churn = ShardedRegion::<u32>::_tls_claim_count_for_tests();
        assert!(
            claims_after_churn <= 2,
            "after the churn at most the stable live claim and the last dead \
             transient may remain: got {claims_after_churn}"
        );

        // The stable region (and its claim) still works on this thread.
        assert_eq!(stable.get_cloned(sh), Some(1));
        assert!(stable.remove(sh));

        // Sample 1: still alive, 12 transient backings destroyed. Sample 2:
        // the stable backing dies with its region — still before thread exit.
        let with_stable = ShardedRegion::<u32>::_live_token_blocks_for_tests();
        drop(stable);
        let after_stable_drop = ShardedRegion::<u32>::_live_token_blocks_for_tests();

        sample_tx
            .send((with_stable, after_stable_drop))
            .expect("main is still waiting for the samples");
        let _ = release_rx.recv(); // Err = main released this thread
    });

    // Take main's own observation while the worker is parked (alive), then
    // let it finish; ASSERT only after the join so no failure path can
    // strand the parked worker.
    let (with_stable, after_stable_drop) =
        sample_rx.recv().expect("worker panicked before sampling");
    let while_worker_alive = ShardedRegion::<u32>::_live_token_blocks_for_tests();
    drop(release_tx);
    worker.join().expect("worker panicked after sampling");

    assert_eq!(
        with_stable,
        baseline + 1,
        "while the worker is ALIVE, only the stable region's token backing \
         may remain — the 12 transient regions' backings must be destroyed \
         before thread exit"
    );
    assert_eq!(
        after_stable_drop, baseline,
        "the stable region's backing dies at region drop, not thread exit"
    );
    assert_eq!(
        while_worker_alive, baseline,
        "main observed the same pre-exit count while the worker was parked"
    );
}

#[cfg(feature = "internals")]
#[test]
fn live_region_claims_survive_fifo_churn() {
    let _gate = gate();
    let baseline = ShardedRegion::<u32>::_live_token_blocks_for_tests();

    thread::spawn(move || {
        // 12 concurrently LIVE regions — more than MAX_REMEMBERED_REGIONS.
        let regions: Vec<ShardedRegion<u32>> =
            (0..12).map(|_| ShardedRegion::with_shards(2, 4)).collect();
        let handles: Vec<ShardedHandle<u32>> = regions
            .iter()
            .enumerate()
            .map(|(i, r)| r.insert(i as u32).expect("live insert"))
            .collect();

        assert_eq!(
            ShardedRegion::<u32>::_live_token_blocks_for_tests(),
            baseline + 12,
            "every live region's backing is live"
        );

        // Region 0's cache entry was FIFO-evicted by regions 4..=11, so this
        // insert is a COLD claim: shard 0 is occupied by this very thread and
        // must be kept, so the scan claims the free shard 1 — a SECOND live
        // claim on the same region, never an eviction of the first.
        let h_second = regions[0].insert(100).expect("second insert into region 0");
        assert_eq!(
            shard_of(h_second),
            1,
            "cold re-entry claims the second free shard; the first claim of \
             the live region is retained (multi-claim preserved)"
        );

        // Every claim of every live region still resolves (I1).
        for (i, r) in regions.iter().enumerate() {
            assert_eq!(r.get_cloned(handles[i]), Some(i as u32), "region {i}");
        }
        assert_eq!(regions[0].get_cloned(h_second), Some(100));

        // Routing for the cached-out first claim follows the documented
        // semantics: its shard (0) is no longer the cached binding (1), so
        // the removal takes the REMOTE path (frees into shard 0's remote
        // queue); the owner-path removal of the second claim leaves shard 1's
        // queue untouched.
        let q0_before = queue_len(&regions[0], 0);
        assert!(regions[0].remove(handles[0]));
        assert_eq!(
            queue_len(&regions[0], 0) - q0_before,
            1,
            "removing the cached-out claim routes remote"
        );
        assert!(!regions[0].remove(handles[0]), "second remove is a no-op");
        let q1_before = queue_len(&regions[0], 1);
        assert!(regions[0].remove(h_second));
        assert_eq!(
            queue_len(&regions[0], 1),
            q1_before,
            "owner-path removal pushes nothing"
        );
    })
    .join()
    .expect("worker panicked");

    // Leak guard only: no backing may outlive its region. This post-join
    // assert does NOT distinguish strong TLS retention — a strong-claim
    // regression also cleans up at thread exit; observing the retention
    // contract before thread exit is test 1's pre-exit oracle.
    assert_eq!(
        ShardedRegion::<u32>::_live_token_blocks_for_tests(),
        baseline,
        "no backing survives its region's drop"
    );
}

/// X binds itself to shards 0 and 1 of a live 3-shard region (two exclusive
/// claims), removes the values it issued, exits and is joined. E then claims
/// shard 0, reports the shard and PARKS while still holding it; with E
/// parked, main (as G) claims shard 1 and removes its value; main then
/// releases E and joins it. Assertions run after the join. Counterfactual:
/// with a non-releasing regression X's tokens stay `occupied`, so E can only
/// claim shard 2 and G can only fall back to shard 0 (ticket 0) — the
/// asserted 0/1 fail visibly, with no thread left blocked.
#[test]
fn thread_exit_release_and_reclaim() {
    let _gate = gate();
    let region = Arc::new(ShardedRegion::<u32>::with_shards(3, 4));

    // Initial worker: two explicit binds, hygiene removes, exit. No other
    // waiter exists yet, so these asserts cannot strand anyone.
    let x_region = Arc::clone(&region);
    thread::spawn(move || {
        assert!(
            x_region.bind_current_thread_to_shard(0),
            "bind X to shard 0"
        );
        let h0 = x_region.insert(1).expect("X insert 0");
        assert_eq!(shard_of(h0), 0, "X routes to its bound shard 0");
        assert!(
            x_region.bind_current_thread_to_shard(1),
            "bind X to shard 1"
        );
        let h1 = x_region.insert(2).expect("X insert 1");
        assert_eq!(shard_of(h1), 1, "X routes to its re-bound shard 1");
        // h0 now routes remote (the cached binding is 1); h1 routes owner.
        assert!(x_region.remove(h0));
        assert!(x_region.remove(h1));
        // X returns -> joined below -> its guard releases tokens 0 and 1.
    })
    .join()
    .expect("X panicked");

    // E claims shard 0, reports it, parks on the release receiver while
    // STILL HOLDING the claim.
    let (sample_tx, sample_rx) = sync_channel::<u16>(1);
    let (release_tx, release_rx) = sync_channel::<()>(1);
    let e_region = Arc::clone(&region);
    let e = thread::spawn(move || {
        let h = e_region.insert(5).expect("E insert");
        let shard = shard_of(h);
        sample_tx
            .send(shard)
            .expect("main is still waiting for E's shard");
        let _ = release_rx.recv(); // Err = main released this thread
        assert!(e_region.remove(h));
    });

    // E's sample proves it claimed shard 0 and is parked: only now may main
    // act as G, so G's cold claim deterministically scans past shard 0.
    let e_shard = sample_rx.recv().expect("E panicked before reporting");

    // While E is parked on shard 0, main (as G) claims shard 1 and cleans up.
    let g = region.insert(6).expect("G insert");
    let g_shard = shard_of(g);
    let g_removed = region.remove(g);

    // Release E and join before asserting, so no failure path leaves a
    // parked worker behind.
    drop(release_tx);
    e.join().expect("E panicked while parked or cleaning up");

    assert_eq!(
        e_shard, 0,
        "E must RECLAIM shard 0 via the claim scan; 2 means X's release was \
         lost"
    );
    assert_eq!(
        g_shard, 1,
        "G must RECLAIM shard 1 via the claim scan; 0 means G degraded to \
         fallback because X's release was lost"
    );
    assert!(g_removed, "G owner-removes its own value");
}
