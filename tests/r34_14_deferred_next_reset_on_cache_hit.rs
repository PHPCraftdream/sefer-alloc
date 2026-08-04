//! R34-14 (task #533): regression test for the `deferred_next` carry-forward
//! leak bug in the large-cache hit path.
//!
//! ## What this guards against
//!
//! Commit eb2463a (F12, task #498) replaced the large-cache hit path's
//! full-struct `Node::write_struct` with four targeted field writes
//! (`magic`/`large_size`/`large_align`/`bump`). The full-struct write used
//! to reset `deferred_next` to `ABANDONED_TAIL` (via `SegmentHeader::large`'s
//! constructor); the targeted writes left it carried forward from the
//! segment's prior lifecycle.
//!
//! A segment that went through the deferred-large-free path retains a
//! non-`ABANDONED_TAIL` `deferred_next` (the link value set by
//! `push_large_deferred_free`). After drain → reclaim → cache deposit →
//! cache-hit reuse, this stale value persists. When the reused segment is
//! subsequently freed cross-thread, `push_large_deferred_free`'s CAS from
//! `ABANDONED_TAIL` FAILS — the push is silently dropped as a "double-push"
//! → the segment is permanently leaked.
//!
//! R34-14 fixes this by resetting `deferred_next` to `ABANDONED_TAIL` before
//! `register()` on the hit path.
//!
//! ## Test shape
//!
//! 1. Owner allocates a 2 MiB Large segment A.
//! 2. A remote thread frees A cross-thread (deferred-large push —
//!    `deferred_next` transitions from `ABANDONED_TAIL` to a link value).
//! 3. Owner allocates again → drain reclaims A → deposits to cache →
//!    cache HIT → reuses A (R34-14 resets `deferred_next` to
//!    `ABANDONED_TAIL` before register).
//! 4. Record `baseline = DBG_LARGE_XTHREAD_RECLAIMED`.
//! 5. A remote thread frees the reused A cross-thread again.
//! 6. Owner allocates again → drain → check
//!    `DBG_LARGE_XTHREAD_RECLAIMED > baseline`.
//!
//! ## Counterfactual (non-vacuity)
//!
//! Without the R34-14 fix, step 5's push CAS fails (stale `deferred_next`),
//! the free is dropped, and step 6's drain finds nothing for A →
//! `DBG_LARGE_XTHREAD_RECLAIMED` does NOT advance → the test FAILS.

#![cfg(all(
    all(
        feature = "alloc-global",
        feature = "alloc-xthread",
        feature = "alloc-decommit"
    ),
    feature = "internals"
))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use sefer_alloc::registry::{bootstrap, HeapRegistry, DBG_LARGE_XTHREAD_RECLAIMED};

// Serialise: `DBG_LARGE_XTHREAD_RECLAIMED` is process-global.
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

// 2 MiB — safely above `SMALL_MAX` even under `medium-classes`.
const SIZE: usize = 2 * 1024 * 1024;

#[test]
fn deferred_next_reset_on_cache_hit_second_xthread_free_is_not_dropped() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    let layout = Layout::from_size_align(SIZE, 8).unwrap();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    // ── Step 1: owner allocates segment A (fresh, via alloc_large_slow). ──
    let a = unsafe { (*heap).alloc(layout) };
    assert!(!a.is_null(), "step 1 alloc returned null");

    // ── Step 2: remote thread frees A cross-thread. ──
    let addr = a as usize;
    thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote = HeapRegistry::claim();
        assert!(!remote.is_null(), "remote claim failed");
        unsafe { (*remote).dealloc(addr as *mut u8, layout) };
        unsafe { HeapRegistry::recycle(remote) };
    })
    .join()
    .unwrap();

    // ── Step 3: owner allocates again → drain reclaims A → deposits to
    // cache → cache HIT → reuses A. ──
    let reused = unsafe { (*heap).alloc(layout) };
    assert!(!reused.is_null(), "step 3 alloc returned null");

    // ── Step 4: record baseline AFTER the first reclaim cycle. ──
    let baseline = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);
    assert!(
        baseline > 0,
        "first drain should have reclaimed at least one segment"
    );

    // ── Step 5: remote thread frees the reused segment cross-thread AGAIN.
    // Without the R34-14 fix, this push CAS fails (stale deferred_next) and
    // the free is silently dropped → permanent leak. ──
    let addr2 = reused as usize;
    thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote = HeapRegistry::claim();
        assert!(!remote.is_null(), "remote claim failed (2)");
        unsafe { (*remote).dealloc(addr2 as *mut u8, layout) };
        unsafe { HeapRegistry::recycle(remote) };
    })
    .join()
    .unwrap();

    // ── Step 6: owner allocates again → drain should reclaim the reused
    // segment (its free was NOT dropped). ──
    let _final = unsafe { (*heap).alloc(layout) };
    assert!(!_final.is_null(), "step 6 alloc returned null");

    let reclaimed = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);
    assert!(
        reclaimed > baseline,
        "DBG_LARGE_XTHREAD_RECLAIMED did not advance after the second \
         cross-thread free (baseline={baseline}, now={reclaimed}) — the \
         reused segment's deferred_next was not reset to ABANDONED_TAIL on \
         the cache-hit path, so the second push_large_deferred_free CAS \
         failed and the free was silently dropped (the R34-14 leak bug)."
    );

    // Cleanup.
    unsafe { (*heap).dealloc(_final, layout) };
    unsafe { HeapRegistry::recycle(heap) };
}
