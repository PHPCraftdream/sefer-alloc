//! Foreign-thread dealloc routing of the former flat `heap_core_xthread.rs`
//! (reorg step 4): the own-segment `contains_base` fast path, the outlined
//! cold cross-thread tail, and the heap-instance-independent foreign routing
//! core shared with bind-less threads. Pure code-movement sibling files; no
//! behavior changed.

use core::alloc::Layout;
use core::sync::atomic::AtomicPtr;

use crate::alloc_core::os;
use crate::alloc_core::segment_header::SegmentMeta;
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind, SEGMENT_MAGIC};

use crate::registry::heap_core::HeapCore;

impl HeapCore {
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
    pub(crate) fn dealloc_routing(&mut self, ptr: *mut u8, layout: Layout) {
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
    /// Thin wrapper (R6-OPT-P0-1): computes `our_head` (the ONE piece of this
    /// routing that genuinely needs `&self` — see
    /// [`dealloc_foreign_routing`]'s doc comment for why every other step is
    /// heap-instance-independent) and delegates to the shared, `&self`-free
    /// [`dealloc_foreign_routing`] with `Some(our_head)`, preserving this
    /// bound-thread caller's behavior byte-for-byte (the `owner_tf ==
    /// our_head` defensive no-op branch still fires exactly as before).
    #[cfg(feature = "alloc-xthread")]
    #[cold]
    #[inline(never)]
    fn dealloc_foreign_slow(&mut self, ptr: *mut u8, base: *mut u8, layout: Layout) {
        let our_head = self.thread_free_head();
        Self::dealloc_foreign_routing(ptr, base, layout, Some(our_head));
    }

    /// R6-OPT-P0-1: the heap-instance-independent core of the cross-thread
    /// dealloc routing tail, shared by two callers:
    ///
    /// - [`dealloc_foreign_slow`](Self::dealloc_foreign_slow) (a BOUND
    ///   thread's cold cross-thread-free path, via `dealloc_routing`) passes
    ///   `Some(our_head)` — `our_head` being ITS `thread_free_head()` — so the
    ///   `owner_tf == our_head` defensive no-op guard (see below) still
    ///   applies exactly as before this split.
    /// - a bind-less thread's dealloc resolver (`global::tls_heap`'s
    ///   `current_for_dealloc`, reached from `SeferAlloc::dealloc` WITHOUT
    ///   constructing or dereferencing any `*mut HeapCore`) passes `None` —
    ///   there is no "our_head" to compare against, because a bind-less
    ///   thread (TLS never bound, or `TORN` — its slot already recycled) has
    ///   no live heap of its own to compare the segment's owner stamp
    ///   against. Every valid pointer reaching `dealloc` on such a thread is
    ///   foreign BY CONSTRUCTION: `SeferAlloc::alloc` always binds a heap on
    ///   first use, so a thread whose TLS is null/TORN never allocated
    ///   anything itself under this allocator instance — the pointer, if
    ///   valid, was necessarily produced by (and stamped with the owner of)
    ///   some OTHER thread's heap.
    ///
    /// Byte-identical body to the pre-split `dealloc_foreign_slow` tail (same
    /// statements, same order, same `return` points) EXCEPT that the
    /// `owner_tf == our_head` half of the defensive check is skipped entirely
    /// when `our_head` is `None` — see the `match our_head` below. Every
    /// other call in this function is already an associated function taking
    /// no `&self`/`&mut self` (`Self::push_large_deferred_free`,
    /// `Self::push_with_overflow_retry`) — they operate purely on
    /// `base`/`packed`/`head` parameters and the process-global registry via
    /// `super::bootstrap::ensure()` — which is what makes this split
    /// possible at all: routing a foreign pointer never actually needed a
    /// live `&mut HeapCore`, only (for a bound thread) its own head for the
    /// self-check.
    #[cfg(feature = "alloc-xthread")]
    #[cold]
    #[inline(never)]
    pub(crate) fn dealloc_foreign_routing(
        ptr: *mut u8,
        base: *mut u8,
        layout: Layout,
        our_head: Option<*const AtomicPtr<u8>>,
    ) {
        // `base` is not one of OUR segments. Two possibilities:
        //   (a) a LIVE segment owned by ANOTHER heap — mapped, its owner's
        //       table contains it (just not ours) — reading its header is
        //       safe, and this cross-thread free must be routed to its owner.
        //   (b) a segment WE (or someone) already released — decommitted +
        //       unmapped, its table slot recycled — reading its header would
        //       fault.
        // We cannot O(1)-distinguish (a) from (b) without a global registry
        // (out of scope here); this is the same limitation every allocator
        // has for a double-free-after-full-release. Any double-free violates
        // the caller contract, including one into a live/mapped segment;
        // M2 catches only some such misuse. A released, unmapped base can
        // fault before even those checks. See the module-level note
        // referenced from task #135's report for the full argument.
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
        let owner_tf = SegmentHeader::owner_thread_free_at(base);
        // R6-OPT-P0-1: `Some(head)` preserves the exact bound-thread
        // defensive check (`owner_tf.is_null() || owner_tf == head`).
        // `None` (bind-less caller) skips the `== head` half entirely — there
        // is no "us" to compare against — but keeps the `is_null()` no-op
        // (an unstamped segment; defensive, should not happen for a live
        // block, same as today).
        let is_self_or_unstamped = match our_head {
            Some(head) => owner_tf.is_null() || owner_tf == head,
            None => owner_tf.is_null(),
        };
        if is_self_or_unstamped {
            // `contains_base` was false (for the bound-thread caller) — or
            // there simply is no `contains_base` table to consult (the
            // bind-less caller has no heap at all) — yet the header claims
            // this segment is unstamped, or (bound-thread only) stamped as
            // ours. For the bound-thread case this can only happen for a
            // segment that used to be ours and was released (case (b) above,
            // reading now-decommitted-but-still-committed metadata pages of a
            // NOT-YET-actually-unmapped segment is impossible in this
            // process — metadata pages are only unmapped by `os::release_segment`,
            // at which point this read would fault, not return a stale value).
            // Defensive no-op: do NOT route to ourselves via a table state we
            // just proved does not list this segment (or, for the bind-less
            // caller, do not touch an unstamped segment at all).
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
            // check that `layout`'s size AND align (R22-5, task #356) match
            // the CURRENT occupant's `large_size`/`large_align` in the
            // header. A stale double-free whose segment was ALREADY
            // reclaimed+reused between the original free and this call will,
            // in the overwhelming majority of cases, observe a header
            // describing a DIFFERENT allocation — this is NOT a full fix (a
            // reuse that happens to request the bit-identical size AND align
            // is not caught; double-free is UB by contract) but narrows the
            // post-reuse corruption window. See
            // `alloc_core::deferred_large::large_layout_consistent`'s doc
            // comment for the full rationale and residual limit.
            if crate::alloc_core::deferred_large::large_layout_consistent(base, layout) {
                Self::push_large_deferred_free(owner_tf, base);
            }
            return;
        }
        // Variant-2 first tries the per-segment ring with (offset, class);
        // that ring and the per-heap sidecar do not touch block bytes. If both
        // fill, the final spill tier writes into the exclusively transferred
        // freed block. The freer HAS the `Layout`, so it derives the size class
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
            Self::push_with_overflow_retry(&ring, ptr, base, packed);
        }
        #[cfg(not(feature = "hardened"))]
        {
            let packed = crate::alloc_core::remote_free_ring::pack_entry(off, class_idx);
            let ring = SegmentMeta::new(base).remote_ring();
            Self::push_with_overflow_retry(&ring, ptr, base, packed);
        }
    }
}
