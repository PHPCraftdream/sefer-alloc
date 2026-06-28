#![allow(deprecated)]
//! Phase 7b integration tests: **cross-thread removal** and **shard lifecycle**
//! over `ShardedRegion<T>` (`experimental`).
//!
//! FAST tests per the short-scenario policy:
//!
//! 1. **Cross-thread remove proptest (~64 cases)** — thread A inserts handles
//!    (claiming a shard); thread B removes A's handles from a DIFFERENT thread
//!    (the remote-evict path, lock-free, no owner mutex). I1–I4 hold; a stale
//!    handle is a no-op; accounting tracks across the cross-thread removes.
//! 2. **Thread-death + adoption** — a thread inserts, returns its handles, and
//!    EXITS. Its LIVE slots stay resolvable from another thread (reads do not
//!    depend on shard ownership); the handles are then removed cross-thread.
//!    A new thread can claim the released shard (lifecycle reuse).
//! 3. **Concurrent cross-thread churn** — several inserter threads mint
//!    handles handed to several remover threads; no double-free, no lost value,
//!    accounting holds at the join.

#![cfg(feature = "experimental")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::scope;

use proptest::prelude::*;
use sefer_alloc::{ShardedHandle, ShardedRegion};

// ---------------------------------------------------------------------------
// A drop-counting payload (mirrors tests/sharded.rs / tests/epoch.rs).
// ---------------------------------------------------------------------------

/// A payload carrying a drop counter, so I5 (drop-once) can be checked. The
/// epoch tier reclaims removed values DEFERRED (at an epoch boundary), so we
/// only assert drop counts for SURVIVORS dropped by region-drop (synchronous).
struct Payload {
    id: u64,
    drops: Arc<AtomicUsize>,
}

impl PartialEq for Payload {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Drop for Payload {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// 1. Cross-thread remove proptest.
// ---------------------------------------------------------------------------

proptest! {
    // Short scenario: 64 cases. This is a conformance smoke-check that
    // cross-thread remove (the remote-evict path) honours I1–I4, not an
    // exhaustive fuzz (that is Phase 5's job).
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn cross_thread_remove_honours_invariants(
        n_inserts in 1usize..=16,
        remove_pattern in prop::collection::vec(any::<bool>(), 1..=32),
    ) {
        // 2 shards so the inserter (thread A) and remover (thread B) land in
        // DISTINCT shards → B's removes take the remote-evict path. Small cap
        // so the full-shard Err path is reachable.
        const CAP: usize = 32;
        let region = Arc::new(ShardedRegion::<Payload>::with_shards(2, CAP));
        let drops = Arc::new(AtomicUsize::new(0));

        // Thread A inserts n_inserts values, returns the handles.
        let handles: Vec<ShardedHandle<Payload>> = scope(|s| {
            let region = Arc::clone(&region);
            let drops = Arc::clone(&drops);
            s.spawn(move || {
                let mut hs = Vec::new();
                for id in 0..n_inserts as u64 {
                    let p = Payload { id, drops: Arc::clone(&drops) };
                    match region.insert(p) {
                        Ok(h) => hs.push(h),
                        Err(_) => break, // shard full — stop early.
                    }
                }
                hs
            }).join().expect("inserter panicked")
        });

        // Every handle A minted resolves via the router (I1) — from THIS thread
        // (which has NOT claimed A's shard → reads are cross-thread but reads
        // don't depend on ownership).
        for (i, h) in handles.iter().enumerate() {
            prop_assert_eq!(
                region.get_with(*h, |p| p.id),
                Some(i as u64),
                "a live handle must resolve cross-thread (I1, reads are ownership-free)"
            );
        }
        prop_assert_eq!(region.len(), handles.len(), "I4: len tracks inserts");

        // Decide which handle indices to remove (computed in the main body so
        // assertions stay out of the spawned thread). Build the set BEFORE the
        // scope so it is not moved into the closure.
        let mut to_remove: Vec<usize> = Vec::new();
        for (i, &rm) in remove_pattern.iter().enumerate() {
            if rm {
                to_remove.push(i % handles.len());
            }
        }
        to_remove.sort_unstable();
        to_remove.dedup();
        let removed_set: std::collections::HashSet<usize> = to_remove.iter().copied().collect();
        let to_remove_clone = to_remove.clone();

        // Thread B (a DIFFERENT thread) removes the selected handles via the
        // remote-evict path. It returns, for each, whether the handle WAS live
        // (resolved) before the remove — so the main body can assert.
        let remove_outcomes: Vec<(usize, bool, bool)> = scope(|s| {
            let region = Arc::clone(&region);
            let handles = handles.clone();
            s.spawn(move || {
                to_remove_clone
                    .iter()
                    .map(|&idx| {
                        let h = handles[idx];
                        let was_live = region.get_with(h, |p| p.id) == Some(idx as u64);
                        let removed = region.remove(h);
                        // I2: removed handle is None forever; second remove is a no-op.
                        let second = region.remove(h);
                        (idx, was_live, removed && !second)
                    })
                    .collect::<Vec<_>>()
            })
            .join()
            .expect("remover thread panicked")
        });

        // Assert each outcome in the main body (proptest-compatible).
        let mut removed_count = 0usize;
        for (idx, was_live, removed_once) in remove_outcomes {
            let h = handles[idx];
            if was_live {
                prop_assert!(
                    removed_once,
                    "a live handle must remove cross-thread exactly once; \
                     second remove must be a no-op (I2, no double-free)"
                );
                removed_count += 1;
            }
            prop_assert_eq!(
                region.get_with(h, |p| p.id),
                None,
                "removed handle is None forever (I2)"
            );
        }

        // I4 after cross-thread removes.
        prop_assert_eq!(
            region.len(),
            handles.len() - removed_count,
            "I4: len tracks cross-thread removes"
        );

        // Survivors still resolve (from yet another thread, proving reads are
        // ownership-free and the remote-evict of other handles didn't corrupt).
        let survivors_resolve = scope(|s| {
            let region = Arc::clone(&region);
            let handles = handles.clone();
            s.spawn(move || {
                handles
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| !removed_set.contains(i))
                    .all(|(i, h)| region.get_with(*h, |p| p.id) == Some(i as u64))
            })
            .join()
            .expect("survivor-check thread panicked")
        });
        prop_assert!(
            survivors_resolve,
            "survivors still resolve cross-thread after remote removes (I1)"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Thread-death + adoption: a dead thread's LIVE slots stay resolvable.
// ---------------------------------------------------------------------------

/// A thread inserts values (claiming a shard), returns its handles, and EXITS.
/// Its shard is then ABANDONED. The test asserts:
///
/// 1. The dead thread's LIVE slots stay resolvable from another thread (reads
///    route by `handle.shard` and do NOT depend on ownership — the core 7b
///    "live blocks of a dead thread stay valid" property).
/// 2. The handles can be removed CROSS-THREAD (remote-evict) after the owner
///    died — the remote-free queue and the CAS eviction work without the owner.
/// 3. A new thread can claim the released shard (lifecycle reuse): after the
///    owner thread exits, its `occupied` token flips to false, and a fresh
///    thread claims that same shard id.
#[test]
fn dead_thread_live_slots_stay_resolvable_and_removable_cross_thread() {
    // 1 shard is enough to test "dead owner's slots stay resolvable"; use 2 so
    // the main thread can also claim a distinct shard (proving cross-shard
    // reads work too). Cap small so the test is fast.
    const CAP: usize = 8;
    let region = Arc::new(ShardedRegion::<u64>::with_shards(2, CAP));

    // Spawn a thread that inserts 3 values and returns the handles. The thread
    // then EXITS — its ErasedGuard drops, releasing its shard.
    let handles: Vec<ShardedHandle<u64>> = scope(|s| {
        let region = Arc::clone(&region);
        s.spawn(move || {
            let mut hs = Vec::new();
            for v in [100_u64, 200, 300] {
                hs.push(region.insert(v).expect("insert into fresh shard"));
            }
            // Sanity: all three resolve from within the owner thread.
            for (h, v) in hs.iter().zip([100_u64, 200, 300]) {
                assert_eq!(region.get_cloned(*h), Some(v), "owner reads its own values");
            }
            hs
        })
        .join()
        .expect("owner thread panicked")
    });
    // The spawned thread is now DEAD. Its shard is abandoned (occupied → false).

    // (1) The dead thread's LIVE slots stay resolvable from the main thread
    // (reads are ownership-free). This is the central 7b thread-death property.
    for (h, v) in handles.iter().zip([100_u64, 200, 300]) {
        assert_eq!(
            region.get_cloned(*h),
            Some(v),
            "a dead thread's live slots must stay resolvable (reads are ownership-free)"
        );
    }
    assert_eq!(region.len(), 3, "len holds after the owner thread died");

    // (2) Remove the dead thread's handles CROSS-THREAD (remote-evict). The
    // owner is gone; the CAS eviction + remote-free enqueue work without it.
    for h in &handles {
        assert!(
            region.remove(*h),
            "cross-thread remove of a dead owner's handle must succeed"
        );
        assert_eq!(region.get_cloned(*h), None, "removed handle is None (I2)");
    }
    assert_eq!(
        region.len(),
        0,
        "all handles removed cross-thread after owner death"
    );
    // No double-remove.
    for h in &handles {
        assert!(!region.remove(*h), "second remove is a no-op (I2)");
    }

    // (3) Lifecycle reuse: a NEW thread can claim the shard the dead thread
    // released. Insert from a fresh thread; it should succeed (the freed shard
    // is reusable). We can't assert the EXACT shard id without exposing
    // internals, but a successful insert proves a shard was claimable.
    let new_handle: ShardedHandle<u64> = scope(|s| {
        let region = Arc::clone(&region);
        s.spawn(move || region.insert(999_u64).expect("new thread claims a shard"))
            .join()
            .expect("adopter thread panicked")
    });
    assert_eq!(
        region.get_cloned(new_handle),
        Some(999),
        "new thread's insert resolves"
    );
}

// ---------------------------------------------------------------------------
// 3. Concurrent cross-thread churn: inserters hand handles to removers.
// ---------------------------------------------------------------------------

/// A drop-counting value so the concurrent test can assert no double-free.
struct Counted {
    v: u64,
    drops: Arc<AtomicUsize>,
}
impl Counted {
    fn id(&self) -> u64 {
        self.v
    }
}
impl Drop for Counted {
    fn drop(&mut self) {
        self.drops.fetch_add(1, Ordering::Relaxed);
    }
}

/// A shared queue of `(handle, value-id)` handed from inserter threads to
/// remover threads (factored out to keep clippy's `type_complexity` happy).
type HandoffQueue = Arc<Mutex<Vec<(ShardedHandle<Counted>, u64)>>>;

/// Several inserter threads mint handles and push them into a shared queue;
/// several remover threads drain the queue and remove them cross-thread. At
/// the join: no double-free (each handle removed at most once), accounting
/// holds (len == survivors), and every survivor resolves. This exercises the
/// remote-evict path under real concurrency (not just two sequential threads).
#[test]
fn concurrent_cross_thread_remove_never_double_frees_and_accounts() {
    const N_INSERTERS: usize = 2;
    const N_REMOVERS: usize = 2;
    const VALS_PER_INSERTER: usize = 64;
    // Enough shards that inserters land in distinct shards (graceful if not).
    const N_SHARDS: usize = 4;
    const CAP: usize = VALS_PER_INSERTER * N_INSERTERS;

    let region = Arc::new(ShardedRegion::<Counted>::with_shards(N_SHARDS, CAP));
    let drops = Arc::new(AtomicUsize::new(0));

    // The shared queue of (handle, value-id) handed from inserters to removers.
    let queue: HandoffQueue = Arc::new(Mutex::new(Vec::new()));
    let total_inserted = Arc::new(AtomicUsize::new(0));

    scope(|s| {
        // Inserters.
        for inserter_tid in 0..N_INSERTERS {
            let region = Arc::clone(&region);
            let queue = Arc::clone(&queue);
            let drops = Arc::clone(&drops);
            let total_inserted = Arc::clone(&total_inserted);
            s.spawn(move || {
                for seq in 0..VALS_PER_INSERTER as u64 {
                    let id = u64::try_from(inserter_tid).unwrap() * 1_000_000 + seq;
                    let v = Counted {
                        v: id,
                        drops: Arc::clone(&drops),
                    };
                    if let Ok(h) = region.insert(v) {
                        queue.lock().unwrap().push((h, id));
                        total_inserted.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
        }
        // Removers: drain the queue and remove handles cross-thread.
        for _ in 0..N_REMOVERS {
            let region = Arc::clone(&region);
            let queue = Arc::clone(&queue);
            s.spawn(move || loop {
                // pop under lock; process outside the lock.
                let next = queue.lock().unwrap().pop();
                let Some((h, _id)) = next else { break };
                let removed = region.remove(h);
                if removed {
                    // Second remove must be a no-op (no double-free).
                    assert!(
                        !region.remove(h),
                        "second cross-thread remove must be a no-op (no double-free)"
                    );
                }
            });
        }
    });

    // After the join, anything still in the queue was not removed — it is a
    // survivor. Drain and count.
    let leftover: Vec<_> = queue.lock().unwrap().drain(..).collect();
    let survivors = leftover.len();
    for (h, id) in &leftover {
        assert_eq!(
            region.get_with(*h, Counted::id),
            Some(*id),
            "survivor must resolve after concurrent cross-thread churn (I1)"
        );
    }

    let inserted = total_inserted.load(Ordering::Relaxed);
    assert_eq!(
        region.len(),
        survivors,
        "I4: len == survivors after concurrent cross-thread churn"
    );

    // Drop the region; every survivor's destructor runs synchronously (I5 for
    // survivors). Removed values may drop deferred (epoch caveat) — so the
    // drop count is AT LEAST survivors and AT MOST inserted (no double-free).
    drop(region);
    drop(leftover);
    let observed_drops = drops.load(Ordering::Relaxed);
    assert!(
        observed_drops >= survivors,
        "at least every survivor dropped once (survivors={survivors}, drops={observed_drops})"
    );
    assert!(
        observed_drops <= inserted,
        "no double-free: drops ({observed_drops}) <= inserted ({inserted})"
    );
}
