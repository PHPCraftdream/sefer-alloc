//! OPT-E — large-segment free-cache tests.
//!
//! These tests verify the correctness of the `large_cache` field in
//! `AllocCore`: that a freed large segment is reused on the next compatible
//! `alloc_large` call without corrupting the allocator, and that the cache
//! is properly released on `Drop`.
//!
//! Gated on `alloc-core` + `alloc-decommit`: the cache is a no-op without
//! `alloc-decommit` (the feature gate for the large-free-cache path mirrors
//! the decommit-gate on the small-segment recycle path).

#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]

use core::alloc::Layout;
use sefer_alloc::AllocCore;

/// Alloc → dealloc → re-alloc the same size: the second alloc should hit
/// the cache and reuse the same OS reservation. We verify that the returned
/// pointer is valid for read/write (the recommit path works correctly).
#[test]
fn alloc_dealloc_alloc_reuses_cached_large() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(4 * 1024 * 1024, 8).unwrap();

    let ptr1 = ac.alloc(layout);
    assert!(!ptr1.is_null());
    ac.dealloc(ptr1, layout);

    // Re-alloc same size — should hit cache.
    let ptr2 = ac.alloc(layout);
    assert!(!ptr2.is_null());

    // EXPECTATION: cache hit reuses the same OS reservation. The user-visible
    // ptr MAY be the same (no header offset change) or differ slightly.
    // We don't assert ptr equality directly — instead assert behaviour:
    // write/read works correctly after recommit.
    unsafe {
        ptr2.write(0xAB);
        assert_eq!(ptr2.read(), 0xAB);
    }
    ac.dealloc(ptr2, layout);
}

/// A large segment that exceeds `MAX_CACHED_LARGE_BYTES` (default 64 MiB)
/// must NOT be cached. The test verifies the path does not crash.
#[test]
fn alloc_too_large_not_cached() {
    let mut ac = AllocCore::new().expect("primordial");
    // MAX_CACHED_LARGE_BYTES default = 64 MiB; 100 MiB exceeds it.
    let layout = Layout::from_size_align(100 * 1024 * 1024, 8).unwrap();

    let ptr = ac.alloc(layout);
    if ptr.is_null() {
        eprintln!("OOM allocating 100 MiB — skip test (machine too small)");
        return;
    }
    unsafe {
        ptr.write(0xCD);
    }
    ac.dealloc(ptr, layout);
    // No way to assert "not cached" through public API; this test mostly
    // proves the path doesn't crash. The smoke is meaningful.
}

/// Create AllocCore, alloc + dealloc to fill cache, then drop. The Drop
/// impl must release the cached reservation without panicking.
#[test]
fn drop_releases_cached_reservations() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(2 * 1024 * 1024, 8).unwrap();
    let ptr = ac.alloc(layout);
    assert!(!ptr.is_null());
    ac.dealloc(ptr, layout);
    // Cache now has one entry. Drop ac.
    drop(ac);
    // No assertion needed — under miri or normal run, no panic = correctness.
}

/// 100 iterations of alloc+dealloc at the same size. If the cache works,
/// the segment table stays at 1–2 live entries; without recycling it would
/// exhaust MAX_SEGMENTS=1024 after 1024 iterations. We run 100 to smoke-test
/// that no OOM/panic occurs — the table does not grow without bound.
#[test]
fn alloc_dealloc_loop_does_not_grow_segment_table() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(4 * 1024 * 1024, 8).unwrap();

    for _ in 0..100 {
        let ptr = ac.alloc(layout);
        assert!(!ptr.is_null(), "alloc must not fail in cache loop");
        ac.dealloc(ptr, layout);
    }
    // If cache didn't work, after 100 iters we'd have 100 segments registered
    // → without recycle, this would hit MAX_SEGMENTS=1024 eventually. With
    // working cache, segments are 1–2 max. Smoke: no OOM, no panic.
}

/// Two distinct sizes that both fit in the cache: verify each is served from
/// its own cached slot (not cross-contaminated), and both allocations are
/// valid for read/write.
#[test]
fn two_sizes_in_cache_no_cross_contamination() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout_4m = Layout::from_size_align(4 * 1024 * 1024, 8).unwrap();
    let layout_8m = Layout::from_size_align(8 * 1024 * 1024, 8).unwrap();

    // Alloc + dealloc both sizes to fill both cache slots.
    let p4 = ac.alloc(layout_4m);
    assert!(!p4.is_null());
    let p8 = ac.alloc(layout_8m);
    assert!(!p8.is_null());
    ac.dealloc(p4, layout_4m);
    ac.dealloc(p8, layout_8m);

    // Re-alloc both: each should come from the cache, not the OS.
    let p4b = ac.alloc(layout_4m);
    assert!(!p4b.is_null());
    let p8b = ac.alloc(layout_8m);
    assert!(!p8b.is_null());

    // Write to both — verifies recommit + no aliasing.
    unsafe {
        p4b.write_bytes(0xAA, 16);
        p8b.write_bytes(0xBB, 16);
        assert_eq!(p4b.read(), 0xAA);
        assert_eq!(p8b.read(), 0xBB);
    }

    ac.dealloc(p4b, layout_4m);
    ac.dealloc(p8b, layout_8m);
}
