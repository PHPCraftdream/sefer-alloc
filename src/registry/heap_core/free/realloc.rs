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

use crate::alloc_core::segment_header::{SegmentHeader, SegmentKind};
use crate::alloc_core::{node::Node, AllocCore};

use crate::registry::heap_core::HeapCore;
use crate::registry::segment_route::RouteKind;

// `MEDIUM_REALLOC_PROMOTION_THRESHOLD` moved to the sibling `dealloc`
// module with the rest of the former file head; the
// `medium_promotion_reachable!` macro moved to the crate-wide single source
// of truth `alloc_core::config::medium_promotion` (R6-T20). The macro is
// invoked unconditionally below (its `#[cfg]` arms gate the expansions
// themselves); the const is used only inside the gated promotion call-site
// block, so its import carries the SAME promotion-reachable predicate.
use crate::alloc_core::medium_promotion_reachable;
medium_promotion_reachable! {
    use super::dealloc::MEDIUM_REALLOC_PROMOTION_THRESHOLD;
}
// Ph3b: the promotion gate asks the PHYSICAL kind, not the layout's
// classification — a block that already lives in a Large segment must not be
// promoted (and is already served by the Large leg). Gated exactly like the
// threshold const above, since it is used only inside that call site.
medium_promotion_reachable! {
    use crate::alloc_core::segment_header::BlockKind;
}

impl HeapCore {
    /// Shrink/grow an allocation. Returns null on OOM (leaving the old
    /// allocation intact). Null `ptr` returns null.
    ///
    /// ## Own-segment pointers (task C2 + #164)
    ///
    /// If `ptr` lives in one of THIS heap's segments (`contains_base`), the
    /// resize takes the
    /// magazine-aware fast path:
    ///
    ///   1. **A1 terminal large-sidecar ingress drain** (`alloc-xthread` only): BEFORE the
    ///      in-place attempt, if the NEW size classifies as Large
    ///      (`class_for(...).is_none()`), drain this heap's terminal
    ///      large-sidecar ingress (`drain_large_sidecar_ingress`;
    ///      MUST-1/A1 — a realloc-growth-only thread still reclaims
    ///      cross-thread-freed large segments; otherwise its published-frees
    ///      backlog accumulates unboundedly). This drain is load-bearing — it runs
    ///      whether or not the in-place path succeeds.
    ///   2. **In-place attempt**: call `AllocCore::try_realloc_inplace_known_base`, which
    ///      applies the OPT-F (Small same-class) and OPT-G (Large grow-in-span)
    ///      short-circuits. On success it returns the SAME `ptr` (mutating the
    ///      block's header in place, never moving it) — we return immediately.
    ///   3. **Move leg**: on in-place failure, build the new `Layout`, call
    ///      `HeapCore::alloc` (magazine-aware — drains via the checked
    ///      predicate), copy `min(old, new)` bytes, then
    ///      `HeapCore::dealloc` the old pointer.
    ///
    /// The move leg routes through `HeapCore::alloc`/`HeapCore::dealloc`
    /// (NOT `AllocCore::realloc`'s internal alloc+copy+dealloc) so that the
    /// checked drain `HeapCore::alloc` applies fires on the freshly
    /// allocated block. Foreign frees no longer consult the header
    /// `owner_state` at all — routing resolves exclusively through the
    /// route-directory terminal descriptors / `Entry.owner` — so no
    /// per-alloc ownership stamp is needed. The historical failure mode is
    /// gone by construction: Historically (A1/#114), `dealloc_routing`
    /// resolved the segment as not-ours and silently no-op'd → the whole segment
    /// (4+ MiB) and its `SegmentTable` slot leaked forever (the resurrected
    /// A1/#114 leak-to-abort).
    ///
    /// ## Foreign pointers
    ///
    /// A `ptr` we do NOT own (a block that lives in ANOTHER heap's segment)
    /// takes the foreign leg. R2-1: before copying, the leg looks `ptr` up
    /// in the route directory and checks NUMERIC DESCRIPTOR CAPACITY — the
    /// address must fall inside the route's payload range and
    /// `old_layout.size()` must fit the descriptor's end
    /// (`contains_payload`), and the move leg's read is bounded by the
    /// segment's live commit frontier (R6-T50: `span_usable` for Large,
    /// the metadata/payload boundary for Small/Primordial). This is still
    /// NOT a live-instance identity check: the directory is keyed by
    /// address, so a stale address whose route is still linked can pass;
    /// safe access to the copied bytes rests on the CALLER's `GlobalAlloc`
    /// contract (a live allocation paired with this exact `old_layout`),
    /// the same contract every `realloc` implementation relies on. A
    /// missing route, capacity rejection, or frontier rejection returns
    /// null as defence in depth; it is not a claim that every dangling,
    /// interior, or bogus pointer is caught before the copy. A legitimate
    /// cross-heap sefer pointer passes the checks, copies `min(old, new)`,
    /// then frees the OLD pointer via `self.dealloc` (which routes
    /// cross-thread correctly — `alloc-global`, this module's gate,
    /// unconditionally implies `alloc-xthread`, R5-01).
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
            let own_block = self.core.owned_block_of(ptr);
            // Task #135 (Part 2): O(1) membership test (`AllocCore::contains_base`
            // → the OPT-B hash table) replaces the O(segment count) linear scan
            // `segment_bases().any(|b| b == base)`. Same semantics: `true` iff
            // `base` is one of THIS heap's registered, live segments.
            if let Some(owned) = own_block {
                let (base, block) = (owned.root(), owned.block());
                if AllocCore::large_payload_mismatch(base, block) {
                    return core::ptr::null_mut();
                }
                // Own-segment pointer. The resize proceeds in up to three
                // phases (see the doc comment above): A1 Large drain, in-place
                // attempt, then move leg — all funnelled through the
                // magazine-aware `HeapCore::alloc`/`dealloc` (NOT
                // `AllocCore::realloc`'s internal alloc, which bypasses the
                // ownership hooks).
                //
                // MUST-1 (0.3.0, C2 regression fix): the in-place attempt and
                // any move-leg alloc may carve a FRESH segment. That substrate
                // alloc does NOT run the A1 terminal large-sidecar ingress
                // drain (`drain_large_sidecar_ingress`) that
                // `HeapCore::alloc` applies. No ownership stamp is needed:
                // foreign frees route through the terminal descriptors, never
                // the header `owner_state`. The historical hazard was silent:
                // Historically (A1/#114), `dealloc_routing` resolved the
                // segment as not-ours and silently no-op'd → the whole
                // segment (4+ MiB) and its `SegmentTable` slot leak forever
                // (the resurrected A1/#114 leak-to-abort).
                //
                //   (1) A1 Large drain — BEFORE the in-place attempt, if the
                //       NEW request classifies as Large
                //       (`class_for(...).is_none()`, the exact predicate
                //       `alloc` uses), drain this heap's terminal
                //       large-sidecar ingress so a realloc-growth-only thread
                //       still reclaims cross-thread-freed large segments
                //       (otherwise its published-frees backlog accumulates
                //       unboundedly — the A1 drain-bypass leg of
                //       the bug). This drain is load-bearing and runs
                //       regardless of whether the in-place path then succeeds.
                #[cfg(feature = "alloc-xthread")]
                {
                    let class = crate::alloc_core::size_classes::SizeClasses::class_for(
                        new_size.max(crate::alloc_core::size_classes::MIN_BLOCK),
                        old_layout.align(),
                    );
                    if class.is_none() {
                        // Full sweep kept: cold path, no measured cost to justify a bounded probe.
                        self.drain_large_sidecar_ingress();
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
                    .try_realloc_inplace_known_base(base, block, old_layout, new_size)
                {
                    // `try_realloc_inplace_known_base` mutates the block's header in
                    // place and always returns the SAME pointer on success
                    // (it never moves the block). The segment was already
                    // stamped when first allocated, so there is nothing to
                    // re-stamp here.
                    debug_assert_eq!(
                        p.addr(),
                        ptr.addr(),
                        "known-base realloc must preserve the address"
                    );
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
                //       realloc of a block that PHYSICALLY still sits in a
                //       Small/Primordial segment, once `new_size` crosses
                //       `MEDIUM_REALLOC_PROMOTION_THRESHOLD`,
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
                // Ph3b: the THIRD conjunct used to be a plain
                // `class_for(old_layout).is_some()` — a derivation of "this
                // block is Small" from the caller's layout, exactly the
                // second source of truth this step removes. A block that
                // PHYSICALLY lives in a Large segment (e.g. one grown
                // in-place by OPT-G, whose layout can still classify Small)
                // must skip the Small/medium->Large promotion entirely: the
                // in-place attempt above already declined it and the Large
                // leg is the correct next stop. `BlockKind::of` reads the
                // header's `kind` byte; the layout is only re-used, as
                // always, to resolve the `class` payload.
                medium_promotion_reachable! {
                if new_size > old_layout.size()
                    && new_size >= MEDIUM_REALLOC_PROMOTION_THRESHOLD
                    && matches!(
                        BlockKind::of(
                            base,
                            crate::alloc_core::size_classes::SizeClasses::class_for(
                                old_layout
                                    .size()
                                    .max(crate::alloc_core::size_classes::MIN_BLOCK),
                                old_layout.align(),
                            ),
                        ),
                        BlockKind::Small { .. } | BlockKind::Primordial
                    )
                {
                    // R6-CR-4: validate the promoted Layout BEFORE handing new_size to
                    // alloc_large — an ill-formed size (usize::MAX-scale) must return null
                    // here, not overflow alloc_large's segment rounding (debug panic inside
                    // GlobalAlloc::realloc).
                    if Layout::from_size_align(new_size, old_layout.align()).is_err() {
                        return core::ptr::null_mut();
                    }
                    if let Some(p) = self.try_promote_to_large(owned, old_layout, new_size) {
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
                //       `old_layout.size()`. This is an `unsafe fn` (its
                //       `# Safety` contract requires an accurate `old_layout`),
                //       but the bound is defence-in-depth (oxx R2-07): a
                //       caller that violates the contract with a bogus layout
                //       (e.g. 8 MiB for a 16-byte block) must still not drive
                //       an OOB read. `base` was proven live above by
                //       `contains_base`. The write side is always safe (`copy
                //       <= new_size`); the read is bounded here.
                //
                //       Ownership: `self.core.contains_base(base)`
                //       above proved `base` is one of THIS heap's own
                //       segments, so this is a same-thread read of the
                //       owner-only commit frontier (R2-02) — see
                //       `safe_payload_read_span`'s doc.
                if old_layout.size() > AllocCore::safe_payload_read_span(base, block) {
                    return core::ptr::null_mut();
                }
                let new_layout = match Layout::from_size_align(new_size, old_layout.align()) {
                    Ok(l) => l,
                    Err(_) => return core::ptr::null_mut(),
                };
                let new_ptr = self.alloc(new_layout);
                if new_ptr.is_null() {
                    // R6-CR-3: a Large SHRINK whose move leg failed to allocate must not
                    // concede OOM: keeping the block in place is always possible (the
                    // shrunk size fits the span it already occupies). Record the new
                    // effective size and return the original pointer — correct per the
                    // GlobalAlloc contract (the block still holds >= new_size bytes). The
                    // success-path in-place shrink stays declined (perf item 83 NO-GO);
                    // this is the OOM leg only.
                    let old_eff = old_layout
                        .size()
                        .max(crate::alloc_core::size_classes::MIN_BLOCK);
                    let new_eff = new_size.max(crate::alloc_core::size_classes::MIN_BLOCK);
                    let large_shrink =
                        new_eff < old_eff && SegmentHeader::kind_at(base) == SegmentKind::Large;
                    if large_shrink {
                        let payload_off = SegmentHeader::read_at(base).payload_offset;
                        if ptr.addr() == base.addr() + payload_off {
                            SegmentHeader::set_large_size_at(base, new_eff);
                            return ptr;
                        }
                    }
                    return core::ptr::null_mut();
                }
                let copy = old_layout.size().min(new_size);
                crate::alloc_core::node::Node::copy_nonoverlapping(block, new_ptr, copy);
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
                //   - `dec_live_and_maybe_decommit`
                //     (`src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs`)
                //     returns `false` unless `live == 0`; its own-thread free
                //     and sidecar-drain callers route an eligible segment to
                //     `release_or_pool_empty_segment`.
                //   - `flush_run` uses the separate
                //     `dec_live_batch_and_maybe_decommit` check before routing.
                //     All these paths retire owner-side credits before any
                //     release or pool decision.
                //   - `drain_small_pool`
                //     (`src/alloc_core/small/alloc_core_small_pool/alloc_core_small_pool_impl.rs`)
                //     recycles segments already sitting in the empty-segment
                //     hysteresis pool (which they enter only after an
                //     empty-transition).
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
                self.dealloc_own_thread_with_base(owned, old_layout);
                #[cfg(not(all(feature = "alloc-global", feature = "fastbin")))]
                self.dealloc_own_thread(block, old_layout, base);
                return new_ptr;
            }
        }
        // A foreign allocation is validated without dereferencing another
        // owner's payload. The caller's Layout bounds the actual payload
        // read; the stable descriptor adds a capacity check, and the move
        // leg's read is bounded by the segment's live commit frontier
        // below (R6-T50).
        {
            let Some(route) = crate::registry::segment_route::RouteDirectory::global().lookup(ptr)
            else {
                return core::ptr::null_mut();
            };
            if !route.contains_payload(ptr, old_layout.size()) {
                return core::ptr::null_mut();
            }
            // R6-T50 (R6-MS-3): bound the move leg's read by the segment's
            // CURRENT committed frontier, not the reservation end — under
            // `large-reserved-capacity` a forged `old_layout` fits the
            // reservation while reaching reserved-but-uncommitted VA. Same
            // defence-in-depth shape as the own branch's
            // `safe_payload_read_span` above, read from the live header at
            // the descriptor's root (a foreign caller has no
            // `SegmentTable` membership, so the owner's table cannot be
            // consulted here).
            //
            // Why this header read is race-free — it needs exactly the
            // same caller-contract trust as the `copy_nonoverlapping`
            // below: the contract makes `ptr` a LIVE allocation of
            // exactly `old_layout`, so its segment is live and its header
            // (in the first committed pages at `root`) is mapped. The
            // descriptor pin keeps `root` stable while held (counted
            // reference); the pin does NOT keep the OS reservation alive —
            // that is the owner ledger's job (`segment_route/mod.rs`:
            // "Pins do not keep reservation memory alive: an independent
            // owner ledger must do that") — but a live caller block pins
            // the segment itself: every unregister/release path requires
            // the empty-segment transition (`dec_live_and_maybe_decommit`
            // declines unless `live == 0`), impossible while the caller's
            // block is live. `span_usable` of a live Large segment is
            // written only by that block's own OPT-G in-place grow
            // (`try_grow_large_reserved_capacity`), performed by the
            // block's exclusive owner BEFORE the pointer reaches this
            // caller (happens-before the handoff), or by large-cache
            // reuse — which re-registers only after the previous block
            // was freed, so it cannot race a live caller. Small and
            // Primordial segments commit the whole span eagerly, so only
            // the payload START is checked: metadata bytes (header, page
            // map, bins, bitmaps) are not a routable block start.
            let root = route.root();
            let frontier_ok = match route.kind() {
                RouteKind::Large => {
                    let committed_end =
                        root.addr().checked_add(SegmentHeader::span_usable_at(root));
                    match ptr.addr().checked_add(old_layout.size()) {
                        Some(read_end) => committed_end.is_some_and(|end| read_end <= end),
                        None => false,
                    }
                }
                RouteKind::Small => {
                    ptr.addr()
                        >= root.addr() + crate::alloc_core::segment_header::Layout::small_meta_end()
                }
                RouteKind::Primordial => {
                    ptr.addr()
                        >= root.addr()
                            + crate::alloc_core::segment_header::Layout::primordial_meta_end()
                }
            };
            if !frontier_ok {
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
            // SAFETY: the current source remains uniquely owned through the
            // copy; only successful destination allocation transfers its free.
            unsafe { self.dealloc(ptr, old_layout) };
            new_ptr
        }
    }

    medium_promotion_reachable! {
    /// R14-4 (task #289), Stage 2 of `docs/perf/
    /// R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md`: attempt to promote a
    /// physically-Small/Primordial, own-segment block directly to a
    /// Large allocation, instead of letting the caller's growing `realloc`
    /// fall through to the ladder-walk move leg. Called from `realloc`'s
    /// own-segment branch, between the in-place attempt (OPT-F/OPT-G) and the
    /// unconditional move leg, only when `medium-classes` is compiled in, the
    /// resize is a GROW, the block's segment header still says Small (the
    /// caller's `old_layout` resolves its class — Ph3b's kind witness), and
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
    /// (Historical note: the original citation pointed at a different location;
    /// this paragraph contains the design rationale.)
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
        owned: crate::alloc_core::OwnedBlock,
        old_layout: Layout,
        new_size: usize,
    ) -> Option<*mut u8> {
        let (base, ptr) = (owned.root(), owned.block());
        // R2-1 parity with the move leg just below: bound the read by the
        // block's actual committed span, not the caller-supplied
        // `old_layout.size()` — a bogus layout must not drive an OOB read.
        // `base` was already proven live by the caller's `contains_base`
        // check, which is also an ownership proof (R2-02) — see
        // `safe_payload_read_span`'s doc.
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
        // entirely, exactly as the design doc's §4.1 sketch specifies). The
        // A1 terminal large-sidecar ingress drain (`drain_large_sidecar_ingress`,
        // `alloc-xthread`) runs BEFORE the call. No ownership stamp is
        // needed on the result: routing no longer depends on the owner
        // stamp — foreign frees resolve through the route-directory terminal
        // descriptors / `Entry.owner`, never the header `owner_state` — so
        // a Vec grown via promotion cannot leak through a missing stamp.
        #[cfg(feature = "alloc-xthread")]
        {
            self.drain_large_sidecar_ingress();
        }
        let (new_ptr, _is_fresh) = self.core.alloc_large(new_size, old_layout.align());
        if new_ptr.is_null() {
            return None;
        }
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
        // `self.core.alloc_large(..)` ran,
        // and — under `alloc-xthread` — the drains that ran BEFORE
        // `alloc_large`)? YES, by the SAME live-block argument as `realloc`'s
        // move leg (see that call site's comment for the full enumeration):
        // `ptr`'s OLD segment (`base`, still Small/medium-classified at this
        // point — promotion has not happened yet) is not freed until THIS
        // call, so its `live_count` stays > 0 for the entire body above,
        // which no unregister path can act on without `live_count == 0`
        // first. `self.core.alloc_large` operates on the Large-segment
        // table/cache machinery, structurally disjoint from the Small
        // segment `base` still belongs to; the `alloc-xthread` ingress drain
        // (`drain_large_sidecar_ingress`) reclaims
        // OTHER already-published Large segments and never touch `base` either.
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
        self.dealloc_own_thread_with_base(owned, old_layout);
        #[cfg(not(all(feature = "alloc-global", feature = "fastbin")))]
        self.dealloc_own_thread(ptr, old_layout, base);
        Some(new_ptr)
    }
    }
}
