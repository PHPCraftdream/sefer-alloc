//! Smoke tests for `sefer-region`: insert/get/remove round-trip, stale-handle
//! tombstone, len/is_empty accounting, and thread-safe SyncRegion basics
//! including poison recovery.

use sefer_region::{Handle, Region};

// ── single-threaded Region<T> ─────────────────────────────────────────────────

#[test]
fn region_insert_get_remove_roundtrip() {
    let mut r: Region<String> = Region::new();
    assert!(r.is_empty());

    let h: Handle<String> = r.insert("hello".to_string());

    // I1: fresh handle resolves to the inserted value.
    assert_eq!(r.get(h).map(String::as_str), Some("hello"));
    assert!(r.contains(h));

    // Remove returns the value.
    let v = r.remove(h).expect("remove live handle");
    assert_eq!(v, "hello");

    // I2: after remove, get returns None; second remove is a no-op None.
    assert!(r.get(h).is_none());
    assert!(!r.contains(h));
    assert!(r.remove(h).is_none());
}

/// Extracts the slot index from a `Handle`'s `{:?}` output.
///
/// `Handle`'s `Debug` impl embeds `slotmap::DefaultKey`'s own Debug output
/// (`{idx}v{version}`) as the `key` field, alongside a `region_id` field
/// (added by F2's domain-aware identity redesign), e.g.
/// `Handle { key: DefaultKey(1v1), region_id: 2 }`. Parsing this is the only
/// way to observe slot identity from outside the crate — `key` is
/// `pub(crate)`. Fragile w.r.t. slotmap's exact Debug format, but that is
/// the tradeoff this test deliberately accepts to prove reuse directly
/// rather than vacuously (see the module-level comment on the test below).
fn slot_index(h: impl std::fmt::Debug) -> u32 {
    let s = format!("{h:?}");
    let start = s
        .find("DefaultKey(")
        .expect("Handle's Debug output includes `DefaultKey(...)`")
        + "DefaultKey(".len();
    let rest = &s[start..];
    let end = rest
        .find('v')
        .expect("DefaultKey's Debug format is `{idx}v{version}`");
    rest[..end]
        .parse::<u32>()
        .expect("the index portion of DefaultKey's Debug output should parse as u32")
}

#[test]
fn region_stale_handle_returns_none() {
    // I3 — no ABA: a slot reused for a new value does NOT resolve via the old handle.
    //
    // NOTE: `capacity()` does NOT discriminate slot reuse from fresh growth —
    // slotmap pre-allocates, so capacity stays flat across a second insert
    // regardless of whether a slot was removed first (verified: three
    // consecutive inserts into a fresh Region with ZERO removals leave
    // capacity() unchanged after the first insert). An earlier version of
    // this test asserted exactly that non-discriminating capacity equality
    // and would have kept passing even if slotmap's freelist policy stopped
    // reusing slots entirely — the class of vacuous test this file's own I3
    // doc comment warns against. Proving reuse requires comparing slot
    // IDENTITY (the index component of the key), not a size proxy.
    let mut r: Region<u32> = Region::new();

    let h_old = r.insert(1u32);

    // Verified non-vacuous: commenting out this remove call (so the second
    // insert lands on a FRESH slot instead of reusing h_old's freed one) makes
    // the slot_index equality assertion below fail on its own, isolated from
    // the len()/None checks around it (`left: 1, right: 2`), then reverted.
    r.remove(h_old); // retire slot; generation bumped inside slotmap
    assert_eq!(r.len(), 0, "length after remove");

    // Insert a new value — may reuse the same physical slot.
    let h_new = r.insert(2u32);
    assert_eq!(r.len(), 1, "length after second insert");

    // Prove slot reuse actually happened: the new handle's slot INDEX must
    // match the old handle's (only the generation differs).
    assert_eq!(
        slot_index(h_old),
        slot_index(h_new),
        "second insert should reuse the freed slot's index (proves slot reuse, not fresh allocation)"
    );

    // Old handle must NOT resolve (generation mismatch).
    assert!(r.get(h_old).is_none(), "stale handle must not resolve (I3)");

    // New handle resolves correctly.
    assert_eq!(r.get(h_new).copied(), Some(2u32));
}

#[test]
fn region_handle_from_different_instance_is_rejected() {
    let mut region_a: Region<u32> = Region::new();
    let mut region_b: Region<u32> = Region::new();

    let h_a = region_a.insert(1u32);
    let h_b = region_b.insert(2u32);

    // Cross-region access should now FAIL (return None):
    assert_eq!(region_b.get(h_a), None, "cross-region get should fail");
    assert_eq!(region_a.get(h_b), None, "cross-region get should fail");

    // Cross-region remove should now FAIL (return None):
    assert_eq!(
        region_b.remove(h_a),
        None,
        "cross-region remove should fail"
    );
    assert_eq!(
        region_a.remove(h_b),
        None,
        "cross-region remove should fail"
    );

    // Verify handles still work in their own regions:
    assert_eq!(region_a.get(h_a).copied(), Some(1u32));
    assert_eq!(region_b.get(h_b).copied(), Some(2u32));

    // Verify contains respects region_id:
    assert!(
        !region_b.contains(h_a),
        "cross-region contains should be false"
    );
    assert!(
        !region_a.contains(h_b),
        "cross-region contains should be false"
    );
    assert!(
        region_a.contains(h_a),
        "same-region contains should be true"
    );
    assert!(
        region_b.contains(h_b),
        "same-region contains should be true"
    );
}

/// I7 (`get_mut`): a handle from a *different* `Region<T>` instance must be
/// rejected by `get_mut` exactly like `get`/`remove`/`contains` already are
/// — this is the guard-place F5 flagged as untested (deleting the
/// `region_id` check inside `get_mut` left the crate's full test suite
/// green before this test existed).
#[test]
fn region_get_mut_from_different_instance_is_rejected() {
    let mut region_a: Region<u32> = Region::new();
    let mut region_b: Region<u32> = Region::new();

    let h_a = region_a.insert(1u32);
    let h_b = region_b.insert(2u32);

    // Cross-region get_mut should fail in BOTH directions:
    assert_eq!(
        region_b.get_mut(h_a),
        None,
        "cross-region get_mut should fail (a's handle into b)"
    );
    assert_eq!(
        region_a.get_mut(h_b),
        None,
        "cross-region get_mut should fail (b's handle into a)"
    );

    // A rejected get_mut must not have mutated anything in the wrong region:
    assert_eq!(region_a.get(h_a).copied(), Some(1u32));
    assert_eq!(region_b.get(h_b).copied(), Some(2u32));

    // Same-region get_mut still works and mutates in place:
    *region_a
        .get_mut(h_a)
        .expect("same-region get_mut should succeed") = 100;
    *region_b
        .get_mut(h_b)
        .expect("same-region get_mut should succeed") = 200;
    assert_eq!(region_a.get(h_a).copied(), Some(100u32));
    assert_eq!(region_b.get(h_b).copied(), Some(200u32));
}

/// I7 churn sanity: a handle from a `Region` that has been dropped must be
/// rejected by a freshly-constructed `Region`, under ordinary (non-overflow)
/// churn — i.e. `region_id` allocation does not accidentally recycle IDs
/// across an ordinary drop/recreate cycle in the same process.
#[test]
fn region_handle_from_dropped_instance_is_rejected_by_fresh_region() {
    let mut region_a: Region<u32> = Region::new();
    let h_a = region_a.insert(1u32);
    drop(region_a);

    let mut region_b: Region<u32> = Region::new();
    let h_b = region_b.insert(2u32);

    // The handle from the dropped region must not resolve through the
    // freshly-created region, even though the underlying raw key commonly
    // collides (first insert into a fresh Region tends to produce the same
    // DefaultKey).
    assert_eq!(
        region_b.get(h_a),
        None,
        "handle from a dropped Region must not resolve in a fresh Region"
    );
    assert!(
        !region_b.contains(h_a),
        "handle from a dropped Region must not be `contains`ed by a fresh Region"
    );
    assert_eq!(
        region_b.remove(h_a),
        None,
        "handle from a dropped Region must not be removable from a fresh Region"
    );

    // The fresh region's own handle still works normally.
    assert_eq!(region_b.get(h_b).copied(), Some(2u32));
}

#[test]
fn handles_from_different_regions_are_distinct() {
    use std::collections::{HashMap, HashSet};

    let mut region_a: Region<u32> = Region::new();
    let mut region_b: Region<u32> = Region::new();

    // First insert in each region typically produces the same DefaultKey
    let h_a = region_a.insert(100);
    let h_b = region_b.insert(200);

    // Despite having the same raw key, handles from different regions are NOT equal
    assert_ne!(
        h_a, h_b,
        "handles from different regions should not compare equal"
    );

    // Hash contract: equal handles must hash to the same value.
    // Test this by comparing a handle to itself.
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    let mut hasher_1 = DefaultHasher::new();
    h_a.hash(&mut hasher_1);
    let hash_1 = hasher_1.finish();

    let mut hasher_2 = DefaultHasher::new();
    h_a.hash(&mut hasher_2);
    let hash_2 = hasher_2.finish();

    assert_eq!(
        hash_1, hash_2,
        "same handle must hash to same value (Hash contract)"
    );

    // Behavioral test: HashSet/HashMap treat unequal handles as distinct.
    // This is what matters — the Hash contract allows collisions, but
    // correct behavior distinguishes unequal keys.
    let mut set: HashSet<Handle<u32>> = HashSet::new();
    set.insert(h_a);
    assert!(set.contains(&h_a), "set should contain h_a");
    assert!(
        !set.contains(&h_b),
        "set should NOT contain h_b (different region)"
    );
    set.insert(h_b); // This adds a second entry
    assert_eq!(
        set.len(),
        2,
        "set should have 2 entries (different handles)"
    );

    // HashMap should NOT collide keys from different regions
    let mut map: HashMap<Handle<u32>, &str> = HashMap::new();
    map.insert(h_a, "region_a_value");
    map.insert(h_b, "region_b_value");
    assert_eq!(map.len(), 2, "map should have 2 entries (no collision)");
    assert_eq!(map.get(&h_a), Some(&"region_a_value"));
    assert_eq!(map.get(&h_b), Some(&"region_b_value"));

    // Verify handles still work correctly in their own regions
    assert_eq!(region_a.get(h_a).copied(), Some(100));
    assert_eq!(region_b.get(h_b).copied(), Some(200));
}

#[test]
fn region_len_is_empty_track_live() {
    // I4: len / is_empty reflect exactly the live count.
    let mut r: Region<i32> = Region::new();
    assert_eq!(r.len(), 0);
    assert!(r.is_empty());

    let h1 = r.insert(10);
    assert_eq!(r.len(), 1);
    assert!(!r.is_empty());

    let h2 = r.insert(20);
    assert_eq!(r.len(), 2);

    r.remove(h1);
    assert_eq!(r.len(), 1);

    r.remove(h2);
    assert_eq!(r.len(), 0);
    assert!(r.is_empty());
}

// ── SyncRegion<T> (std feature, default-on) ──────────────────────────────────

#[cfg(feature = "std")]
mod sync_tests {
    use sefer_region::SyncRegion;

    #[test]
    fn sync_region_basic() {
        let sr: SyncRegion<&str> = SyncRegion::new();
        assert!(sr.is_empty());

        let h = sr.insert("world");
        assert!(sr.contains(h));
        assert_eq!(sr.len(), 1);

        let v = sr.remove(h).expect("remove live handle");
        assert_eq!(v, "world");

        assert!(!sr.contains(h));
        assert_eq!(sr.len(), 0);
        assert!(sr.is_empty());
    }

    #[test]
    fn sync_region_poison_recovery() {
        // A panic inside a write guard poisons the RwLock.
        // SyncRegion recovers from poison (PoisonError::into_inner) — the
        // region must remain usable after the panicking thread finishes.
        use std::sync::Arc;

        let sr: Arc<SyncRegion<u32>> = Arc::new(SyncRegion::new());
        let sr2 = Arc::clone(&sr);

        // Spawn a thread that inserts then panics while holding the lock.
        let join = std::thread::spawn(move || {
            let mut guard = sr2.write();
            let _h = guard.insert(42u32);
            // Panic with the write guard held — this poisons the RwLock.
            panic!("intentional poison");
        });

        // The spawned thread panics — join returns Err.
        assert!(join.join().is_err(), "thread should have panicked");

        // After poison, SyncRegion must still be usable (recover-from-poison policy).
        // The region is structurally intact; we can insert and retrieve normally.
        let h2 = sr.insert(99u32);
        assert_eq!(sr.get_cloned(h2), Some(99u32));
        assert_eq!(sr.len(), 2); // 42 inserted before panic + 99 just now
    }

    /// I7 on `SyncRegion`: a handle from a *different* `SyncRegion` instance
    /// must be rejected — mirrors `region_handle_from_different_instance_is_rejected`
    /// (single-threaded `Region`) but exercises `SyncRegion`'s own surface:
    /// the one-shot convenience methods (`get_cloned`, `contains`, `remove`)
    /// AND the guard-based forms (`sr.read().get(..)`, `sr.write().get_mut(..)`),
    /// since `SyncRegion` had ZERO cross-instance coverage before this test.
    #[test]
    fn sync_region_handle_from_different_instance_is_rejected() {
        let sr_a: SyncRegion<u32> = SyncRegion::new();
        let sr_b: SyncRegion<u32> = SyncRegion::new();

        let h_a = sr_a.insert(1u32);
        let h_b = sr_b.insert(2u32);

        // One-shot convenience methods: get_cloned/contains/remove.
        assert_eq!(
            sr_b.get_cloned(h_a),
            None,
            "cross-region get_cloned should fail (a's handle into b)"
        );
        assert_eq!(
            sr_a.get_cloned(h_b),
            None,
            "cross-region get_cloned should fail (b's handle into a)"
        );

        assert!(
            !sr_b.contains(h_a),
            "cross-region contains should be false (a's handle into b)"
        );
        assert!(
            !sr_a.contains(h_b),
            "cross-region contains should be false (b's handle into a)"
        );

        assert_eq!(
            sr_b.remove(h_a),
            None,
            "cross-region remove should fail (a's handle into b)"
        );
        assert_eq!(
            sr_a.remove(h_b),
            None,
            "cross-region remove should fail (b's handle into a)"
        );

        // Guard-based forms: read().get(..) and write().get_mut(..).
        assert_eq!(
            sr_b.read().get(h_a),
            None,
            "cross-region read().get(..) should fail (a's handle into b)"
        );
        assert_eq!(
            sr_a.read().get(h_b),
            None,
            "cross-region read().get(..) should fail (b's handle into a)"
        );
        assert_eq!(
            sr_b.write().get_mut(h_a),
            None,
            "cross-region write().get_mut(..) should fail (a's handle into b)"
        );
        assert_eq!(
            sr_a.write().get_mut(h_b),
            None,
            "cross-region write().get_mut(..) should fail (b's handle into a)"
        );

        // Nothing was disturbed by the rejected cross-region calls: both
        // handles still resolve correctly in their own SyncRegion.
        assert_eq!(sr_a.get_cloned(h_a), Some(1u32));
        assert_eq!(sr_b.get_cloned(h_b), Some(2u32));
        assert!(sr_a.contains(h_a));
        assert!(sr_b.contains(h_b));
    }
}
