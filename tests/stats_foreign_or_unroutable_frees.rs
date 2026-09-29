//! `AllocStats::foreign_or_unroutable_frees` (xxs R5-01): a live, always-on
//! counter of frees dropped for violating the `GlobalAlloc` contract,
//! incremented by `HeapCore::dealloc_foreign_routing`'s cold drop branches
//! (no `alloc-stats` needed).
//!
//! Probe: a real Large block freed cross-thread with a WRONG `Layout` size is
//! rejected by the layout-consistency check. That branch is a pure no-op (a
//! read-only header comparison), so the block stays live and the correct-layout
//! free that follows is its one real free. A synthetic non-sefer pointer would
//! be unsound here: the routing reads the candidate segment header and can fault
//! on unmapped memory. The healthy-run counterpart (delta 0) lives in
//! `regression_r5_01_alloc_global_cross_thread.rs`.

#![cfg(feature = "alloc-global")]

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::sync_channel;
use std::thread;

use sefer_alloc::SeferAlloc;

#[global_allocator]
static GLOBAL: SeferAlloc = SeferAlloc::new();

// `AllocStats` counters are process-wide — serialise against the other test
// in this binary (same discipline as `tests/regression_r5_01_alloc_global_cross_thread.rs`).
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

/// A Large block allocated on thread A, freed cross-thread on thread B with a
/// WRONG `Layout` size, must be DROPPED (not queued/reclaimed) and counted in
/// `foreign_or_unroutable_frees`; freeing it again afterward with the CORRECT
/// layout must succeed cleanly (nothing leaked, heap stays usable).
#[test]
fn wrong_layout_cross_thread_free_is_dropped_and_counted() {
    let _guard = SerialGuard::acquire();

    // 2 MiB — comfortably above SMALL_MAX in every feature combination
    // (even `medium-classes`), unambiguously routed to Large.
    const SIZE: usize = 2 * 1024 * 1024;
    let real_layout = Layout::from_size_align(SIZE, 8).unwrap();
    // A deliberately wrong size for the same pointer, mirroring
    // `tests/regression_xthread_large_free_layout_mismatch.rs`'s `wrong_layout`.
    let wrong_layout = Layout::from_size_align(SIZE / 4, 8).unwrap();

    let (tx, rx) = sync_channel::<usize>(0);
    let producer = thread::spawn(move || {
        // SAFETY: `real_layout` is a valid non-zero `Layout`.
        let p = unsafe { GLOBAL.alloc(real_layout) };
        assert!(!p.is_null(), "producer allocation failed");
        // SAFETY: `p` is a fresh live `SIZE`-byte allocation.
        unsafe { std::ptr::write_bytes(p, 0xCC, SIZE) };
        tx.send(p as usize).expect("consumer channel closed early");
    });
    let addr = rx.recv().expect("producer did not send a pointer");
    producer.join().expect("producer thread panicked");
    let p = addr as *mut u8;

    let before = GLOBAL.stats().foreign_or_unroutable_frees;

    // Cross-thread free with the WRONG layout size. Per this file's module
    // doc, this is a documented `GlobalAlloc::dealloc` contract violation
    // that is sound here because the mismatched-layout branch is a pure
    // no-op (read-only header check, no write, no state mutation) — `p`
    // stays live and un-freed.
    // SAFETY: `p` is a live pointer returned by the `alloc` above, not yet
    // freed (this call is provably a no-op); the mismatched `wrong_layout`
    // is the exact probe this test exists to exercise.
    unsafe { GLOBAL.dealloc(p, wrong_layout) };

    // The segment must be untouched by the dropped free.
    // SAFETY: `p` is still the live, un-freed allocation from above.
    let intact = unsafe { std::slice::from_raw_parts(p, SIZE) }
        .iter()
        .all(|&b| b == 0xCC);
    assert!(intact, "segment corrupted by a dropped free");

    let after = GLOBAL.stats().foreign_or_unroutable_frees;
    assert!(
        after > before,
        "wrong-layout cross-thread free was not counted as dropped: \
         before={before}, after={after}"
    );

    // The block was never actually freed above (the wrong-layout call was a
    // no-op), so this is the ONE real free — not a double-free.
    // SAFETY: `p` is the live allocation from `real_layout`'s `alloc` above,
    // freed here exactly once with its matching layout.
    unsafe { GLOBAL.dealloc(p, real_layout) };

    // Heap stays fully usable afterward — nothing leaked into an unusable
    // state.
    // SAFETY: `real_layout` is a valid non-zero `Layout`.
    let p2 = unsafe { GLOBAL.alloc(real_layout) };
    assert!(
        !p2.is_null(),
        "heap unusable after the dropped mismatched free"
    );
    // SAFETY: `p2` is the live allocation just returned, freed once here.
    unsafe { GLOBAL.dealloc(p2, real_layout) };
}
