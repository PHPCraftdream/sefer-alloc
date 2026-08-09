//! Coverage gap tests for `sefer-region`: I5 drop-once, clear() happy path,
//! and basic coverage for iter/iter_mut/get_mut/Default/capacity/with_capacity/reserve.

use sefer_region::Region;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// ── Drop-counting wrapper (simple, no panic behavior) ─────────────────────

#[derive(Debug, Clone)]
struct DropCounter {
    id: usize,
    drop_count: Arc<AtomicUsize>,
}

impl DropCounter {
    fn new(id: usize, drop_count: Arc<AtomicUsize>) -> Self {
        Self { id, drop_count }
    }
}

impl Drop for DropCounter {
    fn drop(&mut self) {
        self.drop_count.fetch_add(1, Ordering::SeqCst);
    }
}

// ── I5 drop-once tests for Region<T> ───────────────────────────────────────

#[test]
fn region_remove_drop_count_one() {
    // (a) Value removed via Region::remove is dropped exactly once.
    let drop_count = Arc::new(AtomicUsize::new(0));
    let mut r: Region<DropCounter> = Region::new();

    let counter = DropCounter::new(0, Arc::clone(&drop_count));
    let h = r.insert(counter);

    assert_eq!(drop_count.load(Ordering::SeqCst), 0, "no drops yet");

    let removed = r.remove(h).expect("handle should be live");
    assert_eq!(removed.id, 0);

    // The returned Option<T> going out of scope triggers drop.
    drop(removed);

    assert_eq!(
        drop_count.load(Ordering::SeqCst),
        1,
        "removed value should be dropped exactly once"
    );

    // COUNTERFACTUAL: If we leak the value instead, the count stays at 0.
    // Verified by temporarily commenting out `drop(removed)` above and
    // confirming the assertion fails (count stays 0 instead of 1).
    //
    // Also verified by temporarily asserting the wrong count (e.g., 2),
    // which fails — test actually checks what it claims.
}

#[test]
fn region_drop_drops_all_values_once() {
    // (b) When Region holding N live values is dropped, all N values drop exactly once.
    let drop_count = Arc::new(AtomicUsize::new(0));
    let n = 5;

    {
        let mut r: Region<DropCounter> = Region::new();
        for i in 0..n {
            let counter = DropCounter::new(i, Arc::clone(&drop_count));
            r.insert(counter);
        }

        // All values live, none dropped yet.
        assert_eq!(drop_count.load(Ordering::SeqCst), 0);
        assert_eq!(r.len(), n);

        // Region drops here, all N values should drop exactly once.
    }

    assert_eq!(
        drop_count.load(Ordering::SeqCst),
        n,
        "all {} values should be dropped exactly once",
        n
    );

    // COUNTERFACTUAL: Verified by temporarily wrapping Region in
    // `std::mem::forget(r)` and confirming the count stays 0 instead of n.
}

#[test]
fn region_mixed_sequence_drop_count_matches_inserts() {
    // (c) Mixed insert/remove/churn sequence: total drops == total inserts.
    let drop_count = Arc::new(AtomicUsize::new(0));

    let mut r: Region<DropCounter> = Region::new();

    // Insert 3 values.
    let h1 = r.insert(DropCounter::new(0, Arc::clone(&drop_count)));
    let _h2 = r.insert(DropCounter::new(1, Arc::clone(&drop_count)));
    let h3 = r.insert(DropCounter::new(2, Arc::clone(&drop_count)));

    // Remove one (drops once when the returned Option goes out of scope).
    let _removed1 = r.remove(h1).expect("h1 should be live");
    assert_eq!(
        drop_count.load(Ordering::SeqCst),
        0,
        "value not dropped yet - remove() returns it"
    );

    // Insert 2 more values.
    let _h4 = r.insert(DropCounter::new(3, Arc::clone(&drop_count)));
    let _h5 = r.insert(DropCounter::new(4, Arc::clone(&drop_count)));

    // Remove another (drops once when the returned Option goes out of scope).
    let _removed2 = r.remove(h3).expect("h3 should be live");
    assert_eq!(
        drop_count.load(Ordering::SeqCst),
        0,
        "value not dropped yet - remove() returns it"
    );

    // Drop the removed values explicitly (each drops once).
    drop(_removed1);
    drop(_removed2);
    assert_eq!(
        drop_count.load(Ordering::SeqCst),
        2,
        "2 removed values should have dropped"
    );

    // Region drops here, remaining 3 values (_h2, _h4, _h5) should drop once each.
    drop(r);

    assert_eq!(
        drop_count.load(Ordering::SeqCst),
        5,
        "total drops should equal total inserts: 5 inserts = 5 drops (2 via explicit drop, 3 via Region drop)"
    );

    // COUNTERFACTUAL: Verified by temporarily leaking one of the removed
    // values with `std::mem::forget(_removed1)` and confirming count becomes 4
    // instead of 5.
}

// ── I5 drop-once tests for SyncRegion<T> ───────────────────────────────────

#[cfg(feature = "std")]
mod sync_drop_once_tests {
    use super::*;
    use sefer_region::SyncRegion;

    #[test]
    fn sync_region_remove_drop_count_one() {
        // (a) Value removed via SyncRegion::remove is dropped exactly once.
        let drop_count = Arc::new(AtomicUsize::new(0));
        let sr: SyncRegion<DropCounter> = SyncRegion::new();

        let counter = DropCounter::new(0, Arc::clone(&drop_count));
        let h = sr.insert(counter);

        assert_eq!(drop_count.load(Ordering::SeqCst), 0);

        let removed = sr.remove(h).expect("handle should be live");
        assert_eq!(removed.id, 0);

        // The returned Option<T> going out of scope triggers drop.
        drop(removed);

        assert_eq!(
            drop_count.load(Ordering::SeqCst),
            1,
            "removed value should be dropped exactly once"
        );

        // COUNTERFACTUAL: Verified by temporarily forgetting the removed value
        // and confirming count stays 0.
    }

    #[test]
    fn sync_region_drop_drops_all_values_once() {
        // (b) When SyncRegion holding N live values is dropped, all N drop once.
        let drop_count = Arc::new(AtomicUsize::new(0));
        let n = 5;

        {
            let sr: SyncRegion<DropCounter> = SyncRegion::new();
            for i in 0..n {
                let counter = DropCounter::new(i, Arc::clone(&drop_count));
                sr.insert(counter);
            }

            assert_eq!(drop_count.load(Ordering::SeqCst), 0);
            assert_eq!(sr.len(), n);

            // SyncRegion drops here.
        }

        assert_eq!(
            drop_count.load(Ordering::SeqCst),
            n,
            "all {} values should be dropped exactly once",
            n
        );

        // COUNTERFACTUAL: Verified by temporarily forgetting the SyncRegion
        // and confirming count stays 0.
    }
}

// ── clear() happy-path tests ───────────────────────────────────────────────

#[test]
fn region_clear_happy_path() {
    // clear() on Region<T> with non-panicking Drop: len/is_empty reset,
    // capacity retained, handles invalidated, region reusable.
    let mut r: Region<String> = Region::new();

    // Insert several values.
    let h1 = r.insert("first".to_string());
    let h2 = r.insert("second".to_string());
    let h3 = r.insert("third".to_string());

    assert_eq!(r.len(), 3);
    assert!(!r.is_empty());

    // Record capacity before clear.
    let cap_before = r.capacity();
    assert!(cap_before >= 3);

    // Clear the region.
    r.clear();

    // len/is_empty reset.
    assert_eq!(r.len(), 0, "len should be 0 after clear");
    assert!(r.is_empty(), "is_empty should be true after clear");

    // Capacity retained (not dropped to zero).
    assert_eq!(
        r.capacity(),
        cap_before,
        "capacity should be retained after clear"
    );

    // All previously-live handles now resolve None.
    assert!(
        r.get(h1).is_none(),
        "handle h1 should resolve None after clear"
    );
    assert!(
        r.get(h2).is_none(),
        "handle h2 should resolve None after clear"
    );
    assert!(
        r.get(h3).is_none(),
        "handle h3 should resolve None after clear"
    );

    // Region is reusable: insert new values, they resolve correctly.
    let h_new1 = r.insert("new-first".to_string());
    let h_new2 = r.insert("new-second".to_string());

    assert_eq!(r.len(), 2);
    assert_eq!(r.get(h_new1).map(String::as_str), Some("new-first"));
    assert_eq!(r.get(h_new2).map(String::as_str), Some("new-second"));

    // Old handles still resolve None (no ABA/confusion).
    assert!(r.get(h1).is_none());
    assert!(r.get(h2).is_none());
    assert!(r.get(h3).is_none());
}

#[cfg(feature = "std")]
mod sync_clear_tests {
    use sefer_region::SyncRegion;

    #[test]
    fn sync_region_clear_happy_path() {
        // clear() on SyncRegion<T> with non-panicking Drop.
        let sr: SyncRegion<i32> = SyncRegion::new();

        // Insert several values.
        let h1 = sr.insert(10);
        let h2 = sr.insert(20);
        let h3 = sr.insert(30);

        assert_eq!(sr.len(), 3);
        assert!(!sr.is_empty());

        // Record capacity before clear.
        let cap_before = sr.read().capacity();
        assert!(cap_before >= 3);

        // Clear the region.
        sr.clear();

        // len/is_empty reset.
        assert_eq!(sr.len(), 0, "len should be 0 after clear");
        assert!(sr.is_empty(), "is_empty should be true after clear");

        // Capacity retained.
        assert_eq!(
            sr.read().capacity(),
            cap_before,
            "capacity should be retained after clear"
        );

        // All previously-live handles now resolve None.
        assert!(sr.get_cloned(h1).is_none());
        assert!(sr.get_cloned(h2).is_none());
        assert!(sr.get_cloned(h3).is_none());

        // Region is reusable.
        let h_new = sr.insert(100);
        assert_eq!(sr.get_cloned(h_new), Some(100));
        assert_eq!(sr.len(), 1);
    }
}

// ── Cheap coverage for remaining untested surface ─────────────────────────

#[test]
fn region_iter_mut_and_iter() {
    // iter_mut() mutates values, mutations visible via iter() or get().
    let mut r: Region<i32> = Region::new();

    let h1 = r.insert(10);
    let h2 = r.insert(20);
    let h3 = r.insert(30);

    // Mutate through iter_mut().
    for val in r.iter_mut() {
        *val *= 2;
    }

    // Mutations visible via iter().
    let values: Vec<&i32> = r.iter().collect();
    assert_eq!(values.len(), 3);
    assert!(values.contains(&&20)); // 10 * 2
    assert!(values.contains(&&40)); // 20 * 2
    assert!(values.contains(&&60)); // 30 * 2

    // Mutations visible via get().
    assert_eq!(r.get(h1), Some(&20));
    assert_eq!(r.get(h2), Some(&40));
    assert_eq!(r.get(h3), Some(&60));
}

#[test]
fn region_get_mut() {
    // get_mut() allows mutation, visible afterward.
    let mut r: Region<String> = Region::new();

    let h = r.insert("hello".to_string());

    // Mutate through get_mut().
    if let Some(s) = r.get_mut(h) {
        s.push_str(" world");
    }

    // Mutation visible via get().
    assert_eq!(r.get(h).map(String::as_str), Some("hello world"));

    // get_mut on removed handle returns None.
    r.remove(h);
    assert!(r.get_mut(h).is_none());
}

#[test]
fn region_default() {
    // Region<T>: Default implementation creates an empty region.
    let mut r: Region<u32> = Region::default();

    assert!(r.is_empty(), "default Region should be empty");
    assert_eq!(r.len(), 0);
    // capacity() returns usize, which is always >= 0. We just verify it exists.

    // Usable after default construction.
    let h = r.insert(42);
    assert_eq!(r.get(h), Some(&42));
}

#[test]
fn region_with_capacity() {
    // Region<T>::with_capacity(n) allocates at least n slots.
    let n = 10;
    let mut r: Region<i32> = Region::with_capacity(n);

    assert!(r.is_empty());
    assert_eq!(r.len(), 0);
    assert!(
        r.capacity() >= n,
        "capacity should be at least requested {}",
        n
    );

    // Can insert up to n values without reallocation (we don't verify
    // "without reallocation" directly, but capacity shouldn't shrink).
    let caps: Vec<usize> = (0..n)
        .map(|_| {
            r.insert(0);
            r.capacity()
        })
        .collect();

    // Capacity should never drop below the initial capacity during inserts.
    let initial_cap = caps[0];
    assert!(caps.iter().all(|&c| c >= initial_cap));
}

#[test]
fn region_reserve() {
    // Region<T>::reserve(n) increases capacity appropriately.
    let mut r: Region<i32> = Region::new();
    let initial_cap = r.capacity();

    // Reserve additional space.
    let additional = 10;
    r.reserve(additional);

    let new_cap = r.capacity();
    assert!(
        new_cap >= initial_cap + additional,
        "capacity should increase by at least {} (was {}, now {})",
        additional,
        initial_cap,
        new_cap
    );

    // Can insert up to `additional` values without further reserve.
    for _ in 0..additional {
        r.insert(0);
    }
    assert_eq!(r.len(), additional);
}

#[test]
fn region_reserve_reuses_freed_slots_on_churn() {
    // Verify that Region::reserve's documented claim ("After a churn that
    // removes entries, the freed slots live on the free list, so re-inserting
    // reuses existing capacity and does not grow unboundedly") actually holds.
    // This is the same scenario as captrack_probe.rs workload 2, but without
    // the manual/ignored restriction.
    //
    // COUNTERFACTUAL: If Region were incorrectly growing capacity on every
    // insert (e.g., by ignoring the free list and always allocating fresh
    // slots), this test would fail with cap_after_refill > cap_after_remove.
    // Verified by temporarily raising the refill loop below from 500 to 700
    // (over-running the 500 freed slots): the assertion correctly failed
    // (after_remove=1023, after_refill=2047), then the change was reverted.

    let mut r: Region<u64> = Region::new();

    // Insert 1000 values.
    let handles: Vec<_> = (0..1000u64).map(|i| r.insert(i)).collect();

    // Remove every other value (500 freed slots).
    for (i, h) in handles.iter().enumerate() {
        if i % 2 == 0 {
            r.remove(*h);
        }
    }
    let cap_after_remove = r.capacity();

    // Re-insert 500 values — these should reuse the freed slots, not grow.
    for i in 0..500u64 {
        r.insert(i);
    }
    let cap_after_refill = r.capacity();

    assert!(
        cap_after_refill <= cap_after_remove,
        "refilling freed slots should not grow capacity past the post-removal high-water \
         mark: after_remove={cap_after_remove}, after_refill={cap_after_refill}"
    );
}

#[test]
fn region_reserve_overflow_panics() {
    // Region<T>::reserve() panics for genuine capacity-overflow arguments
    // in both debug and release builds (profile-independent).
    // len() == 1 is required to reach the checked_add guard — on an empty
    // region, 0.checked_add(usize::MAX) is Some(_), so the guard never fires.
    let mut r: Region<i32> = Region::new();
    r.insert(0); // len() == 1 — required to reach the guard (see F1 review)
    let msg = catch_panic_message(AssertUnwindSafe(|| {
        r.reserve(usize::MAX);
    }));
    assert!(
        msg.contains("Region::reserve: capacity overflow"),
        "expected the crate's own guard message, got: {msg:?}"
    );
}

#[test]
fn region_with_capacity_overflow_panics() {
    // Region::with_capacity(usize::MAX) panics in both debug and release builds
    // (profile-independent) -- slotmap reserves one extra slot for an internal
    // sentinel (capacity + 1), so usize::MAX is exactly the one value that would
    // overflow that reservation; guarded up front via checked_add before
    // delegating to slotmap.
    let msg = catch_panic_message(AssertUnwindSafe(|| {
        let _r: Region<i32> = Region::with_capacity(usize::MAX);
    }));
    assert!(
        msg.contains("Region::with_capacity: capacity overflow"),
        "expected the crate's own guard message, got: {msg:?}"
    );
}

/// Serializes calls to [`catch_panic_message`] across concurrently-running
/// test threads, since it temporarily installs a process-global panic hook.
static PANIC_HOOK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Runs `f`, which is expected to panic, and returns the formatted panic
/// message text -- so a test can pin the SPECIFIC panic it expects instead of
/// accepting any panic whatsoever (an unrelated panic silently satisfying
/// `result.is_err()` is exactly the vacuous-test shape this helper exists to
/// close off). Captures the message via a temporary panic hook rather than
/// downcasting the `catch_unwind` payload's `Box<dyn Any>` directly: in this
/// crate's own test binary, a panic payload originating inside the
/// `sefer-region` library crate (a separate compilation unit linked in as a
/// dependency) does not downcast to `&str`/`String` even though the panic
/// macro produces exactly those types -- `TypeId` mismatches across the
/// library/test-binary boundary for reasons not fully root-caused here.
/// The hook-based capture sidesteps that entirely by reading the already-
/// formatted `Display` text instead of downcasting the raw payload.
///
/// Panics (test-harness-level, not the caught panic) if `f` does not panic.
fn catch_panic_message<F: FnOnce() + std::panic::UnwindSafe>(f: F) -> String {
    use std::panic;
    use std::sync::Mutex;

    let _serialize = PANIC_HOOK_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    let captured: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
    let captured_in_hook = Arc::clone(&captured);
    let previous_hook = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        *captured_in_hook.lock().unwrap() = Some(info.to_string());
    }));

    let result = panic::catch_unwind(f);

    panic::set_hook(previous_hook);

    assert!(
        result.is_err(),
        "expected the closure to panic, but it did not"
    );
    let message = captured
        .lock()
        .unwrap()
        .take()
        .expect("panic hook should have captured a message");
    message
}

#[cfg(feature = "std")]
mod sync_misc_tests {
    use sefer_region::SyncRegion;

    #[test]
    fn sync_region_default() {
        // SyncRegion<T>: Default implementation creates an empty region.
        let sr: SyncRegion<u32> = SyncRegion::default();

        assert!(sr.is_empty(), "default SyncRegion should be empty");
        assert_eq!(sr.len(), 0);
        // capacity() returns usize, which is always >= 0. We just verify it exists.

        // Usable after default construction.
        let h = sr.insert(42);
        assert_eq!(sr.get_cloned(h), Some(42));
    }

    #[test]
    fn sync_region_with_capacity() {
        // SyncRegion<T>::with_capacity(n) allocates at least n slots.
        let n = 10;
        let sr: SyncRegion<i32> = SyncRegion::with_capacity(n);

        assert!(sr.is_empty());
        assert_eq!(sr.len(), 0);
        assert!(
            sr.read().capacity() >= n,
            "capacity should be at least requested {}",
            n
        );
    }

    #[test]
    fn sync_region_reserve() {
        // SyncRegion<T>::reserve(n) increases capacity appropriately.
        let sr: SyncRegion<i32> = SyncRegion::new();
        let initial_cap = sr.read().capacity();

        let additional = 10;
        sr.write().reserve(additional);

        let new_cap = sr.read().capacity();
        assert!(
            new_cap >= initial_cap + additional,
            "capacity should increase by at least {} (was {}, now {})",
            additional,
            initial_cap,
            new_cap
        );
    }
}

// ── genuine multi-thread concurrency test for SyncRegion<T> ───────────────

#[cfg(feature = "std")]
mod sync_concurrency_tests {
    use super::*;
    use sefer_region::SyncRegion;
    use std::thread;

    #[test]
    fn sync_region_concurrent_insert_remove_get_is_consistent() {
        // SyncRegion's struct doc claims it is "correct under any interleaving
        // because every mutation serialises through the lock" -- but every
        // OTHER threaded test in this crate spawns exactly ONE thread and
        // joins it BEFORE any assertion runs (sequential-by-construction, not
        // genuinely concurrent). This is the first test with multiple threads
        // actually racing inside the RwLock-wrapped API at the same time.
        // Kept small/fast per this repo's speed rules -- a correctness smoke
        // test, not a perf harness (see examples/contended_reads.rs for the
        // measurement-focused version added by task #685).
        const THREADS: usize = 4;
        const OPS_PER_THREAD: usize = 200;

        let drop_count = Arc::new(AtomicUsize::new(0));
        let sr: Arc<SyncRegion<DropCounter>> = Arc::new(SyncRegion::new());

        thread::scope(|s| {
            for t in 0..THREADS {
                let sr = Arc::clone(&sr);
                let drop_count = Arc::clone(&drop_count);
                s.spawn(move || {
                    for i in 0..OPS_PER_THREAD {
                        let id = t * OPS_PER_THREAD + i;
                        let h = sr.insert(DropCounter::new(id, Arc::clone(&drop_count)));
                        // Exercise reads concurrently with other threads' writes.
                        let _ = sr.contains(h);
                        let cloned = sr.get_cloned(h);
                        assert_eq!(cloned.map(|c| c.id), Some(id));
                        let removed = sr.remove(h);
                        drop(removed);
                    }
                });
            }
        });
        // `thread::scope` blocks until every spawned thread has joined, so
        // this point is only reached after all 4 threads' work has fully
        // interleaved through the shared SyncRegion and completed.

        assert_eq!(sr.len(), 0, "every inserted value was also removed");
        assert_eq!(
            drop_count.load(Ordering::SeqCst),
            THREADS * OPS_PER_THREAD * 2,
            "each iteration produces exactly 2 drops (the get_cloned clone, \
             dropped when it goes out of scope, plus the original after \
             remove) -- any other count means something is double-dropped, \
             leaked, or the clone is missing/duplicated"
        );

        // COUNTERFACTUAL (verified, then reverted): temporarily replaced
        // `drop(removed)` above with `std::mem::forget(removed)`; the
        // assertion correctly failed (left: 800, right: 1600 -- half the
        // expected drops, since only the get_cloned clone was still dropped),
        // confirming the drop-count assertion is not vacuous.
    }
}
