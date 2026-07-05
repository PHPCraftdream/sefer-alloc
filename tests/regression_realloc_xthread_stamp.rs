//! Regression test for MUST-1 (0.3.0, C2 regression) —
//! `HeapCore::realloc`'s own-segment branch used to delegate STRAIGHT to
//! `AllocCore::realloc` without applying the two ownership hooks that
//! `HeapCore::alloc` applies (segment-ownership stamping and the A1
//! deferred-large drain).
//!
//! ## What this guards against
//!
//! `AllocCore::realloc` internally `alloc`s a FRESH segment for the
//! non-in-place cases — a large→large grow carves a NEW dedicated Large
//! segment. Pre-fix, `HeapCore::realloc`'s own-segment branch was:
//!
//! ```ignore
//! if self.core.contains_base(base) {
//!     return self.core.realloc(ptr, old_layout, new_size);
//! }
//! ```
//!
//! so that freshly-carved segment's header was written with
//! `owner_thread_free == null` and NEVER stamped (stamping lives only in
//! `HeapCore::alloc`, not in `AllocCore`). Consequence: a Vec grown via
//! realloc on thread A lives in an unstamped Large segment; when A sends it
//! to thread B and B drops it, `dealloc_routing` sees not-ours + magic OK +
//! `owner_tf == null` → silent no-op → the whole 4+ MiB segment and its
//! `SegmentTable` slot leak forever (the resurrected A1/#114
//! leak-to-abort, on the everyday "Vec grows on one thread, freed on
//! another" pattern).
//!
//! Post-fix, `HeapCore::realloc`'s own-segment branch mirrors `alloc`'s two
//! hooks: it drains the deferred-large stack for a Large-classified new size
//! and stamps the realloc RESULT when it lands in a different (freshly
//! carved) segment. The grown pointer's segment is therefore stamped with
//! `owner_thread_free`, so a later cross-thread free routes back to the
//! owner via the deferred-free stack and the segment is reclaimed (counted
//! by `DBG_LARGE_XTHREAD_RECLAIMED`) on the owner's next large alloc.
//!
//! ## Counterfactual (non-vacuity)
//!
//! With the fix reverted (comment out the stamp/drain additions in
//! `HeapCore::realloc`'s own-segment branch), this test FAILS: the grown
//! segment is unstamped, the remote free no-ops, nothing is queued, and
//! `DBG_LARGE_XTHREAD_RECLAIMED`'s delta stays `0` after the owner's second
//! round of large allocations. This RED was verified by hand during
//! development.
//!
//! Feature-map: needs `--features production` (which enables both
//! `alloc-global` and `alloc-xthread`).

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use sefer_alloc::registry::{bootstrap, HeapRegistry, DBG_LARGE_XTHREAD_RECLAIMED};

// Serialise: the registry and the reclaim counter are process-global
// statics; concurrent test-fn execution in the same binary would make the
// `DBG_LARGE_XTHREAD_RECLAIMED` delta assertion flaky.
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
fn realloc_growth_segment_is_stamped_and_reclaims_xthread() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    const N: usize = 60;
    // Both sizes are comfortably above `SMALL_MAX`, so each is unambiguously
    // routed to `AllocCore::alloc_large`. The grow from OLD to NEW MUST cross
    // segment boundaries (exceed `span_usable`) so OPT-G does NOT fire and
    // `AllocCore::realloc` carves a FRESH, larger dedicated Large segment for
    // the result (the segment that pre-fix went un-stamped). OLD occupies one
    // 4 MiB segment; NEW at 5 MiB exceeds that span, forcing relocation.
    const OLD: usize = 3 * 1024 * 1024; // 3 MiB — fits one 4 MiB segment
    const NEW: usize = 5 * 1024 * 1024; // 5 MiB — exceeds one segment
    let old_layout = Layout::from_size_align(OLD, 8).unwrap();
    let new_layout = Layout::from_size_align(NEW, 8).unwrap();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let heap_id = unsafe { (*heap).id() };

    let baseline = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);

    // ── Round 1: owner allocs a Large block, then realloc-GROWS it into a
    // fresh, larger Large segment. The GROWN pointer is what we hand to the
    // remote thread — its segment is the one that must be stamped for the
    // cross-thread free to route back here.
    let mut grown: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = unsafe { (*heap).alloc(old_layout) };
        assert!(!p.is_null(), "alloc[{i}] returned null");
        unsafe {
            std::ptr::write_bytes(p, (i & 0xFF) as u8, OLD);
        }
        // Grow: forces AllocCore::realloc to carve a NEW dedicated Large
        // segment for the result (large→large grow, not in-place).
        let g = unsafe { (*heap).realloc(p, old_layout, NEW) };
        assert!(!g.is_null(), "realloc[{i}] returned null");
        assert_ne!(
            g, p,
            "realloc[{i}] returned the same pointer — the grow was in-place, \
             so no fresh segment was carved and this test would not exercise \
             the bug (adjust OLD/NEW so the grow relocates)"
        );
        // The grown segment MUST carry our ownership stamp now (this is the
        // direct symptom of the fix — pre-fix it read `OWNER_ID_NONE`).
        let owner = unsafe { (*heap).dbg_owner_id_for(g) };
        assert_eq!(
            owner,
            Some(heap_id),
            "realloc[{i}] grown segment not stamped with owner id — \
             the C2 regression: realloc bypassed stamp_segment_owner"
        );
        unsafe {
            std::ptr::write_bytes(g, 0xCD, NEW);
        }
        grown.push(g);
    }

    // ── A REMOTE thread frees every grown block via its OWN claimed heap's
    // cross-thread routing path. If the grown segment was left unstamped
    // (`owner_thread_free == null`), `dealloc_routing` silently no-ops and
    // the segment leaks; if stamped, it is queued onto the owner's
    // deferred-free stack.
    let addrs: Vec<usize> = grown.iter().map(|&p| p as usize).collect();
    thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
        for addr in addrs {
            let p = addr as *mut u8;
            // Free at the GROWN (NEW) layout — the layout the block now has.
            unsafe { (*remote_heap).dealloc(p, new_layout) };
        }
        unsafe { HeapRegistry::recycle(remote_heap) };
    })
    .join()
    .unwrap();

    // ── Round 2: owner allocs N more large blocks, forcing `alloc_large`'s
    // slow path (and therefore `drain_large_deferred_free`) to run, which
    // reclaims the segments the remote thread queued.
    let mut ptrs2: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = unsafe { (*heap).alloc(new_layout) };
        assert!(!p.is_null(), "round 2 alloc[{i}] returned null");
        unsafe {
            std::ptr::write_bytes(p, 0xEE, NEW);
            assert_eq!(p.read(), 0xEE, "round 2 alloc[{i}] read-back mismatch");
        }
        ptrs2.push(p);
    }

    // ── Key assertion: the grown (realloc-carved) segments were actually
    // reclaimed, not leaked. Pre-fix this delta is 0 (unstamped → remote
    // free no-op → nothing queued → nothing to drain).
    let reclaimed = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed) - baseline;
    assert!(
        reclaimed > 0,
        "DBG_LARGE_XTHREAD_RECLAIMED delta is 0 — realloc-grown Large \
         segments were never stamped, so their cross-thread free leaked \
         them (the MUST-1 C2 regression). Expected > 0 (up to {N}), got 0."
    );

    // Cleanup.
    for &p in &ptrs2 {
        unsafe { (*heap).dealloc(p, new_layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}
