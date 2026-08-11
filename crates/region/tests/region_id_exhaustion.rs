//! Boundary and exhaustion tests for `region_id` counter minting (task #813).
//!
//! These tests call the REAL `try_mint_region_id` helper from `src/region.rs`
//! through its `#[doc(hidden)]` test-only forwarder `dbg_try_mint_region_id`
//! (re-exported at the crate root) — not a hand-copied duplicate — so a
//! regression in the actual production logic fails these tests. Each test
//! constructs its own local `AtomicUsize` counter rather than mutating the
//! process-wide `NEXT_REGION_ID` static, which is shared with every other
//! test in this process.

use core::num::NonZeroUsize;
use core::sync::atomic::AtomicUsize;
use sefer_region::dbg_try_mint_region_id;

#[test]
fn region_id_exhaustion_boundary_max_minus_one() {
    let counter = AtomicUsize::new(usize::MAX - 1);

    // First call: gets MAX - 1
    let id1 = dbg_try_mint_region_id(&counter).expect("should get ID");
    assert_eq!(id1.get(), usize::MAX - 1);

    // Counter should now be at MAX
    assert_eq!(
        counter.load(core::sync::atomic::Ordering::Relaxed),
        usize::MAX
    );

    // Second call: gets MAX (last valid ID)
    let id2 = dbg_try_mint_region_id(&counter).expect("should get ID");
    assert_eq!(id2.get(), usize::MAX);

    // Counter should now be at exhausted sentinel (0)
    assert_eq!(counter.load(core::sync::atomic::Ordering::Relaxed), 0);
}

#[test]
fn region_id_exhaustion_boundary_max() {
    let counter = AtomicUsize::new(usize::MAX);

    // Call when at MAX: gets MAX (last valid ID) and transitions to exhausted
    let id = dbg_try_mint_region_id(&counter).expect("should get ID");
    assert_eq!(id.get(), usize::MAX);

    // Counter should now be at exhausted sentinel (0)
    assert_eq!(counter.load(core::sync::atomic::Ordering::Relaxed), 0);
}

#[test]
fn region_id_exhaustion_permanent_sentinel() {
    let counter = AtomicUsize::new(usize::MAX);

    // Exhaust the counter
    let _id = dbg_try_mint_region_id(&counter).expect("should get ID");
    assert_eq!(counter.load(core::sync::atomic::Ordering::Relaxed), 0);

    // All subsequent calls must fail, forever
    for _ in 0..10 {
        let result = dbg_try_mint_region_id(&counter);
        assert!(result.is_err(), "must fail after exhaustion");
    }

    // Counter stays at 0 forever
    assert_eq!(counter.load(core::sync::atomic::Ordering::Relaxed), 0);
}

#[test]
fn region_id_exhaustion_multiple_calls_all_fail() {
    let counter = AtomicUsize::new(usize::MAX);

    // Exhaust the counter
    let _id = dbg_try_mint_region_id(&counter).expect("should get ID");
    assert_eq!(counter.load(core::sync::atomic::Ordering::Relaxed), 0);

    // Many subsequent calls must all fail
    let failures = (0..100)
        .filter(|_| dbg_try_mint_region_id(&counter).is_err())
        .count();
    assert_eq!(failures, 100, "all 100 calls must fail after exhaustion");
}

#[test]
fn region_id_exhaustion_no_reuse() {
    let counter = AtomicUsize::new(usize::MAX - 3);

    // Collect all IDs before exhaustion
    let mut ids: Vec<NonZeroUsize> = Vec::new();
    for _ in 0..4 {
        let id = dbg_try_mint_region_id(&counter).expect("should get ID");
        ids.push(id);
    }

    // Verify we got exactly MAX - 3, MAX - 2, MAX - 1, MAX (no duplicates)
    let expected = vec![usize::MAX - 3, usize::MAX - 2, usize::MAX - 1, usize::MAX];
    assert_eq!(ids.iter().map(|id| id.get()).collect::<Vec<_>>(), expected);

    // Counter is now exhausted
    assert_eq!(counter.load(core::sync::atomic::Ordering::Relaxed), 0);

    // No call after exhaustion can return any of the previously issued IDs
    for _ in 0..10 {
        let result = dbg_try_mint_region_id(&counter);
        assert!(result.is_err(), "must fail after exhaustion");
    }
}

#[test]
fn region_id_exhaustion_already_at_zero() {
    let counter = AtomicUsize::new(0);

    // Counter already at exhausted sentinel: must fail immediately
    let result = dbg_try_mint_region_id(&counter);
    assert!(result.is_err(), "must fail when already at 0");

    // Counter stays at 0
    assert_eq!(counter.load(core::sync::atomic::Ordering::Relaxed), 0);

    // All subsequent calls also fail
    for _ in 0..5 {
        let result = dbg_try_mint_region_id(&counter);
        assert!(result.is_err(), "must fail when already at 0");
    }
}

#[cfg(feature = "std")]
#[test]
fn region_id_exhaustion_concurrent_threads() {
    use std::collections::HashSet;
    use std::sync::Arc;
    use std::thread;

    // Start counter near MAX to quickly hit exhaustion
    let counter = Arc::new(AtomicUsize::new(usize::MAX - 5));

    let mut handles = Vec::new();
    for _ in 0..8 {
        let counter = Arc::clone(&counter);
        handles.push(thread::spawn(move || {
            let mut ids = Vec::new();
            // Each thread tries to mint IDs
            for _ in 0..10 {
                match dbg_try_mint_region_id(&counter) {
                    Ok(id) => ids.push(id),
                    Err(_) => break, // Exhausted
                }
            }
            ids
        }));
    }

    // Collect all successful IDs from all threads
    let mut all_ids = HashSet::new();
    for handle in handles {
        let ids = handle.join().expect("thread should not panic");
        for id in ids {
            let id_val = id.get();
            assert!(
                all_ids.insert(id_val),
                "thread got duplicate ID {} - ID reuse detected!",
                id_val
            );
        }
    }

    // Verify counter is exhausted
    assert_eq!(
        counter.load(core::sync::atomic::Ordering::Relaxed),
        0,
        "counter must be at exhausted sentinel (0)"
    );

    // Verify we got the expected IDs: MAX - 5 through MAX
    let expected_ids: HashSet<_> = (usize::MAX - 5..=usize::MAX).collect();
    assert_eq!(all_ids, expected_ids, "got unexpected set of IDs");
}
