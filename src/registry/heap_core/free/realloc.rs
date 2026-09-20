//! Free-side hot path for [`HeapCore`] (mechanical split of `heap_core.rs`,
//! task R6-CQ-7b; further split of the former flat `heap_core_free.rs`).
//!
//! This file holds the `impl HeapCore { .. }` block for `realloc` (one of
//! the two largest, most safety-critical methods in the former monolith,
//! carrying a full `# Safety` doc from R6-MS-1/2) and — under
//! `medium-classes` MINUS the zero-headroom `exact-span-large` exclusion
//! (R15-3, task #305; see `try_promote_to_large`'s own doc) — the R14-4
//! (task #289) Small/medium->Large realloc-promotion helper
//! (`try_promote_to_large`), Stage 2 of the design in `docs/perf/
//! R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md`. The `dealloc` side of
//! the split lives in the sibling modules `dealloc` / `dealloc_own_base`.
//! Otherwise a pure code-movement sibling of `heap_core.rs`; no other
//! behavior changed — the `# Safety` docs and `#[allow(unsafe_code)]`
//! attributes moved byte-for-byte.

use core::alloc::Layout;

#[cfg(feature = "alloc-global")]
use crate::alloc_core::os;
#[cfg(feature = "alloc-xthread")]
use crate::alloc_core::segment_header::{SegmentHeader, SEGMENT_MAGIC};
use crate::alloc_core::{node::Node, AllocCore};

use crate::registry::heap_core::HeapCore;

// `medium_promotion_reachable!` / `MEDIUM_REALLOC_PROMOTION_THRESHOLD` moved
// to the sibling `dealloc` module with the rest of the former file head. The
// macro is invoked unconditionally below (its `#[cfg]` arms gate the
// expansions themselves); the const is used only inside the gated promotion
// call-site block, so its import carries the SAME promotion-reachable
// predicate.
use super::dealloc::medium_promotion_reachable;
#[cfg(all(
    feature = "medium-classes",
    any(
        not(feature = "exact-span-large"),
        all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
    )
))]
use super::dealloc::MEDIUM_REALLOC_PROMOTION_THRESHOLD;

impl HeapCore {
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
                //       compiled under `medium-classes` PLUS the R15-3
                //       headroom exclusion below (the promotion only makes
                //       sense for medium-classified blocks in the first
                //       place — under plain `production` without
                //       `medium-classes`, every size in the medium range
                //       already routes Large, so there is nothing to promote
                //       FROM — see the R15-3 paragraph below for the second,
                //       narrower condition that actually gates the `#[cfg]`
                //       on this call site). Diverts a GROWING
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
                medium_promotion_reachable! {
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
                // F6 (task #494): `base` is already in hand from the
                // `contains_base(base)` check above — hand it directly to the
                // own-thread free body instead of routing back through
                // `HeapCore::dealloc` -> (`alloc-xthread`) `dealloc_routing`,
                // which would RECOMPUTE `os::segment_base_of_ptr(ptr)` and
                // RE-RUN `contains_base` from scratch. Mirrors
                // `dealloc_routing`'s own `contains_base(base) == true` arm
                // (`heap_core_xthread`) exactly, including its `#[cfg]`
                // split, so the two can be diffed by eye.
                //
                // Correctness (is `base` still ours & live here, AFTER
                // `self.alloc(new_layout)` ran?): YES — the OLD block at
                // `ptr` is still LIVE at this point (it is not freed until
                // the call below), so its segment's `live_count` is > 0 the
                // entire time `self.alloc` runs. Every path that can
                // unregister a segment from the table requires the segment
                // to be EMPTY (`live_count == 0`) first:
                //   - `AllocCore::dec_live_and_maybe_decommit`
                //     (`alloc_core_small_pool.rs`) returns `false` unless
                //     `live == 0`; only a `true` return routes the caller to
                //     `release_or_pool_empty_segment`, which is the ONLY
                //     path that can reach `SegmentTable::recycle`/
                //     `unregister`. Every one of its call sites
                //     (`dealloc_small`, the ring-drain loops in
                //     `find_segment_with_free_impl`, `flush_run` via
                //     `dec_live_batch_and_maybe_decommit`) is therefore also
                //     gated on the segment having just gone empty.
                //   - `drain_small_pool` (`alloc_core_small_pool.rs`) only
                //     recycles segments already sitting in the empty-segment
                //     hysteresis pool (which a segment enters only via the
                //     same empty-transition above).
                //   - The large-cache `evict_at_least`/`evict_one_oldest`/
                //     `evict_all` paths (`alloc_core_large_cache.rs`) operate
                //     on cached (already-freed, already-unregistered-at-
                //     deposit) large spans — structurally disjoint from a
                //     live own-segment `base` still holding a not-yet-freed
                //     block.
                // So no unregister path reachable from `self.alloc` above can
                // have removed `base` from the table: `contains_base(base)`
                // is still `true` here, exactly as it was when first checked.
                // This is the SAME live-block argument `try_promote_to_large`
                // below already relies on for its own `self.dealloc(ptr,
                // old_layout)` call with a pre-proven `base`.
                //
                // `contains_base(base)` proved `ptr`'s segment is ours & live
                // (and, per the argument above, still is), the read was
                // bounded by `safe_payload_read_span`, and `new_ptr` holds
                // the copied prefix; freeing the old block once completes
                // the contract-honouring realloc. Both own-thread free
                // bodies are safe `fn`s (each wraps its own internal
                // `unsafe { self.core.dealloc(..) }` call) — no `unsafe`
                // block needed at this call site, mirroring
                // `dealloc_routing`'s identical two-arm call (`heap_core_
                // xthread.rs`).
                #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
                self.dealloc_own_thread_with_base(ptr, old_layout, base);
                #[cfg(not(all(feature = "alloc-global", feature = "fastbin")))]
                self.dealloc_own_thread(ptr, old_layout);
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

    medium_promotion_reachable! {
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
    /// (Historical citation note: commit `912740f`'s message cited
    /// this same soundness argument as `heap_core_free.rs:929-933` at that
    /// commit — those lines held an unrelated `dealloc_foreign_slow`
    /// null-base guard, not this paragraph. This IS the paragraph the
    /// commit message meant to cite. R19-4, task #340.)
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
    /// bare `new_size` doesn't already get for free.
    ///
    /// **R17-4 (task #321) correction:** the probe's ORIGINAL measurement
    /// showed a paradox — `fixed2mib` reserved only 17 distinct segments
    /// (232 large-cache hits, ~68 MiB steady-state commit) while
    /// `nopad`/`floor512kib` reserved up to 249 distinct segments (0 hits,
    /// ~1 GiB commit) — the OPPOSITE of what identical 4 MiB `usable`
    /// rounding predicts. R14-4 could not explain this and left it as the
    /// gate report's §2.2 open question. R17-4 root-caused it: the fastbin
    /// magazine dealloc dispatch keyed on `class_for(layout.size())`, not on
    /// segment `kind`, so a promoted-then-OPT-G-in-place-grown Large block
    /// whose dealloc layout classified small (≤ `SMALL_MAX`, 1 MiB under
    /// `medium-classes`) — i.e. the `nopad`/`floor512kib` arms, whose final
    /// grown size ≤ 1 MiB — was misrouted into the small magazine path and
    /// NEVER reached the Large dealloc branch, leaking its 4 MiB segment
    /// every round; `fixed2mib`'s 2 MiB dealloc layout classified `None` and
    /// so accidentally took the correct substrate path. The R17-4 fix (route
    /// Large segments to the kind-keyed Large dealloc unconditionally in
    /// `dealloc_own_thread_with_base`) makes all three pad targets produce
    /// statistically indistinguishable ~68 MiB commit — exactly what §2.1's
    /// SEGMENT-rounding argument predicted. The pad-target decision (`nopad`,
    /// no padding) therefore stands on §2.1's rounding rationale, which is
    /// INDEPENDENT of the (now-fixed) anomaly; see
    /// `docs/perf/R14_4_MEDIUM_REALLOC_PROMOTION_GATE.md` §2.2 (closed) and
    /// `tests/r17_4_inplace_grown_large_dealloc_routes_by_kind.rs`. No padding
    /// is the default; a caller whose growth pattern would benefit from headroom
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
    ///
    /// R19-8 (task #344): the `#[cfg]` gating this fn is now generated by the
    /// `medium_promotion_reachable!` macro defined in `free/dealloc.rs`,
    /// not hand-written.
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
        // (`heap_core/alloc/hot.rs`) own Large branch line for line: the A1
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
        // R29-5 (task #436): record this promotion event for the
        // promotion-frequency / copied-byte-distribution gate
        // (`docs/perf/R29_5_PROMOTION_FREQUENCY_GATE.md`). Observation ONLY —
        // the per-event increment is gated `bench-internals` (NOT in
        // `production`, per CLAUDE.md's benchmark-hook rule #2), mirroring
        // `OPT_H_ATTEMPTS`'s gating discipline; plain `--features production`
        // (which does not include `medium-classes` either) emits nothing here.
        // The copy above is exactly `old_layout.size()` bytes, so that is the
        // copied-byte count recorded. No allocator decision is altered.
        #[cfg(feature = "bench-internals")]
        {
            use core::sync::atomic::Ordering;
            let copied = old_layout.size() as u64;
            crate::alloc_core::PROMOTION_COUNT.fetch_add(1, Ordering::Relaxed);
            crate::alloc_core::PROMOTION_BYTES_SUM.fetch_add(copied, Ordering::Relaxed);
            crate::alloc_core::PROMOTION_BYTES_MIN.fetch_min(copied, Ordering::Relaxed);
            crate::alloc_core::PROMOTION_BYTES_MAX.fetch_max(copied, Ordering::Relaxed);
            let bucket = crate::alloc_core::promotion_byte_bucket(old_layout.size());
            crate::alloc_core::PROMOTION_BYTES_HIST[bucket].fetch_add(1, Ordering::Relaxed);
        }
        // F6 (task #494): `base` is already a parameter here (the caller in
        // `realloc` passed it in already proven ours & live via
        // `contains_base`) — hand it directly to the own-thread free body
        // instead of routing back through `HeapCore::dealloc` ->
        // (`alloc-xthread`) `dealloc_routing`, which would RECOMPUTE
        // `os::segment_base_of_ptr(ptr)` and RE-RUN `contains_base` from
        // scratch. Same fix, same call shape, as `realloc`'s own move-leg
        // dealloc above.
        //
        // Correctness (is `base` still ours & live here, AFTER
        // `self.core.alloc_large(..)` + `self.stamp_segment_owner(..)` ran,
        // and — under `alloc-xthread` — the drains that ran BEFORE
        // `alloc_large`)? YES, by the SAME live-block argument as `realloc`'s
        // move leg (see that call site's comment for the full enumeration):
        // `ptr`'s OLD segment (`base`, still Small/medium-classified at this
        // point — promotion has not happened yet) is not freed until THIS
        // call, so its `live_count` stays > 0 for the entire body above,
        // which no unregister path can act on without `live_count == 0`
        // first. `self.core.alloc_large` operates on the Large-segment
        // table/cache machinery, structurally disjoint from the Small
        // segment `base` still belongs to; the `alloc-xthread` drains
        // (`drain_large_deferred_free`, `drain_heap_overflow`) reclaim
        // OTHER already-freed Large segments and never touch `base` either.
        // So `contains_base(base)` is still `true` here, exactly as when the
        // caller (`realloc`) first checked it.
        //
        // `base`'s segment is Small/medium-classified here (this is the
        // PRE-promotion free — the OLD block, before it becomes the new
        // Large allocation), so this is an ordinary own-thread small/medium
        // free, the same shape `realloc`'s move leg frees. `new_ptr` now
        // holds the copied prefix; freeing the old block once completes the
        // contract-honouring promotion. Both own-thread free bodies are safe
        // `fn`s (each wraps its own internal `unsafe { self.core.dealloc(..)
        // }` call) — no `unsafe` block needed at this call site.
        #[cfg(all(feature = "alloc-global", feature = "fastbin"))]
        self.dealloc_own_thread_with_base(ptr, old_layout, base);
        #[cfg(not(all(feature = "alloc-global", feature = "fastbin")))]
        self.dealloc_own_thread(ptr, old_layout);
        Some(new_ptr)
    }
    }
}
