//! Regression test for task A1 (0.3.0) — cross-thread free of a Large/huge
//! segment used to be a PERMANENT leak.
//!
//! ## What this guards against
//!
//! Pre-fix, `HeapCore::dealloc_routing`'s `SegmentKind::Large` branch was a
//! bare `return` whenever the freeing thread was NOT the segment's owner:
//!
//! ```ignore
//! if SegmentHeader::kind_at(base) == SegmentKind::Large {
//!     return;
//! }
//! ```
//!
//! No code path ever revisited that segment again. The whole OS reservation
//! (>= 4 MiB — a full `SEGMENT`, more for an oversized allocation) stayed
//! mapped forever, and its `SegmentTable` slot was never recycled — a silent,
//! permanent leak under any allocate-here/free-there workload for large
//! blocks (the canonical case: an async runtime that migrates a task holding
//! a large buffer to a different worker thread, which then drops it).
//!
//! Post-fix, a remote free of a Large segment pushes the segment's `base`
//! onto the OWNING heap's deferred-free stack (`HeapCore::thread_free`,
//! reused as a second Treiber-stack head — see the field doc in
//! `src/registry/heap_core.rs`). The owner drains that stack lazily on its
//! own `alloc_large` slow path (`HeapCore::drain_large_deferred_free`,
//! called from `HeapCore::alloc` before a Large-classified request reaches
//! `AllocCore::alloc_large`), reclaiming each queued segment via
//! `AllocCore::reclaim_large_segment` — which either deposits it in the
//! `alloc-decommit` large-cache for reuse, or releases the OS reservation
//! immediately and frees the `SegmentTable` slot.
//!
//! ## Counterfactual (non-vacuity)
//!
//! This test was run BOTH ways during development:
//!
//! 1. **Pre-fix** (temporarily restoring the bare `return` in
//!    `dealloc_routing`'s Large branch, and reverting the
//!    `drain_large_deferred_free` call in `HeapCore::alloc`): the test FAILS
//!    — `DBG_LARGE_XTHREAD_RECLAIMED.load(Relaxed)` stays `0` after the
//!    owner's second round of large allocations, because nothing ever drains
//!    the (in that revert, nonexistent) deferred-free path; the segments the
//!    remote thread "freed" are never reclaimed.
//! 2. **Post-fix** (the code as committed): the test PASSES —
//!    `DBG_LARGE_XTHREAD_RECLAIMED` is `> 0` (in fact `== N`, one reclaim per
//!    remotely-freed segment) after the owner's second allocation round,
//!    proving the segments were actually recycled, not merely
//!    not-yet-observed-to-leak.
//!
//! ## Test shape
//!
//! Directly against `HeapRegistry`/`HeapCore` (the registry substrate the
//! bug lives in), mirroring the `tests/heap_core_tcache_stamp.rs` /
//! `tests/heap_cross_thread.rs` harness patterns: claim a heap on the "owner"
//! thread, do `N` large allocations (size > `SMALL_MAX`, using 512 KiB so
//! each allocation is unambiguously routed to the Large path), hand the
//! pointers to a second ("remote") thread which frees every one of them via
//! `dealloc_routing`, then have the owner do a second round of `N` large
//! allocations (forcing `alloc_large`'s slow path to run, and therefore the
//! drain). `DBG_LARGE_XTHREAD_RECLAIMED` must be `> 0` afterward.

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;

use sefer_alloc::registry::{bootstrap, HeapRegistry, DBG_LARGE_XTHREAD_RECLAIMED};

/// Miri-shrink for the whole-block `write_bytes`/read-back canary every test
/// in this file does per Large block. Native fills the entire block (a real
/// use-after-unmap would corrupt anywhere in it); under miri (which tracks
/// every byte of every `write_bytes`/read individually — the dominant cost of
/// this file's Large-allocation-heavy tests, see each test's own miri-N
/// comment) a small window at the front is exactly as sensitive to the
/// reclaim-then-reuse corruption these tests guard against (a reused segment
/// still gets reset/re-carved from its start), so it is Miri-gated only;
/// native coverage is unchanged. Mirrors `tests/stress_boundary_sweep.rs`'s
/// established `LARGE_WINDOW` pattern.
#[cfg(not(miri))]
fn canary_write_len(size: usize) -> usize {
    size
}
#[cfg(miri)]
fn canary_write_len(_size: usize) -> usize {
    4096
}

// Serialise all tests in this file: the registry and the reclaim counter are
// process-global statics; concurrent test-fn execution would make the
// `DBG_LARGE_XTHREAD_RECLAIMED` delta assertion flaky (another test in the
// same binary could bump it concurrently under `cargo test`'s default
// multi-threaded test runner).
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
fn xthread_large_free_reclaims_segments_no_leak() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    // Only needs enough blocks to prove reclaim actually happens (`> 0`), not
    // a statistically large sample. Under miri (each 512 KiB `write_bytes` +
    // each alloc/dealloc call is individually byte-tracked — the dominant
    // cost here) shrink to 10; native keeps the original 100.
    #[cfg(not(miri))]
    const N: usize = 100;
    #[cfg(miri)]
    const N: usize = 10;
    // 512 KiB — comfortably above `SMALL_MAX` (a few KiB), so every
    // allocation is unambiguously routed to `AllocCore::alloc_large`.
    const SIZE: usize = 512 * 1024;
    let layout = Layout::from_size_align(SIZE, 8).unwrap();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");
    let heap_id = unsafe { (*heap).id() };

    let baseline = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);

    // ── Round 1: owner allocates N large blocks, writes a pattern ──────────
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "round 1 alloc[{i}] returned null");
        let owner = unsafe { (*heap).dbg_owner_id_for(p) };
        assert_eq!(
            owner,
            Some(heap_id),
            "round 1 alloc[{i}] segment not stamped with owner id"
        );
        unsafe {
            std::ptr::write_bytes(p, (i & 0xFF) as u8, canary_write_len(SIZE));
        }
        ptrs.push(p);
    }

    // ── A REMOTE thread frees every block via the cross-thread routing
    // path. Critically, the remote thread must call `dealloc` on ITS OWN
    // claimed heap (not the owner's) — `dealloc_routing` distinguishes
    // own-thread vs. cross-thread by comparing the CALLING heap's
    // `thread_free_head()` against the segment's stamped owner; calling
    // through the owner's own `HeapCore` object would always take the
    // own-thread branch and never exercise the bug. Raw pointers are
    // `!Send`; ship addresses instead.
    let addrs: Vec<usize> = ptrs.iter().map(|&p| p as usize).collect();
    thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
        for addr in addrs {
            let p = addr as *mut u8;
            unsafe { (*remote_heap).dealloc(p, layout) };
        }
        unsafe { HeapRegistry::recycle(remote_heap) };
    })
    .join()
    .unwrap();

    // ── Round 2: owner allocates N more large blocks. This forces
    // `alloc_large`'s slow path to run repeatedly (nothing was cached for
    // the owner's OWN large-dealloc path — the blocks were freed remotely),
    // which is where `drain_large_deferred_free` reclaims the segments the
    // remote thread queued.
    let mut ptrs2: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "round 2 alloc[{i}] returned null");
        unsafe {
            std::ptr::write_bytes(p, 0xEE, canary_write_len(SIZE));
            assert_eq!(p.read(), 0xEE, "round 2 alloc[{i}] read-back mismatch");
        }
        ptrs2.push(p);
    }

    // ── The key assertion: segments were actually reclaimed, not leaked. ──
    let reclaimed = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed) - baseline;
    assert!(
        reclaimed > 0,
        "DBG_LARGE_XTHREAD_RECLAIMED delta is 0 — cross-thread-freed Large \
         segments were never reclaimed (the A1 permanent-leak regression). \
         Expected > 0 (up to {N}), got 0."
    );

    // Cleanup: free round-2 blocks (own-thread path) and recycle the heap.
    for &p in &ptrs2 {
        unsafe { (*heap).dealloc(p, layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}

/// Regression test (0.3.0 hardening, post-A1) — double-push guard in
/// [`HeapCore::push_large_deferred_free`].
///
/// ## What this guards against
///
/// A1 gave `push_large_deferred_free` no protection against pushing the SAME
/// `base` twice. A **double-free of the same Large pointer from remote
/// thread(s)** — already UB under the `GlobalAlloc` contract, but a case this
/// allocator otherwise degrades SAFELY on (a no-op) via the M2 double-free
/// guard used everywhere else — used to corrupt this particular stack: the
/// second push would read `head == base` (from the first push's CAS) and
/// write `base.next_abandoned = base`, a self-loop. A drain would pop `base`
/// once, reclaim it (unregister + unmap/recycle), and then — because the
/// self-loop pop never advanced `head` away from `base` — read
/// `next_abandoned` off the now-unmapped memory on the NEXT loop iteration
/// (a use-after-free) and could reclaim the same segment a second time (a
/// double-unmap).
///
/// The fix (see the doc comment on `push_large_deferred_free` in
/// `src/registry/heap_core.rs`) makes the push idempotent per-`base` via a
/// `compare_exchange` on the link word from `ABANDONED_TAIL`: only the first
/// pusher of a given `base` may link it; a second push of the SAME `base`
/// observes the link word already claimed and returns as a no-op.
///
/// ## What this test checks
///
/// This test does not assert on a defined "correct" result for a double-free
/// (there is none — it's UB by contract). It asserts on the OBSERVABLE
/// SYMPTOM the fix eliminates: after a remote thread double-frees the SAME
/// large pointer for every block in a batch, the process does not
/// crash/hang, `DBG_LARGE_XTHREAD_RECLAIMED` advances by EXACTLY one per
/// distinct segment (never double-counted — a doubled count would indicate
/// the same segment was reclaimed twice, the double-unmap symptom), and the
/// owner heap remains fully usable afterward (it can keep allocating and
/// freeing normally — the allocator stays sound).
///
/// ## Counterfactual (honesty about UB limits)
///
/// With the guard temporarily removed (reverting to an unconditional
/// `next_atomic.store` before the head CAS, i.e. the pre-hardening A1 code),
/// this test's `reclaimed == N` assertion fails deterministically on this
/// author's dev machine and in CI runs performed during development: the
/// self-loop causes `drain_large_deferred_free` to reclaim fewer THAN `N`
/// distinct segments while double-counting others, so
/// `DBG_LARGE_XTHREAD_RECLAIMED`'s delta no longer equals `N` (the corrupted
/// chain drops some segments off the tail entirely — those slots leak instead
/// of double-free crashing, since a self-loop pop terminates the drain loop
/// at that node). Whether a GIVEN run additionally segfaults on the
/// use-after-free read of `next_abandoned` on now-unmapped memory is
/// UB-dependent (page reuse timing, allocator-internal unmap granularity) —
/// this repo does not rely on a guaranteed crash to prove the bug; the
/// `reclaimed == N` symptom is the deterministic, portable oracle used here.
#[test]
fn xthread_large_double_free_no_double_reclaim() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    // See `xthread_large_free_reclaims_segments_no_leak`'s identical
    // miri-shrink rationale.
    #[cfg(not(miri))]
    const N: usize = 50;
    #[cfg(miri)]
    const N: usize = 10;
    const SIZE: usize = 512 * 1024;
    let layout = Layout::from_size_align(SIZE, 8).unwrap();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let baseline = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);

    // Owner allocates N large blocks.
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc[{i}] returned null");
        ptrs.push(p);
    }

    // A REMOTE thread double-frees every block: two `dealloc` calls per
    // pointer, back-to-back, with no drain in between (this is the exact
    // "two consecutive pushes of the same base" race the guard closes).
    let addrs: Vec<usize> = ptrs.iter().map(|&p| p as usize).collect();
    thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
        for &addr in &addrs {
            let p = addr as *mut u8;
            unsafe { (*remote_heap).dealloc(p, layout) };
            // Second free of the SAME pointer: a double-free (UB by
            // `GlobalAlloc` contract). With the guard, this must be a sound
            // no-op — it must NOT queue `base` a second time.
            unsafe { (*remote_heap).dealloc(p, layout) };
        }
        unsafe { HeapRegistry::recycle(remote_heap) };
    })
    .join()
    .unwrap();

    // Owner allocates N more large blocks, forcing `alloc_large`'s slow path
    // (and therefore `drain_large_deferred_free`) to run repeatedly.
    let mut ptrs2: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "post-double-free alloc[{i}] returned null");
        unsafe {
            std::ptr::write_bytes(p, 0xAB, canary_write_len(SIZE));
            assert_eq!(
                p.read(),
                0xAB,
                "post-double-free alloc[{i}] read-back mismatch — heap corrupted"
            );
        }
        ptrs2.push(p);
    }

    // The key assertion: every distinct segment was reclaimed EXACTLY once
    // (never zero — that would mean the corrupted chain dropped it, a leak;
    // never more than N — that would mean a segment was double-reclaimed,
    // the double-unmap symptom the guard prevents).
    let reclaimed = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed) - baseline;
    assert_eq!(
        reclaimed, N as u64,
        "expected exactly {N} reclaims (one per distinct double-freed segment), \
         got {reclaimed} — a double-push corrupted the deferred-free stack \
         (either a leaked segment, if < N, or a double-reclaim, if > N)."
    );

    // Sanity: the owner heap remains fully usable after the double-free —
    // no corruption bled into the substrate.
    for &p in &ptrs2 {
        unsafe { (*heap).dealloc(p, layout) };
    }
    let p_final = unsafe { (*heap).alloc(layout) };
    assert!(
        !p_final.is_null(),
        "heap unusable after double-free double-push scenario"
    );
    unsafe { (*heap).dealloc(p_final, layout) };

    unsafe { HeapRegistry::recycle(heap) };
}

/// Sister check: the reclaim counter is monotonically non-decreasing and
/// genuinely reflects NEW reclaims (not a stuck/saturating counter that
/// would pass the primary test vacuously). Runs a second independent heap +
/// round and confirms the counter advances again.
#[test]
fn xthread_large_free_reclaim_counter_advances_again() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    // See `xthread_large_free_reclaims_segments_no_leak`'s identical
    // miri-shrink rationale.
    #[cfg(not(miri))]
    const N: usize = 20;
    #[cfg(miri)]
    const N: usize = 6;
    const SIZE: usize = 512 * 1024;
    let layout = Layout::from_size_align(SIZE, 8).unwrap();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());

    let before: AtomicU64 = AtomicU64::new(DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed));

    let mut ptrs = Vec::with_capacity(N);
    for _ in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null());
        ptrs.push(p);
    }

    let addrs: Vec<usize> = ptrs.iter().map(|&p| p as usize).collect();
    thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null());
        for addr in addrs {
            unsafe { (*remote_heap).dealloc(addr as *mut u8, layout) };
        }
        unsafe { HeapRegistry::recycle(remote_heap) };
    })
    .join()
    .unwrap();

    // Drain by allocating again.
    for _ in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null());
        unsafe { (*heap).dealloc(p, layout) };
    }

    let after = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);
    assert!(
        after > before.load(Ordering::Relaxed),
        "reclaim counter did not advance on a second independent round \
         (before={}, after={after})",
        before.load(Ordering::Relaxed)
    );

    unsafe { HeapRegistry::recycle(heap) };
}
