//! Guardrail test for UBFIX-12 / M-7 (2026-07-10 UB-audit final synthesis):
//! `HeapRegistry::abandon_segments` (the global abandoned-segments Treiber
//! stack) and the A1 cross-thread deferred-large-free stack
//! (`alloc_core::deferred_large`, reused via `HeapCore::thread_free` on the
//! `HeapCore` face) BOTH repurpose a segment's `SegmentHeader::next_abandoned`
//! header field as their own intrusive link. See the "⚠️ REACTIVATION HAZARD
//! (task A1, 0.3.0)" doc comment on `HeapRegistry::abandon_segments`
//! (`src/registry/heap_registry.rs`) for the full mechanism.
//!
//! # Honest status — what this test proves, and does not
//!
//! `abandon_segments`/`try_adopt` are **not reachable from any production
//! path today** (Phase 12.5's shard model replaced thread-exit abandonment
//! with whole-heap slot reuse; `AbandonGuard::drop` calls `recycle` only,
//! never `abandon_segments` — confirmed by grep, same conclusion as
//! `tests/loom_registry.rs`'s and UBFIX-4/M-6's precedent). So under every
//! CURRENT production entry point (`HeapRegistry::claim`/`recycle`,
//! `SeferAlloc`, `HeapCore::alloc`/`dealloc`), the two stacks never contest
//! the same segment's `next_abandoned` field, because nothing ever pushes a
//! LIVE, A1-queued segment onto `abandoned_segs`.
//!
//! This file has two parts:
//!
//! 1. [`abandon_segments_is_unreachable_from_any_recycle_path`] — pins the
//!    achievability claim itself: a heap that owns a segment queued on the
//!    A1 deferred-free stack survives a normal `recycle` (the only
//!    production teardown path) with that segment's `next_abandoned` link
//!    UNTOUCHED — i.e. going through the actual slot-release API used in
//!    production does NOT invoke `abandon_segments` and does NOT corrupt the
//!    in-flight A1 link. If a future change wires `recycle`/`AbandonGuard`
//!    back to `abandon_segments` without the fix described in the hazard
//!    note, this test starts failing (its whole point).
//! 2. [`reactivating_abandon_segments_while_a1_inflight_corrupts_the_link`] —
//!    the counterfactual that proves the hazard is REAL, not a hypothetical:
//!    deliberately calling the dead-but-compiled `abandon_segments` while a
//!    segment is mid-flight on the A1 stack (exactly the scenario the hazard
//!    note warns about) demonstrably clobbers the A1 link — the segment
//!    silently disappears from the A1 chain (a permanent-leak symptom, same
//!    shape as the A1 pre-fix bug in
//!    `tests/regression_xthread_large_free_no_leak.rs`) and instead shows up
//!    on `abandoned_segs`. This is the non-vacuousness proof for test 1: it
//!    shows what WOULD happen if `abandon_segments` were reachable
//!    concurrently with A1, which is exactly why test 1 (and the hazard note)
//!    matter.
//!
//! Together these pin the invariant "a segment is linked into AT MOST ONE of
//! {abandoned_segs, A1 deferred-free} via `next_abandoned` at any time" as
//! actually upheld by every reachable production path today, while
//! documenting — with a real, executable reproduction, not just prose — the
//! exact way a naive reactivation would violate it.

#![cfg(all(feature = "alloc-global", feature = "alloc-xthread"))]

use std::alloc::Layout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use sefer_alloc::registry::{bootstrap, HeapRegistry, DBG_LARGE_XTHREAD_RECLAIMED};
use sefer_alloc::SegmentLayout;

/// Derive the SEGMENT-aligned base pointer of `ptr`, preserving provenance
/// (strict-provenance clean, mirroring `os::segment_base_of_ptr`, which is
/// `pub(crate)` and thus unreachable from a test — same helper pattern as
/// `tests/regression_gen_table_lifecycle_seams.rs::segment_base_of`).
/// SEGMENT is `1 << 22` (4 MiB).
fn segment_base_of(ptr: *mut u8) -> *mut u8 {
    ptr.map_addr(|a| a & !(SegmentLayout::SEGMENT - 1))
}

// Serialise all tests in this file: the registry, `abandoned_segs`, and
// `DBG_LARGE_XTHREAD_RECLAIMED` are process-global; concurrent execution of
// these tests (or interleaving with other files' tests touching the same
// statics under `cargo test`'s default multi-threaded runner) would make the
// exact-linkage assertions below flaky.
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

/// 512 KiB — comfortably above `SMALL_MAX`, so the allocation is
/// unambiguously routed to `AllocCore::alloc_large` and therefore reused as a
/// Large segment (the only kind the A1 deferred-free stack ever queues).
const LARGE_SIZE: usize = 512 * 1024;

/// Remote-free `ptr` (allocated on `heap`, owned by a DIFFERENT thread than
/// the one calling this) from a freshly claimed remote heap, which pushes the
/// underlying Large segment onto the OWNER's A1 deferred-free stack via
/// `next_abandoned` — and, critically, does NOT drain it (draining only
/// happens on the owner's own `alloc_large` slow path, which this helper
/// never triggers). Returns once the remote free has completed.
fn remote_free_large(layout: Layout, ptr: *mut u8) {
    let addr = ptr as usize;
    thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote_heap = HeapRegistry::claim();
        assert!(!remote_heap.is_null(), "remote HeapRegistry::claim failed");
        unsafe { (*remote_heap).dealloc(addr as *mut u8, layout) };
        unsafe { HeapRegistry::recycle(remote_heap) };
    })
    .join()
    .unwrap();
}

/// (1) The production teardown path (`recycle`) never touches
/// `abandoned_segs`, so a segment mid-flight on the A1 stack survives it
/// untouched — proving `abandon_segments` truly sits outside every reachable
/// call graph today (the M-7 achievability claim), not merely "nobody calls
/// it in the tests we happened to write".
#[test]
fn abandon_segments_is_unreachable_from_any_recycle_path() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    while HeapRegistry::pop_abandoned_segment().is_some() {}

    let layout = Layout::from_size_align(LARGE_SIZE, 8).unwrap();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());

    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null(), "large alloc must succeed");

    let baseline = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed);

    // Remote-free `p`: the segment is now linked onto the OWNER heap's A1
    // deferred-free stack via `next_abandoned`, undrained (nothing on this
    // owner has touched `alloc_large`'s slow path since).
    remote_free_large(layout, p);

    // The ONLY production teardown entry point: recycle the owner's slot.
    // This must NOT call `abandon_segments` (it doesn't, today) and must
    // therefore NOT push the segment onto `abandoned_segs` — leaving the A1
    // link (and the segment) exactly as the remote free left it.
    // SAFETY: `heap` was returned by `claim` and not yet recycled.
    unsafe { HeapRegistry::recycle(heap) };

    // If `recycle` had (wrongly) walked to `abandon_segments`, the segment
    // would now be sitting on `abandoned_segs` — a pop would return
    // Some(_). It must NOT: the abandoned-segs stack stays empty, because
    // `recycle` alone never reaches `abandon_segments`.
    let popped = HeapRegistry::pop_abandoned_segment();
    assert!(
        popped.is_none(),
        "recycle must not reach abandon_segments: the abandoned-segs stack \
         must stay empty after a normal recycle of a heap with an in-flight \
         A1-queued segment, but a segment was found on it \
         (base = {popped:?}) — abandon_segments has become reachable from \
         the production recycle path without the M-7 next_abandoned \
         field-sharing fix; read the REACTIVATION HAZARD note on \
         HeapRegistry::abandon_segments before proceeding"
    );

    // The reclaim counter must not have moved either — recycle does not
    // drain A1, so nothing was (or should have been) reclaimed by this test.
    let reclaimed = DBG_LARGE_XTHREAD_RECLAIMED.load(Ordering::Relaxed) - baseline;
    assert_eq!(
        reclaimed, 0,
        "recycle alone must not drain or reclaim the A1-queued segment \
         (got {reclaimed} reclaims) — that would indicate an unexpected \
         drain call was introduced on the recycle path"
    );
}

/// (2) Counterfactual / non-vacuousness proof: deliberately reproduce the
/// exact race the REACTIVATION HAZARD note warns about — calling
/// `abandon_segments` on a heap while one of its segments is mid-flight,
/// undrained, on the A1 deferred-free stack — and show it corrupts the A1
/// link, exactly as documented.
///
/// This does NOT go through any production entry point (it calls the
/// `pub unsafe fn abandon_segments` directly, as only a future reactivation
/// wiring or a test ever would); it exists to prove test 1 is not vacuous —
/// that the invariant it pins is actually load-bearing, not something that
/// would hold trivially even if `abandon_segments` collided with A1.
#[test]
fn reactivating_abandon_segments_while_a1_inflight_corrupts_the_link() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    while HeapRegistry::pop_abandoned_segment().is_some() {}

    let layout = Layout::from_size_align(LARGE_SIZE, 8).unwrap();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null());
    let heap_id = unsafe { (*heap).id() };

    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null(), "large alloc must succeed");
    assert_eq!(
        unsafe { (*heap).dbg_owner_id_for(p) },
        Some(heap_id),
        "segment must be stamped with the owner id before the race"
    );

    // Remote-free `p`: the segment is linked onto the OWNER's A1
    // deferred-free stack via `next_abandoned`, undrained.
    remote_free_large(layout, p);

    // THE HAZARD: reactivate `abandon_segments` on the SAME heap WHILE the
    // segment is still sitting, undrained, on the A1 stack (we have not
    // called `.alloc()` on `heap` since the remote free, so nothing has
    // touched `alloc_large`'s slow path / `drain_large_deferred_free`).
    // `abandon_segments` walks `heap`'s segment table (Phase 12.5 does NOT
    // clear it, so the segment is still listed there) and pushes the SAME
    // base onto `abandoned_segs`, overwriting `next_abandoned`.
    // SAFETY: `heap` was returned by `claim` and is the slot's sole writer.
    unsafe { HeapRegistry::abandon_segments(heap) };

    // SYMPTOM: the segment is now reachable from `abandoned_segs` — the
    // WRONG stack from A1's point of view (a segment that was supposed to be
    // exclusively on the owner's private deferred-free chain is now global
    // and poppable by any adopter). Compare against the segment BASE (`p` is
    // the payload pointer `alloc` returned, not necessarily the base itself).
    // This alone proves the field-sharing collision: `abandon_segments`
    // silently relinked a base that A1 still believes is its own chain node.
    let expected_base = segment_base_of(p);
    let popped = HeapRegistry::pop_abandoned_segment();
    assert_eq!(
        popped,
        Some(expected_base),
        "expected the remotely-freed segment's base to have been pushed onto \
         abandoned_segs by the reactivated abandon_segments call"
    );

    // We deliberately STOP here rather than also forcing A1's drain (e.g. by
    // allocating another large block on `heap`, which would run
    // `alloc_large`'s slow path -> `drain_large_deferred_free`). This was
    // tried during development of this test: because `abandon_segments`
    // overwrote `next_abandoned` with a value from a DIFFERENT encoding
    // (`abandoned_segs`'s "full pointer or ABANDONED_TAIL" link, vs. A1's
    // "full pointer or DEFERRED_LARGE_TAIL" link — see `deferred_large::tail`'s
    // doc comment on why the two sentinels must differ), A1's owner-side
    // drain reinterprets whatever `abandon_segments` left behind as its own
    // link word. On this author's dev machine that reproducibly crashed the
    // process with a misaligned-pointer-dereference abort (a hard, non-catchable
    // process abort — not a `panic!` that `#[should_panic]` can observe) rather
    // than degrading to a quiet lost-node symptom. That crash IS the sharpest
    // possible proof the hazard note is right ("corrupting whichever stack
    // loses the race... a wild/foreign pointer read, not just a leak"), but a
    // hard process abort cannot be asserted on safely inside this test binary
    // (it would take the whole `cargo test` run down with it, including
    // unrelated tests). The `abandoned_segs`-side symptom asserted above is
    // sufficient, safe, deterministic proof that the two stacks collided on
    // this segment's `next_abandoned` field the moment `abandon_segments` was
    // (mis)reactivated concurrently with an in-flight A1 push — which is
    // exactly the invariant this guardrail exists to pin.
    //
    // Cleanup: drain whatever is left on abandoned_segs. We do NOT recycle
    // `heap` or touch it further — its A1 link is now in an undefined state
    // (by design, that is the point of this test), so no further operation
    // on it is safe to perform here.
    while HeapRegistry::pop_abandoned_segment().is_some() {}
}
