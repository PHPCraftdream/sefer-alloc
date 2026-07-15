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

/// R6-OPT-P0-4: the bounded spin-retry loop in
/// [`HeapCore::push_with_overflow_retry`] runs for this many iterations, NOT
/// the raw [`RING_PUSH_RETRY_SPINS`]. `RING_PUSH_RETRY_SPINS` (8,192) was
/// calibrated (task #99) against the PRE-R6-OPT-P0-4 per-iteration cost — a
/// COUNTED `RemoteFreeRing::push`, whose failure branch does two locked-RMW
/// counter increments. That cost was incidental to the counting, but it also
/// served as real wall-clock pacing: "8,192 iterations" was calibrated to
/// mean "~32 drain opportunities" in TIME (see the calibration comment above
/// `RING_PUSH_RETRY_SPINS` in `heap_core.rs`), not literally "8,192 loop
/// bodies". R6-OPT-P0-4's `try_push_uncounted` (on the ring) and
/// `HeapOverflow::push_uncounted` (on the heap-level overflow ring, added
/// after a zero-trust review caught the first version of this fix still
/// calling the COUNTED `HeapOverflow::push` in the loop — see that method's
/// doc comment) together remove BOTH tiers' counter RMWs (that is the whole
/// point of the fix), which makes each iteration far cheaper — so the SAME
/// iteration count now represents much LESS real time, and was measured to
/// reintroduce non-zero `DBG_RING_PUSH_RETRY_EXHAUSTED` on
/// `tests/remote_fanin.rs::remote_fanin_high_contention_budget_is_sufficient`'s
/// 32-producer judge specifically under host CPU contention.
///
/// Multiplying the iteration count restores comparable per-iteration real
/// time WITHOUT reintroducing an atomic RMW or any backoff/growing-gap shape
/// (the flat-spin-not-backoff lesson from task #99's own calibration still
/// holds — this is MORE spinning, not DIFFERENT spinning). The factor was
/// measured empirically against the SAME judge on this project's own dev
/// hardware via an INTERLEAVED baseline-vs-modified A/B (alternating one
/// baseline run, one modified run, same tight loop, so both see the SAME
/// fluctuating host-load window — a non-interleaved "run baseline 10x, then
/// modified 10x" comparison was tried first and produced misleadingly noisy
/// deltas, since this shared dev box's OTHER concurrent load swings by 5-10x
/// between successive batches). `4`x/`8`x/`32`x (the ring-only fix's factor)
/// all left a measurably worse flake rate than baseline once the overflow
/// ring's counter was ALSO removed; `64`x and `128`x were still inconsistent
/// across interleaved batches; `256`x — three interleaved rounds (n=12, n=12,
/// n=20) — converged to a modified-vs-baseline flake rate statistically
/// indistinguishable from the unmodified baseline (combined: baseline 9/44,
/// modified 10/44, across host-load windows ranging from ~50% to ~100% CPU
/// busy from other concurrent processes on this shared dev machine). Like
/// `RING_PUSH_RETRY_SPINS` itself, `#[cfg(miri)]` keeps the interpreted-
/// execution budget separately small (unscaled — miri's interpreter overhead
/// already dwarfs any native RMW-vs-uncounted timing difference, and a 32x
/// miri budget was independently measured impractically slow).
#[cfg(all(feature = "alloc-xthread", not(miri)))]
const RETRY_LOOP_ITERATIONS: u32 = RING_PUSH_RETRY_SPINS.saturating_mul(256);
#[cfg(all(feature = "alloc-xthread", miri))]
const RETRY_LOOP_ITERATIONS: u32 = RING_PUSH_RETRY_SPINS;

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
    /// discipline as `deferred_next_atomic`/`owner_state_atomic`) rather
    /// than inside the shared (seam-free) module.
    #[cfg(feature = "alloc-xthread")]
    fn push_large_deferred_free(head: *const AtomicPtr<u8>, base: *mut u8) {
        // `heap_core.rs` is NOT an allowed `unsafe` seam (see `src/lib.rs`'s
        // seam whitelist), so the pointer-to-reference deref is delegated to
        // `Node::atomic_ptr_ref` (the `alloc_core::node` seam), same
        // discipline as `deferred_next_atomic`/`owner_state_atomic`.
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

    /// RAD-4 (Phase 4, E3a); extended by RAD-4b (task #72); reordered by
    /// R6-OPT-P0-4: push `packed` (the block's segment-relative `(offset,
    /// class)` word, already packed by the caller) onto `ring`, falling back
    /// through a three-tier chain — segment ring → heap-level overflow ring →
    /// bounded spin-retry against both — before conceding to the original
    /// documented-sound bounded leak.
    ///
    /// **R6-OPT-P0-4 — overflow-first policy (current).** The PRE-R6-OPT-P0-4
    /// policy exhausted the WHOLE [`RING_PUSH_RETRY_SPINS`] (8,192) spin
    /// budget against the segment ring FIRST, and only tried the heap-level
    /// [`HeapOverflow`](super::heap_overflow::HeapOverflow) ring after that
    /// budget was exhausted. Each failed `RemoteFreeRing::push` attempt inside
    /// that budget ticks TWO diagnostic counters (`overflow()` +
    /// `DBG_RING_OVERFLOW`, both locked RMWs) — so a single logical free
    /// landing on a saturated ring with a LIVE owner (the common, non-
    /// pathological case) paid up to 8,193 full ring-state checks and 16,386
    /// counter RMWs before ever trying the second-chance ring that was sitting
    /// right there the whole time with 8x the capacity
    /// (`HeapOverflow::HEAP_OVERFLOW_CAP` = 2048 vs. `RING_CAP` = 256). The
    /// policy is now:
    ///
    /// 1. One normal (counted) [`RemoteFreeRing::push`] attempt — the fast
    ///    path; the common case never proceeds past this.
    /// 2. On that push failing (ring full): IMMEDIATELY try
    ///    [`push_to_heap_overflow`](Self::push_to_heap_overflow) — BEFORE any
    ///    spinning. This is the actual inversion: try the cheap,
    ///    already-provisioned second-chance ring first, instead of last.
    /// 3. Only if BOTH the ring push AND the immediate overflow attempt
    ///    failed (both momentarily full — a rare double-saturation case) does
    ///    this fall into the bounded spin-retry loop below, still gated by
    ///    [`owner_slot_is_live`](Self::owner_slot_is_live) exactly as before
    ///    (see that method's doc comment — the gate exists to stop
    ///    pathological aggregate stall against a dead/exited owner; see
    ///    `tests/race_norecycle.rs` and the big comment block above
    ///    [`RING_PUSH_RETRY_SPINS`] in `heap_core.rs` explaining why flat spin
    ///    (not backoff) was chosen and why the budget is calibrated to
    ///    8,192 = 32×`RING_CAP`). The loop runs for [`RETRY_LOOP_ITERATIONS`]
    ///    — NOT the raw `RING_PUSH_RETRY_SPINS` — see that constant's own doc
    ///    comment for why a scale-up was needed on top of the counter removal.
    /// 4. Inside that retry loop, every poll retries BOTH tiers: the segment
    ///    ring via [`RemoteFreeRing::try_push_uncounted`] — NOT `push` — so a
    ///    failed ring poll does not re-tick either ring diagnostic counter
    ///    (both already ticked once, in step 1's single counted attempt) —
    ///    AND the heap-level overflow ring, via the SAME `&'static
    ///    HeapOverflow` reference [`resolve_heap_overflow`](
    ///    Self::resolve_heap_overflow) resolves ONCE before the loop starts
    ///    (not re-resolved on every poll — see that function's doc comment for
    ///    the measured per-poll resolution cost this hoist avoids). Retrying
    ///    BOTH on every poll (not just the ring) matters under sustained high
    ///    fan-in: the owner's opportunistic `drain_heap_overflow` runs only
    ///    between its own `alloc()` calls, so a transient overflow-ring-full
    ///    moment needs the SAME spin window's repeated chances the ring gets,
    ///    not a single try-once-and-never-again attempt — see race model (a)
    ///    in this method's correctness notes ("owner-drain racing producer-
    ///    reservation on EITHER tier"). Skipping this and only retrying the
    ///    ring measurably regressed
    ///    `tests/remote_fanin.rs::remote_fanin_high_contention_budget_is_sufficient`
    ///    (32-producer live-owner fan-in) during this policy's development.
    ///    On a successful retry (either tier), [`DBG_RING_PUSH_RETRIED`] is
    ///    bumped exactly once (a single, meaningful, low-frequency event —
    ///    not per-attempt). If the owner is NOT live, spinning on the ring is
    ///    skipped entirely (nothing will drain it — see
    ///    [`owner_slot_is_live`](Self::owner_slot_is_live)'s doc comment) but
    ///    ONE more `push_to_heap_overflow` attempt still runs (that ring is
    ///    drained by whichever thread next claims the slot, not necessarily
    ///    "this owner"). Only once every avenue above is exhausted does this
    ///    concede to the bounded leak and bump [`DBG_RING_PUSH_RETRY_EXHAUSTED`]
    ///    — which now means "truly nothing worked: initial ring attempt
    ///    failed, immediate overflow attempt failed, AND the bounded retry
    ///    against both never recovered".
    ///
    /// Net effect: the overwhelmingly common case (ring full, overflow has
    /// room) now costs 2 checks total (1 counted ring push + 1 overflow push)
    /// instead of up to 8,193. The genuinely rare double-saturation case still
    /// gets the full retry protection against both tiers, just without the
    /// ring's own counter-RMW tax on every failed ring poll.
    ///
    /// Does NOT touch either ring's own push/drain/cursor PROTOCOL — this is
    /// a caller-side wrapper composing `RemoteFreeRing::push` /
    /// `try_push_uncounted` and `HeapOverflow::push` / `push_uncounted`. The
    /// two `_uncounted` siblings are new (added by this task, byte-identical
    /// to their counted namesakes except for the diagnostic-counter bump on
    /// the full-ring branch — see each one's own doc comment); the counted
    /// `push` methods themselves are unmodified.
    #[cfg(feature = "alloc-xthread")]
    #[inline]
    fn push_with_overflow_retry(
        ring: &crate::alloc_core::remote_free_ring::RemoteFreeRing,
        base: *mut u8,
        packed: u32,
    ) {
        if ring.push(packed).is_ok() {
            return; // Fast path: the common case never proceeds further.
        }
        // R6-OPT-P0-4: the segment ring is full. Try the heap-level
        // second-chance overflow ring IMMEDIATELY — before any spinning. This
        // is the policy inversion: the pre-R6-OPT-P0-4 code spent the WHOLE
        // spin budget against the segment ring first; `push_to_heap_overflow`
        // is a single cheap CAS-reserve attempt against an already-provisioned
        // ring with 8x the capacity, so trying it first resolves the
        // overwhelmingly common case (ring momentarily full, overflow has
        // room) in exactly 2 checks total.
        if Self::push_to_heap_overflow(base, packed) {
            return;
        }
        // Both the segment ring AND the immediate overflow attempt failed —
        // the rare double-saturation case. Fall into the bounded spin-retry,
        // gated by `owner_slot_is_live` exactly as before R6-OPT-P0-4 (see
        // that method's doc comment for the full "why gate" rationale,
        // repeated briefly here): the spin window exists to buy time for the
        // OWNER to drain the ring; it is pure waste when no owner CAN drain.
        // Under the Phase 12.5 shard model a segment's rings are drained only
        // by its slot's CURRENT claimant (lazily, on that thread's alloc
        // path); when the owning slot is FREE (its thread exited, nobody has
        // re-claimed it), no drain can happen until a future claim, so
        // spinning cannot succeed. Without this gate, EVERY free into a full
        // ring of an owner-less segment paid the whole spin-retry budget — a
        // send-then-exit producer pattern
        // (`tests/race_norecycle.rs`: producers exit while ~10⁵ of their
        // blocks are still in flight to a long-lived freeing consumer)
        // multiplied that into MINUTES of aggregate dealloc() stall, tripping
        // the test's 30 s watchdog (`process::abort` → 0xC0000409). A LIVE
        // owner keeps the designed behaviour (`tests/remote_fanin.rs` remains
        // the judge for that shape).
        if Self::owner_slot_is_live(base) {
            // R6-OPT-P0-4: resolve the target `HeapOverflow` ONCE before the
            // loop (not on every poll — see `resolve_heap_overflow`'s doc
            // comment for the measured cost of re-resolving thousands of
            // times: it thins this loop's effective poll rate enough to
            // matter under host CPU contention). `None` only for a
            // defensively-unstamped/garbled owner id (should be unreachable
            // for a live segment); the loop still polls the ring alone in
            // that case, matching `push_to_heap_overflow`'s own "returns
            // false" defensive behaviour.
            let overflow = Self::resolve_heap_overflow(base);
            for _ in 0..RETRY_LOOP_ITERATIONS {
                core::hint::spin_loop();
                // R6-OPT-P0-4: uncounted — the ring's own overflow diagnostics
                // already ticked once (step 1's counted `push` attempt above);
                // re-ticking them on every one of up to `RETRY_LOOP_ITERATIONS`
                // failed polls here would tax the diagnostic counters with a
                // locked RMW per poll for no informational gain (see
                // `try_push_uncounted`'s doc comment for the full argument).
                if ring.try_push_uncounted(packed).is_ok() {
                    DBG_RING_PUSH_RETRIED.fetch_add(1, Ordering::Relaxed);
                    return;
                }
                // Also retry the heap-level overflow ring on every poll, not
                // just once before/after this loop. Under sustained
                // high-fan-in pressure (many producers racing the SAME
                // segment ring), the immediate single overflow attempt above
                // can itself land on a momentarily-full overflow ring — the
                // owner's opportunistic `drain_heap_overflow` only runs
                // between its own `alloc()` calls, so both tiers need the
                // SAME spin window to give the owner repeated chances to
                // drain EITHER one (race model (a) in the task spec: "owner-
                // drain racing producer-reservation on either tier"). Without
                // this, a transient overflow-ring-full moment graduates
                // straight to burning the whole spin budget against the
                // segment ring alone, which measurably regressed the
                // high-contention judge (`remote_fanin_high_contention_
                // budget_is_sufficient`) during development of this fix. A
                // coarser cadence (checking overflow only every Nth poll) was
                // also tried and measured WORSE — throttling the overflow
                // retries at contention this high loses more than the
                // ring-poll-rate dilution it was meant to avoid, so
                // every-poll is the retained shape (made affordable by
                // resolving `overflow` once above instead of per-poll).
                // Zero-trust review finding: this MUST be `push_uncounted`,
                // not `push` — `HeapOverflow::push`'s "ring full" branch bumps
                // its OWN `overflow_count` diagnostic (a locked RMW on a
                // cache line shared by every producer targeting this heap
                // slot's overflow ring), so calling the counted `push` here
                // reintroduces exactly the per-poll atomic-storm class this
                // whole task exists to close — just relocated from the
                // segment ring's counters to the overflow ring's counter,
                // and now scaled by `RETRY_LOOP_ITERATIONS` (2,097,152 native,
                // `RING_PUSH_RETRY_SPINS` x256) instead of the old
                // `RING_PUSH_RETRY_SPINS` (8,192). The ONE
                // counted overflow attempt already made (the immediate step-2
                // attempt above this loop, or the single not-live-path
                // attempt in the `else` branch below) remains the signal
                // "this heap's overflow ring saturated at all"; every in-loop
                // poll here is uncounted, mirroring the ring's own
                // `try_push_uncounted` discipline exactly.
                if let Some(overflow) = overflow {
                    if overflow.push_uncounted(base, packed) {
                        DBG_RING_PUSH_RETRIED.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                }
            }
        } else if Self::push_to_heap_overflow(base, packed) {
            // Owner not live: no point spinning on the segment ring (nothing
            // will drain it), but the heap-level overflow ring is drained by
            // whichever thread next CLAIMS this slot, not by "this specific
            // owner" — so one attempt here still has a chance (mirrors the
            // pre-loop immediate attempt; kept as a distinct branch so the
            // not-live path does not fall through to ANOTHER redundant
            // overflow attempt below when it already just tried and failed).
            return;
        }
        // Every tier exhausted (retry budget spent with the owner live, or
        // the owner was not live and the single not-live-path attempt above
        // also failed): the genuinely-unrecovered case. The segment ring's
        // own `DBG_RING_OVERFLOW` / per-segment `overflow_count` ticked ONCE
        // (step 1's single counted attempt, not on every retry poll); this
        // counter marks ONLY this fully-unrecovered case.
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
        match Self::resolve_heap_overflow(base) {
            Some(overflow) => overflow.push(base, packed),
            None => false, // Defensive: unstamped/garbled owner id.
        }
    }

    /// R6-OPT-P0-4: factored out of [`push_to_heap_overflow`](
    /// Self::push_to_heap_overflow) so the bounded spin-retry loop in
    /// [`push_with_overflow_retry`](Self::push_with_overflow_retry) can
    /// resolve `base`'s owning [`HeapOverflow`](super::heap_overflow::HeapOverflow)
    /// ONCE before the loop and reuse the `&'static` reference across up to
    /// [`RETRY_LOOP_ITERATIONS`] poll iterations, instead of
    /// re-reading the `owner_state` header atomic and re-indexing the
    /// registry's slot array on EVERY poll. The re-resolution cost (an extra
    /// atomic load plus an array index, repeated thousands of times) was
    /// measured to matter under contention: it slows this loop's effective
    /// poll rate enough to visibly increase `DBG_RING_PUSH_RETRY_EXHAUSTED`
    /// flakes on `tests/remote_fanin.rs::remote_fanin_high_contention_
    /// budget_is_sufficient` specifically when the host machine is ALSO under
    /// concurrent CPU load (multiple `cargo`/build processes contending for
    /// cores) — a same-machine, same-code A/B (10 runs each) measured 1/10
    /// baseline-shaped flakes vs. 8/10 with per-iteration re-resolution,
    /// dropping back to a baseline-comparable rate once resolved once here.
    ///
    /// Same staleness argument as [`push_to_heap_overflow`]'s own doc comment
    /// applies UNCHANGED, just amortised across the loop instead of repeated
    /// per iteration: a transient stale read (segment recycled and
    /// re-stamped between this resolution and a later poll inside the loop)
    /// still resolves to either the SAME heap (harmless) or a DIFFERENT live
    /// heap's slot (the pushed entry sits in the wrong heap's overflow ring,
    /// drained on ITS next opportunistic pass — not a correctness hazard, see
    /// that doc comment for the full argument). Returns `None` if the owner
    /// id is out of range (defensive — should be unreachable for a live,
    /// correctly-stamped segment).
    #[cfg(feature = "alloc-xthread")]
    #[inline]
    fn resolve_heap_overflow(base: *mut u8) -> Option<&'static super::heap_overflow::HeapOverflow> {
        use crate::alloc_core::segment_header::unpack_owner_id;
        let owner_atomic = SegmentMeta::new(base).owner_state_atomic();
        let owner_id = unpack_owner_id(owner_atomic.load(Ordering::Relaxed));
        let reg = super::bootstrap::ensure();
        let idx = owner_id as usize;
        if idx >= super::bootstrap::MAX_HEAPS {
            return None; // Defensive: unstamped/garbled owner id.
        }
        // SAFETY-FREE: `idx < MAX_HEAPS` just checked; `reg.slots` is a plain
        // `'static` array — ordinary bounds-checked indexing, no `unsafe`.
        Some(&reg.slots[idx].overflow)
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
