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

                    // ── F7 (task #25; R17-4/task #321; R18-3/task #330):
                    //    Large-segment kind routing vs. defensive no-op ──
                    //
                    // `class_for` returning `Some(c)` above keys the free on
                    // the *layout*, not on where `ptr` actually lives. Two
                    // structurally different situations can land here with the
                    // block physically in a Large segment:
                    //
                    // (A) **Under `medium-classes` — a LEGITIMATE state.**
                    //     `SMALL_MAX` is 1 MiB, and R14-4's medium→Large
                    //     promotion (task #289) diverts a medium block to a
                    //     dedicated 4 MiB Large segment at the 256 KiB
                    //     threshold; OPT-G then grows that block IN PLACE up to
                    //     any size ≤ `SMALL_MAX` while it stays a Large segment.
                    //     Its dealloc layout (the post-grow size, exactly what
                    //     the `GlobalAlloc` contract requires) classifies small
                    //     even though the block lives in a Large segment. So
                    //     this routing is a CORRECTNESS REQUIREMENT, not a
                    //     defensive guard: without it the free falls into the
                    //     magazine path below, reads M2 oracle (bitmap/
                    //     magazine) state out of the Large allocation's PAYLOAD,
                    //     and NEVER reaches `AllocCore::dealloc`'s Large branch
                    //     — the 4 MiB segment is neither deposited into
                    //     `large_cache` nor released: it leaks, and every
                    //     subsequent promotion reserves a fresh segment (the
                    //     R14-4 gate §2.2 `nopad`/`floor512kib` 0-cache-hit /
                    //     249-segment anomaly). `AllocCore::dealloc` frees the
                    //     whole segment via the header's `span_usable`,
                    //     ignoring the layout — correct for the in-place-grown
                    //     case. Under `hardened`, a genuine contract violation
                    //     (a fabricated small layout on a never-promoted Large
                    //     pointer) is now caught by the consistency check in
                    //     branch (A) below (R19-1/task #337) and degrades to a
                    //     no-op rather than really freeing the segment; only a
                    //     LEGITIMATE promoted-and-grown free (or any
                    //     non-hardened build, which makes no defence promise)
                    //     reaches the real `self.core.dealloc` call.
                    //
                    // (B) **Promotion-OFF builds — contract violations only.**
                    //     When promotion is compiled OUT (non-`medium-classes`
                    //     OR `medium-classes` with `exact-span-large` zero-
                    //     headroom, e.g. `--all-features` where `numa-aware`
                    //     defeats `large-reserved-capacity`), a LEGITIMATE
                    //     Large-with-small-layout is structurally unreachable.
                    //     But the ILLEGITIMATE case — a `GlobalAlloc`-contract
                    //     violation (caller frees a Large pointer with a
                    //     fabricated small layout) — is still reachable, and
                    //     `hardened` exists to defend against it: without a
                    //     guard the magazine oracles read bitmap state out of
                    //     the Large payload → silent aliasing. F7's task #25
                    //     defensive no-op covers it. R18-3 broadens (B)'s cfg
                    //     from `hardened && not(medium-classes)` to
                    //     `hardened && NOT(promotion-predicate)` so it ALSO fires
                    //     under `medium-classes`-without-promotion — the gap the
                    //     original bare-`medium-classes` branch (A) accidentally
                    //     covered by routing-to-substrate, which R18-3's cfg
                    //     narrowing of (A) exposed (`regression_hardened_large_
                    //     kind_own_free` went RED: 2 MiB Large freed with 64-byte
                    //     layout aliased via the magazine when neither branch
                    //     compiled). For non-`medium-classes` builds (B)'s cfg
                    //     is equivalent to pre-R18-3 (promotion predicate is
                    //     false when `medium-classes` is absent) — byte-identical
                    //     behaviour. Under plain `production` (no `hardened`,
                    //     no `medium-classes`) both branches compile out: no
                    //     `kind_at` load, no `unsafe` call on the hot path.
                    //
                    // Mutually exclusive: (A) requires promotion ON; (B) requires
                    // promotion OFF (and `hardened`). Together they cover every
                    // `hardened` build + every promotion build; the only configs
                    // where NEITHER fires are non-hardened non-promotion (plain
                    // `production`, and `production,medium-classes,exact-span-
                    // large` without `hardened`) — consistent with the crate's
                    // stance that contract-violation defence is `hardened`-opt-in.
                    //
                    // R17-4's "zero hot-path cost" iai claim was measured only
                    // under plain `production` (where neither branch compiles).
                    // R18-3 closes the proof gap: `medium_class_dealloc_churn_16b`
                    // records the FIRST instruction-count baseline under
                    // `production,medium-classes` — where (A) DOES compile in —
                    // so the runtime-gated `kind_at` check's cost is a tracked
                    // metric, not an unmeasured assumption.

                    // (A) Promotion ON: route-to-substrate. Correctness-critical
                    //     for legit promotion-grown blocks (must reach the
                    //     Large dealloc branch), AND defensive for hardened
                    //     builds (contract violations route-to-substrate too).
                    //     The runtime size gate (skip `kind_at` for sub-threshold
                    //     frees) is DISABLED under `hardened` — see the inline
                    //     comment at the `if` below for the full RED-test
                    //     rationale.
                    medium_promotion_reachable! {
                    {
                        // Under `hardened`: always check (defence-in-depth — a
                        // contract violation can use ANY small layout; the
                        // promotion soundness argument only covers LEGITIMATE
                        // allocations). Under non-hardened: skip `kind_at` for
                        // sub-threshold frees (a legit Large-with-small-layout
                        // requires size ≥ the promotion threshold). `cfg!`
                        // constant-folds — hardened builds get an unconditional
                        // check, non-hardened get the size gate. Verified RED
                        // without the `cfg!` short-circuit: the
                        // `regression_hardened_large_kind_own_free` test (2 MiB
                        // Large freed with 64-byte layout) aliases via the
                        // magazine when the size gate skips `kind_at`.
                        if (cfg!(feature = "hardened")
                            || layout.size() >= MEDIUM_REALLOC_PROMOTION_THRESHOLD)
                            && SegmentHeader::kind_at(base) == SegmentKind::Large
                        {
                            // R19-1 (task #337): under `hardened`, verify the
                            // caller's layout is consistent with the segment's
                            // CURRENT `large_size` before treating this as the
                            // correctness-required real Large free. Reuses the
                            // exact primitive (task #138) the cross-thread
                            // Large-free routing path already uses for the same
                            // kind of check (`heap_core_xthread`'s
                            // `dealloc_routing`): `large_layout_consistent`
                            // compares `layout.size().max(MIN_BLOCK)` against
                            // `SegmentHeader::large_size_at(base)`, AND (R22-5,
                            // task #356) `layout.align()` against
                            // `SegmentHeader::large_align_at(base)` — exact
                            // match on both for a LEGITIMATE promoted-and-grown
                            // free (the header's `large_size` is updated on both
                            // initial promotion and every subsequent OPT-G
                            // in-place grow; `large_align` is fixed at promotion
                            // and never changed by a grow — a realloc's align is
                            // fixed by contract), mismatch for essentially any
                            // fabricated/mismatched layout. On a mismatch under
                            // `hardened`, degrade to the SAME defensive no-op
                            // branch (B) uses instead of really freeing the
                            // segment (a `GlobalAlloc` contract violation is
                            // exactly what `hardened`'s task #25 exists to
                            // defend against as a detected no-op, NOT a silent
                            // real free). The non-hardened path makes no such
                            // defence promise and is left untouched (touching
                            // it risks an unmeasured perf/behavior change on
                            // the production hot path). `alloc-xthread` — the
                            // gate on `large_layout_consistent` — is always on
                            // wherever this branch compiles: `fastbin` (which
                            // this function is gated on) implies
                            // `alloc-xthread`.
                            if !cfg!(feature = "hardened")
                                || crate::alloc_core::deferred_large::large_layout_consistent(
                                    base, layout,
                                )
                            {
                                // SAFETY: this own-thread body is reached only
                                // from `HeapCore::dealloc`, an `unsafe fn`
                                // whose caller bound `ptr`/`layout` to the
                                // `GlobalAlloc::dealloc` contract; `base` was
                                // proven ours by `dealloc_routing`'s
                                // `contains_base` check. The substrate routes
                                // by `kind` (Large) and frees the segment via
                                // `span_usable`, ignoring the layout.
                                #[allow(unsafe_code)]
                                unsafe {
                                    self.core.dealloc(ptr, layout)
                                };
                                return;
                            }
                            // hardened + inconsistent layout: a fabricated/
                            // illegitimate free of a Large pointer with a
                            // mismatched small layout. Defensive no-op (task
                            // #25), matching branch (B)'s contract. Branch (B)
                            // cannot compile here (its cfg is the negation of
                            // this branch's, so the two are mutually
                            // exclusive), so this returns directly instead of
                            // falling through to it.
                            //
                            // R22-12 (task #363): count this detected-and-
                            // rejected contract violation — see
                            // `HARDENED_LARGE_NOOP_COUNT`'s doc for why this
                            // shares ONE counter with branch (B) below.
                            #[cfg(feature = "alloc-stats")]
                            HARDENED_LARGE_NOOP_COUNT.fetch_add(
                                1,
                                core::sync::atomic::Ordering::Relaxed,
                            );
                            return;
                        }
                    }
                    }

                    // (B) Promotion-OFF + hardened: defensive no-op (task #25).
                    //     R18-3 broadens the cfg from bare `not(medium-classes)`
                    //     to the negation of the promotion predicate, so it also
                    //     fires under `medium-classes`-without-promotion (e.g.
                    //     `--all-features`, where `numa-aware` defeats
                    //     `large-reserved-capacity`). Under plain
                    //     `production,medium-classes,exact-span-large` (no
                    //     hardened) NEITHER (A) nor (B) compiles — consistent
                    //     with plain `production` (no defence without hardened).
                    //     For non-`medium-classes` builds the cfg is equivalent
                    //     (promotion predicate is false when `medium-classes` is
                    //     absent) so behaviour is byte-identical to pre-R18-3.
                    //
                    //     R19-8 (task #344): the inner `all(medium-classes,
                    //     any(...))` term below is the negation of the SAME
                    //     promotion-reachable predicate canonicalized into the
                    //     `medium_promotion_reachable!` macro above — it must
                    //     stay hand-written and in sync by inspection, because
                    //     `#[cfg(...)]` cannot accept a macro invocation as its
                    //     argument (this term is negated and combined with
                    //     `hardened`, not the bare predicate the macro wraps).
                    #[cfg(all(
                        feature = "hardened",
                        not(all(
                            feature = "medium-classes",
                            any(
                                not(feature = "exact-span-large"),
                                all(
                                    feature = "large-reserved-capacity",
                                    not(feature = "numa-aware")
                                )
                            )
                        ))
                    ))]
                    {
                        if SegmentHeader::kind_at(base) == SegmentKind::Large {
                            // R22-12 (task #363): count this detected-and-
                            // rejected contract violation — see
                            // `HARDENED_LARGE_NOOP_COUNT`'s doc for why this
                            // shares ONE counter with branch (A) above.
                            #[cfg(feature = "alloc-stats")]
                            HARDENED_LARGE_NOOP_COUNT
                                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
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
}
