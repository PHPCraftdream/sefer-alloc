//! OPT-B — O(1) `SegmentTable::contains_base` via open-addressing hash.
//!
//! These tests exercise the correctness of the hash table introduced in
//! task #67: insertion on `register`, tombstoning on `recycle`/`unregister`,
//! and O(1) `contains_base` lookup.
//!
//! The tests operate at the public `AllocCore` boundary (alloc/dealloc), since
//! the hash table is an internal implementation detail. Correctness is observed
//! indirectly: if `contains_base` returns wrong answers, `dealloc` either
//! treats own pointers as foreign (no-op → memory leak → eventual OOM) or
//! double-frees foreign pointers (corruption / crash).

/// Allocate and free many blocks, spanning multiple segments. After dealloc,
/// re-allocate the same count: if the hash broke (e.g. tombstone chain
/// corrupted, or `contains_base` returned false on a live segment), segments
/// would be leaked and eventually `alloc` would return null.
#[cfg(feature = "alloc-core")]
#[cfg_attr(miri, ignore)] // 50k × 2 iterations is too slow under miri.
#[test]
fn register_many_then_contains_each() {
    use core::alloc::Layout;
    use sefer_alloc::alloc_core::AllocCore;

    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(64, 8).unwrap();

    // First pass: allocate 50 000 blocks, then free them all.
    let mut ptrs = Vec::with_capacity(50_000);
    for i in 0..50_000_usize {
        let p = ac.alloc(layout);
        assert!(!p.is_null(), "alloc returned null at i={i} (pass 1)");
        ptrs.push(p);
    }
    for p in ptrs.drain(..) {
        ac.dealloc(p, layout);
    }

    // Second pass: allocate the same count again. If hash/slot bookkeeping is
    // broken, segments are leaked and we run out before reaching 50 000.
    for i in 0..50_000_usize {
        let p = ac.alloc(layout);
        assert!(
            !p.is_null(),
            "alloc returned null at i={i} (pass 2, after dealloc)"
        );
        ptrs.push(p);
    }
    for p in ptrs.drain(..) {
        ac.dealloc(p, layout);
    }
}

/// A foreign pointer (allocated outside `AllocCore`) must not crash `dealloc`.
/// Under the correct implementation, `contains_base` returns `false` for its
/// segment base (which was never registered) and `dealloc` is a no-op.
#[cfg(feature = "alloc-core")]
#[test]
fn dealloc_foreign_pointer_is_noop() {
    use core::alloc::Layout;
    use sefer_alloc::alloc_core::AllocCore;

    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(64, 8).unwrap();

    // Allocate one own pointer so the allocator is initialised.
    let own = ac.alloc(layout);
    assert!(!own.is_null());

    // Allocate a foreign pointer from the std allocator (outside AllocCore).
    let foreign = Box::into_raw(Box::new([0u8; 64])) as *mut u8;

    // This must NOT crash. `contains_base` must return false for `foreign`'s
    // segment base (a heap address, not one of our SEGMENT-aligned bases).
    ac.dealloc(foreign, layout);

    // Clean up: return the box and our own block.
    // SAFETY: `foreign` was obtained from `Box::into_raw` above and has not
    // been freed through any other path (the dealloc above was a no-op).
    unsafe { drop(Box::from_raw(foreign as *mut [u8; 64])) };
    ac.dealloc(own, layout);
}

/// Cycle through many alloc/dealloc rounds so that hash entries are inserted,
/// tombstoned (via `recycle`/`unregister`), and re-inserted many times. If the
/// probe chain is corrupted by a bad tombstone or a reset to `null_mut()` (which
/// would terminate searches prematurely), `dealloc` would either no-op on live
/// pointers (leaking) or `alloc` would return null once slots fill up.
#[cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]
#[cfg_attr(miri, ignore)] // 200 rounds × 1000 allocs = 200k ops — too slow for miri.
#[test]
fn recycle_then_register_uses_hash_correctly() {
    use core::alloc::Layout;
    use sefer_alloc::alloc_core::AllocCore;

    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(64, 8).unwrap();

    for round in 0..200_usize {
        let mut ptrs = Vec::with_capacity(1_000);
        for i in 0..1_000_usize {
            let p = ac.alloc(layout);
            assert!(
                !p.is_null(),
                "alloc returned null at round={round} i={i} — \
                 hash corruption caused segment leak or slot exhaustion"
            );
            ptrs.push(p);
        }
        for p in ptrs {
            ac.dealloc(p, layout);
        }
    }
}
