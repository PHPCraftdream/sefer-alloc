//! Free-side hot path for [`HeapCore`] (mechanical split of `heap_core.rs`,
//! task R6-CQ-7b).
//!
//! This file holds the `impl HeapCore { .. }` block for `dealloc` and
//! `realloc` (the two largest, most safety-critical methods in the former
//! monolith, each carrying a full `# Safety` doc from R6-MS-1/2) plus their
//! own-thread free bodies (`dealloc_own_thread`, `dealloc_own_thread_with_base`)
//! and — under `medium-classes` MINUS the zero-headroom `exact-span-large`
//! exclusion (R15-3, task #305; see `try_promote_to_large`'s own doc) — the
//! R14-4 (task #289) Small/medium->Large realloc-promotion helper
//! (`try_promote_to_large`), Stage 2 of the design in `docs/perf/
//! R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md`. Otherwise a pure
//! code-movement sibling of `heap_core.rs`; no other behavior changed — the
//! `# Safety` docs and `#[allow(unsafe_code)]` attributes moved byte-for-byte.

use core::alloc::Layout;

#[cfg(feature = "alloc-global")]
use crate::alloc_core::os;
#[cfg(all(feature = "hardened", feature = "alloc-global", feature = "fastbin"))]
use crate::alloc_core::segment_header::SegmentKind;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
use crate::alloc_core::segment_header::SegmentMeta;
#[cfg(feature = "alloc-xthread")]
use crate::alloc_core::segment_header::{SegmentHeader, SEGMENT_MAGIC};
use crate::alloc_core::{node::Node, AllocCore};

use super::heap_core::HeapCore;

/// R14-4 (task #289): the requested-size threshold above which a GROWING
/// realloc of a Small/medium-classified block (`medium-classes` only) is
/// diverted directly to a Large allocation instead of walking the medium
/// size-class ladder one class at a time. 256 KiB — the smallest
/// `medium-classes` class — per the design doc's recommendation
/// (`R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md` §0): by this size the
/// object has already paid for at least one medium-class carve, so promoting
/// it is not premature for a buffer that turns out to be a one-shot small
/// object, while still capturing roughly half the growth ladder's copy cost
/// (the doc's own threshold sweep: 128 KiB promotes too eagerly, paying RSS
/// for objects that may never grow again; 384 KiB defers too long, leaving
/// most of the ladder-walk cost on the table).
///
/// R15-3 (task #305, review finding P2-3): gated by the SAME extended
/// predicate as the promotion call site and `try_promote_to_large` below, not
/// bare `medium-classes` — see the call site's doc comment for the full root
/// cause. This constant is used ONLY inside that narrower-gated call site, so
/// its own `#[cfg]` must match exactly or it becomes an "unused constant"
/// warning (promoted to a hard error under `-D warnings`, per this crate's
/// `npm run check`) in any build where `medium-classes` is on but the
/// zero-headroom exclusion has switched promotion off.
#[cfg(all(
    feature = "medium-classes",
    any(
        not(feature = "exact-span-large"),
        all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
    )
))]
const MEDIUM_REALLOC_PROMOTION_THRESHOLD: usize = 256 * 1024;

impl HeapCore {
    /// Deallocate `ptr` (previously returned by [`alloc`](Self::alloc)).
    ///
    /// Own-thread path: routes to the owning segment's `BinTable` via
    /// [`AllocCore::dealloc`] (which applies the M2 double-free guard).
    /// Under `alloc-xthread`: if the segment is stamped with another heap's
    /// head, route cross-thread via the TFS (the §2.2 protocol re-based on
    /// the registry). Foreign pointers (not a sefer segment) are a safe
    /// no-op.
    ///
    /// This is an **`unsafe fn`** (R6-MS-1/2): it forwards the
    /// [`AllocCore::dealloc`] caller-pointer contract. The crate's former
    /// posture was a safe `pub fn`; reversed after the round5 review — see
    /// [`AllocCore::dealloc`]'s `# Safety` and `CHANGELOG.md` (R6-MS-1/2).
    ///
    /// # Safety
    ///
    /// The caller must uphold the [`GlobalAlloc::dealloc`] contract for `ptr`
    /// and `layout`:
    ///
    /// - `ptr` is **null** OR the exact **start** pointer of a currently-LIVE
    ///   allocation made by *this* `HeapCore` (own segment, or — under
    ///   `alloc-xthread` — a live segment owned by another heap in the same
    ///   process that this call will route cross-thread). It MUST NOT be an
    ///   interior pointer.
    /// - `layout` exactly matches the allocation's layout.
    /// - The allocation is freed **at most once**; `ptr` is not re-issued
    ///   after this call.
    /// - `ptr` is not a foreign / already-released-unmapped base.
    ///
    /// Null `ptr` is always safe (early return). The M2 defensive paths remain
    /// as defence-in-depth, not a substitute for the contract.
    #[inline(always)]
    #[allow(unsafe_code)] // R6-MS-1/2: `unsafe fn` boundary (caller-pointer contract).
    pub unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }
        #[cfg(feature = "alloc-xthread")]
        {
            self.dealloc_routing(ptr, layout);
        }
        #[cfg(not(feature = "alloc-xthread"))]
        {
            self.dealloc_own_thread(ptr, layout);
        }
    }

    /// Own-thread dealloc: small frees go to the magazine (under fastbin),
    /// everything else to `core.dealloc`. Called from the `!alloc-xthread`
    /// path (no routing needed) and from `dealloc_routing` after confirming
    /// the block is ours.
    ///
    /// Э9 (P7.1, task #160): under fastbin this delegates to
    /// [`dealloc_own_thread_with_base`](Self::dealloc_own_thread_with_base),
    /// computing `base = os::segment_base_of_ptr(ptr)` itself. The
    /// Э9 (P7.1): under fastbin the magazine body lives in
    /// [`dealloc_own_thread_with_base`](Self::dealloc_own_thread_with_base)
    /// (which takes the pre-computed `base`), and BOTH callers of the
    /// own-thread path under fastbin already hold `base` (the cross-thread
    /// `dealloc_routing` from its `contains_base` check; there is no
    /// `!alloc-xthread` caller under fastbin since `fastbin ⟹ alloc-xthread`).
    /// So this own-arg wrapper is compiled ONLY when fastbin is OFF — where the
    /// own-thread path has no magazine and simply delegates to `core.dealloc`.
    /// Callers: the `!alloc-xthread` branch of [`dealloc`](Self::dealloc) and
    /// the non-fastbin arm of `dealloc_routing`.
    #[cfg(not(all(feature = "alloc-global", feature = "fastbin")))]
    #[inline(always)]
    pub(super) fn dealloc_own_thread(&mut self, ptr: *mut u8, layout: Layout) {
        // Non-fastbin own-thread free: no magazine — delegate to core.
        // SAFETY: this own-thread body is reached only from `HeapCore::dealloc`,
        // an `unsafe fn` whose caller bound `ptr`/`layout` to the
        // `GlobalAlloc::dealloc` contract (valid live start pointer, matching
        // layout, freed once); we forward the same pair unchanged.
        #[allow(unsafe_code)] // R6-MS-1/2: unsafe call into `AllocCore::dealloc`.
        unsafe {
            self.core.dealloc(ptr, layout)
        };
    }

    /// Э9 (P7.1, task #160): own-thread dealloc body, taking a pre-computed
    /// `base = os::segment_base_of_ptr(ptr)` so the cross-thread
    /// [`dealloc_routing`](Self::dealloc_routing) path — which already
    /// computed `base` for its `contains_base` ownership check — does not
    /// recompute it. Behaviour is byte-identical to the former inline body:
    /// the R1 `off >= bump` stale-free guard and the Э6 magazine/bitmap M2
    /// oracles all operate on this passed-in `base` (which equals what they
    /// used to compute locally, `segment_base_of_ptr` being pure).
    ///
    /// Only compiled under fastbin (the only build with a magazine + the only
    /// consumer of `base` on this path).
    #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
    #[inline(always)]
    pub(super) fn dealloc_own_thread_with_base(
        &mut self,
        ptr: *mut u8,
        layout: Layout,
        base: *mut u8,
    ) {
        {
            use super::tcache::{FLUSH_N, TCACHE_CAP};
            use crate::alloc_core::size_classes::{SizeClasses, MIN_BLOCK};
            let size = layout.size().max(MIN_BLOCK);
            let align = layout.align();
            // C1 (0.3.0): gate removed — see the matching comment in `alloc`'s
            // magazine fast path above for the full soundness argument
            // (`class_for` guarantees `block_size % align == 0` for any
            // `Some(c)` it returns, so keying the magazine by class alone is
            // sound for any align it accepted, not just align<=16).
            {
                if let Some(c) = SizeClasses::class_for(size, align) {
                    let cnt = self.tcache.classes[c].count as usize;

                    // ── F7 (task #25): Large-segment kind guard (HARDENED) ──
                    // `class_for` returning `Some(c)` above keys the free on
                    // the *layout*, not on where `ptr` actually lives. If the
                    // caller frees a pointer that sits in a LARGE segment with
                    // a small layout (a GlobalAlloc-contract violation — the
                    // real UB is on the caller side), the M2 oracles below
                    // would read the "bitmap"/magazine state out of the bytes
                    // of the Large allocation's PAYLOAD — potentially routing
                    // that block into the magazine and later re-issuing it as a
                    // small block, aliasing the still-live Large block.
                    //
                    // The substrate path (`AllocCore::dealloc`) routes by
                    // segment `kind` FIRST and so degrades to a no-op on the
                    // same violation. This restores that symmetry: reject the
                    // Large-in-small-layout free as a no-op BEFORE the oracles.
                    // A single `kind_at(base)` header read (a table-free field
                    // load), gated behind `hardened` (default OFF) like the
                    // interior-pointer guard just below it.
                    #[cfg(feature = "hardened")]
                    {
                        if SegmentHeader::kind_at(base) == SegmentKind::Large {
                            return; // Large-segment free via small layout — no-op
                        }
                    }

                    // ── H1 (task #167): interior-pointer guard (HARDENED) ──
                    // A block start of class `c` always sits at a segment
                    // offset that is a whole multiple of `block_size(c)`
                    // (carve aligns the bump to `block_size`). An INTERIOR
                    // pointer (offset into a live block, not its start) has
                    // `off % block_size(c) != 0`. The M2 oracles below are
                    // BLIND to this: the alloc bitmap is indexed at
                    // `off >> MIN_BLOCK_SHIFT` (16 B granularity), so an
                    // interior offset that is still 16 B-aligned maps to a
                    // DIFFERENT bit that reads "allocated" → the bogus pointer
                    // falls through and is pushed into the magazine → a later
                    // alloc hands out a mid-block address → silent aliasing /
                    // corruption. This guard rejects it as a no-op.
                    //
                    // Cost: a `%` by a non-power-of-two `block_size` (a real
                    // division, ~tens of cycles) on EVERY small free — NOT
                    // free, so gated behind `hardened` (default OFF), never on
                    // the production hot path. `block_size(c)` is a table load.
                    #[cfg(feature = "hardened")]
                    {
                        let off_h = (ptr as usize).wrapping_sub(base as usize);
                        let bs = SizeClasses::block_size(c);
                        if !off_h.is_multiple_of(bs) {
                            return; // interior-pointer free — no-op
                        }
                    }

                    // ── M2 double-free guard (Э6, P6.1) ──────────────────
                    // The two exact oracles are consulted on every free (no
                    // block-body filter gates them), and the block body is never
                    // read or written on the free path. They are EXACT for the
                    // two own-thread resting places (this class's magazine + the
                    // BinTable free list); see the RESIDUAL M2 LIMIT note below
                    // for the cross-thread-double-free case (undrained
                    // RemoteFreeRing entry) they do NOT cover — task #164.
                    //
                    // The pre-Э6 design used a per-heap key stamped into the
                    // block's word1 (bytes 8..16) as a fast-path FILTER:
                    // `word1 != key` skipped the oracles and pushed directly.
                    // That filter cost a read+write of the BLOCK BODY on every
                    // push (a cold/conflict cache line at block stride — the
                    // 256 B churn regression), and — worse — it was UNSOUND
                    // under user writes: once the user wrote to bytes 8..16 of
                    // a block (legitimate use of allocated memory), a later
                    // double-free saw `word1 != key`, SKIPPED the oracles, and
                    // fell through to push → the block landed BOTH in the
                    // magazine AND on a BinTable free list → the same pointer
                    // issued twice.
                    //
                    // Э6 removes the filter and always runs the two exact
                    // oracles, in this exact order:
                    //
                    //   (1) in-magazine scan  — catches a block freed but not
                    //       yet flushed (still queued in `slots`). Bounded by
                    //       `cnt <= TCACHE_CAP` (16); in churn cnt is 1–3 and
                    //       the array is hot/L1.
                    //   (2) BinTable bitmap   — catches a block that was
                    //       flushed to a free list (`is_free(off)` set). The
                    //       bitmap line is shared by hundreds of blocks → hot.
                    //
                    // A genuinely live block is in neither → push. Order is
                    // load-bearing: scan FIRST (unflushed), bitmap SECOND
                    // (flushed); do NOT reorder.
                    //
                    // This STRENGTHENS M2 for the OWN-THREAD double-free: the
                    // pre-Э6 flushed-double-free hole (user overwrote word1 →
                    // stale/garbage key → oracles skipped → double-issue) is
                    // now closed unconditionally — the bitmap oracle no longer
                    // depends on the block body being pristine. That is a strict
                    // correctness improvement, not a trade, and it is EXACT for
                    // the two own-thread resting places a freed block can be in:
                    // (1) this class's magazine (the scan), and (2) the segment's
                    // BinTable free list (the bitmap). The magazine free path now
                    // touches no block body at all (mimalloc, by contrast, must
                    // write `next` into the body on every free — we are
                    // structurally cheaper per free on cold working sets).
                    //
                    // ── RESIDUAL M2 LIMIT (cross-thread double-free) — #164 NARROWED
                    // ─────────────────────────────────────────────────────────
                    // The two oracles are exact ONLY for those two resting
                    // places. They are BLIND to a third, transient one: a block
                    // whose CROSS-THREAD free is still in-flight — packed into
                    // its segment's `RemoteFreeRing` but NOT YET DRAINED by the
                    // owner.
                    //
                    // Task #164 NARROWED the window: ALL production drain paths
                    // now consult the magazine via `reclaim_offset_checked`'s
                    // `is_in_magazine` predicate (refill via `refill_magazine_slow`,
                    // realloc via `try_realloc_inplace` + `HeapCore::alloc`,
                    // debug via `dbg_drain_all_rings_checked`). A block that is
                    // simultaneously magazine-resident AND in the ring is detected
                    // and the ring entry is dropped (the magazine copy stays
                    // canonical). GREEN tests:
                    // `drain_resident_xthread_double_free_no_corruption`,
                    // `realloc_path_drain_respects_magazine`.
                    //
                    // Task R1 (retro C1, 2026-07-06) closed a SECOND leg the X2
                    // campaign missed: the refill-window in-out-buffer leg.
                    // `refill_class_bump_impl` pulls freelist blocks into the
                    // caller-owned `out[0..filled]` buffer BEFORE draining rings;
                    // the predicate's `if k == c { return false; }` shortcut
                    // (justified only by count[c]==0 borrow-safety) is blind to
                    // those magazine-destined blocks, so a stale ring note for a
                    // block already in `out` was reclaimed → relinked → re-pulled
                    // in the SAME refill call (P double-issued at consecutive
                    // positions). Fix: wrap the predicate with an out-membership
                    // guard (`is_in_magazine(ptr,k) || (k == c && out[..filled].contains(ptr))`).
                    // GREEN test: `refill_window_does_not_double_issue_in_out_buffer_resident_block`.
                    //
                    // REMAINING residual = re-issue-before-drain / delayed xfree
                    // (the THIRD leg): if the block was popped (re-issued to the
                    // user) before the drain runs, the state is information-
                    // theoretically identical to a genuine delayed cross-thread
                    // free (bitmap allocated, not in magazine, not in the refill
                    // out-buffer, (off,class)-only entry). Pinned RED by
                    // `residual_xthread_double_free_no_corruption` (#[ignore]d).
                    // Full fix: task X7 (hardened, generational ring entry — see
                    // RING_MAGAZINE_XTHREAD_DOUBLE_FREE_FIX.md §8.4).
                    //
                    // (1) in-magazine DF oracle — ALWAYS. RAD-5 (E4) GO/NO-GO
                    // EXPERIMENT: replaced the Э10 branchless chunked scan
                    // (which walked up to `cnt` <= TCACHE_CAP=16 magazine
                    // slots) with an O(1) probe of the second
                    // (magazine-residency) bitmap. `off`/`meta` are hoisted
                    // here (previously computed AFTER this oracle, for the
                    // flushed-DF oracle below) so both oracles share them.
                    // Semantics: exact replacement — a block is
                    // magazine-resident in the bitmap's view iff it is one of
                    // `{slots[c][i] : i < cnt}` in the old scan's view, by
                    // construction (mark on push, clear on pop/flush — see
                    // `magazine_bitmap.rs`'s module doc). See
                    // `docs/perf/IAI_BASELINE.md`'s RAD-5 entry for the
                    // measured verdict on whether this probe is actually
                    // cheaper than the scan it replaces.
                    let off = (ptr as usize - base as usize) as u32;
                    let meta = SegmentMeta::new(base);
                    if meta.magazine_bitmap().is_in_magazine(off) {
                        return; // in-magazine double-free — no-op
                    }
                    // (2) flushed DF oracle — ALWAYS. `base`/`off`/bitmap are
                    // read on a segment already PROVEN ours and mapped by
                    // `dealloc_routing`'s `contains_base` ownership check
                    // (fastbin ⇒ alloc-xthread structurally), exactly as
                    // before. Э9 (P7.1): `base` is the pre-computed argument
                    // (same value `segment_base_of_ptr` would return — pure),
                    // threaded in from `dealloc_routing` so it is computed
                    // once on the own-thread free path.
                    // Stale-free guard, parity with `dealloc_small`
                    // (alloc_core.rs). A block that was carved into a segment
                    // later decommitted+reset has `off >= bump` (bump was reset
                    // to small_meta_end and the bitmap zeroed = "allocated", so
                    // the bitmap oracle below would NOT catch it); likewise a
                    // never-carved in-segment address. A real, currently-carved
                    // live block always has `off < bump`, so no false positive
                    // on a legitimate free. Owner-only `bump` read
                    // (single-writer), gated to the feature that resets the
                    // bump — exactly as `dealloc_small`.
                    #[cfg(feature = "alloc-decommit")]
                    if (off as usize) >= meta.bump_of() {
                        return;
                    }
                    if meta.alloc_bitmap().is_free(off) {
                        return; // flushed-then-double-freed — no-op
                    }

                    if cnt < TCACHE_CAP {
                        // Legit free → push. NO key stamp, NO block-body write.
                        //
                        // RAD-5 (E4) GO/NO-GO EXPERIMENT: mark this block
                        // magazine-resident in the second bitmap. `meta`/`off`
                        // are already computed above for the M2 bitmap read —
                        // this reuses them, paying only the new bitmap's own
                        // read-modify-write. See `magazine_bitmap.rs`'s module
                        // doc; `docs/perf/IAI_BASELINE.md`'s RAD-5 entry has
                        // the measured verdict on whether this is worth it.
                        meta.magazine_bitmap().mark_magazine(off);
                        self.tcache.classes[c].slots[cnt] = ptr;
                        self.tcache.classes[c].count = (cnt + 1) as u8;
                        // R13-3 (task #273): a pushed-back block was
                        // previously issued (it is being FREED right now) —
                        // by the dispatch conjunct (§2 of both virgin-skip
                        // design docs) it is NEVER virgin, regardless of the
                        // bit any earlier occupant of physical slot `cnt`
                        // left behind. `PerClass::virgin_mask`'s own
                        // invariant ("bits >= count are 0") already
                        // guarantees bit `cnt` reads 0 here (it was `>=
                        // count` the instant before this push bumped
                        // `count`) — this is a defensive no-op AND, not a
                        // load-bearing clear, kept explicit so the invariant
                        // is visibly re-asserted at every mutation site
                        // rather than relying on readers to re-derive it.
                        #[cfg(feature = "virgin-zero-skip")]
                        {
                            self.tcache.classes[c].virgin_mask &= !(1u16 << cnt);
                        }
                        return;
                    }
                    // ── Magazine overflow (cnt == TCACHE_CAP) ──────────
                    // P3 (task #147): the P7 dealloc-side bulk-mode bypass is
                    // RETIRED together with the alloc-side bypass and the
                    // `alloc_streak` counter. That branch fired only when the
                    // alloc side had advanced the streak past BULK_THRESHOLD;
                    // with the counter gone it could never fire, so keeping it
                    // would be dead code guarded by a stuck-at-0 condition.
                    // The always-taken half-flush + compact + push below is the
                    // sole overflow policy now. D1/M2 unchanged: `flush_class`
                    // returns blocks to the substrate via `dealloc_small`
                    // (mark_free + dec_live) exactly as before.
                    //
                    // RAD-5: the FLUSH_N blocks about to be flushed leave the
                    // magazine — clear their bit BEFORE calling `flush_class`
                    // (mirrors "flush = clear_magazine + mark_free": the
                    // AllocBitmap side of `mark_free` happens inside
                    // `flush_class`/`flush_run`; this bitmap's clear happens
                    // here since `AllocCore` has no magazine concept).
                    for &flushed in &self.tcache.classes[c].slots[0..FLUSH_N] {
                        let fbase = os::segment_base_of_ptr(flushed);
                        let foff = (flushed as usize - fbase as usize) as u32;
                        SegmentMeta::new(fbase)
                            .magazine_bitmap()
                            .clear_magazine(foff);
                    }
                    // Normal overflow: half-flush, then push.
                    // SAFETY (R6-MS-3): every slot in `slots[0..FLUSH_N]` is a
                    // valid live small-class-`c` allocation owned by this core's
                    // magazine (just freed into it under the alloc-xthread/fastbin
                    // path); each is returned to the substrate exactly once here.
                    #[allow(unsafe_code)] // R6-MS-3: unsafe call into `AllocCore::flush_class`.
                    unsafe {
                        self.core
                            .flush_class(c, &self.tcache.classes[c].slots[0..FLUSH_N])
                    };
                    // Compact: shift entries [FLUSH_N..CAP] down to [0..CAP-FLUSH_N].
                    let remaining = TCACHE_CAP - FLUSH_N;
                    for i in 0..remaining {
                        self.tcache.classes[c].slots[i] = self.tcache.classes[c].slots[i + FLUSH_N];
                    }
                    // R13-3 (task #273): the virgin mask must undergo the
                    // IDENTICAL shift as `slots` — bit `i` of the compacted
                    // mask is bit `i + FLUSH_N` of the pre-compaction mask
                    // (byte-identical to the `slots[i] = slots[i + FLUSH_N]`
                    // loop just above, just on the bitmask instead of the
                    // pointer array). The flushed low half (`slots[0..FLUSH_N]`,
                    // now returned to the substrate via `flush_class`) is
                    // dropped entirely, not merely cleared-and-kept.
                    #[cfg(feature = "virgin-zero-skip")]
                    {
                        self.tcache.classes[c].virgin_mask >>= FLUSH_N;
                    }
                    // Push (Э6: NO key stamp, NO block-body write). The oracles
                    // above already ran before this overflow branch, so a
                    // double-free is caught even when the magazine is full.
                    // RAD-5: mark the newly-pushed block magazine-resident.
                    meta.magazine_bitmap().mark_magazine(off);
                    self.tcache.classes[c].slots[remaining] = ptr;
                    self.tcache.classes[c].count = (remaining + 1) as u8;
                    // R13-3: the newly-pushed block is a freed (previously
                    // issued) block — never virgin (dispatch conjunct), same
                    // reasoning as the non-overflow push arm above. Clear bit
                    // `remaining` explicitly: after the `>>= FLUSH_N` shift
                    // above, that bit holds whatever was at pre-shift index
                    // `remaining + FLUSH_N` (`== TCACHE_CAP`, i.e. always 0 by
                    // the mask invariant since no valid slot index reaches
                    // `TCACHE_CAP`) — so this is defensive, not load-bearing,
                    // matching the non-overflow arm's identical comment.
                    #[cfg(feature = "virgin-zero-skip")]
                    {
                        self.tcache.classes[c].virgin_mask &= !(1u16 << remaining);
                    }
                    return;
                }
            }
        }
        // Large / non-small / non-fastbin: delegate to core.
        // SAFETY: `dealloc_own_thread_with_base` is reached only from
        // `HeapCore::dealloc` (own-thread or routing path) with a caller-bound
        // `ptr`/`layout` honouring the `GlobalAlloc::dealloc` contract; the
        // magazine oracles above already returned for the small fastbin case,
        // and we forward the same pair to the substrate.
        #[allow(unsafe_code)] // R6-MS-1/2: unsafe call into `AllocCore::dealloc`.
        unsafe {
            self.core.dealloc(ptr, layout)
        };
    }

    /// Shrink/grow an allocation. Returns null on OOM (leaving the old
    /// allocation intact). Null `ptr` returns null.
    ///
    /// ## Own-segment pointers (task C2 + #164)
    ///
    /// If `ptr` lives in one of THIS heap's segments (`contains_base` — the
    /// same O(1) ownership test `dbg_owner_id_for` uses), the resize takes the
    /// magazine-aware fast path:
    ///
    ///   1. **A1 deferred-large drain** (`alloc-xthread` only): BEFORE the
    ///      in-place attempt, if the NEW size classifies as Large
    ///      (`class_for(...).is_none()`), drain this heap's deferred-free
    ///      stack (MUST-1/A1 — a realloc-growth-only thread still reclaims
    ///      cross-thread-freed large segments; otherwise its stack
    ///      accumulates unboundedly). This drain is load-bearing — it runs
    ///      whether or not the in-place path succeeds.
    ///   2. **In-place attempt**: call `AllocCore::try_realloc_inplace_known_base`, which
    ///      applies the OPT-F (Small same-class) and OPT-G (Large grow-in-span)
    ///      short-circuits. On success it returns the SAME `ptr` (mutating the
    ///      block's header in place, never moving it) — we return immediately.
    ///   3. **Move leg**: on in-place failure, build the new `Layout`, call
    ///      `HeapCore::alloc` (magazine-aware — drains via the checked
    ///      predicate and stamps per #169), copy `min(old, new)` bytes, then
    ///      `HeapCore::dealloc` the old pointer.
    ///
    /// The move leg routes through `HeapCore::alloc`/`HeapCore::dealloc`
    /// (NOT `AllocCore::realloc`'s internal alloc+copy+dealloc) so that the
    /// two ownership hooks `HeapCore::alloc` applies — segment-ownership
    /// stamping (`stamp_segment_owner`, which under `alloc-xthread` also
    /// writes `owner_thread_free`, the field that makes a remote free route
    /// back here instead of leaking) and the checked drain — fire on the
    /// freshly allocated block. Without them a Vec grown via realloc on
    /// thread A would live in an UNSTAMPED Large segment; when A hands it
    /// to thread B and B drops it, `dealloc_routing` sees not-ours +
    /// magic OK + `owner_tf == null` → silent no-op → the whole segment
    /// (4+ MiB) and its `SegmentTable` slot leak forever (the resurrected
    /// A1/#114 leak-to-abort).
    ///
    /// ## Foreign pointers
    ///
    /// A `ptr` we do NOT own (e.g. under `alloc-xthread`, a block that lives
    /// in ANOTHER heap's segment) takes the foreign leg. R2-1: before copying,
    /// the leg now validates that `ptr` resolves to a LIVE sefer segment
    /// (segment-header magic check, mirroring `dealloc_foreign_slow`'s first
    /// guard) AND that `old_layout.size()` does not exceed that segment's
    /// committed span. A bogus/foreign pointer (stack, foreign allocator,
    /// dangling) or an oversized claim is rejected (null) BEFORE any copy —
    /// never read out of bounds. A legitimate cross-heap sefer pointer passes
    /// both checks, copies `min(old, new)`, then frees the OLD pointer via
    /// `self.dealloc` (which routes cross-thread correctly under
    /// `alloc-xthread`). Without `alloc-xthread` there is no legitimate
    /// cross-heap owner, so the foreign leg returns null outright (symmetric
    /// with `AllocCore::realloc`'s foreign-pointer null and `dealloc`'s
    /// foreign no-op).
    ///
    /// This is an **`unsafe fn`** (R6-MS-1/2): the move legs' `copy_nonoverlapping`
    /// read out of `ptr` trusts the caller's `old_layout`/`ptr` exactly as
    /// `GlobalAlloc::realloc` does. Reversed from the former safe `pub fn`
    /// posture after the round5 review — see [`AllocCore::realloc`]'s
    /// `# Safety` and `CHANGELOG.md` (R6-MS-1/2).
    ///
    /// # Safety
    ///
    /// The caller must uphold the [`GlobalAlloc::realloc`] contract for `ptr`
    /// and `old_layout`:
    ///
    /// - `ptr` is **null** OR the exact **start** pointer of a currently-LIVE
    ///   allocation (own segment, or — under `alloc-xthread` — a live segment
    ///   owned by another heap in the same process). It MUST NOT be an
    ///   interior pointer.
    /// - `old_layout` exactly matches the allocation's layout; its `.size()`
    ///   must not exceed the block's true size (the move legs copy that many
    ///   bytes out of `ptr`).
    /// - On success (`!null` return) the OLD `ptr` is freed; it MUST NOT be
    ///   used or re-freed afterwards. On null return `ptr` is left intact.
    /// - `ptr` is not a foreign / already-released-unmapped base.
    ///
    /// Null `ptr` is always safe (early return).
    #[allow(unsafe_code)] // R6-MS-1/2: `unsafe fn` boundary (caller-pointer contract).
    pub unsafe fn realloc(&mut self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        if ptr.is_null() {
            return core::ptr::null_mut();
        }
        #[cfg(feature = "alloc-global")]
        {
            let base = os::segment_base_of_ptr(ptr);
            // Task #135 (Part 2): O(1) membership test (`AllocCore::contains_base`
            // → the OPT-B hash table) replaces the O(segment count) linear scan
            // `segment_bases().any(|b| b == base)`. Same semantics: `true` iff
            // `base` is one of THIS heap's registered, live segments.
            if self.core.contains_base(base) {
                // Own-segment pointer. The resize proceeds in up to three
                // phases (see the doc comment above): A1 Large drain, in-place
                // attempt, then move leg — all funnelled through the
                // magazine-aware `HeapCore::alloc`/`dealloc` (NOT
                // `AllocCore::realloc`'s internal alloc, which bypasses the
                // ownership hooks).
                //
                // MUST-1 (0.3.0, C2 regression fix): the in-place attempt and
                // any move-leg alloc may carve a FRESH segment. That substrate
                // alloc does NOT run the two ownership hooks `HeapCore::alloc`
                // applies — segment-ownership stamping (`stamp_segment_owner`,
                // which under `alloc-xthread` also writes `owner_thread_free`,
                // the field that makes a remote free route back here instead
                // of leaking) and the A1 deferred-large drain
                // (`drain_large_deferred_free`). Without them, a Vec grown via
                // realloc on thread A lives in an UNSTAMPED
                // (`owner_thread_free == null`) Large segment; when A hands it
                // to thread B and B drops it, `dealloc_routing` sees not-ours
                // + magic OK + `owner_tf == null` → silent no-op → the whole
                // segment (4+ MiB) and its `SegmentTable` slot leak forever
                // (the resurrected A1/#114 leak-to-abort).
                //
                //   (1) A1 Large drain — BEFORE the in-place attempt, if the
                //       NEW request classifies as Large
                //       (`class_for(...).is_none()`, the exact predicate
                //       `alloc` uses), drain this heap's deferred-free stack
                //       so a realloc-growth-only thread still reclaims
                //       cross-thread-freed large segments (otherwise its stack
                //       accumulates unboundedly — the A1 drain-bypass leg of
                //       the bug). This drain is load-bearing and runs
                //       regardless of whether the in-place path then succeeds.
                #[cfg(feature = "alloc-xthread")]
                {
                    let class = crate::alloc_core::size_classes::SizeClasses::class_for(
                        new_size.max(crate::alloc_core::size_classes::MIN_BLOCK),
                        old_layout.align(),
                    );
                    if class.is_none() {
                        self.drain_large_deferred_free();
                    }
                }
                //   (2) In-place attempt — try OPT-F (Small same-class) and
                //       OPT-G (Large grow-in-span) via the substrate. On
                //       success the block's header is mutated IN PLACE and
                //       the SAME `ptr` is returned (no alloc leg, hence no
                //       alloc-leg drain — the A1 drain above already covered
                //       the Large case). On failure fall through to the move
                //       leg, which routes through `HeapCore::alloc`
                //       (magazine-aware, checked drain) — NOT through
                //       `AllocCore::realloc`'s blind alloc→alloc_small path.
                if let Some(p) = self
                    .core
                    .try_realloc_inplace_known_base(base, ptr, old_layout, new_size)
                {
                    // `try_realloc_inplace_known_base` mutates the block's header in
                    // place and always returns the SAME pointer on success
                    // (it never moves the block). The segment was already
                    // stamped when first allocated, so there is nothing to
                    // re-stamp here.
                    debug_assert_eq!(p, ptr, "known-base realloc must return the same pointer");
                    return p;
                }
                //   (2.5) Small/medium->Large promotion (R14-4, task #289,
                //       Stage 2 of `docs/perf/
                //       R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md`):
                //       ONLY compiled under `medium-classes` (the promotion
                //       only makes sense for medium-classified blocks — under
                //       plain `production` without `medium-classes`, every
                //       size in the medium range already routes Large, so
                //       there is nothing to promote FROM). Diverts a GROWING
                //       realloc of a currently-Small/medium-classified block,
                //       once `new_size` crosses `MEDIUM_REALLOC_PROMOTION_THRESHOLD`,
                //       directly to a Large allocation instead of walking the
                //       medium ladder one class at a time — turning N
                //       ladder-crossing move-legs into 1 promotion copy, with
                //       every SUBSEQUENT grow riding the existing OPT-G
                //       Large-grow-in-span fast path for free. See
                //       `try_promote_to_large`'s doc for the mechanism and why
                //       no new bookkeeping is needed.
                //
                //       R15-3 (task #305, review finding P2-3): the `#[cfg]`
                //       here is narrower than bare `medium-classes` — this is
                //       a fix, not a widened feature gate for its own sake.
                //       `exact-span-large` (opt-in, orthogonal) sizes a fresh
                //       Large segment's committed span to EXACTLY the padded
                //       request (see that feature's own doc comment), so a
                //       promoted block under `exact-span-large` gets ZERO
                //       spare headroom to grow into — the very next grow can
                //       never fit OPT-G's in-place check and must always take
                //       a move leg, even for small subsequent steps that
                //       would have stayed in-place on the medium ladder via
                //       OPT-F (small same-class carve) had promotion never
                //       happened. That is a net pessimization of exactly this
                //       combination, not a win — it already forced two
                //       correctness-hiding test weakenings (task #302 and its
                //       follow-up, `9b59990`) instead of a real fix. Plain
                //       `production` (without `exact-span-large`) is
                //       unaffected: Large there always rounds up to a whole
                //       `SEGMENT` (4 MiB), so headroom exists automatically —
                //       see `try_promote_to_large`'s own doc, "Pad-target
                //       decision". `large-reserved-capacity` restores
                //       headroom on top of `exact-span-large` UNLESS
                //       `numa-aware` is also on, in which case
                //       `alloc_core_large.rs`'s eager NUMA reservation arm
                //       takes over with `reserved_capacity == usable` (no
                //       slack) regardless of `large-reserved-capacity` — see
                //       that feature's own doc comment in
                //       `src/alloc_core/alloc_core_large.rs`. So promotion is
                //       compiled in only when it can't structurally regress:
                //       either there's no `exact-span-large` tightness to
                //       begin with, or `large-reserved-capacity` is present
                //       AND `numa-aware` is not overriding it. When this
                //       `#[cfg]` compiles OUT (zero-headroom
                //       `exact-span-large`), growth simply falls through to
                //       the existing move leg below, which for
                //       `new_size < SMALL_MAX` (1 MiB under `medium-classes`)
                //       already does a single carve+copy up the medium
                //       ladder — identical in shape to what plain
                //       `production` without `medium-classes` has always
                //       done; no functionality is lost, only a
                //       counterproductive promotion in this one triple
                //       combination.
                #[cfg(all(
                    feature = "medium-classes",
                    any(
                        not(feature = "exact-span-large"),
                        all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
                    )
                ))]
                if new_size > old_layout.size()
                    && new_size >= MEDIUM_REALLOC_PROMOTION_THRESHOLD
                    && crate::alloc_core::size_classes::SizeClasses::class_for(
                        old_layout
                            .size()
                            .max(crate::alloc_core::size_classes::MIN_BLOCK),
                        old_layout.align(),
                    )
                    .is_some()
                {
                    if let Some(p) = self.try_promote_to_large(base, ptr, old_layout, new_size) {
                        return p;
                    }
                }
                //   (3) Move leg — in-place did not apply: alloc a fresh block
                //       through `HeapCore::alloc` (magazine-aware: drains via
                //       the checked predicate + stamps per #169), copy the
                //       preserved prefix, then `HeapCore::dealloc` the old
                //       pointer (own-segment → routes through `core.dealloc`).
                //
                //       R2-1 (soundness): bound the move leg's read by the
                //       block's actual committed span, not the caller-supplied
                //       `old_layout.size()`. This is a SAFE `pub fn`; a bogus
                //       layout (e.g. 8 MiB for a 16-byte block) must not drive
                //       an OOB read. `base` was proven live above by
                //       `contains_base`. The write side is always safe (`copy
                //       <= new_size`); the read is bounded here.
                if old_layout.size() > AllocCore::safe_payload_read_span(base, ptr) {
                    return core::ptr::null_mut();
                }
                let new_layout = match Layout::from_size_align(new_size, old_layout.align()) {
                    Ok(l) => l,
                    Err(_) => return core::ptr::null_mut(),
                };
                let new_ptr = self.alloc(new_layout);
                if new_ptr.is_null() {
                    return core::ptr::null_mut();
                }
                let copy = old_layout.size().min(new_size);
                crate::alloc_core::node::Node::copy_nonoverlapping(ptr, new_ptr, copy);
                // SAFETY: own-segment move leg — `contains_base(base)` proved
                // `ptr`'s segment is ours & live, the read was bounded by
                // `safe_payload_read_span`, and `new_ptr` holds the copied
                // prefix; freeing the old block once completes the
                // contract-honouring realloc.
                unsafe { self.dealloc(ptr, old_layout) };
                return new_ptr;
            }
        }
        // Foreign pointer (not one of our segments). Before copying from it,
        // the pointer MUST resolve to a live sefer segment of sufficient
        // committed span; otherwise a safe caller passing a bogus/foreign
        // pointer triggers an out-of-bounds read (R2-1, gap 1).
        //
        // Under `alloc-xthread` this leg is the deliberately-designed
        // cross-heap path (a pointer from ANOTHER live heap is legitimate,
        // and `self.dealloc` routes its free cross-thread). The membership
        // barrier is the segment-header magic check (mirrors
        // `dealloc_foreign_slow`'s first guard): a pointer whose computed
        // base is not a live sefer segment — stack, foreign allocator,
        // dangling — is rejected (null) before any copy. A REAL cross-heap
        // sefer segment passes magic, then the same R2-1 span bound as the
        // own-seg leg applies.
        //
        // Without `alloc-xthread` there is no cross-thread routing and thus
        // no legitimate owner for a pointer this heap does not recognise:
        // copying from it would read arbitrary caller-supplied memory under
        // a safe fn. Return null, `ptr` untouched — symmetric with
        // `AllocCore::realloc`'s foreign-pointer null and `dealloc`'s foreign
        // no-op.
        #[cfg(feature = "alloc-xthread")]
        {
            let base = os::segment_base_of_ptr(ptr);
            // R4-2 (memory_safety_review, R4-MS-1/MS-2): guard the degenerate
            // base BEFORE the raw `magic_at` read. `segment_base_of_ptr` masks
            // the address down to the SEGMENT boundary; a garbage pointer like
            // `1 as *mut u8` masks to `base == 0` (null), and `magic_at(0)`
            // would then dereference address `offset_of!(SegmentHeader, magic)`
            // with no guard — an immediate read of a structurally-impossible
            // "segment". Reject null (and anything that masks to null) as a
            // foreign pointer. This does NOT attempt cross-heap staleness
            // detection (case (a) vs (b) in `dealloc_foreign_slow`); it closes
            // only the narrower class where `base` cannot be a real segment by
            // construction.
            if base.is_null() {
                return core::ptr::null_mut();
            }
            if SegmentHeader::magic_at(base) != SEGMENT_MAGIC {
                return core::ptr::null_mut();
            }
            if old_layout.size() > AllocCore::safe_payload_read_span(base, ptr) {
                return core::ptr::null_mut();
            }
            let new_layout = match Layout::from_size_align(new_size, old_layout.align()) {
                Ok(l) => l,
                Err(_) => return core::ptr::null_mut(),
            };
            let new_ptr = self.alloc(new_layout);
            if new_ptr.is_null() {
                return core::ptr::null_mut();
            }
            let copy = old_layout.size().min(new_size);
            Node::copy_nonoverlapping(ptr, new_ptr, copy);
            // SAFETY: foreign move leg (alloc-xthread) — `ptr`'s base passed
            // the segment-header magic check (live sefer segment) and the read
            // was bounded by `safe_payload_read_span`; `new_ptr` holds the
            // copied prefix, and `self.dealloc` routes the old block's free
            // cross-thread. Freeing once completes the contract-honouring
            // realloc.
            unsafe { self.dealloc(ptr, old_layout) };
            new_ptr
        }
        #[cfg(not(feature = "alloc-xthread"))]
        {
            let _ = new_size;
            core::ptr::null_mut()
        }
    }

    /// R14-4 (task #289), Stage 2 of `docs/perf/
    /// R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md`: attempt to promote a
    /// currently-Small/medium-classified, own-segment block directly to a
    /// Large allocation, instead of letting the caller's growing `realloc`
    /// fall through to the ladder-walk move leg. Called from `realloc`'s
    /// own-segment branch, between the in-place attempt (OPT-F/OPT-G) and the
    /// unconditional move leg, only when `medium-classes` is compiled in, the
    /// resize is a GROW, `old_layout` currently classifies Small, and
    /// `new_size >= MEDIUM_REALLOC_PROMOTION_THRESHOLD` (see the call site
    /// and the constant's own doc).
    ///
    /// Returns `Some(new_ptr)` on success (the old block has already been
    /// freed); `None` on OOM, in which case the OLD block is left completely
    /// intact and the caller falls through to the existing move leg (which
    /// will itself likely also OOM on the same request, but this function
    /// makes no such assumption — it simply declines to promote and lets the
    /// existing, already-correct move leg have the final say).
    ///
    /// ## Why no new bookkeeping is needed (the design doc's §4.2 answer,
    /// exercised for real here)
    ///
    /// A promoted block is not a hybrid — it becomes a GENUINE, ordinary
    /// Large-segment allocation the moment `AllocCore::alloc_large` returns
    /// it. `SegmentHeader::kind_at(base)` (the SAME mechanism every other Large
    /// block's `dealloc`/shrink-realloc already uses to decide routing) reads
    /// `Large` for this segment exactly as it would for any other Large
    /// allocation — `dealloc` and `realloc` route purely off
    /// `SegmentHeader::kind_at(base)`, never off the caller-supplied
    /// `Layout`'s size (see the OPT-G doc comment on
    /// `AllocCore::try_realloc_inplace_known_base` in `alloc_core.rs`). So a
    /// LATER shrink back below the medium range takes the ordinary
    /// Large-to-Small move-leg path (this function adds no in-place
    /// Large->Small shrink fast path — matching the design doc's explicit
    /// non-goal), and a later dealloc frees it as an ordinary Large segment.
    /// No new tag, no new field, no new invariant.
    ///
    /// ## Pad-target decision (resolves the design doc's §4.4 open question)
    ///
    /// The padded target is simply `new_size` — **no artificial padding**
    /// beyond what the caller asked for. Measured via
    /// `examples/r14_4_pad_target_probe.rs` (a fixed-2-MiB pad vs a
    /// `max(new_size, threshold * 2)` floor vs plain `new_size`, all at the
    /// 256 KiB threshold): under the `production` feature bundle (which does
    /// NOT include `exact-span-large`), `AllocCore::alloc_large` already
    /// rounds every request up to a whole `SEGMENT` (4 MiB) multiple
    /// regardless of the logical size requested
    /// (`src/alloc_core/alloc_core_large.rs`) — so any pad target at or below
    /// one `SEGMENT` is moot (rounded up anyway) and buys no extra headroom a
    /// bare `new_size` doesn't already get for free. The probe confirmed this
    /// empirically: `nopad` (plain `new_size`) and a `512 KiB` floor produced
    /// statistically indistinguishable commit (~30 MiB steady-state for an
    /// 8-object working set) and wall-clock, while a fixed 2 MiB pad cost
    /// MORE committed memory for the SAME working set (large-cache admission
    /// stopped reusing a single segment size across rounds once the promoted
    /// request size no longer matched the eventual grown size) for a
    /// wall-clock win that came from a workload-shape artifact (this
    /// particular probe sequence never grows past 2 MiB, so the padded arm
    /// happens to need zero further reallocs) rather than from the padding
    /// itself. Padding is therefore not worth its own RSS cost as a blanket
    /// default; a caller whose growth pattern would benefit from headroom
    /// beyond one `SEGMENT` is exactly what the opt-in `large-reserved-capacity`
    /// feature already exists to provide (via `AllocCore::alloc_large`'s own
    /// `reserved_capacity` mechanism), and that feature's benefit is
    /// orthogonal to this promotion — it does not need a second, independent
    /// padding layer stacked on top here.
    ///
    /// ## R15-3 (task #305, review finding P2-3): the narrower `#[cfg]`
    ///
    /// This function is gated the SAME extended predicate as its one call
    /// site in `realloc` above, not bare `medium-classes` — see that call
    /// site's doc comment for the full root-cause explanation (the
    /// `exact-span-large` zero-headroom interaction that made every
    /// post-promotion grow take a move leg instead of OPT-G, a
    /// pessimization of exactly the `medium-classes` + `exact-span-large`
    /// combination that had twice forced a test-assertion weakening instead
    /// of a real fix, tasks #302 and its follow-up `9b59990`). Excluding
    /// this function entirely from the zero-headroom build (rather than
    /// keeping it compiled but simply never calling it) is deliberate:
    /// dead-but-compiled code inviting a future call site to reintroduce the
    /// same hazard is exactly the kind of drift a `#[cfg]` at the
    /// definition, matching the call site 1:1, forecloses at compile time.
    #[cfg(all(
        feature = "medium-classes",
        any(
            not(feature = "exact-span-large"),
            all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
        )
    ))]
    fn try_promote_to_large(
        &mut self,
        base: *mut u8,
        ptr: *mut u8,
        old_layout: Layout,
        new_size: usize,
    ) -> Option<*mut u8> {
        // R2-1 parity with the move leg just below: bound the read by the
        // block's actual committed span, not the caller-supplied
        // `old_layout.size()` — a bogus layout must not drive an OOB read.
        // `base` was already proven live by the caller's `contains_base`
        // check.
        if old_layout.size() > AllocCore::safe_payload_read_span(base, ptr) {
            return None;
        }
        // Pad target = `new_size` (no artificial padding beyond the caller's
        // request) — see this function's doc comment for the measured
        // reasoning. `old_layout.align()` is preserved so the promoted
        // block's alignment obligation is unchanged.
        //
        // CANNOT route through the plain `self.alloc(promoted_layout)` entry
        // point here: `medium-classes`' `SMALL_MAX` is 1 MiB, strictly ABOVE
        // `MEDIUM_REALLOC_PROMOTION_THRESHOLD` (256 KiB) — so a
        // `promoted_layout` sized to a THRESHOLD-crossing-but-still-under-1-MiB
        // `new_size` would classify right back into a (larger) medium class
        // under ordinary `class_for` rules, defeating the entire point of
        // promoting to Large. The promotion must FORCE Large classification
        // regardless of where `new_size` falls in the medium range — so this
        // calls `AllocCore::alloc_large` directly (bypassing `class_for`
        // entirely, exactly as the design doc's §4.1 sketch specifies), then
        // replicates the SAME ownership-hook bookkeeping `HeapCore::alloc`'s
        // Large branch performs, mirroring `HeapCore::alloc_zeroed`'s
        // (`heap_core_alloc.rs`) own Large branch line for line: the A1
        // deferred-large drain (`alloc-xthread`) and the `HeapOverflow` drain
        // (`alloc-xthread` without `fastbin`) BEFORE the call, then
        // `stamp_segment_owner` on the result — WITHOUT this, a Vec grown via
        // promotion on thread A would live in an UNSTAMPED Large segment and
        // leak forever when thread B frees it (the A1/#114 leak-to-abort
        // hazard this file's `realloc` doc comment warns about at length).
        #[cfg(feature = "alloc-xthread")]
        {
            self.drain_large_deferred_free();
        }
        #[cfg(all(feature = "alloc-xthread", not(feature = "fastbin")))]
        {
            self.drain_heap_overflow();
        }
        let (new_ptr, _is_fresh) = self.core.alloc_large(new_size, old_layout.align());
        if new_ptr.is_null() {
            return None;
        }
        self.stamp_segment_owner(new_ptr);
        // Copy the FULL old buffer — same `old_layout.size()` span the
        // existing move leg copies on a grow (`copy = min(old, new)`, which
        // for a grow is always `old`).
        Node::copy_nonoverlapping(ptr, new_ptr, old_layout.size());
        // SAFETY: `base` was proven ours & live by the caller's
        // `contains_base(base)` check before `try_promote_to_large` was
        // called; the read above was bounded by `safe_payload_read_span`;
        // `new_ptr` now holds the copied prefix. Freeing the old block once
        // completes the contract-honouring promotion, mirroring the existing
        // move leg's identical closing step.
        #[allow(unsafe_code)] // R6-MS-1/2: unsafe call into `HeapCore::dealloc`.
        unsafe {
            self.dealloc(ptr, old_layout)
        };
        Some(new_ptr)
    }
}
