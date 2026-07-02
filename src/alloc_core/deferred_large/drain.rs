//! [`drain_large_deferred_free`] — extracted for #132 (unify the A1
//! guarantee across both public allocator faces, `HeapCore` and `Heap`,
//! without duplicating the drain/reclaim pop loop).

use core::sync::atomic::{AtomicPtr, Ordering};

use super::tail::DEFERRED_LARGE_TAIL;
use crate::alloc_core::segment_header::SegmentMeta;
use crate::alloc_core::AllocCore;

/// TEST-ONLY (0.3.0, task A1; extracted 0.3.x task #132): process-wide count
/// of Large/huge segments reclaimed via the cross-thread deferred-free path
/// ([`drain_large_deferred_free`]). Bumped once per segment successfully
/// drained and handed to
/// [`AllocCore::reclaim_large_segment`](crate::alloc_core::AllocCore::reclaim_large_segment),
/// from EITHER public face (`HeapCore` or `Heap`) that calls this shared
/// primitive.
///
/// Diagnostic only (relaxed, like `DECOMMIT_CALLS` in `alloc_core.rs`),
/// `pub` so `tests/regression_xthread_large_free_no_leak.rs` (HeapCore face)
/// and the new `Heap`-face regression test can assert reclaim actually
/// happened.
#[doc(hidden)]
pub static DBG_LARGE_XTHREAD_RECLAIMED: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(0);

/// 0.3.0 (task A1; extracted 0.3.x task #132): drain a heap's deferred-free
/// stack (identified by `head`, a `&AtomicPtr<u8>` reference to its stable
/// per-heap identity/stack head), reclaiming every queued Large/huge segment
/// base via [`AllocCore::reclaim_large_segment`] on `core`. Called by the
/// OWNER on its own `alloc_large` slow path, before reserving a fresh
/// segment, so a cross-thread-freed large segment becomes available for
/// reuse (via the `alloc-decommit` large-cache) or is released to the OS
/// immediately (without `alloc-decommit`) — either way its `SegmentTable`
/// slot is freed for reuse (the fix for the A1 permanent-leak bug).
///
/// Pop loop: single-consumer (only the owner calls this, on its own `head`
/// and `core`), so a plain pop — no ABA tag, no CAS-retry-on-pop needed
/// beyond racing concurrent PUSHERS (remote frees can still be arriving
/// concurrently; the CAS handles that).
///
/// # Caller's contract
///
/// `head` and `core` MUST belong to the SAME heap (the stack `head` guards
/// and the substrate that owns the segments linked on it) — this function
/// does not and cannot verify that; callers pass `&self.thread_free`/
/// `&mut self.core` (or the `Heap`-face equivalents) from the same `self`.
pub(crate) fn drain_large_deferred_free(head: &AtomicPtr<u8>, core: &mut AllocCore) {
    loop {
        let cur = head.load(Ordering::Acquire);
        if cur.is_null() {
            return;
        }
        let meta = SegmentMeta::new(cur);
        let next_link = meta.next_abandoned_atomic().load(Ordering::Acquire);
        // `DEFERRED_LARGE_TAIL` (not `ABANDONED_TAIL`) is this stack's own
        // "no next" encoding — see `push_large_deferred_free`'s doc comment
        // on why the two sentinels must differ.
        let next = if next_link == DEFERRED_LARGE_TAIL {
            core::ptr::null_mut()
        } else {
            // EXPOSED-PROVENANCE LOAD SITE: `next_link` is a plain `u64`
            // address written by `push_large_deferred_free`'s
            // `cur.expose_provenance()` / `actual.expose_provenance()` store
            // sites (see that function). Reconstructing via
            // `with_exposed_provenance_mut` is sound under the exposed model
            // because the writer always exposed the real pointer's
            // provenance before storing its address here.
            core::ptr::with_exposed_provenance_mut::<u8>(next_link as usize)
        };
        match head.compare_exchange(cur, next, Ordering::Acquire, Ordering::Relaxed) {
            Ok(_) => {
                core.reclaim_large_segment(cur);
                DBG_LARGE_XTHREAD_RECLAIMED.fetch_add(1, Ordering::Relaxed);
            }
            Err(_) => continue, // a concurrent push raced us — retry with fresh head
        }
    }
}
