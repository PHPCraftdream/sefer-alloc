//! Item 186 (R6-CC-8.3) shadow model: route pin acquisition under the shard
//! lock versus unlink and last-pin release. Mirrors
//! `src/registry/segment_route/directory/route_directory.rs`: `lookup` checks index membership
//! and increments `refs` under the shard lock (`EntryHandle::pin_from_array`),
//! `remove` unlinks under the same lock and the caller drops the owner
//! reference only after the lock is released, and `EntryHandle::drop` frees
//! the sidecar only on the final count. The shard lock is modeled by
//! `loom::sync::Mutex` (same mutual exclusion as `ShardLock`); index
//! membership is one flag; `refs` keeps the production AcqRel orderings.
#![cfg(loom)]
#![forbid(unsafe_code)]

use loom::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use loom::sync::{Arc, Mutex};
use loom::thread;

struct Route {
    lock: Mutex<()>,
    linked: AtomicBool,
    refs: AtomicUsize,
    /// Set when the sidecar would be freed (the final count drop).
    freed: AtomicBool,
}

impl Route {
    fn new() -> Self {
        Self {
            lock: Mutex::new(()),
            linked: AtomicBool::new(true),
            refs: AtomicUsize::new(1),
            freed: AtomicBool::new(false),
        }
    }

    /// Production `lookup`/`pin_from_array` shape: the membership check, the
    /// count increment, and the post-increment self-check all hold the shard
    /// lock, so the self-check cannot observe a count that already dropped.
    fn pin(&self, resurrected: &AtomicBool) -> bool {
        let _guard = self.lock.lock().unwrap();
        if !self.linked.load(Ordering::Acquire) {
            return false;
        }
        self.refs.fetch_add(1, Ordering::AcqRel);
        if self.freed.load(Ordering::Acquire) {
            resurrected.store(true, Ordering::Release);
        }
        true
    }

    /// Production `EntryHandle::drop`: the final count frees the route.
    fn unpin(&self) {
        if self.refs.fetch_sub(1, Ordering::AcqRel) == 1 {
            self.freed.store(true, Ordering::Release);
        }
    }

    /// Production `remove` + owner release: unlink under the shard lock; the
    /// owner count drops only after the lock is released.
    fn unlink_and_release_owner(&self) {
        {
            let _guard = self.lock.lock().unwrap();
            self.linked.store(false, Ordering::Release);
        }
        self.unpin();
    }
}

fn bounded(f: impl Fn() + Send + Sync + 'static) {
    let mut builder = loom::model::Builder::new();
    builder.preemption_bound = Some(2);
    builder.max_branches = 2000;
    builder.check(f);
}

#[test]
fn route_pin_under_lock_never_resurrects_unlinked_route() {
    bounded(|| {
        let route = Arc::new(Route::new());
        let resurrected = Arc::new(AtomicBool::new(false));
        let a = Arc::clone(&route);
        let flag_a = Arc::clone(&resurrected);
        let pin_a = thread::spawn(move || {
            let pinned = a.pin(&flag_a);
            if pinned {
                a.unpin();
            }
            pinned
        });
        let b = Arc::clone(&route);
        let flag_b = Arc::clone(&resurrected);
        let pin_b = thread::spawn(move || {
            let pinned = b.pin(&flag_b);
            if pinned {
                b.unpin();
            }
            pinned
        });
        let u = Arc::clone(&route);
        let unlink = thread::spawn(move || u.unlink_and_release_owner());
        let got_a = pin_a.join().unwrap();
        let got_b = pin_b.join().unwrap();
        unlink.join().unwrap();
        // Every count returned: the final one must have freed the route, and
        // a pin acquired under the lock never outlives its count. Both tests
        // share the resurrection oracle: the locked path makes the post-
        // increment self-check sound, the lock-free mutant races it.
        assert!(
            !resurrected.load(Ordering::Acquire),
            "pin resurrected a freed route"
        );
        assert!(
            route.freed.load(Ordering::Acquire),
            "last count frees the route"
        );
        assert_eq!(route.refs.load(Ordering::Acquire), 0, "counts balance");
        let _ = (got_a, got_b);
    });
}

#[test]
#[should_panic(expected = "pin resurrected a freed route")]
fn negative_pin_without_lock_resurrects_freed_route() {
    bounded(|| {
        let route = Arc::new(Route::new());
        let resurrected = Arc::new(AtomicBool::new(false));
        let r = Arc::clone(&route);
        let flag = Arc::clone(&resurrected);
        let racer = thread::spawn(move || {
            // MUTANT: the membership check runs OUTSIDE the shard lock, so
            // the increment can land after unlink's final owner release —
            // the classic lock-free refcounted-get race.
            if !r.linked.load(Ordering::Acquire) {
                return;
            }
            r.refs.fetch_add(1, Ordering::AcqRel);
            if r.freed.load(Ordering::Acquire) {
                flag.store(true, Ordering::Release);
            }
            r.unpin();
        });
        let u = Arc::clone(&route);
        let unlink = thread::spawn(move || u.unlink_and_release_owner());
        racer.join().unwrap();
        unlink.join().unwrap();
        assert!(
            !resurrected.load(Ordering::Acquire),
            "pin resurrected a freed route"
        );
    });
}
