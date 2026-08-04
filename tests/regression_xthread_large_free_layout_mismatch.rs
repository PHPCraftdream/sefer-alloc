//! Regression test for task #138 — A1 post-reuse defensive mitigation
//! (layout-vs-header consistency check in the cross-thread Large-free
//! routing path, `HeapCore::dealloc_routing`).
//!
//! ## What this guards against
//!
//! `push_large_deferred_free`'s double-push guard makes the PRE-reuse window
//! airtight (a double-free of a live, not-yet-reclaimed segment is a sound
//! no-op — see `regression_xthread_large_free_no_leak.rs`'s
//! `xthread_large_double_free_no_double_reclaim` test). The gap this task
//! narrows is the POST-reuse window: a stale free arriving for a segment
//! that has ALREADY been reclaimed and handed to a brand-new allocation is,
//! by address alone, indistinguishable from a legitimate free of that new
//! allocation. The mitigation (`alloc_core::deferred_large::
//! large_layout_consistent`) checks that the freeing `Layout`'s size AND
//! align (R22-5, task #356 — align was added after this task's initial
//! size-only check) match the CURRENT occupant's `large_size`/`large_align`
//! header fields before queuing the segment for reclaim; on a mismatch, the
//! free is dropped as a no-op instead of corrupting the (still-live,
//! still-in-use) reused segment's deferred-free stack.
//!
//! This test does not attempt to reconstruct the exact reuse race (that
//! would require racing the internal reclaim/reuse timing) — it instead
//! verifies the CHECK ITSELF directly: a cross-thread free carrying a
//! `Layout` whose size does NOT match the live segment's actual `large_size`
//! must be dropped (no reclaim, no corruption, segment remains valid), while
//! a cross-thread free carrying the CORRECT (consistent) `Layout` is
//! processed normally. This is exactly the code path the mitigation adds a
//! branch to, so it directly exercises the defended logic.
//!
//! ## Counterfactual (non-vacuity)
//!
//! With the `large_layout_consistent` check removed (reverting to an
//! unconditional `push_large_deferred_free` call), the mismatched-layout
//! free in `xthread_large_free_mismatched_layout_is_dropped` would ALSO get
//! queued and reclaimed — `DBG_LARGE_XTHREAD_RECLAIMED`'s delta would be `1`
//! instead of the asserted `0`, and (more importantly) the segment would be
//! unregistered/reclaimed while the "owner" side of this test still expects
//! it to be a live, addressable segment — the owner's subsequent legitimate
//! free of the SAME pointer would then double-reclaim it. This was verified
//! by hand during development (temporarily reverting the mitigation branch
//! in both `heap_core.rs` and `heap.rs` back to an unconditional push).

#![cfg(all(all(feature = "alloc-global", feature = "alloc-xthread"), feature = "internals"))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use sefer_alloc::registry::{bootstrap, HeapRegistry, DBG_LARGE_XTHREAD_RECLAIMED};

// Serialise all tests in this file against the shared
// `DBG_LARGE_XTHREAD_RECLAIMED` global counter (same discipline as
// `regression_xthread_large_free_no_leak.rs`).
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

/// A cross-thread free whose `Layout` size does NOT match the segment's
/// actual `large_size` must be dropped (no-op): no reclaim, and the segment
/// must remain valid so a SUBSEQUENT, CORRECTLY-sized free (or continued
/// owner-side use) still works.
#[test]
fn xthread_large_free_mismatched_layout_is_dropped() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    // 2 MiB — comfortably above SMALL_MAX in every feature combination (even
    // `medium-classes`, which raises it to 1 MiB), unambiguously routed to
    // Large.
    const SIZE: usize = 2 * 1024 * 1024;
    // Enough slow-path large allocations after the bogus free to guarantee
    // `drain_large_deferred_free` runs repeatedly (it runs once per
    // `alloc_large` slow-path call).
    const N_FILLER: usize = 20;
    let real_layout = Layout::from_size_align(SIZE, 8).unwrap();
    // A deliberately WRONG size for the same pointer — simulates a stale
    // free whose `Layout` no longer matches the segment's current occupant
    // (the exact shape of a post-reuse stale double-free, modelled directly
    // rather than by racing the internal reclaim timing).
    let wrong_layout = Layout::from_size_align(SIZE / 4, 8).unwrap();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let baseline = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);

    let p = unsafe { (*heap).alloc(real_layout) };
    assert!(!p.is_null(), "alloc returned null");
    unsafe {
        std::ptr::write_bytes(p, 0xCC, SIZE);
    }
    let addr = p as usize;

    // Remote thread frees with the WRONG layout size — must be a no-op (not
    // even QUEUED, let alone reclaimed).
    thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
        unsafe { (*remote_heap).dealloc(addr as *mut u8, wrong_layout) };
        unsafe { HeapRegistry::recycle(remote_heap) };
    })
    .join()
    .unwrap();

    // The segment must remain fully valid IMMEDIATELY: the data written
    // before the bogus free is intact (no corruption/unmap happened by the
    // dropped free itself).
    unsafe {
        assert_eq!(p.read(), 0xCC, "segment corrupted by the dropped free");
    }

    // Force the owner's `alloc_large` slow path to run repeatedly (the only
    // place `drain_large_deferred_free` runs). If the mismatched free had
    // been WRONGLY queued (mitigation absent/broken), this drain would
    // reclaim — and therefore unregister/unmap or recycle — a segment that
    // is STILL the live `p` this test is about to read from below, which
    // would either corrupt `p`'s data (large-cache reuse) or fault
    // (immediate unmap without alloc-decommit). Either way the read-back
    // below would fail, non-vacuously distinguishing "dropped" from
    // "queued-but-not-yet-drained".
    let mut filler: Vec<*mut u8> = Vec::with_capacity(N_FILLER);
    for i in 0..N_FILLER {
        let q = unsafe { (*heap).alloc(real_layout) };
        assert!(!q.is_null(), "filler alloc[{i}] returned null");
        filler.push(q);
    }

    // Not reclaimed: the mismatched free must have been dropped, not queued
    // — so none of the filler allocations above could have drained it.
    let after_mismatch = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);
    assert_eq!(
        after_mismatch,
        baseline,
        "a cross-thread free with a mismatched layout size was reclaimed \
         (delta {} != 0) — the layout-vs-header consistency mitigation did \
         not drop it as expected",
        after_mismatch - baseline
    );

    // `p`'s segment must STILL be intact after the drain-forcing round: it
    // was never queued, so it was never reclaimed, unmapped, or handed to
    // one of the filler allocations above.
    unsafe {
        assert_eq!(
            p.read(),
            0xCC,
            "segment corrupted/reused after drain-forcing round — the \
             mismatched free was wrongly queued and later reclaimed"
        );
    }
    // `p`'s address must not have been handed out again to any filler alloc
    // (that would mean it WAS reclaimed and recycled from the large-cache).
    assert!(
        !filler.contains(&p),
        "p's segment was reclaimed and reused by a filler allocation — the \
         mismatched free was wrongly queued"
    );

    // The owner can still legitimately free `p` afterward — heap stays sound.
    unsafe { (*heap).dealloc(p, real_layout) };
    for &q in &filler {
        unsafe { (*heap).dealloc(q, real_layout) };
    }

    let p2 = unsafe { (*heap).alloc(real_layout) };
    assert!(!p2.is_null(), "heap unusable after dropped mismatched free");
    unsafe { (*heap).dealloc(p2, real_layout) };

    unsafe { HeapRegistry::recycle(heap) };
}

/// Sister check: a CONSISTENT cross-thread free (correct layout size) is
/// still processed normally — the mitigation must not false-positive on
/// legitimate frees, which is exactly what A1 exists to fix.
#[test]
fn xthread_large_free_consistent_layout_is_reclaimed() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    const N: usize = 20;
    const SIZE: usize = 2 * 1024 * 1024;
    let layout = Layout::from_size_align(SIZE, 8).unwrap();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let baseline = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);

    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc[{i}] returned null");
        ptrs.push(p);
    }

    let addrs: Vec<usize> = ptrs.iter().map(|&p| p as usize).collect();
    thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
        for addr in addrs {
            unsafe { (*remote_heap).dealloc(addr as *mut u8, layout) };
        }
        unsafe { HeapRegistry::recycle(remote_heap) };
    })
    .join()
    .unwrap();

    // Force the slow path (drain) via a second allocation round.
    let mut ptrs2: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "round 2 alloc[{i}] returned null");
        ptrs2.push(p);
    }

    let reclaimed = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed) - baseline;
    assert!(
        reclaimed > 0,
        "a consistent-layout cross-thread free was NOT reclaimed (delta 0) \
         — the mitigation over-rejected a legitimate free"
    );

    for &p in &ptrs2 {
        unsafe { (*heap).dealloc(p, layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}

/// Review finding (full 0.3.0 review): a LEGITIMATE cross-thread free of a
/// tiny-but-huge-aligned Large block (`size < MIN_BLOCK`, `align >
/// SMALL_MAX` — a valid `Layout` via the raw alloc API; unreachable through
/// `Box` since a type's size is a multiple of its align, but perfectly legal
/// for direct `GlobalAlloc` users) must be RECLAIMED, not dropped.
///
/// The alloc path clamps every request to `MIN_BLOCK` before it reaches
/// `alloc_large`, so the header's `large_size` stores the CLAMPED size (16),
/// while the freeing caller passes back its original raw `layout.size()`
/// (8). The mitigation used to compare raw-vs-clamped and therefore ALWAYS
/// mismatched for `size < MIN_BLOCK` — silently dropping the legitimate free
/// and permanently leaking the segment + its `SegmentTable` slot (the
/// #114/#130 leak-to-abort class, narrow trigger). `large_layout_consistent`
/// now clamps the caller's size symmetrically before comparing.
///
/// Counterfactual: with the clamp removed from `large_layout_consistent`
/// (comparing the raw size again), this free is dropped and the reclaim
/// delta stays 0 — this test fails.
#[test]
fn xthread_large_free_tiny_size_huge_align_is_reclaimed() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    const N: usize = 8;
    // size 8 < MIN_BLOCK (16); align 2 MiB > SMALL_MAX (~253 KiB normally, 1
    // MiB under `medium-classes`) and < SEGMENT (4 MiB) → the request is
    // unambiguously routed to the Large path (class_for → None), with
    // header.large_size = 16 (clamped) while the caller's layout.size() stays
    // 8.
    //
    // R6-OPT-P0-3a note: the ORIGINAL align (32 KiB) is exactly divisible by
    // every one of `medium-classes`' 6 new exact class sizes (256 KiB..1 MiB
    // are all multiples of 32 KiB), so under that feature `class_for`'s
    // divisibility-walk slow path would have resolved this request to a SMALL
    // (medium) class instead of falling through to Large — silently
    // invalidating this test's entire premise (a legitimate Large
    // cross-thread free never happens; `DBG_LARGE_XTHREAD_RECLAIMED` never
    // advances). Confirmed by running `cargo test --all-features` against
    // this file before the fix, which failed with delta 0. 2 MiB is not a
    // multiple of any class this crate can produce (every class tops out at
    // 1 MiB even under `medium-classes`), so it stays a genuine Large request
    // regardless of feature combination.
    let align = 2 * 1024 * 1024;
    let layout = Layout::from_size_align(8, align).unwrap();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let baseline = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);

    let mut ptrs: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "alloc[{i}] returned null");
        assert_eq!(
            (p as usize) % align,
            0,
            "alloc[{i}] not aligned to the requested 2 MiB"
        );
        ptrs.push(p);
    }

    // Remote thread frees each block with the SAME (original) layout — the
    // legitimate cross-thread free A1 exists to reclaim.
    let addrs: Vec<usize> = ptrs.iter().map(|&p| p as usize).collect();
    thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
        for addr in addrs {
            unsafe { (*remote_heap).dealloc(addr as *mut u8, layout) };
        }
        unsafe { HeapRegistry::recycle(remote_heap) };
    })
    .join()
    .unwrap();

    // Force the owner's alloc_large slow path (the only drain site).
    let mut ptrs2: Vec<*mut u8> = Vec::with_capacity(N);
    for i in 0..N {
        let p = unsafe { (*heap).alloc(layout) };
        assert!(!p.is_null(), "round 2 alloc[{i}] returned null");
        ptrs2.push(p);
    }

    let reclaimed = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed) - baseline;
    assert!(
        reclaimed > 0,
        "a legitimate tiny-size/huge-align cross-thread free was NOT \
         reclaimed (delta 0) — the mitigation compared the caller's raw \
         layout.size() against the header's MIN_BLOCK-clamped large_size \
         and over-rejected it (permanent segment leak)"
    );

    for &p in &ptrs2 {
        unsafe { (*heap).dealloc(p, layout) };
    }
    unsafe { HeapRegistry::recycle(heap) };
}

/// R22-5 (task #356): a cross-thread free whose `Layout` has the SAME size
/// as the live segment's `large_size` but a DIFFERENT align than its
/// `large_align` must ALSO be dropped as a no-op — `large_layout_consistent`
/// now checks `layout.align()` against `SegmentHeader::large_align_at(base)`
/// in addition to size (`SegmentHeader::large_align_at` did not exist before
/// this task; the mitigation compared size only, so a same-size-wrong-align
/// fabricated free previously passed and was WRONGLY queued/reclaimed).
///
/// Counterfactual: with the align half of the check removed (reverting to
/// the pre-R22-5 size-only comparison), the mismatched-align free below
/// would ALSO get queued and reclaimed — `DBG_LARGE_XTHREAD_RECLAIMED`'s
/// delta would be `1` instead of the asserted `0`, and the segment would be
/// unregistered/reclaimed while `p` is still expected to be live, exactly
/// the same non-vacuity shape as `xthread_large_free_mismatched_layout_is_dropped`
/// above (verified by the same reasoning: this is the identical code path,
/// gated on the newly-added align comparison instead of the size one).
#[test]
fn xthread_large_free_same_size_wrong_align_is_dropped() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    const SIZE: usize = 2 * 1024 * 1024;
    const N_FILLER: usize = 20;
    // Real allocation: align 8.
    let real_layout = Layout::from_size_align(SIZE, 8).unwrap();
    // A fabricated free layout with the SAME size but a DIFFERENT align
    // (4096 instead of 8) — models a stale/fabricated free whose `Layout`
    // no longer (or never did) match the segment's current occupant's
    // align, the align-analogue of `wrong_layout` above.
    let wrong_align_layout = Layout::from_size_align(SIZE, 4096).unwrap();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let baseline = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);

    let p = unsafe { (*heap).alloc(real_layout) };
    assert!(!p.is_null(), "alloc returned null");
    unsafe {
        std::ptr::write_bytes(p, 0xCC, SIZE);
    }
    let addr = p as usize;

    // Remote thread frees with the WRONG align (same size) — must be a
    // no-op (not even QUEUED, let alone reclaimed).
    thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
        unsafe { (*remote_heap).dealloc(addr as *mut u8, wrong_align_layout) };
        unsafe { HeapRegistry::recycle(remote_heap) };
    })
    .join()
    .unwrap();

    // The segment must remain fully valid IMMEDIATELY.
    unsafe {
        assert_eq!(p.read(), 0xCC, "segment corrupted by the dropped free");
    }

    // Force the owner's `alloc_large` slow path to run repeatedly, same
    // drain-forcing shape as the size-mismatch test above.
    let mut filler: Vec<*mut u8> = Vec::with_capacity(N_FILLER);
    for i in 0..N_FILLER {
        let q = unsafe { (*heap).alloc(real_layout) };
        assert!(!q.is_null(), "filler alloc[{i}] returned null");
        filler.push(q);
    }

    let after_mismatch = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);
    assert_eq!(
        after_mismatch,
        baseline,
        "a cross-thread free with a mismatched align (same size) was \
         reclaimed (delta {} != 0) — the layout-vs-header consistency \
         mitigation did not drop it as expected",
        after_mismatch - baseline
    );

    unsafe {
        assert_eq!(
            p.read(),
            0xCC,
            "segment corrupted/reused after drain-forcing round — the \
             mismatched-align free was wrongly queued and later reclaimed"
        );
    }
    assert!(
        !filler.contains(&p),
        "p's segment was reclaimed and reused by a filler allocation — the \
         mismatched-align free was wrongly queued"
    );

    // The owner can still legitimately free `p` afterward — heap stays sound.
    unsafe { (*heap).dealloc(p, real_layout) };
    for &q in &filler {
        unsafe { (*heap).dealloc(q, real_layout) };
    }

    let p2 = unsafe { (*heap).alloc(real_layout) };
    assert!(
        !p2.is_null(),
        "heap unusable after dropped mismatched-align free"
    );
    unsafe { (*heap).dealloc(p2, real_layout) };

    unsafe { HeapRegistry::recycle(heap) };
}

/// R22-5 (task #356) — the cache-HIT-reuse scenario the align check exists
/// for: a Large segment is freed (own-thread — deposited into the
/// `alloc-decommit` large cache), then reused by a NEW allocation of the
/// SAME size but a DIFFERENT align. The header's `large_align` is rewritten
/// by the reuse (`AllocCore::alloc_large`'s cache-hit path does a FULL-struct
/// `Node::write_struct` with the NEW request's `align` — see
/// `alloc_core_large.rs`'s cache-hit arm and `SegmentHeader::large_align`'s
/// doc: "describe the CURRENT occupant, not the segment's history"). A stale
/// free carrying the ORIGINAL (now-superseded) align must be rejected even
/// though its size still matches the new occupant — this is exactly the gap
/// R22-5 closes (a size-only check would have let this through).
///
/// Gated on `alloc-decommit` (the large cache) in addition to this file's
/// `alloc-global`+`alloc-xthread`.
///
/// Reuse is confirmed structurally (not via a hit-rate counter, which needs
/// `alloc-stats`): the best-fit large-cache admission rule
/// (`slot.usable_size >= usable && slot.usable_size <= usable *
/// LARGE_CACHE_SIZE_FACTOR`) with a SAME-size second request against a
/// freshly-deposited SAME-size slot is a guaranteed hit against that exact
/// slot, so the second allocation's address must equal the first's — this
/// pointer-identity check is the oracle for "the segment was reused", the
/// same structural style `regression_large_cache_span_usable_stable.rs` uses
/// (deriving what a hit implies rather than reading a stats-gated counter).
#[cfg(feature = "alloc-decommit")]
#[test]
fn xthread_large_free_stale_align_after_cache_hit_reuse_is_dropped() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();

    const SIZE: usize = 2 * 1024 * 1024;
    const N_FILLER: usize = 20;
    // A: the original allocation, align 8.
    let layout_a = Layout::from_size_align(SIZE, 8).unwrap();
    // B: the SAME size, but a DIFFERENT align (4096) — a legitimate NEW
    // request that reuses A's cached segment on a cache hit.
    let layout_b = Layout::from_size_align(SIZE, 4096).unwrap();

    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let baseline = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);

    // Allocate and immediately (own-thread) free A — deposits the segment
    // into the large cache under `alloc-decommit`.
    let pa = unsafe { (*heap).alloc(layout_a) };
    assert!(!pa.is_null(), "alloc A returned null");
    let addr_a = pa as usize;
    unsafe { (*heap).dealloc(pa, layout_a) };

    // Allocate B: same size as A, different align. The large-cache admission
    // rule is satisfied (same usable size), so this MUST be served as a
    // cache hit reusing A's exact segment.
    let pb = unsafe { (*heap).alloc(layout_b) };
    assert!(!pb.is_null(), "alloc B returned null");
    assert_eq!(
        pb as usize, addr_a,
        "B did not reuse A's segment address — the cache-hit-reuse \
         precondition for this test did not hold (test premise invalid, \
         not a mitigation failure)"
    );
    unsafe {
        std::ptr::write_bytes(pb, 0xCC, SIZE);
    }

    // Remote thread frees the SAME address with the STALE layout_a (matching
    // size, but align 8 instead of B's actual 4096) — this must be dropped:
    // a size-only check would have let it through (size still matches), but
    // the align now describes A's history, not B's current occupancy.
    thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
        unsafe { (*remote_heap).dealloc(addr_a as *mut u8, layout_a) };
        unsafe { HeapRegistry::recycle(remote_heap) };
    })
    .join()
    .unwrap();

    // B's segment must remain fully valid IMMEDIATELY.
    unsafe {
        assert_eq!(
            pb.read(),
            0xCC,
            "B's segment corrupted by the dropped stale-align free"
        );
    }

    // Force the owner's `alloc_large` slow path to run repeatedly.
    let mut filler: Vec<*mut u8> = Vec::with_capacity(N_FILLER);
    for i in 0..N_FILLER {
        let q = unsafe { (*heap).alloc(layout_b) };
        assert!(!q.is_null(), "filler alloc[{i}] returned null");
        filler.push(q);
    }

    let after_mismatch = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);
    assert_eq!(
        after_mismatch,
        baseline,
        "R22-5 CACHE-HIT-REUSE GAP: a stale cross-thread free carrying A's \
         ORIGINAL align (but matching B's current size) was reclaimed \
         (delta {} != 0) after B reused A's segment via a cache hit — the \
         align half of `large_layout_consistent` failed to catch a \
         same-size-different-align reuse",
        after_mismatch - baseline
    );

    unsafe {
        assert_eq!(
            pb.read(),
            0xCC,
            "B's segment corrupted/reused after drain-forcing round — the \
             stale-align free was wrongly queued and later reclaimed"
        );
    }
    assert!(
        !filler.contains(&pb),
        "B's segment was reclaimed and reused by a filler allocation — the \
         stale-align free was wrongly queued"
    );

    // The owner can still legitimately free B afterward — heap stays sound.
    unsafe { (*heap).dealloc(pb, layout_b) };
    for &q in &filler {
        unsafe { (*heap).dealloc(q, layout_b) };
    }

    unsafe { HeapRegistry::recycle(heap) };
}
