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
    r.remove(h_old); // retire slot; generation bumped inside slotmap

    // Insert a new value — may reuse the same physical slot.
    let h_new = r.insert(2u32);

    // Old handle must NOT resolve (generation mismatch).
    assert!(r.get(h_old).is_none(), "stale handle must not resolve (I3)");

    // New handle resolves correctly.
    assert_eq!(r.get(h_new).copied(), Some(2u32));
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
