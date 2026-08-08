//! Regression test for clear() partial completion under panicking `T::Drop`.
//!
//! Documents and validates that Region::clear() and SyncRegion::clear() complete
//! partially when a value's Drop panics: values already visited (including the
//! panicking one) are removed and dropped, but later values remain live and
//! correctly accounted. The region stays consistent and reusable — no corruption,
//! no double-drop, no leak.

use sefer_region::{Handle, Region};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

// ── Drop-counting wrapper with optional panic ───────────────────────────────

#[derive(Debug, Clone)]
struct DropCounter {
    id: usize,
    bomb_id: usize, // If id == bomb_id, Drop panics.
    drop_count: Arc<AtomicUsize>,
}

impl DropCounter {
    fn new(id: usize, bomb_id: usize, drop_count: Arc<AtomicUsize>) -> Self {
        Self {
            id,
            bomb_id,
            drop_count,
        }
    }
}

impl Drop for DropCounter {
    fn drop(&mut self) {
        self.drop_count.fetch_add(1, Ordering::SeqCst);

        if self.id == self.bomb_id && !std::thread::panicking() {
            // Only panic if we're not already unwinding. This prevents recursive
            // panics during cleanup while still propagating the first panic.
            panic!("intentional drop panic in DropCounter id={}", self.id);
        }
    }
}

// ── Scenario (a): Region::clear() on main thread ─────────────────────────────

#[test]
fn region_clear_partial_under_panic() {
    let drop_count = Arc::new(AtomicUsize::new(0));

    // Insert 5 values, with the 3rd as the bomb.
    let mut r: Region<DropCounter> = Region::new();
    let bomb_id = 2; // 0-indexed, so this is the 3rd insert
    let mut handles: Vec<Handle<DropCounter>> = Vec::new();

    for i in 0..5 {
        let counter = DropCounter::new(i, bomb_id, Arc::clone(&drop_count));
        handles.push(r.insert(counter));
    }

    // All handles should resolve before clear.
    for (i, &h) in handles.iter().enumerate() {
        assert!(
            r.get(h).is_some(),
            "handle {} should resolve before clear",
            i
        );
    }

    // Clear under panic: the bomb at id=2 should panic, leaving ids 3,4 live.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        r.clear();
    }));

    assert!(
        result.is_err(),
        "clear() should have panicked due to Drop panic"
    );

    // After unwinding: exactly 3 drops occurred, 2 survivors remain.
    // Note: slotmap's drain order is unspecified, so we don't assert WHICH IDs
    // were dropped/survived — only that the total accounting is correct and the
    // bomb (id=2) was definitely dropped.
    assert_eq!(
        drop_count.load(Ordering::SeqCst),
        3,
        "exactly 3 values should have been dropped (order unspecified)"
    );
    assert_eq!(r.len(), 2, "region should have 2 survivors");

    // Collect the set of surviving IDs via iter().
    let survivor_ids: std::collections::HashSet<usize> =
        r.iter().map(|counter| counter.id).collect();

    // The bomb (id=2) should NOT be among survivors — it panicked.
    assert!(
        !survivor_ids.contains(&bomb_id),
        "bomb id {} should have been dropped during partial clear",
        bomb_id
    );

    // Verify that every surviving ID corresponds to a handle that resolves correctly.
    for &survivor_id in &survivor_ids {
        let handle = &handles[survivor_id];
        assert!(
            r.get(*handle).is_some(),
            "survivor id {} should resolve correctly",
            survivor_id
        );
        // Verify the value is actually the right one.
        assert_eq!(r.get(*handle).map(|c| c.id), Some(survivor_id));
    }

    // Verify that dropped IDs (complement of survivors) do not resolve.
    for (i, handle) in handles.iter().enumerate() {
        if !survivor_ids.contains(&i) {
            assert!(
                r.get(*handle).is_none(),
                "dropped id {} should not resolve",
                i
            );
        }
    }

    // Region should be reusable: insert new value, it resolves correctly.
    let new_counter = DropCounter::new(10, 999, Arc::clone(&drop_count));
    let h_new = r.insert(new_counter);
    assert_eq!(r.get(h_new).map(|c| c.id), Some(10));
    assert_eq!(
        r.len(),
        3,
        "region should now have 3 values (2 survivors + 1 new)"
    );

    // Total drops should equal total constructions: 5 initial + 1 new = 6 drops total.
    // But we haven't dropped the Region yet, so only the ones explicitly dropped count.
    // At this point: 3 dropped in partial clear = 3.
    assert_eq!(drop_count.load(Ordering::SeqCst), 3);

    // Drop the region: remaining 3 values (ids 3,4,10) should drop normally.
    drop(r);
    assert_eq!(
        drop_count.load(Ordering::SeqCst),
        6,
        "total drops should equal total constructions (6)"
    );
}

// ── Scenario (b): SyncRegion::clear() on spawned thread ──────────────────────

#[cfg(feature = "std")]
mod sync_tests {
    use super::*;
    use sefer_region::SyncRegion;
    use std::sync::Arc;

    #[test]
    fn sync_region_clear_partial_under_panic() {
        let drop_count = Arc::new(AtomicUsize::new(0));
        let sr: Arc<SyncRegion<DropCounter>> = Arc::new(SyncRegion::new());

        // Insert 5 values, with the 3rd as the bomb.
        let bomb_id = 2; // 0-indexed, so this is the 3rd insert
        let mut handles: Vec<Handle<DropCounter>> = Vec::new();

        for i in 0..5 {
            let counter = DropCounter::new(i, bomb_id, Arc::clone(&drop_count));
            handles.push(sr.insert(counter));
        }

        // All handles should resolve before clear.
        for (i, &h) in handles.iter().enumerate() {
            assert!(
                sr.read().get(h).is_some(),
                "handle {} should resolve before clear",
                i
            );
        }

        // Spawn a thread that clears under panic.
        // Use catch_unwind inside the thread to prevent the panic from propagating
        // to the test harness, while still causing the partial-clear behavior.
        let sr_clone = Arc::clone(&sr);
        let join = std::thread::spawn(move || {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                sr_clone.clear();
            }));
        });

        // Wait for the thread to complete (it will panic internally, but the
        // inner catch_unwind swallows it, so join() itself returns Ok).
        let join_result = join.join();
        assert!(join_result.is_ok(), "thread should have completed");

        // After poison recovery: exactly 3 drops occurred, 2 survivors remain.
        // Note: slotmap's drain order is unspecified, so we don't assert WHICH IDs
        // were dropped/survived — only that the total accounting is correct and the
        // bomb (id=2) was definitely dropped.
        assert_eq!(
            drop_count.load(Ordering::SeqCst),
            3,
            "exactly 3 values should have been dropped (order unspecified)"
        );
        assert_eq!(sr.len(), 2, "SyncRegion should have 2 survivors");

        // Collect the set of surviving IDs via iter().
        let survivor_ids: std::collections::HashSet<usize> =
            sr.read().iter().map(|counter| counter.id).collect();

        // The bomb (id=2) should NOT be among survivors — it panicked.
        assert!(
            !survivor_ids.contains(&bomb_id),
            "bomb id {} should have been dropped during partial clear",
            bomb_id
        );

        // Verify that every surviving ID corresponds to a handle that resolves correctly.
        for &survivor_id in &survivor_ids {
            let handle = &handles[survivor_id];
            assert!(
                sr.read().get(*handle).is_some(),
                "survivor id {} should resolve correctly",
                survivor_id
            );
            // Verify the value is actually the right one.
            assert_eq!(sr.read().get(*handle).map(|c| c.id), Some(survivor_id));
        }

        // Verify that dropped IDs (complement of survivors) do not resolve.
        for (i, handle) in handles.iter().enumerate() {
            if !survivor_ids.contains(&i) {
                assert!(
                    sr.read().get(*handle).is_none(),
                    "dropped id {} should not resolve",
                    i
                );
            }
        }

        // SyncRegion should be reusable: insert new value, it resolves correctly.
        let new_counter = DropCounter::new(10, 999, Arc::clone(&drop_count));
        let h_new = sr.insert(new_counter);
        assert_eq!(sr.read().get(h_new).map(|c| c.id), Some(10));
        assert_eq!(
            sr.len(),
            3,
            "SyncRegion should now have 3 values (2 survivors + 1 new)"
        );

        // A second clear() on the partially-populated region should succeed fully
        // (no bomb among the survivors), leaving it empty.
        sr.clear();
        assert_eq!(sr.len(), 0, "second clear() should empty the region");
        assert_eq!(
            drop_count.load(Ordering::SeqCst),
            6,
            "total drops after second clear: 3 (partial) + 3 (full) = 6"
        );

        // Drop the SyncRegion: no additional values to drop.
        drop(sr);
        assert_eq!(
            drop_count.load(Ordering::SeqCst),
            6,
            "total drops should equal total constructions (6)"
        );
    }
}
