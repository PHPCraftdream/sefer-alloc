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

use crate::alloc_core::segment_header::{
    pack_owner, unpack_owner_gen, unpack_owner_id, unpack_owner_state, SegmentMeta, ABANDONED_TAIL,
    OWNER_ID_NONE, OWNER_STATE_ABANDONED,
};
use super::bootstrap::{
    abandoned_head_is_empty, ensure, pack_abandoned_head, unpack_abandoned_head, ABANDONED_HEAD_EMPTY,
    Registry, MAX_HEAPS,
};
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
    /// intrusive stack, so a later adopter can reclaim them. Called by the
    /// owning thread BEFORE recycling the slot (typically on thread exit:
    /// the segments stay mapped, the free state travels with them in their
    /// `BinTable`s, and an adopter CAS-claims them).
    ///
    /// **Phase 12.4:** this is the real implementation. It walks the heap's
    /// owned segments (via [`AllocCore::segment_bases`] — the read-only
    /// segment-table iterator), CASes each segment's `owner_state`
    /// LIVE→ABANDONED (under the owning thread's exclusive writer status this
    /// CAS trivially succeeds), and pushes each base onto the abandoned-segs
    /// intrusive Treiber stack. The free state travels with the segments (it
    /// is in their `BinTable`s), so NO merging is needed — this is the
    /// keystone simplification of the §2.0 segment-centric ownership model.
    ///
    /// # Safety
    ///
    /// `heap` must be either null (treated as a no-op) or a pointer
    /// previously returned by [`claim`](Self::claim) and not yet recycled.
    /// The caller (the owning thread) must be the sole writer of the heap's
    /// segment owner-state (established by the claim CAS).
    pub unsafe fn abandon_segments(heap: *mut HeapCore) {
        if heap.is_null() {
            return;
        }
        let reg = ensure();
        // SAFETY: caller guarantees `heap` was returned by `claim` and is the
        // slot's sole writer. We hold a `&mut HeapCore` for the duration of
        // the walk; no other thread mutates this heap's segments (the slot is
        // still LIVE — we have not recycled it yet).
        let heap_ref: &mut HeapCore = unsafe { &mut *heap };
        let owner_id = heap_ref.id();
        // Walk every segment this heap owns and abandon each. The bases come
        // from the heap's own AllocCore segment table; the registry's global
        // abandoned-segs stack is the destination.
        for base in heap_ref.segment_bases() {
            abandon_one_segment(reg, base, owner_id);
        }
    }

    /// Push a segment base onto the abandoned-segments intrusive Treiber
    /// stack. The registry retains the segment (it stays mapped); a later
    /// adopter pops it via [`pop_abandoned_segment`](Self::pop_abandoned_segment)
    /// and CAS-claims its ownership.
    ///
    /// Sets the segment's `next_abandoned` header link to chain off the
    /// current head, then CASes the head to this base. The head packs the
    /// full 64-bit base (in the high bits — the base is SEGMENT-aligned, so
    /// its low 22 bits are zero) with an ABA tag in those low bits, bumped
    /// per push. This is the fix for FINDINGS №1: the old `AtomicU64` packing
    /// stored the base in the low 32 bits and truncated addresses above 4 GiB
    /// (ASLR); the new packing preserves the full base.
    ///
    /// `base` MUST be a SEGMENT-aligned segment base with a valid header (the
    /// caller — `abandon_segments` — derives it from a registered segment
    /// table). The segment's `owner_state` SHOULD already be ABANDONED (the
    /// caller sets it before pushing); this push does not touch `owner_state`.
    pub fn push_abandoned_segment(base: *mut u8) {
        let reg = ensure();
        push_abandoned_segment_into(reg, base);
    }

    /// Pop the most-recently-abandoned segment base, or `None` if the stack
    /// is empty. Called by an adopter (12.4) on its cold path to reclaim an
    /// abandoned segment. The adopter then CAS-claims the segment's
    /// `owner_state` `ABANDONED → LIVE(me, gen+1)` (the M9 linearization
    /// point) — if that CAS fails (another adopter won, or the segment was
    /// already adopted), the adopter discards this base and pops the next.
    ///
    /// The pop is a Treiber pop: load the (tagged) head, read the segment's
    /// `next_abandoned` link, CAS the head to that next link. The ABA tag in
    /// the head defeats the pop-repush race (if another abandon pushed a new
    /// base between our load and CAS, the tag differs and we retry).
    #[must_use]
    pub fn pop_abandoned_segment() -> Option<*mut u8> {
        let reg = ensure();
        let mut head = reg.abandoned_segs.load(Ordering::Acquire);
        loop {
            if abandoned_head_is_empty(head) {
                return None;
            }
            let (base, tag) = unpack_abandoned_head(head);
            // Read the segment's `next_abandoned` link BEFORE the CAS (the
            // pusher stored it under Release; our Acquire load of the head +
            // this Acquire read see it). The link is the FULL 64-bit base of
            // the next abandoned segment (plain u64), or ABANDONED_TAIL.
            let meta = SegmentMeta::new(base);
            let next_link = meta.next_abandoned_atomic().load(Ordering::Acquire);
            // Compute the new head: the next base (full pointer) with the
            // SAME tag (a pop preserves the tag — only pushes bump it), or
            // empty if `next == ABANDONED_TAIL`.
            let new_head = if next_link == ABANDONED_TAIL {
                ABANDONED_HEAD_EMPTY
            } else {
                let next_base = next_link as *mut u8;
                // Preserve the tag (pop does not bump it). A concurrent
                // re-push of `base` will bump the tag and fail our CAS.
                pack_abandoned_head(next_base, tag)
            };
            // CAS the head to `new_head`. Acquire on success (see the push's
            // Release store of `next_abandoned`); Relaxed on failure (retry).
            match reg.abandoned_segs.compare_exchange(
                head,
                new_head,
                Ordering::Acquire,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Some(base),
                Err(actual) => head = actual, // retry with the new head
            }
        }
    }

    /// Phase 12.4 — the adoption cold path. Called by a heap on its free-list
    /// miss (before reserving a fresh segment): pop an abandoned segment and
    /// CAS-claim its `owner_state` `ABANDONED → LIVE(me, gen+1)`. If the CAS
    /// succeeds, the segment becomes part of `adopter`'s `AllocCore` (it is
    /// registered in the adopter's segment table and becomes the current
    /// small segment); its `ThreadFreeStack` (if any) is drained into its
    /// `BinTable`s. If the CAS fails (another adopter won this segment, or it
    /// was already adopted), the segment is discarded and the next abandoned
    /// segment is tried (the caller loops).
    ///
    /// **M9 (adopt-exactly-once):** the Abandoned→Live CAS on the segment's
    /// `owner_state` is the SINGLE linearization point — exactly one adopter
    /// wins per generation. The CAS expected value encodes the segment's
    /// pre-adoption `(ABANDONED, old_owner, gen)`; the winner writes
    /// `(LIVE, adopter, gen+1)`. A racing adopter that loaded the same
    /// expected value loses the CAS (the winner's store changed the word) and
    /// retries with a fresh pop.
    ///
    /// Returns `true` if a segment was adopted (the caller may retry the
    /// allocation that triggered the cold path); `false` if the abandoned
    /// stack was empty or no segment could be claimed (the caller should
    /// reserve a fresh segment).
    ///
    /// # Safety
    ///
    /// `adopter` must be a pointer previously returned by [`claim`](Self::claim)
    /// and the caller must be its sole writer (the owning thread).
    pub unsafe fn try_adopt(adopter: *mut HeapCore) -> bool {
        if adopter.is_null() {
            return false;
        }
        // Pop abandoned segments until we win one or the stack empties.
        loop {
            let Some(base) = Self::pop_abandoned_segment() else {
                return false; // stack empty — nothing to adopt
            };
            // CAS-claim the segment's owner_state ABANDONED → LIVE(me, gen+1).
            // This is the M9 linearization point.
            let meta = SegmentMeta::new(base);
            let owner_atomic = meta.owner_state_atomic();
            // Load the current state to build the expected word. We expect
            // ABANDONED (the abandon path set it). The owner_id and generation
            // are whatever the abandoner left — we match them exactly so only
            // a segment that has NOT been adopted since we popped it passes
            // the CAS.
            let cur = owner_atomic.load(Ordering::Acquire);
            if unpack_owner_state(cur) != OWNER_STATE_ABANDONED {
                // Not abandoned (another adopter already won it, or it was
                // never abandoned — defensive). Discard and pop the next.
                continue;
            }
            // SAFETY: `adopter` was returned by `claim` and we are its sole
            // writer (caller's contract). We hold `&mut HeapCore` for the
            // register/drain/set_small_current calls below.
            let adopter_ref: &mut HeapCore = unsafe { &mut *adopter };
            let new_gen = unpack_owner_gen(cur).wrapping_add(1);
            let new_word = pack_owner(
                crate::alloc_core::segment_header::OWNER_STATE_LIVE,
                adopter_ref.id(),
                new_gen,
            );
            // The CAS: AcqRel on success (Acquire to see the abandoner's
            // ABANDONED store + BinTable state; Release so a later freer's
            // Acquire read of owner_state sees our LIVE stamp). Relaxed on
            // failure (we discard this segment).
            match owner_atomic.compare_exchange(
                cur,
                new_word,
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // We won the segment. Register it in the adopter's
                    // AllocCore segment table (so alloc/dealloc routing finds
                    // it) and make it the current small segment (so new
                    // allocations carve from it). If the table is full
                    // (pathological — too many segments), we still keep the
                    // segment LIVE-owned but it will not be allocatable from
                    // directly; its BinTable frees still work via
                    // `dealloc_small_by_segment` (which uses segment_base_of,
                    // not the table, for routing).
                    let _ = adopter_ref.register_segment_internal(base);
                    adopter_ref.set_small_current_internal(base);
                    // Under alloc-xthread: drain the segment's TFS (if it has
                    // one) into its BinTable so cross-thread frees that
                    // arrived while it was abandoned are processed.
                    #[cfg(feature = "alloc-xthread")]
                    {
                        adopter_ref.drain_segment_tfs(base);
                    }
                    return true;
                }
                Err(_) => {
                    // Lost the CAS (another adopter won). Discard and pop the
                    // next abandoned segment.
                    continue;
                }
            }
        }
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

// ---------------------------------------------------------------------------
// Phase 12.4 — abandoned-segments intrusive Treiber stack primitives.
//
// The stack head (`Registry::abandoned_segs`) packs the full 64-bit segment
// base (high bits; the base is SEGMENT-aligned so its low 22 bits are zero)
// with an ABA tag in those low 22 bits. Each abandoned segment chains to the
// next via its `next_abandoned` header field (a segment-relative offset or
// `ABANDONED_TAIL`). This is the intrusive analogue of `ThreadFreeStack`,
// with the ABA tag added because abandoned segments CAN be re-abandoned
// (unlike TFS nodes, which are drained once).
// ---------------------------------------------------------------------------

/// Push `base` onto the abandoned-segments stack chained off `reg`'s head.
/// Sets the segment's `next_abandoned` link to the current head's base
/// (a full 64-bit absolute pointer, stored as `u64` plain data), then CASes
/// the head to `base` with a bumped tag. The full 64-bit base is preserved
/// (high bits in the head; full pointer in the link) — this is the FINDINGS
/// №1 fix.
///
/// `base` MUST be a SEGMENT-aligned segment base with a valid header.
fn push_abandoned_segment_into(reg: &Registry, base: *mut u8) {
    let meta = SegmentMeta::new(base);
    let next_atomic = meta.next_abandoned_atomic();
    let mut head = reg.abandoned_segs.load(Ordering::Acquire);
    loop {
        // The `next_abandoned` link: the current head's FULL base address
        // (stored as u64 plain data — a full 64-bit pointer, no truncation),
        // or ABANDONED_TAIL if the stack is empty. A real base is always
        // non-null and SEGMENT-aligned, so it never collides with ABANDONED_TAIL
        // (u64::MAX).
        let next_link = if abandoned_head_is_empty(head) {
            ABANDONED_TAIL
        } else {
            let (head_base, _tag) = unpack_abandoned_head(head);
            head_base as u64
        };
        // Write the link under Release so a concurrent pop's Acquire read of
        // `next_abandoned` (after observing this segment as the head) sees it.
        next_atomic.store(next_link, Ordering::Release);
        // Bump the tag (the ABA fix) and CAS the head to this base.
        let (_cur_base, tag) = unpack_abandoned_head(head);
        let new_tag = tag.wrapping_add(1) & (super::bootstrap::ABANDON_TAG_MASK);
        let new_head = pack_abandoned_head(base, new_tag);
        // CAS: Release on success so a pop's Acquire sees the `next_abandoned`
        // link we just wrote. Relaxed on failure (retry).
        match reg.abandoned_segs.compare_exchange(
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

/// Abandon a single segment: CAS its `owner_state` LIVE→ABANDONED (under the
/// owning thread's exclusive writer status this succeeds), then push its base
/// onto the abandoned-segments stack. Called by `abandon_segments` for each
/// owned segment.
///
/// `owner_id` is the abandoning heap's slot index (recorded in the segment's
/// new ABANDONED owner_state for the adopter's coherence check).
fn abandon_one_segment(reg: &Registry, base: *mut u8, owner_id: u32) {
    let meta = SegmentMeta::new(base);
    let owner_atomic = meta.owner_state_atomic();
    // CAS LIVE→ABANDONED. Under the owning thread's exclusive writer status
    // (the abandon caller is the heap's sole writer, established by the claim
    // CAS), this CAS trivially succeeds — it is a store under AcqRel in
    // practice. We use a CAS (not a plain store) so the adopter's
    // ABANDONED→LIVE CAS has a well-defined expected value, and so a
    // concurrent adopter that already won this segment (race with a prior
    // adoption) makes us skip it (the CAS fails on ABANDONED-or-other).
    //
    // We preserve the existing generation (the adopter bumps it). We expect
    // the current LIVE state with THIS owner_id (a segment we own); if the
    // owner_id is OWNER_ID_NONE (a never-stamped primordial/early segment),
    // we still abandon it (it has no owner to lose).
    loop {
        let cur = owner_atomic.load(Ordering::Acquire);
        let cur_state = unpack_owner_state(cur);
        if cur_state == OWNER_STATE_ABANDONED {
            // Already abandoned (a concurrent abandon raced and won, or a
            // prior adoption abandoned it again). Nothing to do.
            return;
        }
        let cur_owner = unpack_owner_id(cur);
        let cur_gen = unpack_owner_gen(cur);
        // Only abandon segments we own (cur_owner == owner_id) OR unbound
        // segments (OWNER_ID_NONE — stamped at reserve time as LIVE/NONE).
        // A segment owned by ANOTHER heap must not be touched here.
        if cur_owner != owner_id && cur_owner != OWNER_ID_NONE {
            // Not ours and not unbound: skip (defensive — abandon_segments
            // walks THIS heap's table, so every base should be ours).
            return;
        }
        let new_word = pack_owner(OWNER_STATE_ABANDONED, owner_id, cur_gen);
        match owner_atomic.compare_exchange(
            cur,
            new_word,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ) {
            Ok(_) => break,
            Err(_) => continue, // retry the load+CAS
        }
    }
    // Push the base onto the global abandoned-segments stack.
    push_abandoned_segment_into(reg, base);
}
