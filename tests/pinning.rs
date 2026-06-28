#![allow(deprecated)]
//! Phase 7c pinning tests (`pinning` feature).
//!
//! FAST tests per the short-scenario policy. These assert the **routing
//! binding** (deterministic: `bind_current_thread_to_shard(i)` routes inserts
//! to shard `i`), NOT that the OS honored the affinity (which is best-effort
//! and host-dependent — a CI runner may refuse `set_for_current`, and
//! `get_core_ids` may even return `None`). The runner test is therefore gated
//! on `PinnedRunner::new` succeeding; if the host cannot enumerate cores, the
//! runner test is skipped (not failed), mirroring the best-effort contract.
//!
//! 1. `bind_current_thread_to_shard(i)` → subsequent inserts carry `shard == i`.
//! 2. An out-of-range bind is rejected (`false`), no panic, no binding recorded.
//! 3. The thread-per-core runner: each worker inserts from its bound shard; all
//!    values resolve and accounting holds at the join.

#![cfg(feature = "pinning")]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use sefer_alloc::{PinnedRunner, ShardedHandle, ShardedRegion};

// ---------------------------------------------------------------------------
// 1. bind_current_thread_to_shard routes inserts to the bound shard.
// ---------------------------------------------------------------------------

/// `bind_current_thread_to_shard(i)` makes the next `insert` mint a handle
/// whose `shard == i`. Run in a FRESH thread so the TLS binding is isolated
/// from other tests (the router's TLS cells are process-global — see
/// `sharded_region.rs`'s module docs on one-region-per-thread-pool).
#[test]
fn bind_routes_inserts_to_bound_shard() {
    let region = ShardedRegion::<u64>::with_shards(4, 16);
    // Bind to shard 2 in a fresh thread, insert, and assert the handle carries
    // shard == 2.
    let handle = std::thread::scope(|s| {
        s.spawn(|| {
            assert!(
                region.bind_current_thread_to_shard(2),
                "in-range bind must succeed"
            );
            region.insert(0xCAFE_u64).expect("shard has capacity")
        })
        .join()
        .expect("worker must not panic")
    });
    assert_eq!(handle.shard(), 2, "bind must route the insert to shard 2");
    assert_eq!(
        region.get_cloned(handle),
        Some(0xCAFE),
        "the bound-shard value must resolve (I1)"
    );
}

/// Binding different shards in different threads routes each thread's inserts
/// to its own bound shard (the thread-per-core routing property, without
/// relying on OS affinity).
#[test]
fn bind_routes_per_thread() {
    let region = Arc::new(ShardedRegion::<u64>::with_shards(4, 16));
    let handles = Arc::new(Mutex::new(Vec::<ShardedHandle<u64>>::new()));
    std::thread::scope(|s| {
        for shard in 0u16..4 {
            let region = Arc::clone(&region);
            let handles = Arc::clone(&handles);
            s.spawn(move || {
                assert!(region.bind_current_thread_to_shard(shard));
                let h = region.insert(u64::from(shard)).expect("capacity");
                assert_eq!(
                    h.shard(),
                    shard,
                    "worker {shard} must route to its bound shard"
                );
                handles.lock().unwrap().push(h);
            });
        }
    });
    let handles = handles.lock().unwrap();
    assert_eq!(handles.len(), 4, "one handle per worker");
    // The handles vector is in nondeterministic arrival order (4 concurrent
    // workers pushed through a Mutex); collect the shard ids as a sorted set
    // rather than assuming index == shard.
    let mut shards: Vec<u16> = handles.iter().map(|h| h.shard()).collect();
    shards.sort_unstable();
    assert_eq!(shards, vec![0, 1, 2, 3], "each worker bound a distinct shard");
    // Each handle resolves to its worker's value (value == shard id here).
    for h in handles.iter() {
        assert_eq!(region.get_cloned(*h), Some(u64::from(h.shard())));
    }
    assert_eq!(region.len(), 4, "accounting: 4 live values");
}

// ---------------------------------------------------------------------------
// 2. Out-of-range bind is rejected (no panic, no binding recorded).
// ---------------------------------------------------------------------------

/// `bind_current_thread_to_shard(shard_count)` returns `false` and does NOT
/// record a binding (a subsequent insert must not route to an out-of-range
/// shard — it falls back to the normal claim).
#[test]
fn out_of_range_bind_rejected_no_panic() {
    let region = ShardedRegion::<u64>::with_shards(3, 16);
    let n = region.shard_count();
    std::thread::scope(|s| {
        s.spawn(|| {
            // Out-of-range binds: must return false, must not panic.
            assert!(!region.bind_current_thread_to_shard(u16::try_from(n).unwrap()));
            assert!(!region.bind_current_thread_to_shard(u16::MAX));
            // No binding was recorded: an insert now goes through the normal
            // claim path and lands in a VALID shard (< n). It must NOT panic.
            let h = region.insert(1_u64).expect("capacity");
            assert!(
                usize::from(h.shard()) < n,
                "rejected bind must not leave an out-of-range binding"
            );
        })
        .join()
        .expect("must not panic");
    });
}

// ---------------------------------------------------------------------------
// 3. Thread-per-core runner: inserts from each worker, all resolve, accounting.
// ---------------------------------------------------------------------------

/// The `PinnedRunner` inserts from each worker (each bound to its shard); all
/// values resolve and `len` holds at the join. Gated on `PinnedRunner::new`
/// succeeding — if the host cannot enumerate cores (some CI sandboxes), the
/// test is SKIPPED, not failed (the best-effort contract).
#[test]
fn pinned_runner_inserts_resolve_and_accounting_holds() {
    let region = Arc::new(ShardedRegion::<u64>::with_shards(4, 64));
    // Probe cores once; skip (not fail) if the host refuses to enumerate.
    let runner = match PinnedRunner::with_workers(&region, 4) {
        Some(r) => r,
        None => {
            eprintln!("skip: core_affinity::get_core_ids returned None on this host");
            return;
        }
    };
    let workers = runner.worker_count();
    assert!((1..=4).contains(&workers));

    let inserted = Arc::new(AtomicUsize::new(0));
    let per_worker_values = Arc::new(Mutex::new(Vec::<(u16, u64, ShardedHandle<u64>)>::new()));
    let cap = 8usize; // inserts per worker — modest for speed.
    runner.run_arc(&region, |shard_id, region| {
        let mut local = Vec::new();
        for v in 0..u64::try_from(cap).unwrap() {
            let h = region.insert(v).expect("shard has capacity");
            assert_eq!(
                h.shard(),
                shard_id,
                "runner worker must route to its bound shard"
            );
            assert_eq!(region.get_cloned(h), Some(v), "fresh insert resolves (I1)");
            local.push((shard_id, v, h));
        }
        inserted.fetch_add(cap, Ordering::Relaxed);
        per_worker_values.lock().unwrap().extend(local);
    });

    // Accounting: every insert is live, none lost across the join.
    assert_eq!(inserted.load(Ordering::Relaxed), workers * cap);
    assert_eq!(region.len(), workers * cap, "len must equal total live inserts");

    // Every handle minted by a worker still resolves after the join (the shard
    // lifecycle keeps a dead thread's live slots resolvable — 7b).
    let all = per_worker_values.lock().unwrap();
    for (shard_id, v, h) in all.iter() {
        assert_eq!(h.shard(), *shard_id);
        assert_eq!(region.get_cloned(*h), Some(*v), "live value must resolve post-join");
    }
}

/// Removing a value minted by a (now-exited) worker, from the MAIN thread, goes
/// through the remote-evict path (the main thread is not bound to the worker's
/// shard). Asserts the cross-thread remove + accounting decrement.
#[test]
fn pinned_runner_then_cross_thread_remove() {
    let region = Arc::new(ShardedRegion::<u64>::with_shards(4, 64));
    let runner = match PinnedRunner::with_workers(&region, 2) {
        Some(r) => r,
        None => {
            eprintln!("skip: core_affinity::get_core_ids returned None on this host");
            return;
        }
    };
    let workers = runner.worker_count();

    // Each worker inserts exactly one value and hands the handle out.
    let handles = Arc::new(Mutex::new(Vec::<ShardedHandle<u64>>::new()));
    runner.run_arc(&region, |shard_id, region| {
        let h = region.insert(u64::from(shard_id)).expect("capacity");
        assert_eq!(h.shard(), shard_id);
        handles.lock().unwrap().push(h);
    });
    assert_eq!(region.len(), workers);

    // The main thread never bound a shard: every remove is remote-evict.
    let hs = handles.lock().unwrap().clone();
    let mut removed = 0usize;
    for h in &hs {
        if region.remove(*h) {
            removed += 1;
        }
        // Second remove is a no-op (I2).
        assert!(!region.remove(*h), "double remove must be a no-op false");
    }
    assert_eq!(removed, workers, "every worker's value was live and removable");
    assert_eq!(region.len(), 0, "all removed → region empty");
}
