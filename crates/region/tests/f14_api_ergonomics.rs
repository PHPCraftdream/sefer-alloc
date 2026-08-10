//! Tests for F14 API-ergonomics: `into_inner`, `Debug`, `IntoIterator`,
//! `PartialOrd`/`Ord` for `Handle<T>`, and extended `iter`/`iter_mut` bounds.

use sefer_region::{Handle, Region};

#[cfg(feature = "std")]
use sefer_region::SyncRegion;

// ── SyncRegion::into_inner ───────────────────────────────────────────────────

#[cfg(feature = "std")]
#[test]
fn sync_region_into_inner_preserves_handles() {
    let mut region: Region<String> = Region::new();
    let h = region.insert("hello".to_string());

    // Wrap in SyncRegion
    let sync_region: SyncRegion<String> = SyncRegion::from(region);

    // Extract back
    let extracted_region = sync_region.into_inner();

    // The handle should still work
    assert_eq!(extracted_region.get(h).map(String::as_str), Some("hello"));
}

#[cfg(feature = "std")]
#[test]
fn sync_region_into_inner_recovers_from_poison() {
    use std::sync::Arc;

    let region: Region<i32> = Region::new();
    let sr: Arc<SyncRegion<i32>> = Arc::new(SyncRegion::from(region));
    let sr2 = Arc::clone(&sr);

    // Spawn a thread that panics while holding the write lock
    let join = std::thread::spawn(move || {
        let _guard = sr2.write();
        panic!("intentional poison");
    });

    assert!(join.join().is_err(), "thread should have panicked");

    // into_inner should recover from poison
    let _extracted = Arc::try_unwrap(sr).unwrap().into_inner();
    // If we got here, poison recovery worked
}

// ── Debug implementations ────────────────────────────────────────────────────

#[test]
fn region_debug_shows_structural_info() {
    let mut region: Region<i32> = Region::new();
    let _ = region.insert(1);
    let _ = region.insert(2);

    let debug_str = format!("{:?}", region);
    // Should contain the type name and structural info
    assert!(debug_str.contains("Region"));
    assert!(debug_str.contains("len"));
    assert!(debug_str.contains("capacity"));
    assert!(debug_str.contains("region_id"));
}

#[test]
fn region_debug_does_not_require_t_debug() {
    // This test compiles only if Region<T> doesn't require T: Debug
    struct NoDebug;

    let region: Region<NoDebug> = Region::new();
    let _ = format!("{:?}", region); // Should compile and not panic
}

#[cfg(feature = "std")]
#[test]
fn sync_region_debug_shows_structural_info() {
    let sync_region: SyncRegion<i32> = SyncRegion::new();
    let debug_str = format!("{:?}", sync_region);

    // Should contain the type name
    assert!(debug_str.contains("SyncRegion"));
}

#[cfg(feature = "std")]
#[test]
fn sync_region_debug_handles_locked_state() {
    use std::sync::Arc;
    use std::thread;

    let sync_region: Arc<SyncRegion<i32>> = Arc::new(SyncRegion::new());
    let sync_region2 = Arc::clone(&sync_region);

    // Hold the lock in another thread
    let handle = thread::spawn(move || {
        let _guard = sync_region2.write();
        // Sleep a bit so we can try to debug while locked
        std::thread::sleep(std::time::Duration::from_millis(100));
    });

    // Give the other thread time to acquire the lock
    std::thread::sleep(std::time::Duration::from_millis(10));

    // Debug should handle the locked state gracefully (show "<locked>")
    let debug_str = format!("{:?}", sync_region);
    // Should not panic, and should still show something reasonable
    assert!(debug_str.contains("SyncRegion"));

    handle.join().unwrap();
}

#[cfg(feature = "std")]
#[test]
fn sync_region_debug_shows_poisoned_not_locked() {
    use std::sync::Arc;
    use std::thread;

    let sync_region: Arc<SyncRegion<i32>> = Arc::new(SyncRegion::new());
    let sr2 = Arc::clone(&sync_region);

    // Spawn a thread that panics *inside* write(), poisoning the lock, then
    // join it so the lock is free (not held) by the time we format it.
    let handle = thread::spawn(move || {
        let _guard = sr2.write();
        panic!("intentional poison for Debug regression test");
    });
    assert!(handle.join().is_err(), "thread should have panicked");

    // The lock is poisoned but NOT held: Debug must recover the data and
    // report `poisoned: true`, not fall back to the "<locked>" placeholder
    // (F4 regression — try_read()'s catch-all `Err(_)` used to collapse
    // "poisoned-but-free" into the same case as "held by another thread").
    let debug_str = format!("{:?}", sync_region);
    assert!(debug_str.contains("SyncRegion"));
    assert!(
        !debug_str.contains("<locked>"),
        "poisoned-but-free lock must not render as \"<locked>\": {debug_str}"
    );
    assert!(
        debug_str.contains("poisoned: true") || debug_str.contains("poisoned\": true"),
        "poisoned Debug output must explicitly report poisoned: true: {debug_str}"
    );

    // The recovered region must still be usable afterward (poisoning policy).
    let _ = sync_region.insert(42);
    assert_eq!(sync_region.len(), 1);
}

// ── IntoIterator for Region ─────────────────────────────────────────────────

// Note: `IntoIterator for Region<T>` (consuming by value) is deliberately NOT
// implemented because `slotmap::SlotMap::into_iter()` yields `(DefaultKey, T)`
// pairs, and exposing `slotmap::DefaultKey` would break this crate's
// encapsulation (raw keys must never escape through the public API).
// Iteration by reference is provided below.

#[test]
fn region_into_iterator_by_ref() {
    let mut region: Region<i32> = Region::new();
    let _ = region.insert(1);
    let _ = region.insert(2);
    let _ = region.insert(3);

    let mut count = 0;
    for value in &region {
        count += 1;
        assert!((&1..=&3).contains(&value));
    }
    assert_eq!(count, 3);

    // Region should still be usable after iteration
    assert_eq!(region.len(), 3);
}

#[test]
fn region_into_iterator_by_mut() {
    let mut region: Region<i32> = Region::new();
    let _ = region.insert(1);
    let _ = region.insert(2);
    let _ = region.insert(3);

    for value in &mut region {
        *value *= 2;
    }

    assert_eq!(region.len(), 3);
    // Region::iter's order is unspecified (see region.rs's own doc on
    // `iter`), so compare as a multiset — element membership + length —
    // rather than pinning a specific sequence. Same pattern as
    // `coverage_gaps.rs`'s `region_iter_mut_and_iter` test.
    let values: Vec<&i32> = region.iter().collect();
    assert_eq!(values.len(), 3);
    assert!(values.contains(&&2));
    assert!(values.contains(&&4));
    assert!(values.contains(&&6));
}

#[test]
fn region_for_loop_compiles() {
    // This test exists primarily to verify that `for x in &region` compiles
    let mut region: Region<String> = Region::new();
    let _ = region.insert("hello".to_string());
    let _ = region.insert("world".to_string());

    let mut visited = Vec::new();
    for s in &region {
        visited.push(s.clone());
    }
    assert_eq!(visited.len(), 2);
}

// ── PartialOrd/Ord for Handle<T> ───────────────────────────────────────────

#[test]
fn handle_ord_consistent_within_same_region() {
    let mut region: Region<i32> = Region::new();
    let h1 = region.insert(1);
    let h2 = region.insert(2);
    let h3 = region.insert(3);

    let mut handles: Vec<Handle<i32>> = vec![h3, h1, h2];
    handles.sort(); // Should compile and not panic

    // After sorting, handles should have a consistent order
    // (the exact order depends on slotmap's key assignment)
    let len_before = handles.len();
    handles.dedup();
    assert_eq!(len_before, handles.len(), "no handles should be equal");
}

#[test]
fn handle_ord_handles_from_different_regions() {
    use std::cmp::Ordering;
    use std::collections::BTreeSet;

    let mut region_a: Region<i32> = Region::new();
    let mut region_b: Region<i32> = Region::new();

    // Both first inserts commonly land on the same raw `DefaultKey`, so this
    // is the "colliding key, different region_id" case Ord must still order
    // consistently rather than merely "not panic" on.
    let h_a = region_a.insert(1);
    let h_b = region_b.insert(1);

    // Handles from different regions should not compare equal
    assert_ne!(h_a, h_b);

    // Ord must actually distinguish them, not just return `Some(_)`: an
    // Eq/Ord inconsistency (e.g. cmp ignoring region_id while PartialEq
    // does not) would make h_a and h_b compare Equal despite being unequal
    // under PartialEq — assert the real relation, not just its presence.
    let ord_ab = h_a.cmp(&h_b);
    let ord_ba = h_b.cmp(&h_a);
    assert_ne!(
        ord_ab,
        Ordering::Equal,
        "handles with colliding keys from different regions must not compare Equal under Ord"
    );
    assert!(
        h_a < h_b || h_b < h_a,
        "handles with colliding keys from different regions must have a strict order"
    );

    // Antisymmetry: cmp(a, b) and cmp(b, a) must be exact reverses.
    assert_eq!(
        ord_ab,
        ord_ba.reverse(),
        "Ord must be antisymmetric: a.cmp(&b) == b.cmp(&a).reverse()"
    );

    // partial_cmp must agree with cmp (Ord/PartialOrd consistency).
    assert_eq!(h_a.partial_cmp(&h_b), Some(ord_ab));

    // Direct analogue of `smoke.rs`'s `handles_from_different_regions_are_distinct`
    // HashSet case, but for BTreeSet (Ord-keyed instead of Hash-keyed):
    // two colliding-key handles from different regions must both be kept,
    // not collapsed into one entry by a broken Ord/Eq relationship.
    let mut set: BTreeSet<Handle<i32>> = BTreeSet::new();
    set.insert(h_a);
    set.insert(h_b);
    assert_eq!(
        set.len(),
        2,
        "BTreeSet should hold both colliding-key handles from different regions"
    );
    assert!(set.contains(&h_a));
    assert!(set.contains(&h_b));
}

#[test]
fn handle_ord_eq_implies_same_ordering() {
    let mut region: Region<i32> = Region::new();
    let h1 = region.insert(1);

    // A handle should compare equal to itself
    assert_eq!(h1, h1);
    assert_eq!(h1.partial_cmp(&h1), Some(std::cmp::Ordering::Equal));
    assert_eq!(h1.cmp(&h1), std::cmp::Ordering::Equal);
}

#[test]
fn handle_ord_total_order() {
    let mut region: Region<i32> = Region::new();
    let handles: Vec<Handle<i32>> = (0..10).map(|i| region.insert(i)).collect();

    // Verify that Ord provides a total order (no panics, all comparisons are Some)
    for h_i in &handles {
        for h_j in &handles {
            let cmp = h_i.partial_cmp(h_j);
            assert!(cmp.is_some(), "partial_cmp should return Some");

            // Transitivity check: if a < b and b < c, then a < c
            if let Some(std::cmp::Ordering::Less) = cmp {
                for h_k in &handles {
                    if let Some(std::cmp::Ordering::Less) = h_j.partial_cmp(h_k) {
                        assert_eq!(
                            h_i.partial_cmp(h_k),
                            Some(std::cmp::Ordering::Less),
                            "transitivity failed"
                        );
                    }
                }
            }
        }
    }
}

// ── Extended iter/iter_mut bounds ──────────────────────────────────────────

#[test]
fn region_iter_is_exact_size() {
    let mut region: Region<i32> = Region::new();
    let _ = region.insert(1);
    let _ = region.insert(2);
    let _ = region.insert(3);

    let iter = region.iter();
    assert_eq!(iter.len(), 3);
    assert_eq!(region.len(), 3);

    // Verify that iter.len() matches region.len() even with holes
    let h = region.insert(4);
    region.remove(h);

    let iter = region.iter();
    assert_eq!(iter.len(), 3);
    assert_eq!(region.len(), 3);
}

#[test]
fn region_iter_mut_is_exact_size() {
    let mut region: Region<i32> = Region::new();
    let _ = region.insert(1);
    let _ = region.insert(2);
    let _ = region.insert(3);

    let iter = region.iter_mut();
    assert_eq!(iter.len(), 3);
}

#[test]
fn region_iter_is_fused() {
    let mut region: Region<i32> = Region::new();
    let _ = region.insert(1);
    let _ = region.insert(2);

    let mut iter = region.iter();
    assert!(iter.next().is_some());
    assert!(iter.next().is_some());
    assert!(iter.next().is_none());
    assert!(iter.next().is_none()); // Fused iterator continues to return None
}

#[test]
fn region_iter_mut_is_fused() {
    let mut region: Region<i32> = Region::new();
    let _ = region.insert(1);
    let _ = region.insert(2);

    let mut iter = region.iter_mut();
    assert!(iter.next().is_some());
    assert!(iter.next().is_some());
    assert!(iter.next().is_none());
    assert!(iter.next().is_none()); // Fused iterator continues to return None
}

#[test]
fn region_iter_is_clone() {
    let mut region: Region<i32> = Region::new();
    let _ = region.insert(1);
    let _ = region.insert(2);
    let _ = region.insert(3);

    let iter1 = region.iter();
    let iter2 = iter1.clone();

    // Both iterators should produce the same values (in the same order)
    let vals1: Vec<_> = iter1.cloned().collect();
    let vals2: Vec<_> = iter2.cloned().collect();
    assert_eq!(vals1, vals2);
}
