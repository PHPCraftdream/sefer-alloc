//! [`push_large_deferred_free`] — extracted for #132 (unify the A1 guarantee
//! across both public allocator faces, `HeapCore` and `Heap`, without
//! duplicating the double-push-guarded Treiber push).

use core::sync::atomic::{AtomicPtr, Ordering};

use super::tail::DEFERRED_LARGE_TAIL;
use crate::alloc_core::segment_header::{SegmentMeta, ABANDONED_TAIL};

/// 0.3.0 (task A1; extracted 0.3.x task #132): push a Large/huge segment
/// `base` onto the OWNING heap's deferred-free stack, given `head` — a
/// `&AtomicPtr<u8>` reference to the owner's stable per-heap identity/stack
/// head (obtained by a REMOTE freer by dereferencing the
/// `owner_thread_free_at(segment_base)` stamp through the caller's own seam
/// discipline — this function itself takes the reference directly so it
/// requires NO `unsafe`/pointer seam of its own). Called from each face's
/// cross-thread dealloc routing in place of a permanent-leak no-op.
///
/// Classic Treiber push: read `base`'s `next_abandoned` link (repurposed
/// here as this stack's intrusive link), point it at the current head, CAS
/// the head to `base`. Multi-producer (any number of remote threads may race
/// this push); single-consumer (only the owner ever pops, in
/// [`drain_large_deferred_free`](super::drain::drain_large_deferred_free)),
/// so no ABA tag is needed — a pop can only be concurrent with OTHER
/// pushes, never with another pop.
///
/// # Caller's contract
///
/// `head` must be a live owner's stack head (guaranteed by the caller: read
/// fresh from the segment header on every call). `base` must be a
/// currently-registered `Large`-kind segment base (guaranteed by the
/// caller: only reached after confirming `kind_at(base) ==
/// SegmentKind::Large`).
///
/// ## Double-push guard (0.3.0 hardening, post-A1)
///
/// `base`'s `next_abandoned` field starts life as `ABANDONED_TAIL` (both
/// `SegmentHeader::small`/`large` constructors set it, and a segment
/// reclaimed via `AllocCore::reclaim_large_segment` is either unmapped or has
/// its header zeroed/rewritten before any future reuse — so a `base` that is
/// NOT currently linked into some Treiber stack always reads back
/// `ABANDONED_TAIL` here). A **double-free of the same `base` from remote
/// thread(s)** (already UB under the `GlobalAlloc` contract, but one this
/// allocator otherwise degrades safely on via the M2 double-free guard
/// everywhere else) used to be able to push `base` onto this stack TWICE
/// before a drain: the second push would read `head == base` (the result of
/// the first push's CAS) and write `base.next_abandoned = base` — a
/// self-loop. A drain would then pop `base` once, reclaim it (unregister the
/// slot + hand the segment to `os::release_segment`/the large-cache — i.e.
/// UNMAP or recycle it), and, because the self-loop pop never advanced
/// `head` away from `base`, the *next* loop iteration would read
/// `next_abandoned` off the now-unmapped memory (a use-after-free) and could
/// reclaim the same `base` a second time (a double-unmap).
///
/// The fix has two parts:
///
/// 1. **A dedicated on-this-stack tail sentinel, [`DEFERRED_LARGE_TAIL`],
///    distinct from `ABANDONED_TAIL`.** See its doc comment for why
///    conflating the two defeats the guard the very first time it's used
///    (pushing onto an empty stack).
/// 2. **A claim `compare_exchange`** on `next_atomic` from `ABANDONED_TAIL`
///    to the (now correctly tail-sentinel-aware) encoded link value, BEFORE
///    contesting `head`. Only the pusher that wins this CAS may proceed;
///    every other concurrent pusher of the SAME `base` observes
///    `next_atomic` already `!= ABANDONED_TAIL` and returns immediately (a
///    no-op — sound, matching the M2 double-free discipline used elsewhere:
///    a redundant free of an already-queued pointer is silently dropped
///    rather than corrupting state).
///
/// This guard is scoped to a single `base`'s link word — it does NOT block
/// concurrent pushes of DIFFERENT bases (`base1 != base2`): each has its own
/// `next_abandoned` field, so two remote threads freeing two distinct Large
/// segments still race only on the shared `head` CAS, exactly as before.
/// Lock-free multi-producer push (for distinct bases) is preserved.
pub(crate) fn push_large_deferred_free(head: &AtomicPtr<u8>, base: *mut u8) {
    let next_atomic = SegmentMeta::new(base).next_abandoned_atomic();
    let mut cur = head.load(Ordering::Acquire);
    loop {
        // Link `base` to the current head (or the "empty" sentinel,
        // `core::ptr::null_mut()`, encoded as `DEFERRED_LARGE_TAIL` — THIS
        // stack's own tail marker, distinct from `ABANDONED_TAIL` — so a
        // later pop can distinguish "no next" from a real base, and so the
        // double-push guard below can distinguish "on this stack" from "not
        // on any stack" even when the stack was empty at push time).
        let next_link = if cur.is_null() {
            DEFERRED_LARGE_TAIL
        } else {
            // EXPOSED-PROVENANCE STORE SITE: `cur` is the owner's real,
            // dereferenceable stack-head pointer (a Large segment base),
            // about to be packed into a plain `u64` link word inside
            // `next_abandoned`. `expose_provenance` explicitly registers
            // `cur`'s provenance so the paired load site —
            // `drain_large_deferred_free`'s `next_link as *mut u8`
            // reconstruction — may validly re-derive a dereferenceable
            // pointer via `with_exposed_provenance_mut`.
            cur.expose_provenance() as u64
        };
        // Double-push guard: claim `base`'s link word from the "not on any
        // stack" sentinel (`ABANDONED_TAIL`). If another pusher already won
        // this race for the SAME `base` (a double-free), `next_atomic` no
        // longer reads `ABANDONED_TAIL` (it now reads either
        // `DEFERRED_LARGE_TAIL` or a real link) and we bail out — `base` is
        // already queued for reclaim, so this push is a sound no-op. Only
        // the winner of this CAS may proceed to contest `head` below, so
        // `base`'s link word is exclusively ours for the remainder of this
        // call (a plain `store` on a lost `head` CAS retry, below, is
        // therefore safe: no other pusher can be touching this same
        // `base`'s link concurrently).
        if next_atomic
            .compare_exchange(
                ABANDONED_TAIL,
                next_link,
                Ordering::Release,
                Ordering::Relaxed,
            )
            .is_err()
        {
            return;
        }
        match head.compare_exchange(cur, base, Ordering::Release, Ordering::Relaxed) {
            Ok(_) => return,
            Err(actual) => {
                // Lost the head CAS to a concurrent pusher of a DIFFERENT
                // base. We already own this base's link word (guard CAS
                // above succeeded and is exclusive to us — no other pusher
                // of THIS base can be racing us), so a plain store retargets
                // the link to the fresh head before retrying the head CAS.
                next_atomic.store(
                    if actual.is_null() {
                        DEFERRED_LARGE_TAIL
                    } else {
                        // EXPOSED-PROVENANCE STORE SITE: same rationale as
                        // the `cur.expose_provenance()` site above — `actual`
                        // is the fresh head pointer this retry lost to, being
                        // packed into the link word for the same paired load
                        // in `drain_large_deferred_free`.
                        actual.expose_provenance() as u64
                    },
                    Ordering::Release,
                );
                cur = actual;
            }
        }
    }
}
