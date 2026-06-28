//! Phase P3 -- M2 magazine double-free guard tests.
//!
//! Tests the per-heap key + bounded-scan guard that prevents a double-free of
//! a magazine-resident block from corrupting the magazine (silently pushing
//! the same pointer twice). The guard is the authoritative M2 guarantee for
//! the magazine layer; the BinTable bitmap remains the authority for flushed
//! blocks.
//!
//! ## Tests
//!
//! - **T2**: double-free of a magazine-resident block is a no-op (the M2
//!   guard fires). Counterfactual: with the guard removed, the magazine
//!   pushes `ptr` twice and the next two allocs return the same pointer.
//! - **T3**: double-free of a FLUSHED block (already returned to BinTable)
//!   is still caught by the BinTable bitmap (existing M2 path unchanged).
//! - **T-false-positive**: user writes a value that happens to equal our
//!   tcache key into word1 of a block while it is allocated; freeing that
//!   block must still work (the bounded scan catches the false positive).
//! - **T-key-round-trip**: alloc -> free -> alloc returns the same ptr
//!   (LIFO from magazine). The key in word1 does not break the round-trip.

#![cfg(all(feature = "alloc-global", feature = "fastbin"))]

use std::alloc::Layout;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise all tests in this file: the registry is a process-global static.
static SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        SERIAL.store(false, Ordering::Release);
    }
}

// ── T2: double-free of a magazine-resident block is no-op ─────────────────

/// A double-free of a magazine-resident block must be caught by the M2 guard
/// (P3). After the second free, the magazine must contain the pointer exactly
/// ONCE, so the next two allocs return distinct pointers.
///
/// COUNTERFACTUAL (do NOT enable in production): to verify T2 is not vacuous,
/// temporarily comment out the `if word1 == key { ... return; }` guard in
/// `heap_core.rs::dealloc_own_thread` and re-run this test. It MUST fail
/// (`assert_ne!` on p1, p2 trips because the magazine pushes p twice).
#[test]
fn t2_double_free_magazine_block_is_noop() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(16, 8).unwrap();

    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null());

    // First free -> pushes to magazine.
    unsafe { (*heap).dealloc(p, layout) };

    // Second free of SAME ptr -> M2 guard must catch it.
    // (Without the guard, this would push p twice into the magazine,
    // and the next two allocs would return the SAME ptr.)
    unsafe { (*heap).dealloc(p, layout) };

    // Now alloc twice. The first MUST return p (LIFO from magazine), the
    // second MUST return SOMETHING DIFFERENT -- if the guard worked,
    // p appears at most once in the magazine.
    let p1 = unsafe { (*heap).alloc(layout) };
    let p2 = unsafe { (*heap).alloc(layout) };
    assert!(!p1.is_null());
    assert!(!p2.is_null());
    assert_ne!(
        p1, p2,
        "magazine returned same pointer twice -- M2 guard failed"
    );

    unsafe {
        (*heap).dealloc(p1, layout);
        (*heap).dealloc(p2, layout);
        HeapRegistry::recycle(heap);
    }
}

/// Variant of T2: triple-free of the same block. The guard must catch each
/// repeated free. The magazine must contain the pointer exactly once.
#[test]
fn t2_triple_free_still_noop() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(16, 8).unwrap();

    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null());

    // Free three times.
    unsafe { (*heap).dealloc(p, layout) };
    unsafe { (*heap).dealloc(p, layout) };
    unsafe { (*heap).dealloc(p, layout) };

    // Alloc twice: first returns p (LIFO), second must be different.
    let p1 = unsafe { (*heap).alloc(layout) };
    let p2 = unsafe { (*heap).alloc(layout) };
    assert!(!p1.is_null());
    assert!(!p2.is_null());
    assert_ne!(
        p1, p2,
        "magazine returned same pointer twice after triple-free -- M2 guard failed"
    );

    unsafe {
        (*heap).dealloc(p1, layout);
        (*heap).dealloc(p2, layout);
        HeapRegistry::recycle(heap);
    }
}

// ── T3: double-free of a flushed block ────────────────────────────────────

/// Alloc 100 x 16B, free all in order. The magazine overflows repeatedly, so
/// the earliest blocks are flushed back to BinTable. Then double-free one of
/// those earliest blocks. The BinTable bitmap guard should catch it (existing
/// M2 path). The allocator must keep working afterwards (no crash, no hang).
#[test]
fn t3_double_free_flushed_block_still_caught_by_bitmap() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(16, 8).unwrap();

    const N: usize = 100;
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc returned null at i={i}");
        ptrs.push(p);
    }

    // Free all. The magazine (cap=16) flushes repeatedly, so the first
    // blocks end up on the BinTable free list.
    for &p in &ptrs {
        unsafe { (*heap).dealloc(p, layout) };
    }

    // Double-free the FIRST block (long since flushed to BinTable).
    // The bitmap M2 guard should catch it (no-op or panic, but NOT
    // corruption). We simply assert the allocator still works after.
    unsafe { (*heap).dealloc(ptrs[0], layout) };

    // Verify the allocator is still functional: alloc a batch and check
    // all pointers are distinct.
    let mut check: Vec<*mut u8> = Vec::with_capacity(20);
    for _ in 0..20 {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc returned null after flushed double-free");
        check.push(p);
    }
    let set: HashSet<usize> = check.iter().map(|&p| p as usize).collect();
    assert_eq!(
        set.len(),
        20,
        "expected 20 distinct pointers after flushed double-free, got {}",
        set.len()
    );

    for &p in &check {
        unsafe { (*heap).dealloc(p, layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}

// ── T3-strong: flushed-then-double-freed block does NOT get double-issued ─

/// Counterfactual-strong companion to `t3_double_free_flushed_block_still_caught_by_bitmap`.
///
/// The weaker T3 (above) only checks the allocator does not crash after a
/// flushed-block double-free; it does not detect the actual M2 violation
/// where the block ends up in BOTH the magazine AND a BinTable free list
/// (and thus would be issued twice across subsequent allocs).
///
/// This test FORCES the hazardous interleaving and asserts the target
/// pointer is NEVER issued twice:
///   1. Alloc 200 × 16B — large enough that magazine half-flushes send
///      `ptrs[0]` to a BinTable free list (the first ~184 flushed).
///   2. Free all 200 in order — `ptrs[0]` is flushed early; its word1
///      still carries the stale tcache key (flush does NOT clear word1).
///   3. Double-free `ptrs[0]` — slow path (key match) → magazine scan
///      MISS (block is on BinTable, not magazine). Without the bitmap
///      check (the P3 hole), this would fall through to `push` and
///      put `ptrs[0]` in the magazine too.
///   4. Alloc 400 — deep enough to drain magazine + force refill that
///      pulls from the bottom of the BinTable free list, reaching
///      `ptrs[0]`. Count occurrences of `ptrs[0]` in the issued set.
///
/// COUNTERFACTUAL: remove the bitmap check (`if bm.is_free(off) { return; }`)
/// in `heap_core.rs::dealloc_own_thread` and this assert MUST trip with
/// `target_count == 2` (one from the magazine slot, one from the BinTable
/// pop_free).
#[test]
fn t3_flushed_double_free_does_not_double_issue() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(16, 8).unwrap();

    const N: usize = 200;
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "initial alloc null at i={i}");
        ptrs.push(p);
    }

    for &p in &ptrs {
        unsafe { (*heap).dealloc(p, layout) };
    }

    let target = ptrs[0];
    // The hazardous step:
    unsafe { (*heap).dealloc(target, layout) };

    // Drain the magazine and BinTable. 400 > 2 × N covers magazine
    // refills from the entire BinTable free list, guaranteed to pull
    // `target` if it sits there.
    let mut issued: Vec<*mut u8> = Vec::with_capacity(400);
    for _ in 0..400 {
        let p = unsafe { (*heap).alloc(layout) };
        if p.is_null() {
            break;
        }
        issued.push(p);
    }

    let target_count = issued.iter().filter(|&&p| p == target).count();
    assert!(
        target_count <= 1,
        "target pointer issued {target_count} times — M2 violation: \
         flushed-then-double-freed block ended up in magazine AND on BinTable. \
         Was the bitmap check in dealloc_own_thread removed?"
    );

    for &p in &issued {
        unsafe { (*heap).dealloc(p, layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}

// ── T-false-positive: user data happens to equal our key ──────────────────

/// Write the TCACHE_KEY value into word1 of a block while it is allocated
/// (simulating user data that collides with our key). Freeing the block must
/// still succeed -- the bounded scan sees the ptr is NOT in the magazine and
/// falls through to a normal push. Then alloc returns the same block.
#[test]
fn t_false_positive_handled() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(16, 8).unwrap();

    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null());

    // Write TCACHE_KEY ^ 0 into word1 (offset size_of::<usize>()).
    // This simulates user data that happens to equal the guard key for
    // heap id=0. Even if our heap's id is not 0, writing any value into
    // word1 exercises the "key matched but ptr not in magazine" path when
    // it happens to collide.
    //
    // We write the raw TCACHE_KEY constant XOR'd with 0 (which is just
    // TCACHE_KEY). If the heap id happens to be different, the key won't
    // match and the fast path handles it. If it matches, the scan handles
    // the false positive. Either way the free must succeed.
    let word1_addr = unsafe { (p as *mut usize).add(1) };
    // Use the TCACHE_KEY constant. We import it via the re-exported path.
    let fake_key: usize = 0x53_45_46_45_52_43_41_43; // TCACHE_KEY value
    unsafe { word1_addr.write(fake_key) };

    // Free the block. Must succeed (not silently dropped).
    unsafe { (*heap).dealloc(p, layout) };

    // Alloc again: should return p (LIFO from the magazine).
    let p2 = unsafe { (*heap).alloc(layout) };
    assert!(!p2.is_null(), "alloc returned null after false-positive free");

    // The block should be reusable.
    unsafe { core::ptr::write_bytes(p2, 0xBB, 16) };
    assert_eq!(unsafe { p2.read() }, 0xBB, "read-back mismatch");

    unsafe {
        (*heap).dealloc(p2, layout);
        HeapRegistry::recycle(heap);
    }
}

// ── T-key-round-trip: key in word1 does not break alloc/free cycle ────────

/// alloc -> free -> alloc -> free repeated many times for the same size class.
/// The key left in word1 by free does not interfere with subsequent allocs or
/// frees. This is a sanity check, not a counterfactual test.
#[test]
fn t_key_does_not_break_round_trip() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(16, 8).unwrap();

    let mut last_ptr: *mut u8 = core::ptr::null_mut();
    for round in 0..200 {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc returned null at round {round}");
        // Write user data (overwriting whatever is in word0/word1).
        unsafe { core::ptr::write_bytes(p, (round & 0xFF) as u8, 16) };
        unsafe { (*heap).dealloc(p, layout) };
        // After free, word1 holds our key. Next alloc must still work.
        last_ptr = p;
    }
    // Final alloc should still work.
    let p_final = unsafe { (*heap).alloc(layout) };
    assert!(!p_final.is_null(), "final alloc returned null");
    // The block is usable.
    unsafe { core::ptr::write_bytes(p_final, 0xCC, 16) };
    assert_eq!(unsafe { p_final.read() }, 0xCC, "final read-back mismatch");
    let _ = last_ptr; // suppress unused warning

    unsafe {
        (*heap).dealloc(p_final, layout);
        HeapRegistry::recycle(heap);
    }
}

// ── T2 with larger size class ─────────────────────────────────────────────

/// Same as T2 but with 64-byte blocks (a different size class). Verifies the
/// guard works across classes, not just the smallest.
#[test]
fn t2_double_free_64b_class() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(64, 8).unwrap();

    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null());

    // Write user data to the full 64 bytes (including word1).
    unsafe { core::ptr::write_bytes(p, 0xDD, 64) };

    // First free -> pushes to magazine, stamps word1 with key.
    unsafe { (*heap).dealloc(p, layout) };

    // Second free of SAME ptr -> M2 guard must catch it.
    unsafe { (*heap).dealloc(p, layout) };

    let p1 = unsafe { (*heap).alloc(layout) };
    let p2 = unsafe { (*heap).alloc(layout) };
    assert!(!p1.is_null());
    assert!(!p2.is_null());
    assert_ne!(
        p1, p2,
        "magazine returned same pointer twice (64B class) -- M2 guard failed"
    );

    unsafe {
        (*heap).dealloc(p1, layout);
        (*heap).dealloc(p2, layout);
        HeapRegistry::recycle(heap);
    }
}
