//! P6.1 (Э6) — magazine double-free oracle regression tests.
//!
//! Since Э6 the magazine free guard is two EXACT oracles, run unconditionally
//! on every magazine free, with NO per-heap key stamped into the block body:
//!
//!   (1) in-magazine `slots` scan  — catches a freed-but-not-yet-flushed block.
//!   (2) BinTable `is_free` bitmap  — catches a flushed block.
//!
//! These tests are counterfactual: each is RED if its oracle is removed.
//!
//! - (a) `in_magazine_double_free_is_noop` — RED if the `slots` scan is removed.
//! - (b) `flushed_double_free_is_noop` — RED if the bitmap oracle is removed.
//! - (c) `flushed_double_free_with_garbage_word1_is_noop` — THE STRENGTHENING
//!   test. Overwrites word1 (bytes 8..16) with garbage before the double-free
//!   (simulating user writes). RED on PRE-Э6 code (the stale key made the old
//!   guard skip the oracles → double-issue); GREEN on Э6 (the bitmap oracle no
//!   longer depends on the block body being pristine). Proof M2 is STRENGTHENED.
//! - (d) `legit_free_after_pop_is_not_swallowed` — perf-path sanity: a genuine
//!   free is NOT a false-positive no-op.

#![cfg(all(feature = "alloc-global", feature = "fastbin"))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise all tests: the registry is a process-global static.
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

// ── (a) in-magazine double-free → no-op ───────────────────────────────────

/// Alloc, free (block now sits in the magazine), free AGAIN while still in the
/// magazine → second free must be a no-op, and the block is issued at most
/// once afterwards.
///
/// COUNTERFACTUAL: remove the in-magazine `slots` scan
/// (`for i in 0..cnt { if slots[c][i] == ptr { return; } }`) in
/// `heap_core.rs::dealloc_own_thread` → RED (the block is pushed twice, so the
/// next two allocs return the SAME pointer).
#[test]
fn in_magazine_double_free_is_noop() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(16, 8).unwrap();

    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null());

    unsafe { (*heap).dealloc(p, layout) }; // → magazine
    unsafe { (*heap).dealloc(p, layout) }; // double-free while in magazine

    let p1 = unsafe { (*heap).alloc(layout) };
    let p2 = unsafe { (*heap).alloc(layout) };
    assert!(!p1.is_null() && !p2.is_null());
    assert_ne!(
        p1, p2,
        "in-magazine double-free was NOT swallowed — same pointer issued twice"
    );

    unsafe {
        (*heap).dealloc(p1, layout);
        (*heap).dealloc(p2, layout);
        HeapRegistry::recycle(heap);
    }
}

// ── (b) flushed double-free → no-op ───────────────────────────────────────

/// Alloc enough to overflow the magazine so the earliest blocks are flushed to
/// a BinTable free list, then double-free one of those flushed blocks → no-op;
/// the block must not be issued twice afterwards.
///
/// COUNTERFACTUAL: remove the bitmap oracle
/// (`if SegmentMeta::new(base).alloc_bitmap().is_free(off) { return; }`) in
/// `heap_core.rs::dealloc_own_thread` → RED (the flushed block ends up in BOTH
/// the magazine and the BinTable free list → issued twice).
#[test]
fn flushed_double_free_is_noop() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(16, 8).unwrap();

    // N only needs to comfortably exceed TCACHE_CAP (16) to force at least one
    // magazine overflow flush to the BinTable free list — 200 was far more
    // than that margin needs. Under miri (each alloc/dealloc call re-
    // interprets the whole magazine+bitmap+BinTable machinery) shrink to 40:
    // still 2.5x TCACHE_CAP, so the flush this test targets still fires: the
    // 200x N / 400 native retry-loop bound below shrinks proportionally.
    #[cfg(not(miri))]
    const N: usize = 200;
    #[cfg(miri)]
    const N: usize = 40;
    #[cfg(not(miri))]
    const REISSUE_PROBE_ATTEMPTS: usize = 400;
    #[cfg(miri)]
    const REISSUE_PROBE_ATTEMPTS: usize = 2 * N;
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "initial alloc null at i={i}");
        ptrs.push(p);
    }
    for &p in &ptrs {
        unsafe { (*heap).dealloc(p, layout) };
    }

    let target = ptrs[0]; // flushed early to a BinTable free list
    unsafe { (*heap).dealloc(target, layout) }; // flushed double-free

    let mut issued: Vec<*mut u8> = Vec::with_capacity(REISSUE_PROBE_ATTEMPTS);
    for _ in 0..REISSUE_PROBE_ATTEMPTS {
        let p = unsafe { (*heap).alloc(layout) };
        if p.is_null() {
            break;
        }
        issued.push(p);
    }
    let count = issued.iter().filter(|&&p| p == target).count();
    assert!(
        count <= 1,
        "flushed double-free NOT swallowed — target issued {count} times \
         (block landed in both magazine and BinTable)"
    );

    for &p in &issued {
        unsafe { (*heap).dealloc(p, layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}

// ── (c) THE STRENGTHENING TEST ────────────────────────────────────────────

/// Flushed double-free where the user has OVERWRITTEN word1 (bytes 8..16) with
/// garbage between free and double-free (real code writes to memory it
/// allocated). Must be a no-op; the target must not be issued twice.
///
/// This is RED on the PRE-Э6 code: the old guard used `word1 == key` as a
/// filter, so garbage in word1 made it SKIP both oracles and fall through to
/// push → the flushed block landed in BOTH the magazine and the BinTable →
/// double-issue. On Э6 the oracles run unconditionally (word1 is never read),
/// so the bitmap catches it → GREEN. This is the proof M2 is strengthened.
///
/// NOTE on safety of the word1 write: the BinTable free-list link (`next`)
/// lives at word0 (offset 0). word1 (offset 8) is dead space in a free block,
/// so overwriting it does not corrupt the free list — it only simulates user
/// data the pre-Э6 filter would have tripped over.
#[test]
fn flushed_double_free_with_garbage_word1_is_noop() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(16, 8).unwrap();

    // See `flushed_double_free_is_noop`'s identical constants for the
    // miri-shrink rationale (N only needs to comfortably exceed TCACHE_CAP=16
    // to force a flush).
    #[cfg(not(miri))]
    const N: usize = 200;
    #[cfg(miri)]
    const N: usize = 40;
    #[cfg(not(miri))]
    const REISSUE_PROBE_ATTEMPTS: usize = 400;
    #[cfg(miri)]
    const REISSUE_PROBE_ATTEMPTS: usize = 2 * N;
    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "initial alloc null at i={i}");
        ptrs.push(p);
    }
    for &p in &ptrs {
        unsafe { (*heap).dealloc(p, layout) };
    }

    let target = ptrs[0]; // flushed early to a BinTable free list

    // Simulate the user having written to the allocated memory: clobber word1
    // (bytes 8..16) with garbage that does NOT match any key. On PRE-Э6 code
    // this defeats the `word1 == key` filter → oracles skipped → double-issue.
    unsafe {
        (target as *mut usize).add(1).write(0xDEAD_BEEF_CAFE_F00D);
    }

    // The hazardous double-free.
    unsafe { (*heap).dealloc(target, layout) };

    let mut issued: Vec<*mut u8> = Vec::with_capacity(REISSUE_PROBE_ATTEMPTS);
    for _ in 0..REISSUE_PROBE_ATTEMPTS {
        let p = unsafe { (*heap).alloc(layout) };
        if p.is_null() {
            break;
        }
        issued.push(p);
    }
    let count = issued.iter().filter(|&&p| p == target).count();
    assert!(
        count <= 1,
        "M2 STRENGTHENING FAILED — flushed block with garbage word1 issued \
         {count} times. The bitmap oracle must catch a flushed double-free \
         REGARDLESS of block-body contents."
    );

    for &p in &issued {
        unsafe { (*heap).dealloc(p, layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}

// ── (d) legit free after pop is NOT a false-positive no-op ────────────────

/// Alloc, write to the WHOLE block, free, re-alloc → must return a usable
/// block. The free must NOT be swallowed as a false double-free (the oracles
/// must not fire on a genuinely-live block).
#[test]
fn legit_free_after_pop_is_not_swallowed() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let layout = Layout::from_size_align(16, 8).unwrap();

    // Warm the magazine so this class's slots array has residents from prior
    // frees — exercises the scan against a non-empty magazine.
    let warm = unsafe { (*heap).alloc(layout) };
    assert!(!warm.is_null());
    unsafe { (*heap).dealloc(warm, layout) };

    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null());
    // Write to the whole block (including word0/word1).
    unsafe { core::ptr::write_bytes(p, 0xA5, 16) };

    // Genuine free — must push, NOT be swallowed.
    unsafe { (*heap).dealloc(p, layout) };

    // Re-alloc must return a usable block.
    let q = unsafe { (*heap).alloc(layout) };
    assert!(
        !q.is_null(),
        "re-alloc returned null — legit free was swallowed"
    );
    unsafe { core::ptr::write_bytes(q, 0x5A, 16) };
    assert_eq!(unsafe { q.read() }, 0x5A, "block not usable after re-alloc");

    unsafe {
        (*heap).dealloc(q, layout);
        HeapRegistry::recycle(heap);
    }
}
