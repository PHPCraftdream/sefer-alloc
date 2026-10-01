#![allow(deprecated)]
//! R11 P4-3: a thread's shard binding is per region instance, not a single
//! process-global id. Witness is by PATH (shard id + remote-free queue length),
//! not by timing.

#![cfg(feature = "experimental")]

use std::sync::{Barrier, Mutex};
use std::thread::scope;

use sefer_alloc::{ShardedHandle, ShardedRegion};

fn shard_of<T>(h: ShardedHandle<T>) -> u16 {
    ShardedRegion::<T>::split_handle(h).0
}

fn queue_len(r: &ShardedRegion<u32>, shard: u16) -> usize {
    r._remote_free_queue_buffer_identity_for_tests(shard)
        .expect("shard in range")
        .1
}

/// T1 binds shard 0 of region A; T2 exclusively holds shard 0 of region B.
/// T1's insert into B must claim the FREE shard 1 (not reuse the id 0 cached
/// from A), and T1's removal of T2's B/0 handle must take the remote path.
#[test]
fn binding_in_one_region_is_not_trusted_in_another() {
    let a: ShardedRegion<u32> = ShardedRegion::with_shards(2, 4);
    let b: ShardedRegion<u32> = ShardedRegion::with_shards(2, 4);
    let barrier = Barrier::new(2);
    let t2_handle: Mutex<Option<ShardedHandle<u32>>> = Mutex::new(None);

    // Observations are collected and asserted AFTER the scope, so a failing
    // observation cannot strand T2 on a barrier.
    let (a_shard, t2_shard, b_shard, remote_delta, remove_ok, own_queue) = scope(|s| {
        let t1 = s.spawn(|| {
            let a_shard = shard_of(a.insert(1).expect("insert A"));
            barrier.wait(); // A bound
            barrier.wait(); // T2 holds B/0
            let h2 = t2_handle.lock().unwrap().take().expect("t2 handle");
            let t2_shard = shard_of(h2);

            let hb = b.insert(2).expect("insert B");
            let b_shard = shard_of(hb);

            // Remote path: the freed index lands in B/0's remote-free queue.
            let before = queue_len(&b, 0);
            let remove_ok = b.remove(h2);
            let remote_delta = queue_len(&b, 0) - before;
            // Own B handle: owner path pushes nothing on its own queue.
            let own_ok = b.remove(hb);
            let own_queue = queue_len(&b, b_shard) + usize::from(!own_ok);
            barrier.wait(); // release T2
            (
                a_shard,
                t2_shard,
                b_shard,
                remote_delta,
                remove_ok,
                own_queue,
            )
        });
        s.spawn(|| {
            barrier.wait(); // A bound by T1
            let h = b.insert(3).expect("insert B");
            *t2_handle.lock().unwrap() = Some(h);
            barrier.wait(); // publish
            barrier.wait(); // keep B/0 claimed until T1 is done
        });
        t1.join().unwrap()
    });

    assert_eq!(a_shard, 0, "T1 binds A/0");
    assert_eq!(t2_shard, 0, "T2 holds B/0");
    assert_eq!(
        b_shard, 1,
        "T1 must claim free B/1, not share T2's B/0 via the id cached from A"
    );
    assert!(remove_ok);
    assert_eq!(
        remote_delta, 1,
        "removing a B/0 handle from T1 must be remote"
    );
    assert_eq!(
        own_queue, 0,
        "removing T1's own B/1 handle must be owner path"
    );
}

/// More live regions than the per-thread table remembers: every insert still
/// succeeds and every handle resolves (eviction is a throughput loss only).
#[test]
fn binding_table_eviction_stays_correct() {
    let regions: Vec<ShardedRegion<usize>> =
        (0..12).map(|_| ShardedRegion::with_shards(2, 4)).collect();
    std::thread::spawn(move || {
        for round in 0..3 {
            for (i, r) in regions.iter().enumerate() {
                let h = r.insert(i + round).expect("insert");
                assert_eq!(r.get_cloned(h), Some(i + round));
                assert!(r.remove(h));
                assert!(!r.remove(h));
            }
        }
    })
    .join()
    .unwrap();
}
