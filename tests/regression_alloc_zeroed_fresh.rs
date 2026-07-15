//! Regression (task C5, 0.3.0): `alloc_zeroed` must return all-zero memory,
//! both for a genuinely fresh allocation and for a REUSED block (one that
//! previously held non-zero user data and was freed, then re-served by a
//! later `alloc_zeroed` of the same shape).
//!
//! ## Why this test exists independent of any C5 optimisation
//!
//! C5 investigated skipping `Node::zero` in `alloc_zeroed` when the
//! underlying memory is already known-zero (freshly mmap'd/VirtualAlloc'd
//! OS pages are zero-filled by the OS; re-zeroing them is redundant work).
//! That optimisation is easy to get subtly wrong: the allocator freely
//! reuses blocks (small free-list pop, large-segment cache under
//! `alloc-decommit`, decommit/recommit cycles) and a reused block is NOT
//! guaranteed zero — it may still hold the previous occupant's bytes. If an
//! optimisation ever special-cases "skip the zero" based on an unreliable
//! fresh/reused signal, this is the test that catches it: it is written to
//! be independently meaningful (mandated) regardless of whether the C5
//! optimisation was applied, deferred, or reverted.
//!
//! ## This test
//!
//! 1. **All-zero on first touch**: `alloc_zeroed` at several sizes
//!    (small-class and large/dedicated-segment) must return memory that
//!    reads back as entirely zero.
//! 2. **All-zero after reuse**: `alloc_zeroed` → write a non-zero pattern
//!    into every byte → `dealloc` → `alloc_zeroed` of the SAME shape again.
//!    If the allocator serves the freed block back (the common case for a
//!    small class free-list pop, and — with `alloc-decommit` — the
//!    large-segment cache), the returned memory must STILL read back as all
//!    zero. This is the counterfactual-sensitive half: any "skip
//!    `Node::zero` because the memory looks fresh" shortcut that fails to
//!    account for reuse would leave the old 0xEE pattern in place here and
//!    this assertion would fail.

#![cfg(feature = "alloc-global")]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise all tests in this file: the registry is a process-global static
// (matching the discipline in the other `registry`-level regression tests).
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

fn assert_all_zero(ptr: *mut u8, len: usize, ctx: &str) {
    let bytes = unsafe { core::slice::from_raw_parts(ptr, len) };
    assert!(
        bytes.iter().all(|&b| b == 0),
        "{ctx}: alloc_zeroed returned non-zero memory (first non-zero byte at {:?})",
        bytes.iter().position(|&b| b != 0)
    );
}

/// (1) All-zero on first touch, across small-class and large/dedicated-
/// segment shapes.
#[test]
fn alloc_zeroed_is_all_zero_fresh() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // Small-class shapes.
    let small_sizes = [1usize, 16, 64, 200, 4096];
    for &size in &small_sizes {
        let layout = Layout::from_size_align(size, 1).unwrap();
        let p = unsafe { (*heap).alloc_zeroed(layout) };
        assert!(!p.is_null(), "alloc_zeroed({size}) returned null");
        assert_all_zero(p, size, &format!("small size={size}"));
        unsafe { (*heap).dealloc(p, layout) };
    }

    // Large / dedicated-segment shape (well above SMALL_MAX ~253 KiB).
    let large_size = 1024 * 1024; // 1 MiB
    let large_layout = Layout::from_size_align(large_size, 1).unwrap();
    let pl = unsafe { (*heap).alloc_zeroed(large_layout) };
    assert!(!pl.is_null(), "alloc_zeroed(1 MiB) returned null");
    assert_all_zero(pl, large_size, "large 1 MiB");
    unsafe { (*heap).dealloc(pl, large_layout) };

    unsafe { HeapRegistry::recycle(heap) };
}

/// (2) All-zero after reuse: alloc_zeroed -> write non-zero -> dealloc ->
/// alloc_zeroed (same shape) -> must be all-zero again. This is the
/// load-bearing counterfactual-sensitive assertion for any future C5
/// fresh-vs-reused optimisation.
#[test]
fn alloc_zeroed_is_all_zero_after_reuse() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // Small-class reuse: pop the same free-list slot back after a free.
    let small_sizes = [16usize, 64, 200, 4096];
    for &size in &small_sizes {
        let layout = Layout::from_size_align(size, 1).unwrap();

        let p1 = unsafe { (*heap).alloc_zeroed(layout) };
        assert!(!p1.is_null(), "first alloc_zeroed({size}) returned null");
        assert_all_zero(p1, size, &format!("small size={size} (first touch)"));

        // Dirty every byte with a non-zero pattern.
        unsafe { core::ptr::write_bytes(p1, 0xEE, size) };

        unsafe { (*heap).dealloc(p1, layout) };

        // Re-allocate the same shape. The allocator is free to serve the
        // same physical block back (own-segment free-list pop is the
        // expected fast path for a same-thread immediate realloc-of-same-
        // class). Either way, alloc_zeroed's CONTRACT requires all-zero
        // memory.
        let p2 = unsafe { (*heap).alloc_zeroed(layout) };
        assert!(!p2.is_null(), "second alloc_zeroed({size}) returned null");
        assert_all_zero(p2, size, &format!("small size={size} (after reuse)"));

        unsafe { (*heap).dealloc(p2, layout) };
    }

    // Large/dedicated-segment reuse (exercises the alloc-decommit large-
    // cache path when that feature is enabled; without it, a fresh OS
    // mmap/VirtualAlloc is still required to be zero -- either way the
    // contract must hold).
    let large_size = 2 * 1024 * 1024; // 2 MiB, above SMALL_MAX even under medium-classes (1 MiB)
    let large_layout = Layout::from_size_align(large_size, 1).unwrap();

    let l1 = unsafe { (*heap).alloc_zeroed(large_layout) };
    assert!(!l1.is_null(), "first large alloc_zeroed returned null");
    assert_all_zero(l1, large_size, "large (first touch)");
    unsafe { core::ptr::write_bytes(l1, 0xEE, large_size) };
    unsafe { (*heap).dealloc(l1, large_layout) };

    let l2 = unsafe { (*heap).alloc_zeroed(large_layout) };
    assert!(!l2.is_null(), "second large alloc_zeroed returned null");
    assert_all_zero(l2, large_size, "large (after reuse)");
    unsafe { (*heap).dealloc(l2, large_layout) };

    unsafe { HeapRegistry::recycle(heap) };
}
