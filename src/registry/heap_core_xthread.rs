//! Cross-thread free routing for [`HeapCore`] (mechanical split of
//! `heap_core.rs`, task R4-10).
//!
//! This file holds the `impl HeapCore { .. }` block for the cross-thread
//! deferred-free drain and the foreign-thread dealloc routing protocol.
//! All methods are `#[cfg(feature = "alloc-xthread")]`.
//! Pure code-movement sibling of `heap_core.rs`; no behavior changed.

use core::alloc::Layout;
use core::sync::atomic::AtomicPtr;
use core::sync::atomic::Ordering;

use crate::alloc_core::os;
use crate::alloc_core::segment_header::SegmentMeta;
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind, SEGMENT_MAGIC};
use crate::alloc_core::{node::Node, AllocCore};

use super::heap_core::HeapCore;
// Only used by the `#[cfg(feature = "alloc-xthread")]` push_with_overflow_retry
// method below — the items themselves are gated the same way in `heap_core.rs`,
// so this import must match or `alloc-global`-without-`alloc-xthread` fails
// with E0432 (no such item exists to import under that feature set).
#[cfg(feature = "alloc-xthread")]
use super::heap_core::{
    DBG_RING_PUSH_RETRIED, DBG_RING_PUSH_RETRY_EXHAUSTED, RING_PUSH_RETRY_SPINS,
};

impl HeapCore {
    /// 0.3.0 (task A1); extracted for #132: push a Large/huge segment `base`
    /// onto the OWNING heap's deferred-free stack, given `head` — the
    /// owner's `thread_free_head()` (a `*const AtomicPtr<u8>`, obtained by a
    /// REMOTE freer from `owner_thread_free_at(segment_base)`). Called from
    /// [`dealloc_routing`](Self::dealloc_routing) in place of the old
    /// permanent-leak no-op.
    ///
    /// Thin delegation to the shared
    /// [`alloc_core::deferred_large::push_large_deferred_free`] primitive
    /// (byte-for-byte the same push/CAS/double-push-guard logic this method
    /// used to inline — see that function's doc comment for the full
    /// mechanism and the double-push-guard hardening rationale). The
    /// primitive takes `&AtomicPtr<u8>` directly, so the pointer-to-reference
    /// deref of `head` stays HERE (via the `alloc_core::node` seam, same
    /// discipline as `next_abandoned_atomic`/`owner_state_atomic`) rather
    /// than inside the shared (seam-free) module.
    #[cfg(feature = "alloc-xthread")]
    fn push_large_deferred_free(head: *const AtomicPtr<u8>, base: *mut u8) {
        // `heap_core.rs` is NOT an allowed `unsafe` seam (see `src/lib.rs`'s
        // seam whitelist), so the pointer-to-reference deref is delegated to
        // `Node::atomic_ptr_ref` (the `alloc_core::node` seam), same
        // discipline as `next_abandoned_atomic`/`owner_state_atomic`.
        let head_ref: &AtomicPtr<u8> = Node::atomic_ptr_ref(head);
        crate::alloc_core::deferred_large::push_large_deferred_free(head_ref, base);
    }

    /// 0.3.0 (task A1); extracted for #132: drain this heap's deferred-free
    /// stack, reclaiming every queued Large/huge segment base via
    /// [`AllocCore::reclaim_large_segment`]. Called by the OWNER on its own
    /// `alloc_large` slow path, before reserving a fresh segment, so a
    /// cross-thread-freed large segment becomes available for reuse (via the
    /// `alloc-decommit` large-cache) or is released to the OS immediately
    /// (without `alloc-decommit`) — either way its `SegmentTable` slot is
    /// freed for reuse (the fix for the A1 permanent-leak bug).
    ///
    /// Thin delegation to the shared
    /// [`alloc_core::deferred_large::drain_large_deferred_free`] primitive
    /// (byte-for-byte the same pop-loop/reclaim logic this method used to
    /// inline).
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn drain_large_deferred_free(&mut self) {
        // task H1: drain through the `&'static` slot handle, NOT an inline
        // field. `None` only in the pre-bind window — nothing could have been
        // pushed yet then (a remote push needs the stamp, which needs the
        // handle), so an empty-stack no-op is correct. Resolving the handle
        // BEFORE forming the `&mut self.core` split-borrow keeps the head
        // reference (a `&'static` into the slot, outside this `&mut HeapCore`)
        // disjoint from the core borrow.
        if let Some(head) = self.thread_free {
            crate::alloc_core::deferred_large::drain_large_deferred_free(head, &mut self.core);
        }
    }

    /// RAD-4b (task #72): drain THIS heap's slot-resident
    /// [`HeapOverflow`](super::heap_overflow::HeapOverflow) ring — the
    /// second-chance queue [`push_to_heap_overflow`](Self::push_to_heap_overflow)
    /// falls back to once a segment's own `RemoteFreeRing` AND its bounded
    /// retry are both exhausted (see that method's doc comment for the full
    /// design). Called by the OWNER on the SAME opportunistic schedule the
    /// per-segment rings are already drained (every magazine-miss slow path
    /// — see [`refill_magazine_slow`](Self::refill_magazine_slow) — and every
    /// `find_segment_with_free` scan), so overflow entries are reclaimed with
    /// the same liveness assumption every lazy-drain path in this allocator
    /// already relies on ("the owner drains on its own next `alloc()`").
    ///
    /// Each entry's `(base, packed)` pair is reclaimed via
    /// `AllocCore::reclaim_offset` (or, under `fastbin`, the
    /// magazine-checked `reclaim_offset_checked` — mirrors
    /// `dbg_drain_all_rings_impl`'s identical dual-path split) — the SAME
    /// defensively-guarded reclaim primitive the per-segment ring drain
    /// already uses, so a stale/garbled `base` (e.g. a segment recycled
    /// between push and drain) is rejected by its own magic/kind/bounds
    /// checks exactly as it would be for a per-segment ring entry, not
    /// specially trusted here.
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn drain_heap_overflow(&mut self) {
        // RAD-4b: resolve through the pre-planted `&'static` handle (planted
        // by `bind_overflow` at claim time), NOT a fresh `bootstrap::ensure()`
        // + array index on every call — see the `overflow` field's doc
        // comment for the churn-gate cost this hoist recovers. `None` only in
        // the transient pre-bind window (never observed on any alloc/free
        // path — this drain runs only after a claimed heap's `alloc()`).
        let Some(overflow) = self.overflow else {
            return;
        };
        // RAD-4b iai churn-gate discipline: skip the full drain protocol
        // entirely on the overwhelmingly common "nothing has ever overflowed
        // into this ring" case — a single `Relaxed` load compared against our
        // own cached `tail`, mirroring the `last_stamped_segment` OPT-C cache
        // and `RemoteFreeRing`'s own documented `is_likely_empty` idiom. See
        // `HeapOverflow::is_likely_empty`'s doc comment for the full
        // soundness argument.
        if overflow.is_likely_empty(self.overflow_tail_cache) {
            return;
        }
        let small_cur = self.core.small_cur();
        #[cfg(feature = "fastbin")]
        {
            // No "class `c` currently being refilled" context exists at this
            // call site (unlike `refill_class_bump_checked`'s closure in
            // `refill_magazine_slow`, which special-cases `k == c` because
            // `count[c] == 0` is a load-bearing invariant for THAT specific
            // refill) — this drain reclaims entries of ANY class, so the
            // predicate unconditionally checks the magazine-residency bitmap,
            // mirroring `dbg_drain_all_rings_impl`'s general-purpose pattern.
            self.overflow_tail_cache = overflow.drain(|base, packed| {
                let _ = AllocCore::reclaim_offset_checked(base, packed, small_cur, &|ptr, _k| {
                    let pbase = os::segment_base_of_ptr(ptr);
                    let poff = (ptr as usize - pbase as usize) as u32;
                    SegmentMeta::new(pbase)
                        .magazine_bitmap()
                        .is_in_magazine(poff)
                });
            });
        }
        #[cfg(not(feature = "fastbin"))]
        {
            self.overflow_tail_cache = overflow.drain(|base, packed| {
                let _ = AllocCore::reclaim_offset(base, packed, small_cur);
            });
        }
    }

    // -----------------------------------------------------------------------
    // Cross-thread free routing (only under `alloc-xthread`).
    //
    // This re-bases the Phase 10 `Heap::dealloc_small` /
    // `Heap::dealloc_any_thread` discipline on the registry-resident
    // `HeapCore`. The block at `ptr` may belong to:
    //   - a segment THIS heap owns (stamped with our head, or unstamped) →
    //     own-thread path via `AllocCore::dealloc`;
    //   - a segment owned by ANOTHER heap (stamped with its head) → push
    //     onto that heap's TFS via `ThreadFreeStack::push`;
    //   - a foreign (non-sefer) pointer → safe no-op.
    // -----------------------------------------------------------------------

    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(super) fn dealloc_routing(&mut self, ptr: *mut u8, layout: Layout) {
        let base = os::segment_base_of_ptr(ptr);

        // Task #135 (Part 3, M2 hardening): check `self.core.contains_base(base)`
        // FIRST, before touching any segment memory. `contains_base` is an O(1)
        // lookup in OUR OWN `SegmentTable`'s open-addressing hash — it reads
        // only our own primordial-segment-resident table, never `base`'s
        // memory, so it is safe to call even if `base` is unmapped (a
        // released/decommitted segment).
        //
        // `contains_base(base) == true` if and only if `base` is currently
        // registered in OUR table — which happens exactly when we own a live
        // (mapped) segment there (`register_segment`/`alloc_large*` register
        // on creation; `unregister`/`recycle` remove on release — see
        // `segment_table.rs`). So TRUE implies "our segment, definitely
        // mapped" — equivalent to the old `owner_tf.is_null() || owner_tf ==
        // our_head` condition for every segment WE registered (an unstamped
        // own-segment has `owner_tf == null`; a stamped own-segment has
        // `owner_tf == our_head` — both cases are covered by "it's in our
        // table"), without reading `base`'s memory at all. Route it own-thread
        // immediately — no magic/kind read needed.
        if self.core.contains_base(base) {
            // Э9 (P7.1): `base` is already in hand from the `contains_base`
            // ownership check above; under fastbin, hand it to the own-thread
            // body directly so `segment_base_of_ptr` is not recomputed. Under
            // non-fastbin `dealloc_own_thread` just delegates to `core.dealloc`
            // (base unused there).
            #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
            self.dealloc_own_thread_with_base(ptr, layout, base);
            #[cfg(not(all(feature = "alloc-global", feature = "fastbin")))]
            self.dealloc_own_thread(ptr, layout);
            return;
        }
        // `contains_base` is FALSE: not one of our segments. The entire cold
        // cross-thread tail (magic/kind checks, Large deferred-push, ring
        // push) is outlined below — see `dealloc_foreign_slow`'s doc comment
        // (PERF-PASS-2, G10/D2, task #50).
        self.dealloc_foreign_slow(ptr, base, layout);
    }

    /// PERF-PASS-2 (G10/D2, task #50): outlined cold cross-thread dealloc
    /// tail, split out of `dealloc_routing` (which is `#[inline(always)]` and
    /// sits directly behind the hot own-thread `contains_base` hit-check on
    /// EVERY free). Before this split, the entire body below — magic/kind
    /// header reads, the Large-segment deferred-free push, and the small-
    /// block ring push, each with hardened/non-hardened variants — was
    /// inlined into `dealloc_routing` itself, bloating the I-cache footprint
    /// of the hot own-thread free path with code that only ever executes on
    /// a genuine cross-thread free (`contains_base(base) == false`). Mirrors
    /// the existing `refill_magazine_slow` outlining pattern (`#[cold]
    /// #[inline(never)]`, called once behind a single cold branch).
    ///
    /// Pure code motion: the body is byte-identical to the pre-split
    /// `dealloc_routing` tail (same statements, same order, same `return`
    /// points — only now behind a call boundary instead of inlined). `base`
    /// is passed in (already computed by the caller's `contains_base` check)
    /// so it is not recomputed.
    #[cfg(feature = "alloc-xthread")]
    #[cold]
    #[inline(never)]
    fn dealloc_foreign_slow(&mut self, ptr: *mut u8, base: *mut u8, layout: Layout) {
        // `base` is not one of OUR segments. Two possibilities:
        //   (a) a LIVE segment owned by ANOTHER heap — mapped, its owner's
        //       table contains it (just not ours) — reading its header is
        //       safe, and this cross-thread free must be routed to its owner.
        //   (b) a segment WE (or someone) already released — decommitted +
        //       unmapped, its table slot recycled — reading its header would
        //       fault.
        // We cannot O(1)-distinguish (a) from (b) without a global registry
        // (out of scope here); this is the same limitation every allocator
        // has for a double-free-after-full-release. A double-free of a
        // released, unmapped segment is fundamentally UB (as with any
        // allocator) and is NOT fixed by this change — only guarded for the
        // live/mapped case, which is what M2 promises. See the module-level
        // note referenced from task #135's report for the full argument.
        //
        // 0.3.0 (task #138): for the Large branch below, a further
        // POST-reuse mitigation (layout-vs-header size consistency check,
        // `large_layout_consistent`) narrows — but does not close — the
        // remaining window where `base` WAS released and has since been
        // reused for a new allocation before this stale free arrives. See
        // that function's doc comment for the residual limit.
        //
        // Field-specific reads (task #33 root-cause fix): read ONLY `magic`,
        // `kind`, `owner_thread_free` — the cross-thread-read fields written
        // once at init/stamp time and only read thereafter. A full-struct
        // `SegmentHeader::read_at` here raced with the Owner's `bump`-touching
        // `write_header` on `carve_block` (the §11 data race); reading each
        // field individually via its `offset_of!` offset touches bytes
        // disjoint from the owner-mutated `bump`, so there is no race.
        //
        // R4-2 (memory_safety_review, R4-MS-1/MS-2): the first field read is
        // `magic_at(base)`. For a garbage pointer like `1 as *mut u8`,
        // `segment_base_of_ptr` masks to `base == 0`, so `magic_at(0)` would
        // dereference address `offset_of!(SegmentHeader, magic)` with no guard
        // — an immediate read of a structurally-impossible "segment". Reject a
        // null base here as a safe no-op (the same outcome the magic mismatch
        // below produces for a non-segment base). This narrows, but does not
        // close, the cross-heap staleness window noted above (case (a) vs (b)):
        // a base that masks to null cannot be a real segment by construction.
        if base.is_null() {
            return;
        }
        if SegmentHeader::magic_at(base) != SEGMENT_MAGIC {
            return;
        }
        let our_head = self.thread_free_head();
        let owner_tf = SegmentHeader::owner_thread_free_at(base);
        if owner_tf.is_null() || owner_tf == our_head {
            // `contains_base` was false, yet the header claims this segment is
            // unstamped or stamped as ours — this can only happen for a
            // segment that used to be ours and was released (case (b) above,
            // reading now-decommitted-but-still-committed metadata pages of a
            // NOT-YET-actually-unmapped segment is impossible in this
            // process — metadata pages are only unmapped by `os::release_segment`,
            // at which point this read would fault, not return a stale value).
            // Defensive no-op: do NOT route to ourselves via a table state we
            // just proved does not list this segment.
            return;
        }
        if SegmentHeader::kind_at(base) == SegmentKind::Large {
            // 0.3.0 (task A1): used to be a bare `return` here — a PERMANENT
            // leak. The whole segment (4+ MiB, or more for an oversized
            // allocation) was never released and its `SegmentTable` slot was
            // never recycled, because no code path ever revisited a
            // cross-thread-freed Large segment. Fix: push `base` onto the
            // OWNING heap's deferred-free stack (`owner_tf`, already read
            // above — the owner's `thread_free_head()`); the owner reclaims
            // it lazily on its next `alloc_large` slow path (see
            // `drain_large_deferred_free`, called from `alloc`).
            //
            // 0.3.0 (task #138, A1 post-reuse mitigation): before queuing,
            // check that `layout`'s size matches the CURRENT occupant's
            // `large_size` in the header. A stale double-free whose segment
            // was ALREADY reclaimed+reused between the original free and
            // this call will, in the overwhelming majority of cases,
            // observe a header describing a DIFFERENT allocation — this is
            // NOT a full fix (a reuse that happens to request the
            // bit-identical size is not caught; double-free is UB by
            // contract) but narrows the post-reuse corruption window. See
            // `alloc_core::deferred_large::large_layout_consistent`'s doc
            // comment for the full rationale and residual limit.
            if crate::alloc_core::deferred_large::large_layout_consistent(base, layout.size()) {
                Self::push_large_deferred_free(owner_tf, base);
            }
            return;
        }
        // Variant-2: push (offset, class) to the per-segment ring (block bytes
        // untouched). The freer HAS the `Layout`, so it derives the size class
        // here and carries it in the ring entry — the owner's `page_map` is
        // unreliable for the mixed-class pages a shared bump cursor produces, so
        // `reclaim_offset` must NOT derive the class itself (RACE_DRAIN_RECLAIM
        // §13). `kind != Large` is already established above, so a small block's
        // class is always `Some`.
        let off = (ptr as usize - base as usize) as u32;
        let size = layout
            .size()
            .max(crate::alloc_core::size_classes::MIN_BLOCK);
        let class_idx =
            match crate::alloc_core::size_classes::SizeClasses::class_for(size, layout.align()) {
                Some(c) => c as u32,
                None => return, // Large layout on a small segment: contract violation; drop.
            };
        // X7 Ф3 (task #191) touch (b): under `hardened`, stamp the block's
        // CURRENT generation (as observed by THIS freeing thread, Relaxed) into
        // the ring note via `pack_entry_hardened`. The owner's drain (touch (c))
        // compares this stamped gen against the block's gen-at-drain-time; a
        // mismatch means the block was re-issued since this note was stamped,
        // so honouring it would double-free/corrupt the CURRENT occupant — the
        // note is dropped. Non-hardened builds keep the untouched `pack_entry`
        // exactly as before (byte-identical, verified by construction — the
        // `cfg(not)` branch IS the pre-existing code, not a re-implementation).
        // Sibling-block discipline mirrors `Layout::small_meta_end()` (Ф1).
        #[cfg(feature = "hardened")]
        {
            // SAFETY: `base` is a live, exclusively-owned segment; `off` is a
            // MIN_BLOCK-aligned offset of a live block.
            #[allow(unsafe_code)]
            let gen = unsafe { crate::alloc_core::segment_header::gen_at(base, off as usize) };
            let packed =
                crate::alloc_core::remote_free_ring::pack_entry_hardened(gen, class_idx, off);
            let ring = SegmentMeta::new(base).remote_ring();
            Self::push_with_overflow_retry(&ring, base, packed);
        }
        #[cfg(not(feature = "hardened"))]
        {
            let packed = crate::alloc_core::remote_free_ring::pack_entry(off, class_idx);
            let ring = SegmentMeta::new(base).remote_ring();
            Self::push_with_overflow_retry(&ring, base, packed);
        }
    }

    /// RAD-4 (Phase 4, E3a); extended by RAD-4b (task #72): push `packed`
    /// (the block's segment-relative `(offset, class)` word, already packed
    /// by the caller) onto `ring`, retrying on `Err(PushOverflow)` for up to
    /// [`RING_PUSH_RETRY_SPINS`] spin-paced attempts. See the module-level
    /// comment above [`RING_PUSH_RETRY_SPINS`] for the full rationale (why
    /// retry, not a new queue; why bounded, not infinite).
    ///
    /// **RAD-4b addition:** RAD-4 conceded to the documented-sound bounded
    /// leak the moment the retry budget was exhausted — the honestly-measured
    /// residual under full owner starvation
    /// (`tests/remote_fanin.rs::remote_fanin_owner_starved_residual_is_bounded`).
    /// Before conceding, this now tries ONE more thing: push `(base, packed)`
    /// onto `base`'s OWNING heap's slot-resident [`HeapOverflow`](super::heap_overflow::HeapOverflow)
    /// ring (see that module's doc for the full design). The owning slot is
    /// resolved from `base`'s `owner_state` header field (`unpack_owner_id` —
    /// the SAME 12.3 ownership stamp `stamp_segment_owner` writes on every
    /// alloc and `dbg_owner_id_for` already reads cross-thread), indexed
    /// directly into the process-`'static` registry slot array — an ordinary
    /// safe, bounds-checked array access, no new `unsafe` surface. Only if
    /// THAT also fails (the second-chance ring is itself saturated) does this
    /// fall back to the original bounded leak and bump
    /// [`DBG_RING_PUSH_RETRY_EXHAUSTED`].
    ///
    /// Does NOT touch `RemoteFreeRing`'s own push/drain/cursor protocol —
    /// this is a caller-side wrapper that calls the SAME `RemoteFreeRing::push`
    /// every non-retrying call site already used, in a loop.
    #[cfg(feature = "alloc-xthread")]
    #[inline]
    fn push_with_overflow_retry(
        ring: &crate::alloc_core::remote_free_ring::RemoteFreeRing,
        base: *mut u8,
        packed: u32,
    ) {
        if ring.push(packed).is_ok() {
            return; // Fast path: the common case never retries.
        }
        // RAD-4 aggregate-cost fix (task #72 follow-up): the spin window below
        // exists to buy time for the OWNER to drain the ring — it is pure
        // waste when no owner CAN drain. Under the Phase 12.5 shard model a
        // segment's rings are drained only by its slot's CURRENT claimant
        // (lazily, on that thread's alloc path); when the owning slot is FREE
        // (its thread exited, nobody has re-claimed it), no drain can happen
        // until a future claim, so spinning cannot succeed. Without this gate,
        // EVERY free into a full ring of an owner-less segment paid the whole
        // `RING_PUSH_RETRY_SPINS` budget (8,192 spin+CAS attempts under the
        // task #99 retune; was 262,144 pre-calibration) — a send-then-exit producer
        // pattern (`tests/race_norecycle.rs`: producers exit while ~10⁵ of
        // their blocks are still in flight to a long-lived freeing consumer)
        // multiplied that into MINUTES of aggregate dealloc() stall, tripping
        // the test's 30 s watchdog (`process::abort` → 0xC0000409). The gate
        // skips straight to the second-chance `HeapOverflow` push (whose
        // entries a future claimant of the slot drains, exactly like the ring
        // entries themselves) and then to the original documented-sound
        // bounded leak. A LIVE owner keeps RAD-4's designed behaviour
        // unchanged (`tests/remote_fanin.rs` remains the judge for that
        // shape).
        if Self::owner_slot_is_live(base) {
            for _ in 0..RING_PUSH_RETRY_SPINS {
                core::hint::spin_loop();
                if ring.push(packed).is_ok() {
                    DBG_RING_PUSH_RETRIED.fetch_add(1, Ordering::Relaxed);
                    return;
                }
            }
        }
        // Retry budget exhausted: the segment's own ring has stayed
        // saturated across the whole spin window (the owner is not draining
        // fast enough, or not draining at all). RAD-4b: before conceding to
        // the bounded leak, try the owning heap's second-chance overflow
        // ring — this is the mechanism that closes RAD-4's honestly-measured
        // owner-starved residual.
        if Self::push_to_heap_overflow(base, packed) {
            return;
        }
        // Both the segment ring's retry budget AND the heap-level overflow
        // ring are exhausted: the genuinely-unrecovered case. `RemoteFreeRing::
        // push`'s own `DBG_RING_OVERFLOW` / per-segment `overflow_count`
        // already ticked on every attempt above; this counter marks ONLY
        // this fully-unrecovered case.
        DBG_RING_PUSH_RETRY_EXHAUSTED.fetch_add(1, Ordering::Relaxed);
    }

    /// RAD-4b (task #72): resolve `base`'s owning [`HeapSlot`](super::heap_slot::HeapSlot)
    /// from its `owner_state` header stamp and push `(base, packed)` onto
    /// that slot's [`HeapOverflow`](super::heap_overflow::HeapOverflow) ring.
    /// Returns `false` if the owner id is out of range (defensive — should
    /// be unreachable for a live, correctly-stamped segment) or the
    /// second-chance ring is itself saturated.
    ///
    /// `owner_state` is read Relaxed: this is the SAME diagnostic-strength
    /// read `dbg_owner_id_for` already performs cross-thread (the id is
    /// written once per segment-lifetime by the owner's `stamp_segment_owner`
    /// and never concurrently mutated by a second writer — the single-writer
    /// invariant on `owner_state` that every other cross-thread reader of
    /// this field already relies on, e.g. `dealloc_foreign_slow`'s own
    /// `owner_thread_free_at` read a few lines above this call site's
    /// caller). A transient stale read (segment recycled and re-stamped
    /// between this load and the array index below) resolves to either the
    /// SAME heap (harmless) or a DIFFERENT live heap's slot (the pushed
    /// entry sits in the wrong heap's overflow ring, drained on ITS next
    /// opportunistic pass — not a correctness hazard: `HeapOverflow::drain`'s
    /// `reclaim_offset(_checked)` call independently re-validates `base`'s
    /// `magic`/`kind`/bounds before touching anything, exactly as the
    /// existing per-segment ring drain already does for the identical class
    /// of stale-entry hazard).
    #[cfg(feature = "alloc-xthread")]
    #[inline]
    fn push_to_heap_overflow(base: *mut u8, packed: u32) -> bool {
        use crate::alloc_core::segment_header::unpack_owner_id;
        let owner_atomic = SegmentMeta::new(base).owner_state_atomic();
        let owner_id = unpack_owner_id(owner_atomic.load(Ordering::Relaxed));
        let reg = super::bootstrap::ensure();
        let idx = owner_id as usize;
        if idx >= super::bootstrap::MAX_HEAPS {
            return false; // Defensive: unstamped/garbled owner id.
        }
        // SAFETY-FREE: `idx < MAX_HEAPS` just checked; `reg.slots` is a plain
        // `'static` array — ordinary bounds-checked indexing, no `unsafe`.
        let slot = &reg.slots[idx];
        slot.overflow.push(base, packed)
    }

    /// Advisory owner-liveness probe gating
    /// [`push_with_overflow_retry`](Self::push_with_overflow_retry)'s spin
    /// window: `true` iff `base`'s owning registry slot is currently
    /// `STATE_LIVE` — i.e. some thread exists that will (lazily, on its alloc
    /// path) drain this segment's ring, so waiting for that drain is
    /// meaningful. Resolution is the same `owner_state` → `unpack_owner_id` →
    /// `slots[idx]` walk [`push_to_heap_overflow`](Self::push_to_heap_overflow)
    /// performs (see its doc comment for why the Relaxed `owner_state` read is
    /// sound cross-thread).
    ///
    /// **Advisory, not authoritative — both stale outcomes are benign.** The
    /// slot's `state` is read Relaxed with no generation check, so this can
    /// race claim/recycle in either direction:
    /// - stale `LIVE` (owner exited just after the load): ONE free wastes one
    ///   spin budget; the NEXT free re-probes and sees `FREE`. Bounded,
    ///   one-off — not the per-free multiplication this gate exists to stop.
    /// - stale `FREE` (slot re-claimed just after the load): the push skips
    ///   ahead to the `HeapOverflow` ring, whose entries the new claimant
    ///   drains on its own schedule — the same destination those entries had
    ///   anyway. No block is lost that the spin would have saved.
    ///
    /// An out-of-range id (`OWNER_ID_NONE` — an unstamped early segment, or
    /// the process-global fallback heap, whose `id = u32::MAX` masks to
    /// `OWNER_ID_NONE` under `pack_owner`'s 31-bit id field) has no slot to
    /// consult; report "live" to preserve RAD-4's original unconditional spin
    /// there (the fallback heap is process-lived and drains on its own
    /// allocs, so waiting for it is meaningful).
    #[cfg(feature = "alloc-xthread")]
    #[inline]
    fn owner_slot_is_live(base: *mut u8) -> bool {
        use crate::alloc_core::segment_header::unpack_owner_id;
        let owner_atomic = SegmentMeta::new(base).owner_state_atomic();
        let idx = unpack_owner_id(owner_atomic.load(Ordering::Relaxed)) as usize;
        if idx >= super::bootstrap::MAX_HEAPS {
            return true;
        }
        super::bootstrap::ensure().slots[idx]
            .state
            .load(Ordering::Relaxed)
            == super::heap_slot::STATE_LIVE
    }
}
