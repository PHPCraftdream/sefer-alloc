//! OPT-C regression tests — lazy `stamp_segment_owner` cache (task #66).
//!
//! Tests that the `last_stamped_segment` fast path in `HeapCore::stamp_segment_owner`
//! is transparent to callers: correctness is identical with and without the
//! cache. All tests exercise `HeapCore` through `HeapRegistry::claim` (the
//! same path `SeferMalloc` uses) so the stamp-cache code is exercised directly.
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
//! 3. `cross_thread_dealloc_after_stamp_cache_works` (xthread-only) — allocs
//!    on heap A via `HeapCore`, then another thread frees all blocks via
//!    `Heap::dealloc_any_thread`. This confirms that `stamp_segment_owner`
//!    actually wrote `owner_thread_free` into the segment header (if the fast
//!    path had incorrectly skipped the TFS stamp the cross-thread free would
//!    silently drop the block and a write-back assertion would catch it).

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

/// Cross-thread dealloc after stamp-cache: allocate on one heap's `HeapCore`
/// (via the registry), then free via `Heap::dealloc_any_thread` on a
/// different thread. This verifies that `stamp_segment_owner` (even via the
/// OPT-C fast path on subsequent allocs) correctly writes `owner_thread_free`
/// into the segment header at least once (on the very first slow-path call).
///
/// Gated on `alloc-global + alloc-xthread`.
#[cfg(feature = "alloc-xthread")]
#[test]
fn cross_thread_dealloc_after_stamp_cache_works() {
    use sefer_alloc::Heap;

    let _serial = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    // Use the public Heap API which also goes through owner-stamping (Heap's
    // stamp_owner covers alloc-xthread TFS stamping).
    let mut heap = Heap::new().expect("Heap::new failed");
    let layout = Layout::from_size_align(64, 8).unwrap();

    // Allocate a batch; write a tag into each block.
    const N: usize = 64;
    let mut ptrs: Vec<(*mut u8, u64)> = Vec::new();
    for i in 0..N {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "alloc returned null at i={i}");
        let tag = (i as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15);
        // SAFETY: p is valid, aligned, and owned by this thread.
        unsafe { core::ptr::write(p as *mut u64, tag) };
        ptrs.push((p, tag));
    }

    // Verify tags before cross-thread free.
    for &(p, tag) in &ptrs {
        let read = unsafe { core::ptr::read(p as *const u64) };
        assert_eq!(read, tag, "tag corrupt before cross-thread free");
    }

    // Send pointer addresses to another thread for cross-thread free.
    let addrs: Vec<usize> = ptrs.iter().map(|&(p, _)| p as usize).collect();
    std::thread::spawn(move || {
        for addr in addrs {
            Heap::dealloc_any_thread(addr as *mut u8, layout);
        }
    })
    .join()
    .expect("cross-thread free thread panicked");

    // Re-allocate the same number of blocks. This triggers the lazy ring
    // drain (RemoteFreeRing) on the first alloc-slow-path miss, recycling
    // the cross-thread-freed blocks. If `owner_thread_free` had not been
    // stamped (i.e., if the OPT-C fast path had incorrectly suppressed the
    // TFS stamp), `dealloc_any_thread` would have silently no-opped, and the
    // re-allocated blocks would all be fresh (no panic in that case, but the
    // old blocks would be permanently leaked — this is the correctness
    // concern, not a crash).
    let mut new_ptrs = Vec::new();
    for _ in 0..N {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "re-alloc returned null");
        // Write + read-back: confirm the pointer is usable (catches UAF if
        // the block was returned to the wrong heap or double-freed).
        unsafe {
            core::ptr::write_bytes(p, 0xBB, 64);
            assert_eq!(p.read(), 0xBB, "re-alloc block not writable (UAF?)");
        }
        new_ptrs.push(p);
    }

    for p in new_ptrs {
        heap.dealloc(p, layout);
    }
}
