//! Regression test for task #132 (Defect 1) — the explicit `Heap`/
//! `with_heap` public face used to LEAK cross-thread-freed Large segments
//! permanently, unlike the `SeferAlloc`/`HeapCore` face (which has the A1
//! fix — see `tests/regression_xthread_large_free_no_leak.rs`).
//!
//! ## What this guards against
//!
//! Pre-fix, `Heap::dealloc_any_thread`'s `SegmentKind::Large` branch was a
//! bare `return` whenever the freeing thread was NOT the segment's owner:
//! the whole OS reservation (>= a full `SEGMENT`) stayed mapped forever, and
//! `Heap::alloc`'s large-request path never revisited it — a silent,
//! permanent leak, exactly the same shape as the pre-A1 `HeapCore` bug.
//!
//! Post-fix (task #132), `Heap::dealloc_any_thread` and `HeapCore::dealloc`
//! (via `dealloc_routing`) share the SAME extracted primitive
//! (`alloc_core::deferred_large::{push,drain}_large_deferred_free`) and the
//! SAME diagnostic counter (`DBG_LARGE_XTHREAD_RECLAIMED`, defined at
//! `sefer_alloc::alloc_core::deferred_large::DBG_LARGE_XTHREAD_RECLAIMED`
//! and re-exported at `sefer_alloc::registry::DBG_LARGE_XTHREAD_RECLAIMED`
//! for the `HeapCore` face), so this test uses the identical oracle as the
//! `HeapCore`-face regression test.
//!
//! ## Counterfactual (non-vacuity)
//!
//! Temporarily reverting `Heap::dealloc_any_thread`'s Large branch back to a
//! bare `return` (the pre-#132 no-op) makes this test FAIL: the counter's
//! delta stays `0` after the owner's second round of large allocations,
//! because nothing ever pushes the remotely-freed segments onto the owner's
//! deferred-free stack for `Heap::alloc` to drain. This was verified by hand
//! during development (see the task report).

#![cfg(all(feature = "alloc", feature = "alloc-xthread"))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use sefer_alloc::alloc_core::deferred_large::DBG_LARGE_XTHREAD_RECLAIMED;
use sefer_alloc::Heap;

// Serialise: `DBG_LARGE_XTHREAD_RECLAIMED` is a process-global static shared
// with the `HeapCore`-face regression tests; concurrent test-fn execution
// across the WHOLE binary (all `tests/*.rs` files run in one process under
// `cargo test`) would make the delta assertion flaky if another test bumped
// it concurrently. This uses a dedicated lock file (a `std::sync` primitive
// scoped to this binary) — cross-binary races with the other regression
// test file are still possible under `cargo test`'s default parallel
// harness, but each test binary is a SEPARATE process, so the static is NOT
// actually shared across `tests/*.rs` files (each integration test file
// compiles to its own binary). This guard only serialises multiple `#[test]`
// fns WITHIN this file.
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

#[test]
fn heap_face_xthread_large_free_reclaims_segments_no_leak() {
    let _g = SerialGuard::acquire();

    const N: usize = 50;
    // 512 KiB — comfortably above the small-class ceiling, so every
    // allocation is unambiguously routed to the Large path.
    const SIZE: usize = 512 * 1024;
    let layout = Layout::from_size_align(SIZE, 8).unwrap();

    let mut heap = Heap::new().expect("Heap::new bootstrap");

    let baseline = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);

    // ── Round 1: owner allocates N large blocks, writes a pattern. ─────────
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "round 1 alloc[{i}] returned null");
        unsafe {
            std::ptr::write_bytes(p, (i & 0xFF) as u8, SIZE);
        }
        ptrs.push(p);
    }

    // ── A REMOTE thread frees every block via `Heap::dealloc_any_thread`
    // (the public cross-thread-safe entry point — no `Heap` receiver, exactly
    // matching how a task migrated to another worker thread would free a
    // buffer it did not allocate).
    let addrs: Vec<usize> = ptrs.iter().map(|&p| p as usize).collect();
    thread::spawn(move || {
        for addr in addrs {
            let p = addr as *mut u8;
            Heap::dealloc_any_thread(p, layout);
        }
    })
    .join()
    .unwrap();

    // ── Round 2: owner allocates N more large blocks. This forces
    // `Heap::alloc`'s large-request path to run repeatedly, which is where
    // the #132 fix drains the deferred-free stack and reclaims the segments
    // the remote thread queued.
    let mut ptrs2: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = heap.alloc(layout);
        assert!(!p.is_null(), "round 2 alloc[{i}] returned null");
        unsafe {
            std::ptr::write_bytes(p, 0xEE, SIZE);
            assert_eq!(p.read(), 0xEE, "round 2 alloc[{i}] read-back mismatch");
        }
        ptrs2.push(p);
    }

    // ── The key assertion: segments were actually reclaimed, not leaked. ──
    let reclaimed = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed) - baseline;
    assert!(
        reclaimed > 0,
        "DBG_LARGE_XTHREAD_RECLAIMED delta is 0 — the Heap face leaked \
         cross-thread-freed Large segments (task #132 regression). \
         Expected > 0 (up to {N}), got 0."
    );

    // Cleanup: free round-2 blocks (own-thread path).
    for &p in &ptrs2 {
        heap.dealloc(p, layout);
    }
    // `heap` drops here — abandonment-leak under alloc-xthread (expected,
    // documented behaviour, not what this test is checking).
}
