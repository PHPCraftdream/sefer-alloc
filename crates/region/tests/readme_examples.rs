// Mirror of README.md examples — these are compiled as real tests to catch
// API drift (see CLAUDE.md: "the runnable version of an example belongs in
// tests/"). Each test's comment cites the line range it mirrors from the README.

#[test]
fn readme_region_example() {
    // Mirrors README.md:43-57
    use sefer_region::{Handle, Region};

    let mut region = Region::new();
    let h: Handle<String> = region.insert("hello".to_string());

    // I1: fresh handle resolves to the inserted value.
    assert_eq!(region.get(h).map(String::as_str), Some("hello"));

    let v = region.remove(h).unwrap();
    assert_eq!(v, "hello");

    // I2 + I3: stale handle resolves to None.
    assert!(region.get(h).is_none());
}

#[test]
fn readme_sync_region_example() {
    // Mirrors README.md:86-106
    // Note: sr2 from the original example was unused; this test drops it to
    // avoid an unused_variables warning while keeping the single-threaded
    // intent (the SyncRegion API is exercised, but no actual threading is
    // demonstrated in the original snippet).
    #[cfg(feature = "std")]
    {
        use sefer_region::SyncRegion;
        use std::sync::Arc;

        let sr: Arc<SyncRegion<u32>> = Arc::new(SyncRegion::new());
        let _sr2 = Arc::clone(&sr); // original unused; kept for fidelity

        // One-shot convenience: no guard needed for single operations.
        let h = sr.insert(42u32);
        assert_eq!(sr.get_cloned(h), Some(42u32));
        assert_eq!(sr.len(), 1);

        // Multi-op transaction: hold the write guard for atomicity.
        {
            let mut w = sr.write();
            w.insert(1u32);
            w.insert(2u32);
        } // guard dropped, lock released

        assert_eq!(sr.len(), 3);
    }

    #[cfg(not(feature = "std"))]
    {
        // SyncRegion is std-only; this is a compile-time guard that the test
        // itself is gated correctly.
    }
}
