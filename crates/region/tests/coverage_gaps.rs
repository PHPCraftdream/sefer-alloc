//! Coverage gap tests for `sefer-region`: I5 drop-once, clear() happy path,
//! and basic coverage for iter/iter_mut/get_mut/Default/capacity/with_capacity/reserve.

use sefer_region::{Region, TryReserveError};
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
            let _ = r.insert(counter);
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
                let _ = sr.insert(counter);
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

    // Capture capacity IMMEDIATELY after with_capacity, before any insert.
    let initial_cap = r.capacity();

    assert!(r.is_empty());
    assert_eq!(r.len(), 0);
    assert!(
        initial_cap >= n,
        "capacity should be at least requested {} (got {})",
        n,
        initial_cap
    );

    // Can insert up to n values without reallocation (capacity should not
    // decrease during inserts, and should not increase if n <= initial_cap).
    let caps: Vec<usize> = (0..n)
        .map(|_| {
            let _ = r.insert(0);
            r.capacity()
        })
        .collect();

    // Capacity should never drop below the initial capacity during inserts.
    assert!(caps.iter().all(|&c| c >= initial_cap));

    // Since n <= initial_cap, we should NOT have reallocated.
    assert!(caps.iter().all(|&c| c == initial_cap));
}

#[test]
fn region_reserve() {
    // Region<T>::reserve(n) increases capacity appropriately.
    let mut r: Region<i32> = Region::new();
    let initial_cap = r.capacity();

    // Reserve additional space.
    let additional = 10;
    r.reserve(additional);

    // Capture capacity IMMEDIATELY after reserve, before any insert.
    let reserved_cap = r.capacity();

    assert!(
        reserved_cap >= initial_cap + additional,
        "capacity should increase by at least {} (was {}, now {})",
        additional,
        initial_cap,
        reserved_cap
    );

    // Can insert up to `additional` values without further reserve.
    for _ in 0..additional {
        let _ = r.insert(0);
    }
    assert_eq!(r.len(), additional);

    // Verify capacity didn't change during inserts (since we reserved enough).
    assert_eq!(r.capacity(), reserved_cap);
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
        let _ = r.insert(i);
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
    let _ = r.insert(0); // len() == 1 — required to reach the guard (see F1 review)
    let msg = catch_panic_message(AssertUnwindSafe(|| {
        r.reserve(usize::MAX);
    }));
    assert!(
        msg.contains("Region::reserve: capacity overflow"),
        "expected the crate's own guard message, got: {msg:?}"
    );
}

#[test]
fn region_reserve_domain_limit_panic_names_reserve_not_with_capacity() {
    // Region::reserve()'s domain-limit branch (target > SLOTMAP_MAX_LIVE, the
    // CapacityExceeded variant -- distinct from the checked_add overflow
    // branch region_reserve_overflow_panics exercises above) shares its
    // TryReserveError::CapacityExceeded variant -- and therefore its Display
    // text -- with Region::with_capacity's identical domain check. The
    // panic message must correctly say "Region::reserve:", not
    // "Region::with_capacity:", even though both panic wrappers format the
    // same underlying error variant. Task #825 found and fixed a real bug
    // here: TryReserveError::CapacityExceeded's Display impl originally
    // hardcoded the "Region::with_capacity:" prefix unconditionally, so a
    // reserve() call hitting this exact branch would have panicked with a
    // message naming the wrong method.
    const SLOTMAP_MAX_LIVE: usize = ((1u64 << 32) - 2) as usize;
    let mut r: Region<i32> = Region::new();
    let msg = catch_panic_message(AssertUnwindSafe(|| {
        r.reserve(SLOTMAP_MAX_LIVE + 1);
    }));
    assert!(
        msg.contains("Region::reserve: capacity") && msg.contains("exceeds slotmap limit"),
        "expected the reserve-specific guard message, got: {msg:?}"
    );
    assert!(
        !msg.contains("with_capacity"),
        "reserve()'s panic message must not name with_capacity, got: {msg:?}"
    );
}

#[test]
fn region_with_capacity_overflow_panics() {
    // Region::with_capacity(usize::MAX) panics in both debug and release
    // builds (profile-independent). Since task #791/F13, the FIRST (and, on
    // every currently supported target, the ONLY reachable) guard hit is the
    // SlotMap live-entry domain check (max 2^32 - 2 live entries, so max
    // reserve is 2^32 - 3) -- usize::MAX vastly exceeds that limit, so this
    // guard fires before the older checked_add(1)-overflow guard ever gets a
    // chance to. That older guard is dead code on both 32-bit and 64-bit
    // targets today: SLOTMAP_MAX_RESERVE (2^32 - 3) is a small, fixed
    // constant, not `usize::MAX - 3` -- on 32-bit, `usize::MAX` is
    // `2^32 - 1`, so `SLOTMAP_MAX_RESERVE + 1` (= 2^32 - 2) still sits below
    // it with room to spare; on 64-bit there is even more room. It stays as
    // defense-in-depth (see region.rs's own comment at the checked_add call
    // site), not as a guard this test can currently exercise, in case a
    // future slotmap version changes the domain constant.
    let msg = catch_panic_message(AssertUnwindSafe(|| {
        let _r: Region<i32> = Region::with_capacity(usize::MAX);
    }));
    assert!(
        msg.contains("Region::with_capacity: capacity") && msg.contains("exceeds slotmap limit"),
        "expected the crate's own domain-limit guard message, got: {msg:?}"
    );
}

// === Fallible variant tests (task #825) ===

#[test]
fn region_try_new_succeeds_when_counter_not_exhausted() {
    // Region::try_new() returns Ok when the region_id counter is not exhausted.
    let result = Region::<i32>::try_new();
    assert!(result.is_ok(), "expected Ok when counter not exhausted");

    let r = result.unwrap();
    assert_eq!(r.len(), 0);
    assert!(r.is_empty());
}

#[test]
fn region_try_with_capacity_succeeds_for_valid_capacity() {
    // Region::try_with_capacity(n) returns Ok for valid capacities.
    let result = Region::<i32>::try_with_capacity(100);
    assert!(result.is_ok(), "expected Ok for valid capacity");

    let r = result.unwrap();
    assert!(r.capacity() >= 100);
}

#[test]
fn region_try_with_capacity_returns_capacity_exceeded_error() {
    // Region::try_with_capacity(usize::MAX) returns Err(CapacityExceeded { .. })
    // instead of panicking.
    const SLOTMAP_MAX_RESERVE: usize = ((1u64 << 32) - 3) as usize;
    let result = Region::<i32>::try_with_capacity(usize::MAX);

    assert!(result.is_err(), "expected Err for usize::MAX");
    let err = result.unwrap_err();
    match err {
        TryReserveError::CapacityExceeded { requested, limit } => {
            assert_eq!(requested, usize::MAX);
            assert_eq!(limit, SLOTMAP_MAX_RESERVE);
        }
        _ => panic!("expected CapacityExceeded variant, got: {:?}", err),
    }
}

#[test]
fn region_try_reserve_succeeds_for_valid_additional() {
    // Region::try_reserve(n) returns Ok for valid additional capacity.
    let mut r: Region<i32> = Region::new();
    let result = r.try_reserve(100);
    assert!(result.is_ok(), "expected Ok for valid additional");

    assert!(r.capacity() >= 100);
}

#[test]
fn region_try_reserve_returns_capacity_exceeded_error() {
    // Region::try_reserve() returns Err(CapacityExceeded { .. })
    // when len() + additional exceeds slotmap's limit.
    const SLOTMAP_MAX_LIVE: usize = ((1u64 << 32) - 2) as usize;

    let mut r: Region<i32> = Region::new();
    // We can't actually insert SLOTMAP_MAX_LIVE values, but we can
    // verify the error path by asking for more than the limit minus 1.
    let additional = SLOTMAP_MAX_LIVE + 1;
    let result = r.try_reserve(additional);

    assert!(result.is_err(), "expected Err for capacity exceeding limit");
    let err = result.unwrap_err();
    match err {
        TryReserveError::CapacityExceeded { requested, limit } => {
            assert_eq!(requested, SLOTMAP_MAX_LIVE + 1);
            assert_eq!(limit, SLOTMAP_MAX_LIVE);
        }
        _ => panic!("expected CapacityExceeded variant, got: {:?}", err),
    }
}

#[test]
fn region_try_reserve_returns_overflow_error() {
    // Region::try_reserve() returns Err(Overflow) when len() + additional would overflow.
    let mut r: Region<i32> = Region::new();
    let _ = r.insert(0); // len() == 1 — required to reach the overflow guard

    let result = r.try_reserve(usize::MAX);

    assert!(result.is_err(), "expected Err for overflow");
    let err = result.unwrap_err();
    assert!(
        matches!(err, TryReserveError::Overflow),
        "expected Overflow variant, got: {:?}",
        err
    );
}

/// Runs `f`, which is expected to panic, and returns the formatted panic
/// message text -- so a test can pin the SPECIFIC panic it expects instead of
/// accepting any panic whatsoever (an unrelated panic silently satisfying
/// `result.is_err()` is exactly the vacuous-test shape this helper exists to
/// close off). Directly downcasts the `catch_unwind` payload to `&str` or `String`
/// to avoid process-global panic hook races with other intentional-panic tests
/// in the same process (F18 fix).
///
/// Panics (test-harness-level, not the caught panic) if `f` does not panic.
fn catch_panic_message<F: FnOnce() + std::panic::UnwindSafe>(f: F) -> String {
    use std::panic;

    let result = panic::catch_unwind(f);
    assert!(
        result.is_err(),
        "expected the closure to panic, but it did not"
    );

    // Downcast the panic payload directly to avoid process-global hook races.
    match result {
        Err(payload) => {
            // Try as &str first (most common case).
            if let Some(s) = payload.downcast_ref::<&str>() {
                return (*s).to_string();
            }
            // Try as String next.
            if let Some(s) = payload.downcast_ref::<String>() {
                return s.clone();
            }
            // Fallback to Display formatting.
            if let Some(s) = payload.downcast_ref::<Box<dyn std::fmt::Display>>() {
                return format!("{s}");
            }
            // Last resort: give up and return a placeholder.
            "<unextractable panic payload>".to_string()
        }
        Ok(_) => unreachable!(), // We already checked is_err()
    }
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
    use std::sync::{Barrier, Mutex};
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

    #[test]
    fn sync_region_deterministic_shared_handle_races() {
        // F16 fix: use Barrier to force simultaneous access, and add deterministic
        // shared-handle scenarios: two concurrent `remove(h)` → exactly one winner;
        // `get_cloned(h)` racing remove → only old value OR None.
        //
        // Both racing threads MUST be spawned inside the SAME `thread::scope`
        // call: `Barrier::new(2)` needs two waiters to unblock, and
        // `thread::scope` blocks the calling thread until every thread
        // spawned INSIDE that one call finishes. Two separate sequential
        // `thread::scope` calls, each spawning only one waiter, would hang
        // forever on the first call -- there is no second waiter until the
        // (never-reached) second call starts.
        const THREADS: usize = 2;
        let sr: SyncRegion<u64> = SyncRegion::new();

        // Insert a value and race two threads on remove(h).
        let handle = sr.insert(42u64);
        let winner_count = AtomicUsize::new(0);
        let barrier = Barrier::new(THREADS);

        thread::scope(|s| {
            for _ in 0..THREADS {
                s.spawn(|| {
                    barrier.wait();
                    if sr.remove(handle).is_some() {
                        winner_count.fetch_add(1, Ordering::SeqCst);
                    }
                });
            }
        });

        assert_eq!(
            winner_count.load(Ordering::SeqCst),
            1,
            "exactly one of two concurrent remove(h) calls should win"
        );
        assert_eq!(sr.len(), 0, "handle should be removed by winner");
        assert!(
            sr.get_cloned(handle).is_none(),
            "removed handle should not resolve"
        );

        // Test get_cloned(h) racing remove → only old value OR None.
        let handle2 = sr.insert(99u64);
        let barrier2 = Barrier::new(THREADS);
        let get_clone_result: Mutex<Option<Option<u64>>> = Mutex::new(None);

        thread::scope(|s| {
            s.spawn(|| {
                barrier2.wait();
                let result = sr.get_cloned(handle2);
                *get_clone_result.lock().unwrap() = Some(result);
            });
            s.spawn(|| {
                barrier2.wait();
                let _ = sr.remove(handle2);
            });
        });

        let result = get_clone_result.lock().unwrap().take().unwrap();
        // get_cloned(h) racing remove must see either the old value or None.
        // It cannot see some new unrelated value (impossible here).
        assert!(
            result == Some(99) || result.is_none(),
            "get_cloned(h) racing remove must see either old value or None, got {:?}",
            result
        );
    }
}
