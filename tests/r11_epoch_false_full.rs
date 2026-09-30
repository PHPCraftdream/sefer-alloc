#![allow(deprecated)]
#![cfg(all(
    feature = "experimental",
    feature = "internals",
    feature = "bench-internals"
))]

use std::sync::{mpsc, Arc};
use std::thread;

use sefer_alloc::ShardedRegion;

#[test]
fn completed_remote_removal_reuses_capacity_one_with_false_hint() {
    let region = Arc::new(ShardedRegion::<u32>::with_shards(1, 1));
    let old = region.insert(11).expect("owner claims the sole slot");
    assert_eq!(old.shard(), 0);
    assert_eq!(
        region.insert(12),
        Err(12),
        "occupied shard is genuinely full"
    );

    thread::scope(|scope| {
        let (done_tx, done_rx) = mpsc::channel();
        let remote_region = Arc::clone(&region);
        let remover = scope.spawn(move || {
            assert!(remote_region.remove(old));
            done_tx.send(()).expect("owner receives completion");
        });
        done_rx.recv().expect("remote removal completed");
        // The channel's synchronization would mask a naturally stale hint;
        // override it before insertion to test the negative-hint branch.
        assert_eq!(region.len(), 0);
        assert_eq!(
            region
                ._remote_free_queue_buffer_identity_for_tests(0)
                .unwrap()
                .1,
            1
        );
        assert!(region._set_remote_free_hint_for_tests(0, false));
        let fresh = region
            .insert(22)
            .expect("empty free list must inspect queue");
        assert_eq!(region.get_cloned(fresh), Some(22));
        assert_eq!(region.get_cloned(old), None);
        assert_eq!(region.len(), 1);
        assert_eq!(
            region
                ._remote_free_queue_buffer_identity_for_tests(0)
                .unwrap()
                .1,
            0
        );
        remover.join().expect("remote remover completed");
    });
}

#[test]
fn empty_queue_and_spurious_positive_hint_do_not_create_capacity() {
    let region = ShardedRegion::<u32>::with_shards(1, 1);
    let handle = region.insert(1).expect("one slot");
    assert!(region._set_remote_free_hint_for_tests(0, true));
    assert_eq!(region.insert(2), Err(2));
    assert_eq!(region.len(), 1);
    assert_eq!(region.get_cloned(handle), Some(1));
    assert!(!region._set_remote_free_hint_for_tests(1, false));
}
