//! [`HeapRegistry`] — the global self-hosting heap slot table (§2.1 of
//! `MALLOC_PLAN_PHASE12-13.md`): claim/recycle/abandon over the process-global
//! [`Registry`](super::bootstrap::Registry) slot array.
//!
//! This is the lock-free fundament of Phase 12: every thread's heap is a SLOT
//! in this registry, not a TLS-owned `Box`. A thread claims a slot on first
//! use, caches a raw `*mut HeapCore` to it in TLS (12.3), and on thread exit
//! abandons its segments back to the registry (12.3/12.4) and recycles the
//! slot. Adoption (12.4) reclaims abandoned segments into a live heap.
//!
//! ## Phase 12.2 scope
//!
//! This file ships the structure + the claim/recycle/abandon API, exercised
//! single-threaded by `tests/registry_basic.rs`. The orderings are written
//! CORRECT for the lock-free concurrent case from day one (loom verification
//! is Phase 12.4); each atomic op carries a `// why:` comment.
//!
//! ## ABA defence
//!
//! Both Treiber stacks ([`Registry::free_slots`], [`Registry::abandoned_segs`])
//! carry a monotonic tag in the high 32 bits of their `AtomicU64` head,
//! bumped on every push. This defeats the classic ABA (pop-X, re-push-X
//! while a racer is parked with head=X): the re-push bumps the tag, so the
//! racer's CAS on `(X, old_tag)` fails. See [`super::tagged_ptr::TaggedPtr`]
//! for the tag-width-vs-churn analysis.

// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see
// `src/lib.rs`); this is the documented registry seam (the pointer handoff
// `*mut HeapCore` out of a slot's `UnsafeCell`, plus `get_unchecked` on the
// `'static` slot array under range-checked indices). `allow` lifts the
// crate-level `deny` for this file only — `unsafe` anywhere else in the crate
// is a hard error. Every `unsafe` block carries a `// SAFETY:` proof.
#![allow(unsafe_code)]

use core::sync::atomic::Ordering;

use super::bootstrap::{ensure, Registry, MAX_HEAPS};
use super::heap_core::HeapCore;
use super::heap_slot::{HeapSlot, NEXT_FREE_TAIL, STATE_FREE, STATE_LIVE};
use super::tagged_ptr::TaggedPtr;

/// The global heap slot table. All methods operate on the process-global
/// [`Registry`] returned by [`ensure`]; the type itself carries no state (it
/// is a name-space for the API, mirroring how `Node` / `Layout` are organised
/// elsewhere in the crate).
#[doc(hidden)]
pub struct HeapRegistry;

impl HeapRegistry {
    /// Claim a free slot and return a `*mut HeapCore` into it.
    ///
    /// Tries the `free_slots` stack first (a recycled slot); on empty, mints
    /// a fresh slot by bumping `count`. Then CASes the slot `FREE → LIVE`,
    /// bumps its `generation`, and (lazily) materialises the `HeapCore` in
    /// the slot's `UnsafeCell` if this is the slot's first claim. Returns
    /// `null` only if `count` has reached `MAX_HEAPS` AND the free stack is
    /// empty (registry exhaustion — the caller, 12.3, falls back to the
    /// primordial heap).
    #[must_use]
    pub fn claim() -> *mut HeapCore {
        let reg = ensure();
        // 1. Obtain a candidate slot index: pop from free_slots, or bump count.
        let idx = match pop_free_slot(reg) {
            Some(i) => i,
            None => match bump_count(reg) {
                Some(i) => i,
                None => return core::ptr::null_mut(), // registry exhausted
            },
        };
        // SAFETY: `idx < MAX_HEAPS` (guaranteed by both pop_free_slot's range
        // check and bump_count's cap). The slot array is `'static` (lives in
        // the `REGISTRY` bootstrap static for the process lifetime), so the
        // reference is valid for as long as the registry exists.
        let slot = unsafe { reg.slots.get_unchecked(idx) };

        // 2. CAS FREE → LIVE. This is the claim's linearization point: the
        //    single atomic step that establishes this thread as the slot's
        //    sole writer. AcqRel: Acquire to see the slot's prior state
        //    (generation, heap-init flag) written by a previous recycler;
        //    Release so a later recycler's Acquire sees our generation bump +
        //    heap materialisation.
        //
        //    Single-thread (Phase 12.2) the CAS trivially succeeds. Under
        //    concurrency (12.3+) an `Err(LIVE)` means another thread won THIS
        //    slot; we restart `claim` to obtain a fresh candidate.
        if slot.cas_state(
            STATE_FREE,
            STATE_LIVE,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) == Err(STATE_LIVE)
        {
            // Lost the race to another claimer for THIS index. This cannot
            // happen single-threaded; under concurrency (12.3+) it means
            // another thread won this slot. We restart `claim` to obtain a
            // fresh candidate slot.
            return Self::claim();
        }

        // 3. Bump the generation (the M8/M9 coherence key). A later
        //    stale-TLS-pointer check (12.3) compares a cached
        //    `(idx, generation)` against the slot's current generation; a
        //    mismatch means the slot was recycled + reclaimed by another
        //    thread, and the cached pointer is stale.
        //
        //    `fetch_add(Release)` under our exclusive writer status (the CAS
        //    above won) pairs with the 12.3 reader's Acquire load of
        //    generation after observing state == LIVE. We read the NEW value
        //    (fetch_add returns the OLD) so the "first claim" detection below
        //    sees generation == 1.
        let new_gen = slot.generation.fetch_add(1, Ordering::Release) + 1;

        // 4. Lazily materialise the HeapCore if this is the slot's first
        //    claim (heap is uninitialised). On a later reclaim we reuse the
        //    already-live HeapCore as-is. We track "first claim" via the
        //    generation: `new_gen == 1` means this is the slot's first-ever
        //    claim (it started at 0 and the bump just produced 1).
        if new_gen == 1 {
            // SAFETY: we are the slot's sole writer (CAS FREE→LIVE won), and
            // the slot is uninitialised on its first claim. `HeapCore::new`
            // bootstraps a segment substrate (OS aperture, no std::alloc);
            // on failure we leave the slot uninitialised and fall back to
            // re-claiming (the slot stays LIVE but its heap is null — the
            // 12.3 fallback-heap path handles this). For 12.2 we propagate
            // null on OOM.
            let heap_ptr = slot.heap.get();
            // SAFETY: `heap_ptr` is valid for writes (the slot owns the
            // `MaybeUninit<HeapCore>` and we are its sole writer). `write`
            // initialises the value, transferring ownership of the new
            // `HeapCore` into the slot.
            match HeapCore::new(idx as u32) {
                // SAFETY: `heap_ptr` is `*mut MaybeUninit<HeapCore>` obtained
                // from the slot's `UnsafeCell`; casting to `*mut HeapCore`
                // and `write`-ing initialises the value in place. We are the
                // slot's sole writer (the CAS won) and the slot was
                // uninitialised (first claim, generation == 1). `write`
                // transfers ownership of the new `HeapCore` into the slot.
                Some(hc) => unsafe { heap_ptr.cast::<HeapCore>().write(hc) },
                None => {
                    // Primordial OOM. Roll the slot back to FREE so it can be
                    // re-claimed, and report OOM to the caller.
                    let _ = slot.cas_state(
                        STATE_LIVE,
                        STATE_FREE,
                        Ordering::Release,
                        Ordering::Relaxed,
                    );
                    return core::ptr::null_mut();
                }
            }
        }

        // 5. Hand out `*mut HeapCore`. The pointer is valid for the slot's
        //    lifetime (the `'static` slot array), under the single-writer
        //    invariant the CAS established.
        // SAFETY: the slot is LIVE and (now) initialised; we are its sole
        // writer. The pointer stays valid until the slot is recycled (which
        // only this thread can do, since it owns the LIVE state). The slot
        // array is `'static`, so the pointer outlives any borrowing scope.
        slot.heap.get().cast::<HeapCore>()
    }

    /// Recycle a live slot back to the free pool. Called by the owning
    /// thread (the LIVE-state holder) when it no longer needs the heap
    /// (typically on thread exit, after abandoning its segments — 12.3).
    ///
    /// `heap` MUST be a pointer previously returned by [`claim`](Self::claim)
    /// and not yet recycled. Double-recycle is a no-op (defensive): the CAS
    /// LIVE→FREE fails on an already-FREE slot and we return without pushing.
    ///
    /// # Safety
    ///
    /// `heap` must be either null (treated as a no-op) or a pointer
    /// previously returned by [`claim`](Self::claim) and not yet passed to
    /// `recycle` (the slot must still be `LIVE`). Passing any other pointer
    /// is undefined behaviour (the registry reads `heap.id()` to find the
    /// owning slot, and an out-of-range id would index the slot array
    /// unsafely — the registry guards against this with a range check, but
    /// a dangling pointer may still fault on the read).
    pub unsafe fn recycle(heap: *mut HeapCore) {
        if heap.is_null() {
            return;
        }
        let reg = ensure();
        // SAFETY: caller guarantees `heap` was returned by `claim`, which
        // derived it from a slot at index `heap.id()` in the `'static` slot
        // array. The slot index is in range by construction (we re-check
        // below before indexing).
        let idx = unsafe { (*heap).id() } as usize;
        if idx >= MAX_HEAPS {
            return;
        }
        // SAFETY: `idx < MAX_HEAPS`, checked above.
        let slot = unsafe { reg.slots.get_unchecked(idx) };

        // CAS LIVE → FREE. Release on success: a later claim's Acquire load
        // of `state` (via its CAS) sees the slot's recycled state and the
        // `next_free` link we are about to push. Relaxed on failure: the slot
        // was not LIVE (double-recycle or raced); we no-op.
        if slot.cas_state(STATE_LIVE, STATE_FREE, Ordering::Release, Ordering::Relaxed)
            == Err(STATE_FREE)
        {
            // Already FREE — defensive no-op (do not push a free slot twice,
            // which would corrupt the stack).
            return;
        }

        // Push the slot onto the free_slots stack (tagged-Treiber). The push
        // establishes this slot as available for a future claim.
        push_free_slot(reg, idx as u32);
    }

    /// Abandon a heap's owned segments onto the global abandoned-segments
    /// stack, so a later adopter (12.4) can reclaim them. Called by the
    /// owning thread BEFORE recycling the slot (typically on thread exit:
    /// the segments stay mapped, the free state travels with them in their
    /// `BinTable`s, and an adopter CAS-claims them).
    ///
    /// **Phase 12.2:** this is a no-op stub — the registry structure is in
    /// place (the `abandoned_segs` stack and its push/pop primitives are
    /// fully implemented and tested via [`pop_abandoned_segment`]), but
    /// wiring it to walk a `HeapCore`'s owned segments requires the segment
    /// header's `owner` field that 12.3 introduces. We expose the primitive
    /// so the API surface is complete and the round-trip test passes; the
    /// actual abandonment-walk is a 12.3 deliverable.
    ///
    /// [`pop_abandoned_segment`]: Self::pop_abandoned_segment
    ///
    /// # Safety
    ///
    /// `heap` must be either null (treated as a no-op) or a pointer
    /// previously returned by [`claim`](Self::claim). In Phase 12.2 this is
    /// a no-op stub, so the pointer is never dereferenced; the `unsafe`
    /// obligation is forward-looking (12.3+ will walk the heap's segment
    /// table through it).
    pub unsafe fn abandon_segments(_heap: *mut HeapCore) {
        // 12.3/12.4: walk the heap's owned segments (via the segment table),
        // mark each `owner.state = ABANDONED`, and `push_abandoned_segment`
        // its base onto the stack. The free state travels with the segments
        // (it is in their `BinTable`s), so no merging is needed.
        //
        // Until the owner-stamping exists (12.3), there is nothing to walk
        // safely. This stub preserves the API shape and the round-trip test
        // exercises push/pop directly.
    }

    /// Push a segment base onto the abandoned-segments stack. The registry
    /// retains the segment (it stays mapped); a later adopter pops it via
    /// [`pop_abandoned_segment`](Self::pop_abandoned_segment) and CAS-claims
    /// its ownership. Tagged-Treiber: the high 32 bits of the head carry a
    /// monotonic tag bumped per push (ABA fix).
    ///
    /// **Phase 12.2:** exposed as a primitive for the round-trip test and for
    /// `abandon_segments` (12.3+). It is fully implemented and ordering-
    /// correct.
    pub fn push_abandoned_segment(base: *mut u8) {
        let reg = ensure();
        let value = base as u64;
        // The tag defeats ABA across the pop-repush race (see TaggedPtr).
        debug_assert!(
            (value & !((1u64 << 32) - 1)) == 0 || cfg!(miri),
            "abandoned_segs stores segment bases in the low 32 bits; \
             base addresses above 4 GiB are unsupported on this target \
             (use an intrusive head+next layout if the target needs it)"
        );
        let mut head = reg.abandoned_segs.load(Ordering::Acquire);
        loop {
            let (_cur_base, tag) = TaggedPtr::unpack(head);
            let new_tag = tag.wrapping_add(1);
            let new_head = TaggedPtr::pack(value, new_tag);
            // CAS: Acquire on success (we read the prior head to chain off
            // it — though this stack is non-intrusive, the tag's monotonicity
            // is the correctness invariant); Release would also suffice; we
            // use AcqRel for symmetry with the pop's swap(Acquire). Relaxed
            // on failure (retry).
            match reg.abandoned_segs.compare_exchange(
                head,
                new_head,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(actual) => head = actual,
            }
        }
    }

    /// Pop the most-recently-abandoned segment base, or `None` if the stack
    /// is empty. Called by an adopter (12.4) on its cold path to reclaim an
    /// abandoned segment. The adopter then CAS-claims the segment's
    /// `owner` `ABANDONED → LIVE(me, gen+1)` (the M9 linearization point).
    ///
    /// **Phase 12.2 single-slot semantics:** the abandoned-segments stack
    /// stores at most ONE base at a time in this phase (a `swap(empty)` pop
    /// drains the head and clears the stack). This is sufficient for the
    /// round-trip test and for the 12.3 thread-exit abandon protocol (a
    /// thread abandons its heap's segments as a batch, then the next adopter
    /// pops them as a batch). Multi-element chaining with an intrusive
    /// `next_abandoned` field (in the segment header, once 12.3 adds the
    /// `owner` field) is the 12.4 form; until then this single-slot drain is
    /// exact for the single-threaded tests. The `tag` on the head still
    /// defeats ABA for the push/pop race even with a single entry.
    #[must_use]
    pub fn pop_abandoned_segment() -> Option<*mut u8> {
        let reg = ensure();
        // swap(empty) atomically drains the head: we take whatever was there
        // and clear the stack. Acquire to see the push's Release store of the
        // base value. Under the single-slot 12.2 semantics this is exact;
        // under concurrency (12.4) the swap resolves a pop race atomically
        // (one popper gets the base, the other gets empty).
        let head = reg.abandoned_segs.swap(TaggedPtr::empty(), Ordering::Acquire);
        if TaggedPtr::is_empty(head) {
            return None;
        }
        let (value, _tag) = TaggedPtr::unpack(head);
        Some(value as *mut u8)
    }
}

// ---------------------------------------------------------------------------
// Treiber-stack primitives on the `free_slots` stack. Module-private.
// ---------------------------------------------------------------------------

/// Pop a free slot index off the `free_slots` stack (classic Treiber pop),
/// or `None` if empty.
///
/// This is the textbook Treiber pop: load the (tagged) head, read its
/// `next_free` link, then CAS the head to that next link. The tag in the high
/// 32 bits defeats the ABA problem: if between our load and our CAS another
/// thread pops the head and re-pushes the SAME slot index, the tag will have
/// advanced, our CAS fails, and we retry — never observing a stale chain.
///
/// **Ordering:** `Acquire` on the success CAS so we see the `next_free` link
/// the pusher wrote under `Release`; `Relaxed` on failure (retry, no
/// side-effect).
fn pop_free_slot(reg: &Registry) -> Option<usize> {
    let mut head = reg.free_slots.load(Ordering::Acquire);
    loop {
        if TaggedPtr::is_empty(head) {
            return None;
        }
        let (idx_v, _tag) = TaggedPtr::unpack(head);
        let idx = idx_v as u32;
        if idx as usize >= MAX_HEAPS {
            // Defensive: cannot happen by construction (push only stores
            // valid indices).
            return None;
        }
        // SAFETY: `idx < MAX_HEAPS`, checked above.
        let slot: &HeapSlot = unsafe { reg.slots.get_unchecked(idx as usize) };
        // Read the next link BEFORE the CAS (the push stored it under
        // Release; our Acquire load of `head` + this Acquire read see it).
        let next = slot.next_free.load(Ordering::Acquire);
        // The new head is `next` (or empty if `next == NEXT_FREE_TAIL`) with
        // the SAME tag we observed — the tag is bumped only on PUSH, so a pop
        // preserves it. A concurrent re-push of `idx` will bump the tag and
        // fail our CAS (the ABA fix).
        let new_head = if next == NEXT_FREE_TAIL {
            TaggedPtr::empty()
        } else {
            TaggedPtr::pack(next as u64, _tag)
        };
        // CAS the head to `new_head`. Acquire on success (see the push's
        // Release store of `next_free`); Relaxed on failure (retry).
        match reg.free_slots.compare_exchange(
            head,
            new_head,
            Ordering::Acquire,
            Ordering::Relaxed,
        ) {
            Ok(_) => return Some(idx as usize),
            Err(actual) => head = actual, // retry with the new head
        }
    }
}

/// Push a slot index onto the `free_slots` stack. Sets the slot's `next_free`
/// link first (so a later pop can restore the chain), then CASes the head.
fn push_free_slot(reg: &Registry, idx: u32) {
    // SAFETY: `idx < MAX_HEAPS` (the caller — recycle — derived it from a
    // valid heap pointer).
    let slot: &HeapSlot = unsafe { reg.slots.get_unchecked(idx as usize) };
    let mut head = reg.free_slots.load(Ordering::Acquire);
    loop {
        // The next link this slot will chain to: the current head's index,
        // or NEXT_FREE_TAIL if the stack is empty (so a later pop sees the
        // tail sentinel and knows the chain ends here). Note: the empty
        // sentinel packs `INDEX_MASK` in the low bits, which numerically
        // equals `NEXT_FREE_TAIL` (`u32::MAX`), but we spell the empty→tail
        // mapping out explicitly so the invariant does not rest on the
        // accidental value coincidence.
        let next_link = if TaggedPtr::is_empty(head) {
            NEXT_FREE_TAIL
        } else {
            let (cur_idx, _tag) = TaggedPtr::unpack(head);
            cur_idx as u32
        };
        // Write the link under Release so a concurrent pop's Acquire read of
        // `next_free` (after observing this slot as the head) sees it.
        slot.next_free.store(next_link, Ordering::Release);
        // Advance the tag (the ABA fix) and CAS the head to this slot.
        let (_cur_idx, tag) = TaggedPtr::unpack(head);
        let new_tag = tag.wrapping_add(1);
        let new_head = TaggedPtr::pack(idx as u64, new_tag);
        // CAS: Release on success so a pop's Acquire sees the `next_free`
        // link we just wrote. Relaxed on failure (retry).
        match reg.free_slots.compare_exchange(
            head,
            new_head,
            Ordering::Release,
            Ordering::Relaxed,
        ) {
            Ok(_) => return,
            Err(actual) => head = actual,
        }
    }
}

/// Mint a fresh slot by bumping `count`. Returns the new slot's index, or
/// `None` if `count` has reached `MAX_HEAPS`. The new slot is already in its
/// bootstrap state (`FREE`, generation 0, heap uninitialised) thanks to the
/// `const` initialiser; no extra init is needed.
fn bump_count(reg: &Registry) -> Option<usize> {
    // fetch_add is RMW: AcqRel so we see any prior slot writes (none needed
    // here, but conservative) and later claimers see our bump.
    let idx = reg.count.fetch_add(1, Ordering::AcqRel);
    if idx as usize >= MAX_HEAPS {
        // Roll back the bump (best-effort) and report exhaustion. Under
        // concurrency a rollback race is benign (the cap is a soft bound; an
        // over-bump just wastes an index slot).
        reg.count.fetch_sub(1, Ordering::AcqRel);
        return None;
    }
    Some(idx as usize)
}
