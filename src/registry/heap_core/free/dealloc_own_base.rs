//! Own-thread fastbin free body for [`HeapCore`] (mechanical split of the
//! former flat `heap_core_free.rs`, itself a split of `heap_core.rs`,
//! task R6-CQ-7b): the single `dealloc_own_thread_with_base` method — the
//! Э9 (P7.1, task #160) variant of the own-thread dealloc taking the
//! pre-computed segment `base`, whose magazine/bitmap oracles all operate
//! on it. Compiled only under `alloc-global` + `fastbin` (the only build
//! with a magazine and the only consumer of `base` on this path), hence the
//! module gate on the declaration in `free/mod.rs`.

use core::alloc::Layout;

#[cfg(feature = "alloc-global")]
use crate::alloc_core::os;
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
use crate::alloc_core::segment_header::SegmentMeta;
#[cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    // R18-3: `SegmentKind` and `SegmentHeader::kind_at` are used ONLY in
    // branch (A) (promotion predicate) and branch (B) (`hardened &&
    // !promotion`) of the Large-kind routing below, so this import must
    // compile out exactly when both do, or it becomes an unused-import
    // warning. The union simplifies to `promotion || hardened`
    // (absorption: a || (b && !a) == a || b). Under plain `production`
    // neither term is true → import compiles out.
    //
    // R19-8 (task #344): the inner `all(medium-classes, any(...))` term here
    // is the SAME promotion-reachable predicate canonicalized into the
    // `medium_promotion_reachable!` macro (defined in `free/dealloc.rs`) —
    // it must stay hand-written and in sync by inspection, because
    // `#[cfg(...)]` cannot accept a macro invocation as its argument (this
    // term is COMBINED with `hardened` via `any(...)`, not the bare
    // predicate the macro wraps).
    any(
        feature = "hardened",
        all(
            feature = "medium-classes",
            any(
                not(feature = "exact-span-large"),
                all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
            )
        )
    )
))]
use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind};

use crate::registry::heap_core::HeapCore;

/// Task #2002 (registry review): outcome of [`small_free_guard`], the guard
/// chain shared verbatim between [`HeapCore::dealloc_own_thread_with_base`]
/// and [`HeapCore::dealloc_batch_small`] (`free/dealloc_batch.rs`). Before
/// this task the two functions carried independent, hand-copied bodies of
/// this exact guard chain — `dealloc_batch_small`'s own copy was MISSING the
/// F7 branch (A) promoted-Large-block routing entirely (it only had branch
/// B's simpler `hardened`-only no-op), a real, reachable correctness gap
/// under `medium-classes` promotion without `hardened` (a legitimate
/// promoted-and-grown Large block freed via `dealloc_batch` would fall
/// straight into the M2 oracles below and read/write the Large block's own
/// PAYLOAD bytes as bitmap state, exactly the corruption F7 exists to
/// prevent) — closed by this extraction, not a separate fix.
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
pub(super) enum SmallFreeGuard {
    /// All guards passed. `off` is the block's segment-relative offset,
    /// already computed here so the caller does not re-derive it for the
    /// magazine push that follows.
    Accept { off: u32 },
    /// Rejected as a benign no-op (double-free / interior-pointer /
    /// hardened Large-kind contract violation) — already fully handled; the
    /// caller does nothing further for this pointer.
    RejectNoOp,
    /// F7 branch (A): `ptr` is a LEGITIMATE promoted-and-grown Large block
    /// (`medium-classes` promotion). The caller MUST route `ptr`/`layout`
    /// to the REAL substrate free (`self.core.dealloc` / the scalar
    /// `self.dealloc` fallback) — this is a correctness requirement, not a
    /// defensive guard; treating it as a no-op leaks the segment (see this
    /// module's doc comment history, F7/R14-4/R17-4/R18-3/R19-1).
    ///
    /// `#[cfg_attr]`: only ever constructed inside
    /// `medium_promotion_reachable!`'s gated block, so under a build where
    /// that predicate is false (e.g. `--all-features`, where `numa-aware`
    /// defeats `large-reserved-capacity`) this variant is legitimately
    /// unconstructed — same negated predicate F7 branch (B) below already
    /// spells out by hand.
    #[cfg_attr(
        not(all(
            feature = "medium-classes",
            any(
                not(feature = "exact-span-large"),
                all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
            )
        )),
        allow(dead_code)
    )]
    RouteToLargeFree,
}

/// Task #2002: the shared F7/H1/M2 guard chain both
/// [`HeapCore::dealloc_own_thread_with_base`] and
/// [`HeapCore::dealloc_batch_small`] apply to a small-classified free BEFORE
/// it may be pushed into the magazine. Self-less (no `&self`/`&mut self`
/// needed — every check reads only `base`'s own segment metadata, the
/// caller-supplied `layout`, or the process-global `HARDENED_LARGE_NOOP_COUNT`
/// stat), so both callers — one taking `&mut self`, one operating inside a
/// batch loop — can call it identically.
///
/// Guard-by-guard, in the SAME load-bearing order the two call sites
/// previously duplicated by hand (do NOT reorder — each guard's own comment
/// in the function body states why):
///
/// 1. **F7 branch (A)** (task #25; R17-4/task #321; R18-3/task #330;
///    `medium_promotion_reachable!`-gated): `class_for` classifies a free on
///    the caller's *layout*, not on where `ptr` actually lives — under
///    `medium-classes`, R14-4's medium→Large promotion (task #289) diverts a
///    medium block to a dedicated Large segment at the 256 KiB threshold,
///    then OPT-G grows it IN PLACE up to `SMALL_MAX` while it stays a Large
///    segment; its dealloc layout (the post-grow size) classifies small even
///    though the block lives in a Large segment. So THIS routing to the real
///    substrate free is a CORRECTNESS REQUIREMENT, not a defensive guard:
///    without it the free falls into the M2 oracles below and reads/writes
///    bitmap state out of the Large allocation's own PAYLOAD bytes — the
///    segment is also never released (leaks; every subsequent promotion
///    reserves a fresh one, the R14-4 gate §2.2 anomaly). Under `hardened`,
///    R19-1 (task #337) additionally verifies the caller's layout is
///    consistent with the segment's CURRENT `large_size`/`large_align`
///    (`large_layout_consistent`, the same primitive
///    `heap_core_xthread::dealloc_routing` uses) before trusting this as the
///    legitimate case — a mismatch degrades to the SAME counted no-op branch
///    (B) uses, instead of really freeing a pointer a contract-violating
///    caller fabricated. Non-hardened builds make no such defence promise
///    and skip the consistency check (unmeasured perf/behavior change risk
///    on the production hot path). The `cfg!(feature = "hardened")` term
///    inside the `if` additionally widens the check from "size ≥ promotion
///    threshold" (non-hardened: a legit Large-via-small-layout requires
///    that) to unconditional under `hardened` (a contract violation can use
///    ANY small layout) — verified RED without this widening by
///    `regression_hardened_large_kind_own_free` (2 MiB Large freed with a
///    64-byte layout aliased via the magazine when the size gate skipped
///    `kind_at`).
/// 2. **F7 branch (B)** (`hardened && !promotion-reachable`, R18-3 task
///    #330): when promotion is compiled OUT, a legitimate
///    Large-with-small-layout is structurally unreachable, but the
///    ILLEGITIMATE case (a `GlobalAlloc`-contract violation) is still
///    reachable — any Large-segment hit under a small-classified `layout`
///    is a counted no-op. Mutually exclusive with branch (A) by
///    construction (branch (A) requires promotion ON; branch (B) requires
///    promotion OFF) — together they cover every `hardened` build and every
///    promotion build; under plain `production` (neither `hardened` nor
///    promotion) NEITHER branch compiles: no `kind_at` load, no cost.
///    R22-12 (task #363) shares ONE counter (`HARDENED_LARGE_NOOP_COUNT`)
///    between both branches' rejection paths.
/// 3. **H1** (task #167, `hardened`): a block start of class `c` always
///    sits at a segment offset that is a whole multiple of `block_size(c)`
///    (carve aligns the bump to `block_size`) — an INTERIOR pointer
///    (`off % block_size(c) != 0`) is BLIND to the M2 oracles below (the
///    alloc bitmap is indexed at 16 B granularity, so an interior offset
///    that is still 16 B-aligned can map to a DIFFERENT bit that reads
///    "allocated", falling through to a push that later hands out a
///    mid-block address — silent aliasing). Rejected as a no-op. Costs a
///    real `%` by a non-power-of-two divisor (~tens of cycles) on every
///    small free, so `hardened`-gated (never on the production hot path).
/// 4. **M2 oracle 1 — in-magazine double-free** (Э6, P6.1; always): an O(1)
///    probe of the magazine-residency bitmap (RAD-5/E4), replacing the
///    pre-Э6 per-heap-key-stamped-into-the-block-body filter design, which
///    was UNSOUND under user writes (a user write to the stamped bytes made
///    a later double-free skip the oracle and double-issue the pointer).
///    `off`/`meta` computed here are reused by oracle 2 below.
/// 5. **M2 oracle 1.5** (`alloc-decommit`): stale-free (`off >= bump`) — a
///    block carved into a segment later decommitted+reset has `off >= bump`
///    (the reset zeroed the alloc bitmap, so oracle 2 below would NOT catch
///    it); parity with `dealloc_small` (`alloc_core.rs`).
/// 6. **M2 oracle 2 — flushed-then-double-freed** (always): the BinTable
///    alloc-bitmap `is_free` check, catching a block already flushed to a
///    free list. Order 4-then-6 is load-bearing (unflushed resting place
///    first, flushed resting place second) — do NOT reorder.
///
/// **RESIDUAL M2 LIMIT (cross-thread double-free, task #164/X7) — NOT
/// covered by this guard chain**, documented here so it is not silently
/// lost with the guard-chain history: oracles 4/6 are exact only for a
/// block's two OWN-THREAD resting places (this class's magazine, the
/// BinTable free list); they are blind to a block whose cross-thread free
/// is still in-flight in its segment's `RemoteFreeRing`, undrained. Task
/// #164 narrowed this window (all production drain paths now consult the
/// magazine via `reclaim_offset_checked`'s `is_in_magazine` predicate) and
/// closed a second leg (task R1, the refill-window in-out-buffer leg); the
/// remaining re-issue-before-drain leg is pinned RED by
/// `residual_xthread_double_free_no_corruption` (`#[ignore]`d) — full fix
/// tracked as task X7 (hardened, generational ring entry;
/// `RING_MAGAZINE_XTHREAD_DOUBLE_FREE_FIX.md` §8.4).
#[cfg(all(feature = "alloc-global", feature = "fastbin"))]
#[inline(always)]
#[allow(unused_variables)] // `c`/`layout`/`base` each go unused under some feature subsets.
pub(super) fn small_free_guard(
    base: *mut u8,
    ptr: *mut u8,
    c: usize,
    layout: Layout,
) -> SmallFreeGuard {
    #[cfg(feature = "hardened")]
    use crate::alloc_core::size_classes::SizeClasses;

    // ── F7 branch (A): promoted-Large routing (correctness-required) ──
    // See this function's own doc comment above (point 1) for the full
    // case-split rationale.
    medium_promotion_reachable! {
    {
        if (cfg!(feature = "hardened")
            || layout.size() >= MEDIUM_REALLOC_PROMOTION_THRESHOLD)
            && SegmentHeader::kind_at(base) == SegmentKind::Large
        {
            if !cfg!(feature = "hardened")
                || crate::alloc_core::deferred_large::large_layout_consistent(base, layout)
            {
                return SmallFreeGuard::RouteToLargeFree;
            }
            #[cfg(feature = "alloc-stats")]
            HARDENED_LARGE_NOOP_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return SmallFreeGuard::RejectNoOp;
        }
    }
    }

    // ── F7 branch (B): hardened-only, promotion unreachable ──
    #[cfg(all(
        feature = "hardened",
        not(all(
            feature = "medium-classes",
            any(
                not(feature = "exact-span-large"),
                all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
            )
        ))
    ))]
    {
        if SegmentHeader::kind_at(base) == SegmentKind::Large {
            #[cfg(feature = "alloc-stats")]
            HARDENED_LARGE_NOOP_COUNT.fetch_add(1, core::sync::atomic::Ordering::Relaxed);
            return SmallFreeGuard::RejectNoOp;
        }
    }

    // ── H1 (task #167): interior-pointer guard (HARDENED) ──
    #[cfg(feature = "hardened")]
    {
        let off_h = (ptr as usize).wrapping_sub(base as usize);
        let bs = SizeClasses::block_size(c);
        if !off_h.is_multiple_of(bs) {
            return SmallFreeGuard::RejectNoOp;
        }
    }

    // ── M2 double-free guard (Э6, P6.1) — see
    // `dealloc_own_thread_with_base`'s doc comment for the full design
    // rationale (order is load-bearing: in-magazine, THEN stale-free, THEN
    // flushed-bitmap).
    let off = (ptr as usize - base as usize) as u32;
    let meta = SegmentMeta::new(base);
    if meta.magazine_bitmap().is_in_magazine(off) {
        return SmallFreeGuard::RejectNoOp;
    }
    #[cfg(feature = "alloc-decommit")]
    if (off as usize) >= meta.bump_of() {
        return SmallFreeGuard::RejectNoOp;
    }
    if meta.alloc_bitmap().is_free(off) {
        return SmallFreeGuard::RejectNoOp;
    }

    SmallFreeGuard::Accept { off }
}

// `medium_promotion_reachable!` / `MEDIUM_REALLOC_PROMOTION_THRESHOLD` /
// `HARDENED_LARGE_NOOP_COUNT` stayed in the sibling `dealloc` module with
// the rest of the former file head. The macro is invoked unconditionally
// below (its `#[cfg]` arms gate the expansion itself); the const is used
// only inside branch (A)'s gated block, and the counter only at the two
// `alloc-stats`-gated `fetch_add` sites (branch (A) / branch (B)), so those
// two imports carry exactly the predicates gating their use sites.
use super::dealloc::medium_promotion_reachable;
#[cfg(all(
    feature = "alloc-stats",
    any(
        feature = "hardened",
        all(
            feature = "medium-classes",
            any(
                not(feature = "exact-span-large"),
                all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
            )
        )
    )
))]
use super::dealloc::HARDENED_LARGE_NOOP_COUNT;
#[cfg(all(
    feature = "medium-classes",
    any(
        not(feature = "exact-span-large"),
        all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
    )
))]
use super::dealloc::MEDIUM_REALLOC_PROMOTION_THRESHOLD;

impl HeapCore {
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
    pub(crate) fn dealloc_own_thread_with_base(
        &mut self,
        ptr: *mut u8,
        layout: Layout,
        base: *mut u8,
    ) {
        {
            use crate::alloc_core::size_classes::{SizeClasses, MIN_BLOCK};
            use crate::registry::heap_core::state::tcache::{FLUSH_N, TCACHE_CAP};
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

                    // Task #2002: F7 (task #25; R17-4/task #321; R18-3/task
                    // #330) Large-kind routing, H1 (task #167) interior-pointer
                    // guard, and the M2 (Э6, P6.1) double-free oracle pair —
                    // now the shared `small_free_guard` this file defines
                    // above, called identically from `dealloc_batch_small`
                    // (`free/dealloc_batch.rs`). See that function's doc
                    // comment for the full guard-by-guard rationale (case
                    // split for F7's legitimate-promotion vs. contract-
                    // violation branches, H1's interior-pointer blind spot in
                    // the M2 oracles, the Э6 double-free-oracle redesign
                    // history, and the RESIDUAL M2 LIMIT this pair does NOT
                    // cover — task #164/X7).
                    let (off, meta) = match small_free_guard(base, ptr, c, layout) {
                        SmallFreeGuard::Accept { off } => (off, SegmentMeta::new(base)),
                        SmallFreeGuard::RejectNoOp => return,
                        SmallFreeGuard::RouteToLargeFree => {
                            // SAFETY: this own-thread body is reached only
                            // from `HeapCore::dealloc`, an `unsafe fn` whose
                            // caller bound `ptr`/`layout` to the
                            // `GlobalAlloc::dealloc` contract; `base` was
                            // proven ours by `dealloc_routing`'s
                            // `contains_base` check. The substrate routes by
                            // `kind` (Large) and frees the segment via
                            // `span_usable`, ignoring the layout.
                            #[allow(unsafe_code)]
                            unsafe {
                                self.core.dealloc(ptr, layout)
                            };
                            return;
                        }
                    };

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
}
