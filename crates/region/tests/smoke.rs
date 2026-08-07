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

#[test]
fn region_stale_handle_returns_none() {
    // I3 — no ABA: a slot reused for a new value does NOT resolve via the old handle.
    let mut r: Region<u32> = Region::new();

    let h_old = r.insert(1u32);
    let cap_after_first = r.capacity();

    r.remove(h_old); // retire slot; generation bumped inside slotmap
    assert_eq!(r.len(), 0, "length after remove");

    // Insert a new value — may reuse the same physical slot.
    // Verify slot reuse actually happened by checking capacity didn't grow.
    let h_new = r.insert(2u32);
    assert_eq!(
        r.capacity(),
        cap_after_first,
        "second insert reused freed slot (no capacity growth)"
    );
    assert_eq!(r.len(), 1, "length after second insert");

    // Old handle must NOT resolve (generation mismatch).
    assert!(r.get(h_old).is_none(), "stale handle must not resolve (I3)");

    // New handle resolves correctly.
    assert_eq!(r.get(h_new).copied(), Some(2u32));
}

#[test]
fn region_handle_crosses_instance_of_same_type() {
    // Documents the REAL (weaker-than-it-sounds) semantics of Handle<T>'s
    // PhantomData<fn() -> T> branding: it separates handles by value type T
    // (a Handle<Foo> cannot be passed where a Handle<Bar> is expected — that
    // part IS a compile error), but it does NOT separate handles by Region
    // INSTANCE. A Handle<T> minted by one Region<T> is silently accepted by
    // an unrelated Region<T> of the same T, and can resolve to (or remove)
    // whatever value happens to occupy the same slot in that other Region.
    // See crates/region/src/lib.rs and README.md "Why?" for the disclosure
    // this test exists to keep honest.
    let mut region_a: Region<u32> = Region::new();
    let mut region_b: Region<u32> = Region::new();

    let h_a = region_a.insert(1u32);
    let h_b = region_b.insert(2u32);

    // Same slot index/generation on both sides (both are the first insert
    // into a fresh Region), so h_a and h_b are interchangeable in practice —
    // but even without that coincidence, nothing at the type level stops
    // h_a from being handed to region_b: this compiles today and always will.
    assert_eq!(region_b.get(h_a).copied(), Some(2u32));
    assert_eq!(region_a.get(h_b).copied(), Some(1u32));

    // The hazard is not just read confusion — remove() against the WRONG
    // region silently removes the other region's value.
    let removed = region_b
        .remove(h_a)
        .expect("wrong-region remove still succeeds");
    assert_eq!(removed, 2u32);
    assert!(
        region_b.is_empty(),
        "region_b's own value was removed via region_a's handle"
    );
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
}
