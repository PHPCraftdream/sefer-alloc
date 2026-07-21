//! R10-7 follow-up regression: `HeapCore::alloc_batch` must NOT double-issue
//! a magazine-drained block whose stale cross-thread double-free entry is
//! still sitting in its segment's remote-free ring.
//!
//! **Context.** `alloc_batch` (task R10-7, commit `9611a56`) drains the warm
//! magazine into `out[0..]` (step 1), then batch-refills the remainder via
//! `refill_class_bump_checked` (step 2). Step 2's internal ring-drain
//! (`drain_dirty_segments` inside `find_segment_with_free_checked`) encounters
//! any ring entries pushed by cross-thread frees and reclaims them through
//! `reclaim_offset_checked`, which consults an `is_in_magazine` predicate to
//! reject entries whose target block is still magazine-resident (the M2
//! defence-in-depth against the ring↔magazine double-free residual).
//!
//! **The bug (two compounding halves).**
//!
//! 1. **Bit-clear-too-early.** Step 1's drain loop called
//!    `clear_magazine(off)` per pop, so by the time step 2 ran, every
//!    magazine-drained block's residency bit was already clear — the block
//!    looked "not magazine-resident" to step 2's predicate.
//! 2. **`if k == c { return false; }` short-circuit.** Step 2's closure
//!    opened with this shortcut, copy-pasted from `refill_magazine_slow` —
//!    where it is justified by that function's KEY INVARIANT (`count[c] == 0`
//!    at refill time, meaning nothing of class `c` has been claimed).
//!    `alloc_batch` violates this precondition: step 1 has already pulled
//!    `magazine_drained` class-`c` blocks into `out[0..magazine_drained]`, so
//!    the shortcut unconditionally skips the magazine-residency check for
//!    EXACTLY the class under refill — which is the one class that matters.
//!
//! Together: a stale cross-thread double-free ring entry for a block of class
//! `c` that step 1 just drained (and step 1 already cleared its residency bit)
//! sails through BOTH halves of the predicate → `reclaim_offset_checked`
//! links it onto the freelist → `drain_freelist_batch` pulls it into
//! `out[filled..]` → the SAME pointer now appears twice in `out`.
//!
//! **The fix** (applied in `src/registry/heap_core_alloc.rs`):
//! 1. Defer the magazine-residency bit clear: step 1 no longer calls
//!    `clear_magazine` per pop; the bits stay SET through step 2.
//! 2. Remove the `if k == c { return false; }` short-circuit from step 2's
//!    closure so the residency bitmap is actually consulted for class `c`.
//! 3. After step 2 returns, one bulk pass clears the residency bits of
//!    `out[0..magazine_drained]`.
//!
//! **Counterfactual test.** This test constructs the hazardous state:
//! 1. `alloc(P)` — live (bitmap: allocated; magazine: not-resident).
//! 2. `dealloc(P)` own-thread — P enters the magazine (`mark_magazine`;
//!    bitmap still "allocated" since the magazine push does not `mark_free`).
//! 3. Cross-thread `dealloc(P)` from a producer thread — a DELIBERATE
//!    double-free (caller UB under the `unsafe fn` contract, but the M2
//!    guard's documented job is to degrade this benignly). The producer's
//!    `dealloc_foreign_slow` pushes P's offset into the owner's ring AND
//!    sets the dirty bit on P's segment.
//! 4. `alloc_batch(layout, out[..N])` with N large enough to drain the
//!    magazine AND reach into the refill remainder.
//!
//! Under the FIXED code, step 4 produces N distinct pointers (P appears at
//! most once). Under the BUGGY code, step 2's reclaim links the stale ring
//! entry for P onto the freelist, `drain_freelist_batch` pulls it back into
//! `out[filled..]`, and P appears TWICE → the `HashSet` duplicate check fails.
//!
//! **Feature gate.** `alloc-global`, `alloc-xthread`, `fastbin` (the
//! magazine + cross-thread ring substrate). `production` (which bundles all
//! three + `alloc-decommit` + `alloc-segment-directory`) is the primary
//! configuration; `alloc-segment-directory` is needed so `drain_dirty_segments`
//! runs inside `find_segment_with_free_impl` (without it the production
//! ring-drain path is not exercised and the bug would not manifest from the
//! `alloc_batch` call alone). `batch-api` gates the `alloc_batch` method
//! itself (the R10-7 follow-up API-boundary tightening — NOT part of
//! `production`).

#![cfg(all(
    feature = "alloc-global",
    feature = "alloc-xthread",
    feature = "fastbin",
    feature = "alloc-segment-directory",
    feature = "batch-api"
))]

extern crate sefer_alloc;

use std::alloc::Layout;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use sefer_alloc::registry::{bootstrap, HeapRegistry};

// Serialise against other tests in this binary: the registry is a
// process-global static shared across every HeapCore/HeapCore in the process.
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

/// Counterfactual regression for the R10-7 follow-up fix: `alloc_batch` must
/// not double-issue a magazine-drained block whose stale cross-thread
/// double-free entry is still in its segment's remote-free ring.
///
/// **RED before the fix:** the unfixed `alloc_batch` clears the residency bit
/// in step 1 AND short-circuits the class-`c` predicate in step 2, so the
/// stale ring entry is reclaimed (linked to freelist + `mark_free`), then
/// re-issued into `out[filled..]` → P appears twice → the HashSet check
/// (or the targeted `p_count` assertion) fails.
///
/// **GREEN after the fix:** the deferred clear + no-shortcut predicate leaves
/// the residency bit SET through step 2, so `reclaim_offset_checked`'s
/// existing `is_in_magazine` guard drops the stale ring entry (return false:
/// no link, no mark_free) → P is issued exactly once.
#[test]
fn alloc_batch_no_duplicate_on_stale_xthread_double_free_entry() {
    let _g = SerialGuard::acquire();
    let _ = bootstrap::ensure();
    let heap = HeapRegistry::claim();
    assert!(!heap.is_null(), "HeapRegistry::claim returned null");

    let layout = Layout::from_size_align(16, 8).unwrap();

    // (1) Alloc P. This is the first alloc of class 0 (16B) in this test
    //     binary → triggers `refill_magazine_slow` → carves `refill_n` blocks
    //     into the magazine → pops P for the caller. After this, the magazine
    //     holds `refill_n - 1` blocks of class 0, and P is live in the caller.
    // SAFETY: valid layout; heap is the calling thread's own slot.
    let p = unsafe { (*heap).alloc(layout) };
    assert!(!p.is_null(), "alloc(P) returned null");

    // (2) Own-thread free P → P is pushed to the magazine (the magazine had
    //     capacity after step 1's pop). `mark_magazine(P)` is called, so P's
    //     magazine-residency bit is SET. The alloc_bitmap still reads
    //     "allocated" (the magazine push does not `mark_free`).
    // SAFETY: `p` was allocated above with `layout`; freed once here.
    unsafe { (*heap).dealloc(p, layout) };

    // (3) Cross-thread free P from a producer thread. This is a DELIBERATE
    //     double-free (P was already freed in step 2) — caller UB under the
    //     `unsafe fn` contract, but the M2 drain guard's documented job is to
    //     degrade this benignly (drop the stale ring entry, no corruption).
    //     The producer's `dealloc_foreign_slow` pushes P's offset into the
    //     owner's ring AND sets the dirty bit on P's segment, so the
    //     production `drain_dirty_segments` inside `alloc_batch`'s refill
    //     will process the entry.
    let x_addr = p as usize;
    let producer = thread::spawn(move || {
        let _ = bootstrap::ensure();
        let remote = HeapRegistry::claim();
        assert!(!remote.is_null(), "producer HeapRegistry::claim failed");
        // SAFETY (R6-MS-1/2 + raw-deref): `remote` is a live heap; `x_addr`
        // is a block previously allocated by `heap` (the owner). This dealloc
        // from a DIFFERENT thread routes through `dealloc_foreign_slow`, which
        // pushes the offset into the owner's ring and sets the dirty bit. The
        // block was already freed in step 2 — this is a deliberate double-free
        // to exercise the drain's magazine-residency rejection guard. The
        // allocator handles this defensively (the guard returns false, no
        // corruption) under the fixed code.
        unsafe { (*remote).dealloc(x_addr as *mut u8, layout) };
        // SAFETY: `remote` was claimed above; recycled whole here.
        unsafe { HeapRegistry::recycle(remote) };
    });
    producer.join().expect("producer thread must not panic");

    // (4) `alloc_batch` with `out.len()` large enough to (a) drain the entire
    //     magazine into `out[0..magazine_drained]` (P is among the drained)
    //     AND (b) reach into the refill remainder so `refill_class_bump_checked`
    //     calls `find_segment_with_free_checked` → `drain_dirty_segments`,
    //     which processes P's stale ring entry. 64 > TCACHE_CAP (16) + any
    //     `refill_n` for class 0, so both phases are exercised.
    let n = 64usize;
    let mut out: Vec<*mut u8> = vec![std::ptr::null_mut(); n];
    // SAFETY: `layout` is a valid non-zero Layout; `heap` is the calling
    // thread's own slot. Every returned non-null pointer is freed exactly
    // once in the cleanup loop below.
    let filled: usize = unsafe { (*heap).alloc_batch(layout, &mut out) };
    assert!(filled > 0, "alloc_batch returned 0 (OOM?)");

    // (5) Assert NO duplicate pointers in `out[..filled]`. The HashSet check
    //     mirrors `tests/batch_tcache.rs`'s `alloc_batch_valid` pattern.
    let mut seen: HashSet<usize> = HashSet::new();
    for (i, &q) in out[..filled].iter().enumerate() {
        assert!(
            !q.is_null(),
            "alloc_batch returned null at [{i}] (filled={filled})"
        );
        assert!(
            seen.insert(q as usize),
            "alloc_batch issued duplicate ptr at [{i}] (filled={filled}): {:p}",
            q,
        );
    }

    // (6) Targeted check: P (the bug scenario's target) must appear at most
    //     once. Under the buggy code P appears exactly twice (once from the
    //     magazine drain, once from the reclaimed ring entry); under the
    //     fixed code at most once.
    let p_count = out[..filled].iter().filter(|&&q| q == p).count();
    assert!(
        p_count <= 1,
        "P ({:p}) was issued {p_count} times by alloc_batch (expected \u{2264} 1): \
         the stale cross-thread double-free ring entry was reclaimed and \
         re-issued instead of being rejected by the magazine-residency guard",
        p,
    );

    // Cleanup: free every block alloc_batch handed out (exactly once each —
    // the HashSet check above guarantees no duplicates in the GREEN state).
    // SAFETY: every `q` in `out[..filled]` was produced by `alloc_batch` above
    // with `layout`; each is freed exactly once here.
    for &q in &out[..filled] {
        unsafe { (*heap).dealloc(q, layout) };
    }

    // SAFETY: `heap` was claimed above; recycled whole here.
    unsafe { HeapRegistry::recycle(heap) };
}
