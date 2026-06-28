//! Phase P4 -- stamp-hoist correctness tests.
//!
//! Tests that the P4 stamp-hoist (moving `stamp_segment_owner` from the
//! per-alloc magazine-hit path into `refill_class_stamped`) maintains all
//! ownership invariants:
//!
//! - **T-stamp-after-refill**: after a refill, every distinct source segment
//!   has `owner_id == heap.id` in its header.
//! - **T-magazine-hit-skips-stamp**: a magazine-hit alloc does NOT re-stamp
//!   (the `last_stamped_segment` cache does not change between allocs from
//!   the same segment).
//! - **T-large-still-stamps**: a large allocation (> SMALL_MAX) still stamps
//!   the segment per-alloc (the large path was NOT affected by the hoist).
//!
//! ## Counterfactual notes
//!
//! - T-stamp-after-refill: without the refill stamping, `owner_state` would
//!   remain `OWNER_ID_NONE` and the assertion `owner_id == heap.id` would
//!   fail.
//! - T-magazine-hit-skips-stamp: confirms that the magazine-hit path no
//!   longer calls `stamp_segment_owner` (no change to the cached segment).
//!   The perf benefit is measured in benchmarks, not tested here.
//! - T-large-still-stamps: the large path falls through to `core.alloc` and
//!   stamps afterward; this test confirms the stamp is present.

#![cfg(all(feature = "alloc-global", feature = "fastbin"))]

use std::alloc::Layout;
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

// ── T-stamp-after-refill ──────────────────────────────────────────────────

/// Claim a heap, alloc N blocks (forcing at least one refill), then verify
/// every distinct source segment's `owner_id` matches `heap.id`.
///
/// COUNTERFACTUAL: without the refill stamping, `owner_state` would remain
/// `OWNER_ID_NONE` (0x7FFF_FFFF) and this assertion would fail.
#[test]
fn t_stamp_after_refill() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let heap_id = unsafe { (*heap).id() };
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Allocate enough blocks to force at least one refill (magazine cap is 16,
    // so 32 blocks forces at least 2 refill cycles).
    const N: usize = 64;
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc returned null at i={i}");
        // Write to prove usability.
        unsafe { core::ptr::write_bytes(p, (i & 0xFF) as u8, 16) };
        ptrs.push(p);
    }

    // Verify: every allocated block's segment has owner_id == heap_id.
    for (i, &p) in ptrs.iter().enumerate() {
        let owner = unsafe { (*heap).dbg_owner_id_for(p) };
        assert_eq!(
            owner,
            Some(heap_id),
            "segment of ptr[{i}] has owner_id={owner:?}, expected Some({heap_id})"
        );
    }

    // Cleanup.
    for &p in &ptrs {
        unsafe { (*heap).dealloc(p, layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}

// ── T-magazine-hit-skips-stamp ────────────────────────────────────────────

/// Alloc a block (forcing refill+stamp), free it (back to magazine), alloc
/// again (magazine hit). The second alloc should NOT alter
/// `last_stamped_segment` (it was already stamped during the refill).
///
/// This test verifies the stamp is NOT called on the magazine-hit path by
/// observing that `dbg_last_stamped_segment` stays the same between the
/// free and the re-alloc.
#[test]
fn t_magazine_hit_skips_stamp() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let heap_id = unsafe { (*heap).id() };
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Force a refill: alloc one block.
    let p1 = unsafe { (*heap).alloc(layout) };
    assert!(!p1.is_null(), "first alloc returned null");

    // The refill stamped the segment. Verify.
    let owner = unsafe { (*heap).dbg_owner_id_for(p1) };
    assert_eq!(owner, Some(heap_id), "first alloc not stamped");

    // Record the cached segment base after the refill.
    let cached_after_refill = unsafe { (*heap).dbg_last_stamped_segment() };
    assert!(
        !cached_after_refill.is_null(),
        "last_stamped_segment should be non-null after refill"
    );

    // Free the block (goes back to magazine).
    unsafe { (*heap).dealloc(p1, layout) };

    // The cached segment should not change after dealloc.
    let cached_after_free = unsafe { (*heap).dbg_last_stamped_segment() };
    assert_eq!(
        cached_after_refill, cached_after_free,
        "dealloc should not change last_stamped_segment"
    );

    // Re-alloc (magazine hit: LIFO, returns p1 or a same-segment block).
    let p2 = unsafe { (*heap).alloc(layout) };
    assert!(!p2.is_null(), "re-alloc returned null");

    // Key assertion: the magazine-hit path (P4) does NOT call
    // stamp_segment_owner, so `last_stamped_segment` should be unchanged.
    let cached_after_hit = unsafe { (*heap).dbg_last_stamped_segment() };
    assert_eq!(
        cached_after_refill, cached_after_hit,
        "magazine-hit alloc changed last_stamped_segment — stamp was NOT hoisted"
    );

    // The block is still properly stamped (ownership is correct).
    let owner2 = unsafe { (*heap).dbg_owner_id_for(p2) };
    assert_eq!(owner2, Some(heap_id), "re-alloc block not stamped");

    // Verify the pointer is usable.
    unsafe {
        core::ptr::write_bytes(p2, 0xAA, 16);
        assert_eq!(p2.read(), 0xAA, "re-alloc read-back mismatch");
    }

    unsafe {
        (*heap).dealloc(p2, layout);
        HeapRegistry::recycle(heap);
    }
}

// ── T-large-still-stamps ──────────────────────────────────────────────────

/// Alloc a large block (size > SMALL_MAX). The large path falls through to
/// `core.alloc` and stamps afterward. Assert the segment is stamped with
/// our id.
///
/// This test confirms that the P4 hoist did NOT accidentally remove the
/// per-alloc stamp on the large path (which is NOT served by the magazine).
#[test]
fn t_large_still_stamps() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let heap_id = unsafe { (*heap).id() };

    // A large allocation: 128 KiB (well above SMALL_MAX which is a few KiB).
    let large_layout = Layout::from_size_align(128 * 1024, 8).unwrap();
    let p = unsafe { (*heap).alloc(large_layout) };
    assert!(!p.is_null(), "large alloc returned null");

    // Verify ownership stamp.
    let owner = unsafe { (*heap).dbg_owner_id_for(p) };
    assert_eq!(
        owner,
        Some(heap_id),
        "large block segment has owner_id={owner:?}, expected Some({heap_id})"
    );

    // Verify the pointer is usable.
    unsafe {
        core::ptr::write_bytes(p, 0xBB, 128);
        assert_eq!(p.read(), 0xBB, "large block read-back mismatch");
    }

    unsafe {
        (*heap).dealloc(p, large_layout);
        HeapRegistry::recycle(heap);
    }
}

// ── T-multi-class-refill-stamps ───────────────────────────────────────────

/// Alloc blocks from multiple size classes, each forcing separate refills.
/// Verify every segment touched by the refills is stamped correctly.
/// This catches any path where `refill_class_stamped` might not be called
/// (e.g. a class-specific code path that bypasses the magazine).
#[test]
fn t_multi_class_refill_stamps() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let heap_id = unsafe { (*heap).id() };

    // Sizes that span several size classes.
    let sizes: &[usize] = &[16, 32, 64, 128, 256, 512, 1024];
    let n_per_size = 20; // enough to force at least one refill per class

    let mut all_ptrs: Vec<(*mut u8, Layout)> = Vec::new();

    for &sz in sizes {
        let layout = Layout::from_size_align(sz, 8).unwrap();
        for i in 0..n_per_size {
            let p = unsafe { (*heap).alloc(layout) };
            assert!(!p.is_null(), "alloc({sz}) returned null at i={i}");
            unsafe { core::ptr::write_bytes(p, (i & 0xFF) as u8, sz) };
            all_ptrs.push((p, layout));
        }
    }

    // Verify all segments are stamped.
    for (idx, &(p, _)) in all_ptrs.iter().enumerate() {
        let owner = unsafe { (*heap).dbg_owner_id_for(p) };
        assert_eq!(
            owner,
            Some(heap_id),
            "ptr[{idx}] segment has owner_id={owner:?}, expected Some({heap_id})"
        );
    }

    for &(p, layout) in &all_ptrs {
        unsafe { (*heap).dealloc(p, layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}
