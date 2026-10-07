#![allow(deprecated)]
//! R14-04: a late TLS destructor must not orphan a shard token or panic.
//!
//! Reproduction (deterministic by thread-local initialization order — no
//! sleeps): the worker initializes its `LATE_*` TLS object FIRST, then uses
//! the router (which initializes `MY_SHARDS`/`ERASED_GUARD` later). Thread
//! destructors run in reverse initialization order, so at `LATE_*::drop`
//! time the router TLS is already destroyed while an `Arc<ShardedRegion>`
//! (installed in a process static) is still usable.
//!
//! Before the fix, the late `insert` won the shard-0 CAS and THEN panicked
//! with `AccessError` on the destroyed `ERASED_GUARD`, leaving shard 0
//! occupied with no releasing guard; the next writer skipped to shard 1.
//! The fix probes both router cells fallibly BEFORE any exclusive CAS:
//!
//! - a late `insert` degrades to modulo sharing — it succeeds, occupies no
//!   token, and its handle resolves and removes normally;
//! - a late `bind_current_thread_to_shard` returns `false` — no binding is
//!   promised that teardown could not keep, and no token is taken;
//! - the exiting thread's own exit-release is unaffected: after the join a
//!   fresh writer's exclusive claim gets shard 0 again.
//!
//! Every destructor body runs under `catch_unwind` and only RECORDS the
//! outcome (a panic inside a TLS destructor would abort the process), and
//! every worker is joined with `expect` before any assertion — a panicking
//! worker can never strand the test or another thread.

#![cfg(feature = "experimental")]

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::thread;

use sefer_alloc::{ShardedHandle, ShardedRegion};

/// Relaxed is fine: each static is written by exactly one deterministic
/// site and read only after the owning thread is joined.
use Ordering::Relaxed;

fn shard_of<T>(h: ShardedHandle<T>) -> u16 {
    ShardedRegion::<T>::split_handle(h).0
}

// --- Scenario 1: late-destructor `insert` --------------------------------

static REGION_INSERT: LazyLock<Arc<ShardedRegion<u64>>> =
    LazyLock::new(|| Arc::new(ShardedRegion::with_shards(2, 8)));
static LATE_INSERT_PANICKED: AtomicBool = AtomicBool::new(false);
static LATE_INSERT_SHARD: AtomicUsize = AtomicUsize::new(usize::MAX);
static LATE_HANDLE: Mutex<Option<ShardedHandle<u64>>> = Mutex::new(None);

fn insert_region() -> &'static Arc<ShardedRegion<u64>> {
    &REGION_INSERT
}

struct LateInserter;
impl Drop for LateInserter {
    fn drop(&mut self) {
        let region = insert_region();
        // R14-04: must NOT panic (the old AccessError after the CAS); a
        // panic in a TLS destructor would abort, so record instead.
        match catch_unwind(AssertUnwindSafe(|| region.insert(2))) {
            Err(_) => LATE_INSERT_PANICKED.store(true, Relaxed),
            Ok(Err(_)) => LATE_INSERT_PANICKED.store(true, Relaxed),
            Ok(Ok(h)) => {
                let shard = usize::from(shard_of(h));
                LATE_INSERT_SHARD.store(shard, Relaxed);
                *LATE_HANDLE.lock().unwrap_or_else(|p| p.into_inner()) = Some(h);
            }
        }
    }
}

thread_local! {
    static LATE_INSERTER: LateInserter = const { LateInserter };
}

#[test]
fn late_tls_destructor_insert_shares_without_orphan_token() {
    let worker = thread::spawn(|| {
        // Initialize BEFORE any router TLS use: destroyed AFTER it.
        LATE_INSERTER.with(|_| ());
        let region = insert_region();
        let h = region.insert(1).expect("worker insert");
        assert_eq!(shard_of(h), 0, "worker's exclusive claim takes shard 0");
        assert!(region.remove(h));
    });
    worker
        .join()
        .expect("worker thread must not panic (old code panicked in teardown)");

    assert!(
        !LATE_INSERT_PANICKED.load(Relaxed),
        "late-destructor insert must succeed without an AccessError panic"
    );
    assert_eq!(
        LATE_INSERT_SHARD.load(Relaxed),
        0,
        "late insert must modulo-share shard 0, not win a fresh exclusive \
         claim"
    );
    // The late-inserted handle stays LIVE past the join: the parent thread
    // resolves and removes it (cross-thread reads never depend on shard
    // ownership).
    let late_handle = LATE_HANDLE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
        .expect("late destructor must hand off the late handle");
    assert_eq!(
        insert_region().get_cloned(late_handle),
        Some(2),
        "late handle must resolve after the join"
    );
    assert!(insert_region().remove(late_handle));

    // After the join a fresh writer must reclaim shard 0: the late insert
    // occupied nothing and the exit release was preserved.
    let h = insert_region().insert(3).expect("post-join insert");
    assert_eq!(shard_of(h), 0, "shard 0 free after exit + late insert");
    assert!(insert_region().remove(h));
}

// --- Scenario 2: late-destructor explicit bind ----------------------------

static REGION_BIND: LazyLock<Arc<ShardedRegion<u64>>> =
    LazyLock::new(|| Arc::new(ShardedRegion::with_shards(2, 8)));
static LATE_BIND_ACCEPTED: AtomicBool = AtomicBool::new(false);
static LATE_BIND_PANICKED: AtomicBool = AtomicBool::new(false);

fn bind_region() -> &'static Arc<ShardedRegion<u64>> {
    &REGION_BIND
}

struct LateBinder;
impl Drop for LateBinder {
    fn drop(&mut self) {
        let region = bind_region();
        // R14-04: must return false (no binding teardown could keep) and
        // must NOT occupy shard 0's token or panic.
        match catch_unwind(AssertUnwindSafe(|| region.bind_current_thread_to_shard(0))) {
            Err(_) => LATE_BIND_PANICKED.store(true, Relaxed),
            Ok(accepted) => LATE_BIND_ACCEPTED.store(accepted, Relaxed),
        }
    }
}

thread_local! {
    static LATE_BINDER: LateBinder = const { LateBinder };
}

#[test]
fn late_tls_destructor_bind_is_refused_without_token() {
    let worker = thread::spawn(|| {
        LATE_BINDER.with(|_| ());
        let region = bind_region();
        let h = region.insert(1).expect("worker insert");
        assert_eq!(shard_of(h), 0);
        assert!(region.remove(h));
    });
    worker
        .join()
        .expect("worker thread must not panic in teardown");

    assert!(
        !LATE_BIND_PANICKED.load(Relaxed),
        "late bind must not panic"
    );
    assert!(
        !LATE_BIND_ACCEPTED.load(Relaxed),
        "explicit bind during TLS teardown must return false"
    );
    let h = bind_region().insert(3).expect("post-join insert");
    assert_eq!(
        shard_of(h),
        0,
        "no token may be left held by the refused late bind"
    );
    assert!(bind_region().remove(h));
}

// --- Positive control: bind on a LIVE thread still binds ------------------

static REGION_LIVE_BIND: LazyLock<Arc<ShardedRegion<u64>>> =
    LazyLock::new(|| Arc::new(ShardedRegion::with_shards(3, 8)));

fn live_bind_region() -> &'static Arc<ShardedRegion<u64>> {
    &REGION_LIVE_BIND
}

#[test]
fn bind_on_live_thread_still_binds_and_routes() {
    let worker = thread::spawn(|| {
        let region = live_bind_region();
        assert!(
            region.bind_current_thread_to_shard(0),
            "bind on a live thread with intact router TLS"
        );
        let h = region.insert(1).expect("bound insert");
        assert_eq!(
            shard_of(h),
            0,
            "insert must route to the explicitly bound shard"
        );
        assert!(region.remove(h));
    });
    worker.join().expect("worker panicked");

    // The BOUND claim (shard 0) is released at exit: a fresh writer reclaims
    // shard 0 — under a lost release it could only fall back to shard 1.
    let h = live_bind_region().insert(7).expect("post-join insert");
    assert_eq!(shard_of(h), 0);
    assert!(live_bind_region().remove(h));
}

// --- Scenario 3 (requires "internals"): guard initialized BEFORE the custom
// LATE TLS --

#[cfg(feature = "internals")]
static REGION_GUARD_ORDER: LazyLock<Arc<ShardedRegion<u64>>> =
    LazyLock::new(|| Arc::new(ShardedRegion::with_shards(2, 8)));
#[cfg(feature = "internals")]
static GO_LATE_PANICKED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "internals")]
static GO_LATE_SHARD: AtomicUsize = AtomicUsize::new(usize::MAX);
#[cfg(feature = "internals")]
static GO_GUARD_CLAIMS_SEEN: AtomicUsize = AtomicUsize::new(usize::MAX);
#[cfg(feature = "internals")]
static GO_BIND_REFUSED: AtomicBool = AtomicBool::new(false);
#[cfg(feature = "internals")]
static GO_LATE_HANDLE: Mutex<Option<ShardedHandle<u64>>> = Mutex::new(None);

#[cfg(feature = "internals")]
fn guard_order_region() -> &'static Arc<ShardedRegion<u64>> {
    &REGION_GUARD_ORDER
}

#[cfg(feature = "internals")]
struct LateGuardOrder;
#[cfg(feature = "internals")]
impl Drop for LateGuardOrder {
    fn drop(&mut self) {
        let region = guard_order_region();
        // ERASED_GUARD is still ALIVE here (destroyed last); MY_SHARDS is
        // already gone. The observer proves the guard is alive.
        GO_GUARD_CLAIMS_SEEN.store(ShardedRegion::<u64>::_tls_claim_count_for_tests(), Relaxed);
        // With the routing cache dead, an explicit bind must be REFUSED even
        // though the guard is alive (no binding the cache could keep).
        match catch_unwind(AssertUnwindSafe(|| region.bind_current_thread_to_shard(0))) {
            Err(_) => GO_LATE_PANICKED.store(true, Relaxed),
            Ok(accepted) => GO_BIND_REFUSED.store(accepted, Relaxed),
        }
        // Ordinary insert must fall back to modulo sharing WITHOUT winning a
        // new exclusive token (the scan path requires the intact router).
        match catch_unwind(AssertUnwindSafe(|| region.insert(4))) {
            Err(_) => GO_LATE_PANICKED.store(true, Relaxed),
            Ok(Err(_)) => GO_LATE_PANICKED.store(true, Relaxed),
            Ok(Ok(h)) => {
                GO_LATE_SHARD.store(usize::from(shard_of(h)), Relaxed);
                *GO_LATE_HANDLE.lock().unwrap_or_else(|p| p.into_inner()) = Some(h);
            }
        }
    }
}

#[cfg(feature = "internals")]
thread_local! {
    static LATE_GUARD_ORDER: LateGuardOrder = const { LateGuardOrder };
}

#[cfg(feature = "internals")]
#[test]
fn guard_alive_after_routing_tls_death_still_falls_back() {
    let worker = thread::spawn(|| {
        // 1st: ERASED_GUARD (the observer's try_with initializes it, empty).
        let _ = ShardedRegion::<u64>::_tls_claim_count_for_tests();
        // 2nd: the custom LATE TLS — destroyed BEFORE the guard at exit.
        LATE_GUARD_ORDER.with(|_| ());
        // 3rd: MY_SHARDS via the first ordinary router use.
        let region = guard_order_region();
        let h = region.insert(1).expect("worker insert");
        assert_eq!(shard_of(h), 0);
        assert!(region.remove(h));
    });
    worker.join().expect("worker must not panic in teardown");

    assert!(
        !GO_LATE_PANICKED.load(Relaxed),
        "late destructor ops must not panic"
    );
    assert_eq!(
        GO_GUARD_CLAIMS_SEEN.load(Relaxed),
        1,
        "the exit guard must still be ALIVE (holding the worker's claim) \
         when the late destructor runs"
    );
    assert!(
        !GO_BIND_REFUSED.load(Relaxed),
        "bind must be refused while the routing cache is dead, even with \
         the guard alive"
    );
    assert_eq!(
        GO_LATE_SHARD.load(Relaxed),
        0,
        "late insert must modulo-share shard 0 without a new exclusive token"
    );
    let late_handle = GO_LATE_HANDLE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .take()
        .expect("late handle handed off");
    assert_eq!(guard_order_region().get_cloned(late_handle), Some(4));
    assert!(guard_order_region().remove(late_handle));

    // No token was occupied by the late ops: a fresh writer reclaims 0.
    let h = guard_order_region().insert(5).expect("post-join insert");
    assert_eq!(shard_of(h), 0);
    assert!(guard_order_region().remove(h));
}
