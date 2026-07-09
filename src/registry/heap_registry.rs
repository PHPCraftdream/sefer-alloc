//! [`HeapRegistry`] — the global self-hosting heap slot table (§2.1 of
//! `ALLOC_PLAN_PHASE12-13.md`): claim/recycle/abandon over the process-global
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
//! carry a monotonic tag in the high bits of their `AtomicU64` head (48 bits
//! for `free_slots` — the low 16 hold the slot index, task W7a; the
//! abandoned-segs tag lives in the segment-alignment low bits of the base),
//! bumped on every push. This defeats the classic ABA (pop-X, re-push-X
//! while a racer is parked with head=X): the re-push bumps the tag, so the
//! racer's CAS on `(X, old_tag)` fails. See `super::tagged_ptr::TaggedPtr`
//! for the tag-width-vs-churn analysis.

// The crate is `#![deny(unsafe_code)]` with `alloc-global` on (see
// `src/lib.rs`); this is the documented registry seam (the pointer handoff
// `*mut HeapCore` out of a slot's `UnsafeCell`, plus `get_unchecked` on the
// `'static` slot array under range-checked indices). `allow` lifts the
// crate-level `deny` for this file only — `unsafe` anywhere else in the crate
// is a hard error. Every `unsafe` block carries a `// SAFETY:` proof.
#![allow(unsafe_code)]

use core::sync::atomic::Ordering;

use super::bootstrap::{
    abandoned_head_is_empty, ensure, pack_abandoned_head, unpack_abandoned_head, Registry,
    ABANDONED_HEAD_EMPTY, MAX_HEAPS,
};
use super::heap_core::HeapCore;
use super::heap_slot::{HeapSlot, NEXT_FREE_TAIL, STATE_FREE, STATE_LIVE};
use super::tagged_ptr::TaggedPtr;
use crate::alloc_core::segment_header::{
    pack_owner, unpack_owner_gen, unpack_owner_id, unpack_owner_state, SegmentMeta, ABANDONED_TAIL,
    OWNER_ID_NONE, OWNER_STATE_ABANDONED,
};

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
        let idx = match Self::pick_slot() {
            Some(i) => i,
            None => return core::ptr::null_mut(),
        };
        let reg = ensure();
        // SAFETY: `idx < MAX_HEAPS` by `pick_slot`.
        let slot = unsafe { reg.slots.get_unchecked(idx) };

        if slot.cas_state(STATE_FREE, STATE_LIVE, Ordering::AcqRel, Ordering::Acquire)
            == Err(STATE_LIVE)
        {
            return Self::claim(); // lost the slot race — retry
        }
        let new_gen = slot.generation.fetch_add(1, Ordering::Release) + 1;
        if new_gen == 1 {
            let heap_ptr = slot.heap.get();
            match HeapCore::new(idx as u32) {
                // SAFETY: sole writer, uninitialised slot, first claim.
                Some(hc) => unsafe { heap_ptr.cast::<HeapCore>().write(hc) },
                None => {
                    let _ = slot.cas_state(
                        STATE_LIVE,
                        STATE_FREE,
                        Ordering::Release,
                        Ordering::Relaxed,
                    );
                    return core::ptr::null_mut();
                }
            }
            // W3: plant this heap's stable handles to its slot-resident
            // diagnostic hit counters, now that the `HeapCore` is materialised
            // in the slot. See `bind_slot_counters`.
            // SAFETY: we just `write`(hc) into this slot's `UnsafeCell` and are
            // its sole writer (the FREE→LIVE CAS winner); no other thread holds
            // a reference to it yet (`initialised` not yet published).
            unsafe { bind_slot_counters(slot, heap_ptr.cast::<HeapCore>()) };
            // Publish readiness: Release-store `initialised = true` ONLY
            // now that `heap_ptr.write(hc)` has fully completed (task #133
            // hardening — see `HeapSlot::initialised`'s doc comment for the
            // UB window this closes: `count`/`generation` alone are bumped
            // BEFORE `HeapCore::new()` runs and are NOT safe gates for a
            // cross-thread reader to dereference `heap`). This Release
            // store is the publish half of the HB pair; diagnostic
            // aggregation readers (`tcache_hits_total`,
            // `large_cache_hits_total`) pair it with an Acquire load.
            slot.initialised.store(true, Ordering::Release);
        }
        // SAFETY: slot is LIVE and initialised; we are sole writer.
        slot.heap.get().cast::<HeapCore>()
    }

    /// Like [`claim`](Self::claim) but plumbs `config` into the newly
    /// materialised `HeapCore` (first claim only — generation == 1). On
    /// re-claim the existing `HeapCore` is reused as-is; its large-cache
    /// config was set at first claim and persists.
    ///
    /// Only present under `alloc-decommit`.
    #[cfg(feature = "alloc-decommit")]
    #[must_use]
    pub fn claim_with_config(config: crate::alloc_core::LargeCacheConfig) -> *mut HeapCore {
        let idx = match Self::pick_slot() {
            Some(i) => i,
            None => return core::ptr::null_mut(),
        };
        let reg = ensure();
        // SAFETY: `idx < MAX_HEAPS` by `pick_slot`.
        let slot = unsafe { reg.slots.get_unchecked(idx) };

        if slot.cas_state(STATE_FREE, STATE_LIVE, Ordering::AcqRel, Ordering::Acquire)
            == Err(STATE_LIVE)
        {
            return Self::claim_with_config(config); // lost the slot race — retry
        }
        let new_gen = slot.generation.fetch_add(1, Ordering::Release) + 1;
        if new_gen == 1 {
            let heap_ptr = slot.heap.get();
            // First claim: materialise using the caller's config.
            match HeapCore::new_with_config(idx as u32, config) {
                // SAFETY: sole writer, uninitialised slot, first claim.
                Some(hc) => unsafe { heap_ptr.cast::<HeapCore>().write(hc) },
                None => {
                    let _ = slot.cas_state(
                        STATE_LIVE,
                        STATE_FREE,
                        Ordering::Release,
                        Ordering::Relaxed,
                    );
                    return core::ptr::null_mut();
                }
            }
            // W3: plant slot-counter handles — see `claim` above and
            // `bind_slot_counters`.
            // SAFETY: identical to `claim` — sole writer, just materialised,
            // not yet published.
            unsafe { bind_slot_counters(slot, heap_ptr.cast::<HeapCore>()) };
            // Publish readiness — see the identical store in `claim` above
            // for the full rationale (task #133 hardening).
            slot.initialised.store(true, Ordering::Release);
        }
        // SAFETY: slot is LIVE and initialised; we are sole writer.
        slot.heap.get().cast::<HeapCore>()
    }

    /// Pick a candidate slot index: pop from `free_slots` (recycled slot)
    /// or mint a fresh one by bumping `count`. Returns `None` on registry
    /// exhaustion (`count >= MAX_HEAPS` AND free stack empty).
    fn pick_slot() -> Option<usize> {
        let reg = ensure();
        pop_free_slot(reg).or_else(|| bump_count(reg))
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
    /// **Phase 12.5 (shard model — FINDINGS №7 dissolved, not patched):**
    /// `abandon_segments` is NOT called on the hot path. The `AbandonGuard`
    /// releases the slot ONLY (`recycle`), leaving the `HeapCore` whole — its
    /// segments and inline TFS stay with the slot, and the next thread to claim
    /// the slot reuses the same heap in full (whole-heap reuse, the shard
    /// discipline). Because the heap is never fragmented, no segment is ever
    /// co-owned, so the §7 double-ownership cannot arise — the landmine is
    /// dissolved by the architecture, not patched by clearing the table. This
    /// method (and the `abandoned_segs`/`owner_state` substrate it drives)
    /// is retained, loom-proven, as the basis for a FUTURE decommit-when-empty
    /// policy; when that is wired it must coordinate ownership with the new
    /// policy, but it does NOT clear the table here.
    ///
    /// # ⚠️ REACTIVATION HAZARD (task A1, 0.3.0) — read before wiring this up
    ///
    /// `HeapCore::thread_free` (the per-heap cross-thread deferred-free stack
    /// added in task A1 for Large-segment reclaim — see its field doc in
    /// `heap_core.rs`) reuses each segment's `next_abandoned` header field as
    /// its OWN intrusive link — the exact same field this global stack's
    /// [`push_abandoned_segment_into`]/[`pop_abandoned_segment`] use.
    ///
    /// Today this is safe ONLY because `abandon_segments` is unreachable from
    /// any production path (Phase 12.5 replaced thread-exit abandonment with
    /// "release the slot only" — see the paragraph above). If a FUTURE
    /// decommit-when-empty policy reactivates this walk, it MUST NOT walk a
    /// `SegmentKind::Large` segment that could be concurrently linked into a
    /// heap's local A1 deferred-free stack — doing so clobbers
    /// `next_abandoned`, corrupting whichever stack loses the race:
    /// - if the segment is mid-flight on the LOCAL (per-heap) stack when this
    ///   global walk overwrites its link, the local stack's chain past that
    ///   segment becomes unreachable (silent leak of everything behind it),
    ///   and a later local `pop` may read a link that actually points into
    ///   the GLOBAL stack's chain — a wild/foreign pointer read, not just a
    ///   leak.
    ///
    /// Before reactivating: either (a) skip `SegmentKind::Large` segments in
    /// this walk entirely (large segments are already reclaimed via the A1
    /// path, which — unlike this global stack — does not require the
    /// owning heap to be dying), or (b) give each stack its own dedicated
    /// link field instead of sharing `next_abandoned`. Do not skip this
    /// check — there is no test that would catch the corruption (both stacks
    /// are exercised in isolation today; nothing exercises them
    /// concurrently on the same segment, because this walk is currently
    /// dead code on every reachable path).
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
        // Phase 12.5 (shard model): we do NOT clear the heap's table here.
        // `abandon_segments` is retained as a loom-proven substrate primitive
        // (for a future decommit-when-empty policy) but is NOT on the hot
        // path — the `AbandonGuard` releases the slot only, leaving the
        // HeapCore whole. When/if this primitive is wired for decommit, the
        // caller must coordinate table-clearing with the new policy; for now
        // the segments stay referenced by their owning heap's table (which is
        // correct: they ARE still owned by this heap until it is dropped).
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
                // EXPOSED-PROVENANCE LOAD SITE: `next_link` is the FULL 64-bit base of
                // the next abandoned segment, stored as plain `u64` data by
                // `push_abandoned_segment_into` below (which calls
                // `expose_provenance` on the real segment pointer before
                // writing it into `next_abandoned` — see that function). This
                // reconstructs a dereferenceable pointer under the exposed
                // model; `pack_abandoned_head` immediately re-exposes it when
                // repacking into the head word (a redundant but harmless
                // re-expose of an already-exposed address).
                let next_base = core::ptr::with_exposed_provenance_mut::<u8>(next_link as usize);
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
            match owner_atomic.compare_exchange(cur, new_word, Ordering::AcqRel, Ordering::Relaxed)
            {
                Ok(_) => {
                    // We won the segment. Register it in the adopter's
                    // AllocCore segment table (so alloc/dealloc routing finds
                    // it) and make it the current small segment (so new
                    // allocations carve from it). If the table is full
                    // (pathological — too many segments), we still keep the
                    // segment LIVE-owned but it will not be allocatable from
                    // directly; its BinTable frees still work via
                    // `segment_base_of` routing in `AllocCore::dealloc`.
                    let _ = adopter_ref.register_segment_internal(base);
                    adopter_ref.set_small_current_internal(base);
                    // Under alloc-xthread: cross-thread frees that arrived while
                    // the segment was abandoned sit in its `RemoteFreeRing`; the
                    // adopter reclaims them LAZILY on its next alloc miss
                    // (`AllocCore::find_segment_with_free` drains every owned
                    // segment's ring via `reclaim_offset`). No eager TFS drain —
                    // the intrusive TFS is gone (the ring carries the size class,
                    // which the owner's `page_map` cannot reliably supply, §13).
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

/// W3: plant a freshly-materialised heap's stable handles to its OWNING
/// slot's diagnostic hit counters (`HeapSlot::tcache_hits` /
/// `HeapSlot::large_cache_hits`). Called by `claim` / `claim_with_config`
/// exactly once, at the slot's first claim (`new_gen == 1`), AFTER
/// `heap_ptr.write(hc)` and BEFORE the `initialised` Release publish.
///
/// This is the keystone of the W3 aliasing fix: the owner increments its hit
/// counters through these `&'static` handles into the SLOT (which is `Sync`,
/// designed to be shared), so the process-wide aggregators
/// (`tcache_hits_total` / `large_cache_hits_total`) can read the SAME
/// `AtomicU64`s directly off the `&HeapSlot` they already hold — WITHOUT ever
/// materialising a shared `&HeapCore`/`&AllocCore` over a struct the owner
/// concurrently holds a protected `&mut` into. The slot lives in the `'static`
/// registry array, so `&slot.<counter>` is a sound `&'static` for the process
/// lifetime.
///
/// # Safety
///
/// `heap` must point at the `HeapCore` just written into `slot`'s `UnsafeCell`
/// by the caller (the FREE→LIVE CAS winner, sole writer); no other thread may
/// hold a reference to it yet (the caller has not published `initialised`). We
/// form a single `&mut HeapCore` for the duration of the bind calls only.
#[cfg_attr(
    not(any(
        all(feature = "alloc-global", feature = "fastbin"),
        feature = "alloc-decommit",
        feature = "alloc-xthread"
    )),
    allow(unused_variables)
)]
unsafe fn bind_slot_counters(slot: &'static HeapSlot, heap: *mut HeapCore) {
    // SAFETY: caller's contract — `heap` is the just-written, sole-writer,
    // not-yet-published `HeapCore` in `slot`. This exclusive `&mut` is the only
    // live reference to it.
    let heap_ref: &mut HeapCore = unsafe { &mut *heap };
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    heap_ref.bind_tcache_hits(&slot.tcache_hits);
    #[cfg(feature = "alloc-decommit")]
    heap_ref.bind_large_cache_hits(&slot.large_cache_hits);
    // task H1: plant the stable `&'static` handle to this slot's cross-thread
    // free-stack head (moved out of `HeapCore` into the `Sync` slot — see
    // `HeapSlot::thread_free` / `HeapCore::thread_free`). This is what makes the
    // remote CAS target the slot word (outside every `&mut HeapCore` retag
    // range) instead of an inline `HeapCore` field.
    #[cfg(feature = "alloc-xthread")]
    heap_ref.bind_thread_free(&slot.thread_free);
}

// ---------------------------------------------------------------------------
// Treiber-stack primitives on the `free_slots` stack. Module-private.
// ---------------------------------------------------------------------------

/// Pop a free slot index off the `free_slots` stack (classic Treiber pop),
/// or `None` if empty.
///
/// This is the textbook Treiber pop: load the (tagged) head, read its
/// `next_free` link, then CAS the head to that next link. The tag in the high
/// 48 bits defeats the ABA problem: if between our load and our CAS another
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
        match reg
            .free_slots
            .compare_exchange(head, new_head, Ordering::Acquire, Ordering::Relaxed)
        {
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
        match reg
            .free_slots
            .compare_exchange(head, new_head, Ordering::Release, Ordering::Relaxed)
        {
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
            // EXPOSED-PROVENANCE STORE SITE: `head_base` was itself
            // reconstructed by `unpack_abandoned_head` via
            // `with_exposed_provenance_mut` (so it already carries exposed
            // provenance from an earlier `expose_provenance` call), but we
            // re-expose it here because it is about to be written into
            // `next_abandoned` as a NEW plain-`u64` link — the paired load
            // site is the `next_link as *mut u8`... reconstruction in
            // `HeapRegistry::pop_abandoned_segment` above. `expose_provenance`
            // on an already-exposed address is a harmless no-op re-registration,
            // not a correctness issue.
            head_base.expose_provenance() as u64
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
        match owner_atomic.compare_exchange(cur, new_word, Ordering::AcqRel, Ordering::Relaxed) {
            Ok(_) => break,
            Err(_) => continue, // retry the load+CAS
        }
    }
    // Push the base onto the global abandoned-segments stack.
    push_abandoned_segment_into(reg, base);
    // Phase 12.5 (shard model): we do NOT clear `owner_thread_free` here.
    // The abandon/adopt substrate is retained for a future decommit-when-empty
    // policy, but on the shard-model hot path abandon is NOT called (the
    // AbandonGuard releases the slot only; the HeapCore stays whole with its
    // segments + inline TFS). The `owner_thread_free` stamp is set ONCE (on
    // the segment's first alloc) and points at the slot's inline TFS, whose
    // address is stable for the process lifetime — so it never needs clearing
    // or re-stamping across release→claim.
}

/// DIAGNOSTIC (task E1): the high-water mark of minted registry slots — the
/// number of distinct heap slots ever claimed (via `bump_count`) since
/// process start. This is a **high-water mark, not a live count**: a slot
/// that was claimed and later recycled is still counted here (recycled slots
/// are reused, not un-minted). Relaxed load of [`Registry::count`].
/// `#[doc(hidden)]` — diagnostic-only surface, not part of the crate's
/// supported public API; reached via `SeferAlloc::stats()`.
#[doc(hidden)]
#[must_use]
pub fn heaps_claimed_high_water() -> u32 {
    ensure().count.load(Ordering::Relaxed)
}

/// DIAGNOSTIC (task #133 → W3): process-wide magazine (tcache) hit total —
/// aggregated across every slot ever minted, summing each slot's own
/// [`HeapSlot::tcache_hits`] (moved there from `HeapCore` in W3 to close a
/// Stacked-Borrows aliasing gap — the aggregator no longer materialises any
/// `&HeapCore`). Replaces the pre-#133 single global `static`
/// counter (`DBG_TCACHE_HITS`), which was bumped by every thread's alloc
/// fast path and therefore a contended `lock xadd` on an otherwise
/// per-thread hot path (the regression this function's introduction fixes —
/// see the doc comment on [`HeapCore`]'s `tcache_hits` field).
///
/// ## Soundness of reading a foreign slot's counter
///
/// This walks slot indices `0..count` (the high-water mark of minted
/// slots — [`heaps_claimed_high_water`]) and, for each, performs a Relaxed
/// load of that slot's `HeapCore::tcache_hits` — but ONLY after first
/// checking [`HeapSlot::initialised`] with an `Acquire` load.
///
/// **This gate is load-bearing, not defensive.** `count` (bumped by
/// `bump_count`, called from `pick_slot` BEFORE the slot's `FREE → LIVE`
/// CAS) and `generation` (bumped to 1 by `claim` BEFORE `HeapCore::new()`
/// runs, which reserves an OS segment — not fast) are BOTH insufficient:
/// a slot index can be `< count` — and even have `generation == 1` — while
/// `HeapCore::new()` is still executing on the claiming thread and
/// `heap_ptr.write(hc)` has not yet run. `heap`'s storage is still
/// `MaybeUninit::uninit()` bytes at that point. Reading it from THIS
/// function (a different thread, e.g. via `SeferAlloc::stats()` called
/// concurrently with another thread's first-ever `claim`) would be a read
/// of uninitialised memory racing a concurrent non-atomic
/// `MaybeUninit::write` — undefined behaviour, not merely a stale value.
/// (This was a real defect caught in zero-trust review of the initial
/// #133 patch — see `HeapSlot::initialised`'s doc comment for the full
/// writeup, and `tests/regression_registry_initialised_gate.rs` for the
/// regression coverage.)
///
/// The fix: [`HeapRegistry::claim`] (and `claim_with_config`) Release-store
/// `true` into `HeapSlot::initialised` ONLY after `heap_ptr.write(hc)` has
/// fully completed. This function's Acquire load of `initialised`, when it
/// observes `true`, is guaranteed by the C++/Rust memory model to
/// happens-after that Release store — which is itself sequenced-after the
/// `write(hc)` on the same (claiming) thread — so observing `true` here
/// establishes happens-before from the write of `hc` into the
/// `UnsafeCell` to this function's subsequent dereference of `heap_ptr`.
/// That is the standard "publish a fully-constructed value via a
/// Release-store flag" pattern, and it is what makes the dereference below
/// sound. A slot observed with `initialised == false` is skipped entirely:
/// it has never been claimed (or is mid-claim), so it has never
/// incremented `tcache_hits` either — contributing 0 to the sum is correct,
/// not merely safe.
///
/// Once `initialised` is `true` it stays `true` for the process lifetime of
/// the slot (per the slot-reuse discipline documented on [`HeapSlot::heap`]
/// — a minted `HeapCore` is reused as-is across `recycle`/re-`claim`
/// cycles, never dropped or reset), so a slot that was `true` on a prior
/// observation can only still be `true` (or `true` with a newer,
/// larger-or-equal counter value) on a later one — no ABA hazard on this
/// flag itself.
///
/// No `unsafe` beyond what this module's header comment already documents
/// as its seam: `MaybeUninit::assume_init_ref` would be new `unsafe`, but
/// we avoid it entirely by going through the same raw-pointer path `claim`
/// already uses (`heap.get().cast::<HeapCore>()`).
///
/// Only present under `alloc-global + fastbin` (mirrors
/// `HeapCore::tcache_hits`'s cfg-gate).
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
#[doc(hidden)]
#[must_use]
pub fn tcache_hits_total() -> u64 {
    let reg = ensure();
    let count = reg.count.load(Ordering::Acquire) as usize;
    let mut total: u64 = 0;
    for idx in 0..count.min(MAX_HEAPS) {
        // SAFETY: `idx < count <= MAX_HEAPS`, so this is in range of the
        // `'static` slot array.
        let slot = unsafe { reg.slots.get_unchecked(idx) };
        // The `initialised` gate (task #133): keep it for the documented
        // ordering. With the W3 move it is no longer load-bearing for SAFETY
        // (the counter lives in the slot itself — an un-bound slot's
        // `tcache_hits` is a zero `AtomicU64`, sound to read and contributing
        // 0), but a mid-mint slot must still not be summed, and the Acquire
        // here pairs with `claim`'s Release publish for that ordering.
        if !slot.initialised.load(Ordering::Acquire) {
            continue;
        }
        // W3: read the counter DIRECTLY off the `&HeapSlot` — NO
        // `(*heap_ptr).…` deref, so NO shared `&HeapCore` is ever materialised
        // over a struct the owning thread concurrently holds a protected `&mut`
        // into. This closes the Stacked-Borrows aliasing gap the old
        // `(*heap_ptr).tcache_hits()` read had. Relaxed load of a shared
        // `Sync` atomic — sound from any thread; observes the owner's
        // monotonic single-writer increments.
        total = total.saturating_add(slot.tcache_hits.load(Ordering::Relaxed));
    }
    total
}

/// DIAGNOSTIC (task #133): process-wide large-cache hit total — aggregated
/// across every slot ever minted, summing each slot's
/// `AllocCore::dbg_large_cache_hits()`. Replaces the pre-#133 single global
/// `static` counter (`LARGE_CACHE_HITS` in `alloc_core.rs`), which was
/// bumped by every heap's `alloc_large` cache-hit path and therefore a
/// contended `lock xadd` on an otherwise per-heap hot path.
///
/// Soundness of walking foreign slots and reading their `AllocCore` field
/// cross-thread: identical argument to [`tcache_hits_total`] above,
/// including the SAME load-bearing gate — `idx < count` alone does NOT
/// imply the slot's `HeapCore` is materialised (see that function's doc
/// comment for the exact UB window this closes: `count` is bumped by
/// `bump_count` before the claiming thread even starts `HeapCore::new()`).
/// This function gates every slot on an `Acquire` load of
/// [`HeapSlot::initialised`] before dereferencing `heap`, pairing with the
/// `Release` store `claim`/`claim_with_config` perform immediately after
/// `heap_ptr.write(hc)` completes — establishing happens-before to the
/// write. A slot observed `initialised == false` is skipped (never
/// claimed, or mid-claim — either way it has never incremented
/// `large_cache_hits`, so contributing 0 is correct).
///
/// Only present under `alloc-decommit` (mirrors
/// `AllocCore::dbg_large_cache_hits`'s cfg-gate).
#[cfg(feature = "alloc-decommit")]
#[doc(hidden)]
#[must_use]
pub fn large_cache_hits_total() -> u64 {
    let reg = ensure();
    let count = reg.count.load(Ordering::Acquire) as usize;
    let mut total: u64 = 0;
    for idx in 0..count.min(MAX_HEAPS) {
        // SAFETY: `idx < count <= MAX_HEAPS` is in range of the `'static`
        // slot array.
        let slot = unsafe { reg.slots.get_unchecked(idx) };
        // The `initialised` gate — see `tcache_hits_total`'s (identical
        // rationale): kept for the documented ordering, no longer load-bearing
        // for safety after the W3 move (the counter is in the slot itself).
        if !slot.initialised.load(Ordering::Acquire) {
            continue;
        }
        // W3: read the counter DIRECTLY off the `&HeapSlot` — NO
        // `(*heap_ptr).core.…` deref, so NO shared `&HeapCore`/`&AllocCore` is
        // ever materialised over a struct the owning thread concurrently holds
        // a protected `&mut` into. This closes the Stacked-Borrows aliasing gap
        // the old `(*heap_ptr).core.dbg_large_cache_hits()` read had. Relaxed
        // load of a shared `Sync` atomic — sound from any thread.
        total = total.saturating_add(slot.large_cache_hits.load(Ordering::Relaxed));
    }
    total
}
