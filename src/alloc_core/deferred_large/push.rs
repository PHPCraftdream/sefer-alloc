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
    // Link `base` to the current head (or the "empty" sentinel,
    // `core::ptr::null_mut()`, encoded as `DEFERRED_LARGE_TAIL` — THIS stack's
    // own tail marker, distinct from `ABANDONED_TAIL` — so a later pop can
    // distinguish "no next" from a real base, and so the double-push guard
    // below can distinguish "on this stack" from "not on any stack" even when
    // the stack was empty at push time).
    //
    // EXPOSED-PROVENANCE STORE SITE (see `drain_large_deferred_free`'s paired
    // `with_exposed_provenance_mut` load): a non-null `cur` is the owner's
    // real, dereferenceable stack-head pointer being packed into a plain
    // `u64` link word.
    let next_link = if cur.is_null() {
        DEFERRED_LARGE_TAIL
    } else {
        cur.expose_provenance() as u64
    };
    // Double-push guard — claim `base`'s link word from the "not on any stack"
    // sentinel (`ABANDONED_TAIL`) EXACTLY ONCE, before the `head`-CAS retry
    // loop. If another pusher already won this race for the SAME `base` (a
    // double-free), `next_atomic` no longer reads `ABANDONED_TAIL` and we bail
    // out — a sound no-op (the base is already queued for reclaim).
    //
    // CRUCIAL (task #143): this claim MUST NOT live inside the retry loop. It
    // succeeds from `ABANDONED_TAIL` on the first attempt and moves the link
    // word to a real value; a second attempt (after losing the `head` CAS to a
    // concurrent pusher of a DIFFERENT base) would ALWAYS fail its
    // `ABANDONED_TAIL` claim and `return` early — WITHOUT ever winning `head`,
    // silently dropping `base` from the stack (an A1-class permanent leak).
    // That leak was found by `tests/loom_deferred_large.rs`. Claiming once and
    // then looping only on the `head` CAS is correct: the claim already
    // secured EXCLUSIVE ownership of this base's link word for the rest of the
    // call, so no other pusher of THIS base can race the plain `store`s below.
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
    loop {
        match head.compare_exchange(cur, base, Ordering::Release, Ordering::Relaxed) {
            Ok(_) => return,
            Err(actual) => {
                // Lost the head CAS to a concurrent pusher of a DIFFERENT
                // base. We already own this base's link word (the claim CAS
                // above succeeded and is exclusive to us), so a plain store
                // retargets the link to the fresh head before retrying the
                // head CAS — we do NOT re-run the claim (see the #143 note).
                //
                // EXPOSED-PROVENANCE STORE SITE: same rationale as `cur` above.
                next_atomic.store(
                    if actual.is_null() {
                        DEFERRED_LARGE_TAIL
                    } else {
                        actual.expose_provenance() as u64
                    },
                    Ordering::Release,
                );
                cur = actual;
            }
        }
    }
}
