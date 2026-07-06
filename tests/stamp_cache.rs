//! OPT-C regression tests — lazy `stamp_segment_owner` cache (task #66).
//!
//! Tests that the `last_stamped_segment` fast path in `HeapCore::stamp_segment_owner`
//! is transparent to callers: correctness is identical with and without the
//! cache. All tests exercise `HeapCore` through `HeapRegistry::claim` (the
//! same path `SeferAlloc` uses) so the stamp-cache code is exercised directly.
//!
//! Three tests:
//!
//! 1. `alloc_loop_in_one_segment_does_not_panic` — 200 allocations from one
//!    size class must not panic. Exercises the fast path (cache hit on every
//!    alloc after the first).
//!
//! 2. `alloc_across_classes_works` — allocations from multiple size classes
//!    force the `AllocCore` to carve from different segments. Each time the
//!    active segment changes the cache misses and the slow path re-stamps.
//!    All allocations must be non-null and readable.
//!
//! 3. `stamp_cache_writes_owner_thread_free` (xthread-only) — allocs on a heap
//!    via `HeapCore` (through the registry), then reads back the
//!    `owner_thread_free` stamp via `dbg_owner_id_for` to confirm the OPT-C fast
//!    path did not suppress the first slow-path stamp. (An earlier version
//!    verified this end-to-end via the now-removed `Heap::dealloc_any_thread`
//!    cross-thread free; that leg was rewritten because `HeapCore` does not
//!    expose a public cross-thread-free entry point.)

#![cfg(feature = "alloc-global")]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise all tests in this file: the registry is a process-global static;
// a parallel claim race makes slot-index semantics unpredictable and is the
// job of loom (not these sequential-contract tests).
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

// ─── test 1 ────────────────────────────────────────────────────────────────

/// Smoke-test: 200 allocations from the same size class should all succeed
/// and be usable. The stamp-cache fast path is exercised on allocations 2-200.
#[test]
fn alloc_loop_in_one_segment_does_not_panic() {
    let _serial = SerialGuard::acquire();
    // Ensure the registry is bootstrapped.
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let layout = Layout::from_size_align(64, 8).unwrap();
    let mut ptrs: Vec<*mut u8> = Vec::new();

    for i in 0u8..200 {
        // SAFETY: heap is a live slot; single-writer (this thread owns it).
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc returned null at iteration {i}");
        // Write + read-back to verify the pointer is usable.
        unsafe {
            core::ptr::write_bytes(p, i, 64);
            assert_eq!(p.read(), i, "read-back mismatch at iteration {i}");
        }
        ptrs.push(p);
    }

    // Free all (own-thread).
    for p in ptrs {
        unsafe { (*heap).dealloc(p, layout) };
    }

    // SAFETY: we are done with this heap core pointer; return the slot.
    unsafe { HeapRegistry::recycle(heap) };
}

// ─── test 2 ────────────────────────────────────────────────────────────────

/// Alloc from multiple size classes so the substrate carves from different
/// segments. Each segment switch is a cache miss → slow path stamps and
/// updates `last_stamped_segment`. Correctness must hold across all of them.
#[test]
fn alloc_across_classes_works() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // Sizes that span several different size classes.
    let sizes: &[usize] = &[16, 32, 64, 128, 256, 512, 1024, 2048];
    let n_per_size: usize = 16;

    let mut all_ptrs: Vec<(*mut u8, Layout)> = Vec::new();

    for &sz in sizes {
        let layout = Layout::from_size_align(sz, 8).unwrap();
        for i in 0u8..n_per_size as u8 {
            let p = unsafe { (*heap).alloc(layout) };
            assert!(!p.is_null(), "alloc({sz}) returned null at i={i}");
            unsafe {
                core::ptr::write_bytes(p, i.wrapping_add(sz as u8), sz);
                assert_eq!(
                    p.read(),
                    i.wrapping_add(sz as u8),
                    "read-back mismatch at sz={sz} i={i}"
                );
            }
            all_ptrs.push((p, layout));
        }
    }

    // Free everything (own-thread).
    for (p, layout) in all_ptrs {
        unsafe { (*heap).dealloc(p, layout) };
    }

    unsafe { HeapRegistry::recycle(heap) };
}

// ─── test 3 (xthread only) ─────────────────────────────────────────────────

/// Stamp-cache verification under `alloc-xthread`: allocate on one heap's
/// `HeapCore` (via the registry), then confirm `stamp_segment_owner` (even via
/// the OPT-C fast path on subsequent allocs) correctly writes
/// `owner_thread_free` into the segment header at least once (on the very first
/// slow-path call).
///
/// The original version of this test verified the stamp end-to-end by
/// cross-thread freeing the blocks via the now-removed `Heap::dealloc_any_thread`
/// API. `HeapCore` does not expose a public cross-thread-free entry point (the
/// cross-thread routing lives inside `SeferAlloc::dealloc` / the private
/// `dealloc_routing`), so the end-to-end cross-thread leg cannot be faithfully
/// reproduced without inventing new public API. This rewrite preserves the
/// stamp-cache coverage (the OPT-C fast path's correctness) by verifying the
/// stamp directly via `dbg_owner_id_for`, which reads back the
/// `owner_thread_free` field the stamp wrote.
///
/// Gated on `alloc-global + alloc-xthread`.
#[cfg(feature = "alloc-xthread")]
#[test]
fn stamp_cache_writes_owner_thread_free() {
    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let layout = Layout::from_size_align(64, 8).unwrap();

    // Allocate a batch; the first alloc from a new segment takes the slow path
    // and stamps `owner_thread_free`. Subsequent allocs from the SAME segment
    // hit the OPT-C cache (fast path) and skip the re-stamp.
    const N: usize = 64;
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        // SAFETY: heap is a live slot; single-writer (this thread owns it).
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc returned null at i={i}");
        ptrs.push(p);
    }

    // Every allocated block's segment must have a non-null `owner_thread_free`
    // stamp — the OPT-C fast path must not have incorrectly suppressed the
    // first slow-path stamp. `dbg_owner_id_for` returns the owner slot id by
    // reading the stamped field; if the stamp were missing it would return
    // `None` (or a mismatched id).
    for &p in &ptrs {
        let owner = unsafe { (*heap).dbg_owner_id_for(p) };
        assert!(
            owner.is_some(),
            "stamp-cache regression: ptr {:p} has no owner_thread_free stamp \
             (OPT-C fast path suppressed the first slow-path stamp)",
            p
        );
    }

    // Free all (own-thread).
    for p in ptrs {
        unsafe { (*heap).dealloc(p, layout) };
    }

    unsafe { HeapRegistry::recycle(heap) };
}
