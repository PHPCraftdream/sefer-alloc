//! Tcache / magazine batch operations for [`AllocCore`](crate::AllocCore) (mechanical split of
//! `alloc_core_small.rs`, task R4-10).
//!
//! This file holds the `impl AllocCore { .. }` block for the magazine refill
//! and flush batch APIs (`refill_class*`, `flush_class`, `flush_run`).
//! Pure code-movement sibling of `alloc_core_small.rs`; no behavior changed.

use core::ptr::NonNull;

use crate::alloc_core::node::{Node, NODE_SIZE};
use crate::alloc_core::os;
use crate::alloc_core::segment_header::{
    Layout as SegLayout, SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL,
};
use crate::alloc_core::size_classes::SizeClasses;

use crate::alloc_core::alloc_core::AllocCore;

impl AllocCore {
    // -----------------------------------------------------------------------
    // Batch APIs (Phase 103 / P1 — fastbin / tcache substrate)
    //
    // Thin wrappers around the existing `alloc_small` / `dealloc_small`
    // primitives, called in a loop. NO new placement logic, NO new
    // invariants — the audited M2 / decommit / cross-thread paths run
    // UNCHANGED, just grouped into batches for the magazine layer (P2+).
    // -----------------------------------------------------------------------

    /// Pull up to `want` free blocks of class `class_idx` out of the segment
    /// substrate into `out`. Returns how many were written (0 on true OOM,
    /// else `> 0` and `<= want`).
    ///
    /// Each pulled block undergoes EXACTLY the same transition as a single
    /// `alloc_small`: bitmap `mark_alloc` + `inc_live` (under alloc-decommit).
    /// So a magazine-resident block will be "live + bitmap-allocated",
    /// identical to a handed-out block.
    #[doc(hidden)]
    #[inline]
    #[cfg(feature = "internals")]
    pub fn refill_class(&mut self, class_idx: usize, want: usize, out: &mut [*mut u8]) -> usize {
        debug_assert!(
            out.len() >= want,
            "refill_class: out.len() ({}) < want ({})",
            out.len(),
            want,
        );
        // R4-2 (code_quality_review #2): the `debug_assert!` above vanishes in
        // a release build, so a release caller passing `out.len() < want`
        // would have `.take(want)` silently iterate only `out.len()` slots
        // (slice iteration is bounds-safe) yet return `want` — a lying return
        // (caller believes more slots are initialised than were written). Clamp
        // `take` to the actual writable slot count and return THAT, so the
        // return value is truthful in every build profile. The `debug_assert!`
        // stays as a contract signal for debug callers who violate the intended
        // `out.len() >= want` precondition.
        let take = want.min(out.len());
        for (i, slot) in out.iter_mut().take(take).enumerate() {
            let ptr = self.alloc_small(class_idx);
            if ptr.is_null() {
                return i; // OOM or no more capacity
            }
            *slot = ptr;
        }
        take
    }

    /// Э1 (task #147) — **bump-direct batched carve**. Fill `out` with up to
    /// `out.len()` live, bitmap-allocated blocks of class `class_idx`, producing
    /// the IDENTICAL end-state as `refill_class` (each block: `live_count += 1`,
    /// bitmap "allocated", handed to the magazine) but SKIPPING the BinTable
    /// round-trip for freshly-carved blocks. Returns the number of slots filled
    /// (0 on true OOM, else `> 0` and `<= out.len()`).
    ///
    /// ## Source order — NON-NEGOTIABLE (free-drain BEFORE bump)
    ///
    /// For each wanted slot we prefer an EXISTING free block and bump-carve ONLY
    /// when no free block remains:
    ///   1. Drain free blocks first — `pop_free(small_cur)`, and on a miss
    ///      `find_segment_with_free` (which consumes terminal sidecar cuts for
    ///      routed candidates). This MUST run before any bump-carve: reusable
    ///      blocks in sidecars/BinTables must not be stranded while we grow
    ///      the bump cursor.
    ///   2. For the remaining slots, bump-carve DIRECTLY into `out` via
    ///      `carve_block` — no `dealloc_small`, no BinTable push, no subsequent
    ///      `pop_free`. `carve_block` already does `inc_live` + bump + page-map +
    ///      recommit (under `alloc-decommit`) and leaves the alloc bitmap UNSET
    ///      (= "allocated", the M2 convention), so a carved block is already in
    ///      the exact "live, allocated" state a handed-out block must be in
    ///      (see `carve_block` ~1783: it never touches `alloc_bitmap()`).
    ///      On `carve_block` → `None` (current segment full) we
    ///      `reserve_small_segment` and continue; if reserve fails we stop and
    ///      return the count filled so far (graceful — the caller treats `0` as
    ///      OOM and a partial fill as a normal short refill).
    ///
    /// ## D1 (live_count) — exact, per block +1, never double
    ///
    /// Each `out` block receives EXACTLY one `inc_live`: either from `pop_free`
    /// (drain branch) OR from `carve_block` (bump branch), never both — a slot
    /// is filled by exactly one of the two. This equals what `refill_class`
    /// produced (its `alloc_small` did one `inc_live` per block). The removed
    /// BinTable round-trip in the OLD path was net-zero on `live_count` anyway
    /// (`carve_block` +1 then the immediate `dealloc_small` −1 for each refill
    /// extra, then `pop_free` +1 when later re-popped); collapsing it changes
    /// nothing about the final count, only the intermediate churn.
    ///
    /// ## M2 (double-free bitmap) — byte-identical
    ///
    /// Carved blocks keep their bitmap bit UNSET (allocated). They are returned
    /// to the substrate later via `flush_class` → `dealloc_small`, which
    /// `mark_free`s them THEN — the identical lifecycle as `refill_class`, minus
    /// the redundant intermediate set-free-then-clear. A double-free of such a
    /// block still hits `dealloc_small`'s `is_free` guard exactly as before.
    #[doc(hidden)]
    #[inline]
    #[cfg(feature = "internals")]
    pub fn refill_class_bump(&mut self, class_idx: usize, out: &mut [*mut u8]) -> usize {
        self.refill_class_bump_internal(class_idx, out)
    }

    #[inline]
    pub(crate) fn refill_class_bump_internal(
        &mut self,
        class_idx: usize,
        out: &mut [*mut u8],
    ) -> usize {
        self.refill_class_bump_impl(
            class_idx,
            out,
            #[cfg(feature = "virgin-zero-skip")]
            None,
        )
    }

    /// Refill with a virgin bit for each output slot served by a virgin carve.
    /// Free-list blocks never set a bit. Uses the same metadata-guarded discovery
    /// and refill implementation as `refill_class_bump`.
    ///
    /// R2-15: clears `virgin_out` before refilling; only virgin slots below
    /// `filled` may be set afterward. Accepts at most 16 output slots, checked
    /// in every profile. The magazine caller is bounded by `TCACHE_CAP <= 16`.
    #[doc(hidden)]
    #[cfg(all(
        feature = "alloc-xthread",
        feature = "fastbin",
        feature = "virgin-zero-skip"
    ))]
    #[cfg(feature = "internals")]
    pub fn refill_class_bump_virgin(
        &mut self,
        class_idx: usize,
        out: &mut [*mut u8],
        virgin_out: &mut u16,
    ) -> usize {
        self.refill_class_bump_virgin_internal(class_idx, out, virgin_out)
    }

    #[cfg(all(
        feature = "alloc-xthread",
        feature = "fastbin",
        feature = "virgin-zero-skip"
    ))]
    pub(crate) fn refill_class_bump_virgin_internal(
        &mut self,
        class_idx: usize,
        out: &mut [*mut u8],
        virgin_out: &mut u16,
    ) -> usize {
        const VIRGIN_MASK_BITS: usize = u16::BITS as usize;
        assert!(
            out.len() <= VIRGIN_MASK_BITS,
            "refill_class_bump_virgin: out.len() ({}) exceeds the u16 virgin mask capacity ({VIRGIN_MASK_BITS} bits)",
            out.len(),
        );
        // Clear reused output bits before any partial or early return.
        *virgin_out = 0;
        self.refill_class_bump_impl(class_idx, out, Some(virgin_out))
    }

    #[inline]
    fn refill_class_bump_impl(
        &mut self,
        class_idx: usize,
        out: &mut [*mut u8],
        // R13-3 (task #273): `Some(mask)` accumulates a per-`out`-slot virgin
        // bitmask (bit `i` set ⟺ `out[i]` was served by a `carve_batch` run
        // on a segment whose `payload_virgin` bit read true, AND
        // `cfg!(not(miri))` — the identical predicate
        // `AllocCore::alloc_small_with_virgin` already uses). `None` from the
        // ordinary refill skips every bit-set below — one extra `is_none()` branch
        // per producer span (drain call or carve call), not per block, and
        // compiled out entirely when `virgin-zero-skip` is off (the parameter
        // does not exist in that build — see the two thin wrappers above).
        #[cfg(feature = "virgin-zero-skip")] mut virgin_out: Option<&mut u16>,
    ) -> usize {
        let block_size = SizeClasses::block_size(class_idx);
        debug_assert!(block_size >= NODE_SIZE);
        let want = out.len();
        let mut filled = 0usize;
        // Once `find_segment_with_free` reports no reusable block of this
        // class, latch the miss for this refill. Own frees cannot happen
        // mid-refill; concurrent terminal publications after a candidate's
        // cut can wait for the next discovery pass. This avoids repeating
        // discovery on every carved block while preserving free-before-bump.
        let mut free_exhausted = false;
        while filled < want {
            // 1. FREE-DRAIN FIRST (order is non-negotiable — see doc). Prefer
            //    free blocks from the current segment, then from any owned
            //    segment (which also consumes terminal sidecar publications).
            //
            //    Э7 (task #161): drain the segment's freelist in ONE walk via
            //    `drain_freelist_batch` instead of one `pop_free` per block —
            //    `set_head`/`head`-read/`inc_live` are hoisted out of the
            //    per-block loop. The end-state (bitmap bits, live_count,
            //    freelist head) is byte-identical to the per-block path. Source
            //    order is UNCHANGED: current segment's freelist, then the
            //    sidecar-draining discovery scan, then bump-carve.
            //
            //    E1 (task W4): once `free_exhausted` is latched there is nothing
            //    left to reclaim for the rest of this refill (proof below), so we
            //    SKIP the per-iteration `drain_freelist_batch` re-read + subslice
            //    construction — a pure tautology after the latch — and go
            //    straight to the batched bump-carve. The head cannot become
            //    non-null mid-refill: no dealloc / reclaim / flush runs inside
            //    `refill_class_bump` after the latch, and a remote free that
            //    arrives now publishes into the route sidecar, deferred to a
            //    later drain pass. So re-draining the current segment's
            //    freelist would only ever pop 0 — safe to skip.
            if !free_exhausted {
                let n = match self.try_drain_freelist_batch(
                    self.small_cur,
                    class_idx,
                    &mut out[filled..],
                ) {
                    Ok(n) => n,
                    Err(()) => return filled,
                };
                if n != 0 {
                    filled += n;
                    continue;
                }
                // `find_segment_with_free` consumes sidecar publications before
                // returning a usable base. Batch freelist drain uses that base
                // only after discovery's pool/release decision.
                // All discovery paths use the owner primitive's physical
                // free/magazine guards; no caller closure or output scan is needed.
                let found_seg = self.find_segment_with_free(class_idx);
                if let Some(seg) = found_seg {
                    let n = match self.try_drain_freelist_batch(seg, class_idx, &mut out[filled..])
                    {
                        Ok(n) => n,
                        Err(()) => return filled,
                    };
                    if n != 0 {
                        filled += n;
                        continue;
                    }
                }
                // Discovery found no free block in this pass: stop re-scanning
                // for this refill; later publications wait for a later pass.
                free_exhausted = true;
            }
            // 2. No free block anywhere: batched bump-carve DIRECTLY into `out`
            //    (E1, task W4). One `carve_batch` fills the whole remaining run
            //    from the current segment's bump in one shot — no BinTable
            //    round-trip, block live + bitmap-allocated, exactly the
            //    handed-out state (byte-identical to the per-block `carve_block`
            //    loop it replaces; see `carve_batch`).
            //
            // R13-3 (task #273): read the CURRENT segment's `payload_virgin`
            // bit BEFORE the carve (carve never mutates it — see
            // `AllocCore::alloc_small_with_virgin`'s doc for why reading
            // immediately before or after a successful carve on the SAME
            // segment observes the same value) and, if a `virgin_out` mask
            // was supplied, set one bit per block this `carve_batch` call
            // fills — the whole run shares ONE virgin signal (bump-monotonic
            // within a lifetime, §4.4 of both design docs: no intra-run
            // transition is possible).
            #[cfg(feature = "virgin-zero-skip")]
            let cur_virgin = virgin_out
                .is_some()
                .then(|| SegmentMeta::new(self.small_cur).payload_virgin_of());
            let n = match self.try_carve_batch(class_idx, block_size, &mut out[filled..]) {
                Ok(n) => n,
                Err(()) => return filled,
            };
            if n != 0 {
                #[cfg(feature = "virgin-zero-skip")]
                if let (Some(mask), Some(true)) = (virgin_out.as_deref_mut(), cur_virgin) {
                    if cfg!(not(miri)) {
                        let run_bits: u16 = ((1u32 << n) - 1) as u16;
                        *mask |= run_bits << filled;
                    }
                }
                filled += n;
                continue;
            }
            // 3. Current segment is full: reserve a fresh one and retry the
            //    carve. If reserve fails, stop and return what we have.
            match self.reserve_small_segment() {
                Some(_) => {
                    #[cfg(feature = "virgin-zero-skip")]
                    let fresh_virgin = virgin_out
                        .is_some()
                        .then(|| SegmentMeta::new(self.small_cur).payload_virgin_of());
                    let n = match self.try_carve_batch(class_idx, block_size, &mut out[filled..]) {
                        Ok(n) => n,
                        Err(()) => return filled,
                    };
                    if n != 0 {
                        #[cfg(feature = "virgin-zero-skip")]
                        if let (Some(mask), Some(true)) = (virgin_out.as_deref_mut(), fresh_virgin)
                        {
                            if cfg!(not(miri)) {
                                let run_bits: u16 = ((1u32 << n) - 1) as u16;
                                *mask |= run_bits << filled;
                            }
                        }
                        filled += n;
                        continue;
                    }
                    // A fresh segment that cannot fit even one block indicates
                    // metadata corruption; stop gracefully rather than loop.
                    break;
                }
                None => {
                    // R9-8 (task #230): rescue scan before surfacing OOM to the
                    // magazine refill. Same rationale as `alloc_small`'s step-4
                    // rescue: a directory-trust miss (R8-2) may have hidden a
                    // real free block for `class_idx`, leading to a spurious
                    // carve that just OOM'd. Run ONE forced O(S) scan ignoring
                    // the directory-trust and, if it finds a segment, drain its
                    // freelist into `out` instead of stopping short. Physical
                    // metadata guards apply equally to ordinary and rescue scans.
                    #[cfg(all(feature = "alloc-segment-directory", not(feature = "numa-aware")))]
                    if !self.directory_sidecar.is_null() {
                        let seg = self.find_segment_with_free_forced(class_idx);
                        if let Some(seg) = seg {
                            #[cfg(feature = "alloc-stats")]
                            crate::alloc_core::directory_stats::DIRECTORY_RESCUE_OOM_AVOIDED
                                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
                            let n = match self.try_drain_freelist_batch(
                                seg,
                                class_idx,
                                &mut out[filled..],
                            ) {
                                Ok(n) => n,
                                Err(()) => return filled,
                            };
                            if n != 0 {
                                filled += n;
                                continue;
                            }
                        }
                    }
                    break;
                }
            }
        }
        filled
    }

    /// Push a batch of blocks of class `class_idx` back onto their owning
    /// segments' `BinTable`s.
    ///
    /// Accepted blocks are linked and marked free, then their credits are
    /// retired in one batch. The eligibility check selects pool/release policy;
    /// it does not itself decommit or recycle the segment.
    ///
    /// Per-block base is derived per-block via `os::segment_base_of_ptr`
    /// (the magazine CAN hold blocks from multiple segments).
    ///
    /// ## Э8 (task #162) — same-segment run batching, BYTE-IDENTICAL to the
    /// per-block path
    ///
    /// The magazine holds blocks from possibly several segments, but a
    /// cold-storm flush of consecutively-freed blocks is ~100% same-segment, so
    /// scanning for RUNS of consecutive blocks with the same
    /// `segment_base_of_ptr` (ONE mask-compare per block, NO sorting) yields
    /// long runs; a scattered magazine degrades to runs of length 1 — still
    /// correct. For each run (all sharing one `base`) we hoist the metadata
    /// views (`SegmentMeta::new`, `bin_table`, `alloc_bitmap`, and the
    /// `bump_of` load) ONCE and write the freelist head ONCE,
    /// instead of once per block.
    ///
    /// ### Current-state guards remain per-block
    ///
    /// `flush_run` checks the payload lower bound, `off >= bump`, and
    /// `is_free(off)` before linking each block. The bump load is hoisted:
    /// a flush does not carve, and pool/release policy runs after the run.
    /// `reclaim_sidecar_record` returns false for an already-free or magazine-
    /// resident record before payload writes; it does not mark a resident
    /// block free behind the magazine. These checks are defense in depth,
    /// not a universal duplicate or stale-allocation guarantee.
    ///
    /// ### Splice — provably byte-identical to N sequential `dealloc_small`s
    ///
    /// A sequential run `dealloc_small(b0); …; dealloc_small(bk)` (accepted
    /// blocks only; a rejected block never calls `set_head`, so it is simply
    /// absent from the chain) builds a LIFO push: each accepted block becomes
    /// the new head pointing at the prior head. Final state:
    /// `head = off(b_last)`, `b_last.next = off(prev accepted)`, …,
    /// `b_first.next = old_head` (the segment's head captured at run start).
    /// The batch reproduces this EXACTLY: capture `old_head` once, then for each
    /// ACCEPTED block in source order `write_next(b, prev_accepted_or_old_head)`
    /// + `mark_free(off)`, remembering `b` as the new `prev_accepted`; after the
    /// run, `set_head(off(last accepted))` ONCE (only if ≥1 accepted). Every
    /// `write_next` writes the identical `next`, every `mark_free` sets the
    /// identical bit, `set_head` lands on the identical value ⇒ byte-identical.
    ///
    /// ### Batched credit retirement and pool/release eligibility
    ///
    /// Under the caller contract, every unflushed block still carries a credit.
    /// After linking accepted blocks, `flush_run` subtracts `accepted_count`
    /// once and checks the resulting live count. An empty, non-current Small
    /// segment that is not already reset is eligible for pool/release policy.
    /// Pool admission preserves the chain and committed pages; the release leg
    /// resets metadata and recycles the whole reservation. Neither the credit
    /// subtraction nor the eligibility helper performs payload decommit.
    ///
    /// # Safety
    ///
    /// The caller must honour the batch-free contract for every entry in
    /// `blocks`. This is the batched analogue of [`dealloc`](crate::alloc_core::alloc_core::AllocCore::dealloc)'s
    /// `# Safety` contract — the same reasoning that made `dealloc`/`realloc`
    /// `unsafe fn` in R6-MS-1/2 applies here: the method derives each block's
    /// segment `base` arithmetically (`os::segment_base_of_ptr`) and reads/writes
    /// that segment's `SegmentMeta`/`BinTable`/alloc-bitmap/`bump`/`kind` with NO
    /// `contains_base` membership check before the raw access, so a safe entry
    /// point accepting caller-controlled raw pointers was a soundness gap (round5
    /// `memory_safety_review` R5-MS-3). Concretely:
    ///
    /// - every NON-NULL entry of `blocks` is the exact **start** pointer of a
    ///   currently-LIVE small-class allocation owned by *this* `AllocCore`,
    ///   whose size class is exactly `class_idx`. It MUST NOT be an interior
    ///   pointer, a foreign pointer, or a pointer into a segment whose OS
    ///   reservation has already been released/unmapped.
    /// - `class_idx < SMALL_CLASS_COUNT`. (Release-checked inside
    ///   `BinTable::head`/`set_head` — an out-of-range index degrades to a safe
    ///   no-op rather than an out-of-bounds raw access — but a caller MUST still
    ///   pass a valid index.)
    /// - each entry is freed **at most once** within this call (and not re-freed
    ///   afterwards). A duplicate entry within the slice, or a block already on
    ///   the free list, is contract UB; the per-block M2 `is_free` /
    ///   `off >= bump` guards degrade several such cases benignly *at runtime*,
    ///   but they are defence-in-depth, NOT a substitute for honouring the
    ///   contract.
    /// - NULL entries are permitted and skipped (matching the per-block
    ///   `dealloc_small` path).
    ///
    /// Null `ptr` is always safe (early return).
    #[doc(hidden)]
    #[inline]
    #[allow(unsafe_code)] // R6-MS-3: `unsafe fn` boundary (caller-pointer contract).
    #[cfg(feature = "internals")]
    pub unsafe fn flush_class(&mut self, class_idx: usize, blocks: &[*mut u8]) {
        // SAFETY: the caller upholds the identical live-block, ownership,
        // class, mapping, and unique-free contract documented above.
        unsafe { self.flush_class_internal(class_idx, blocks) };
    }

    /// Production magazine flush.
    ///
    /// # Safety
    ///
    /// Every non-null entry must be a live, mapped allocation start owned by
    /// this core, of valid `class_idx`, and freed at most once. Interior,
    /// foreign, stale, or duplicate pointers are forbidden; nulls are skipped.
    #[inline]
    #[allow(unsafe_code)] // Caller-pointer contract, unchanged production body.
    pub(crate) unsafe fn flush_class_internal(&mut self, class_idx: usize, blocks: &[*mut u8]) {
        // L-4 (UBFIX-11): a per-CALL record of segment bases already recycled
        // (decommitted-and-released OR pooled) by an EARLIER run within this
        // same `flush_class` invocation. `flush_class` groups `blocks` into
        // same-segment runs (Э8); the grouping assumes each segment appears
        // in AT MOST ONE run, which holds for a legitimate magazine batch
        // (the magazine never holds two live copies of the same block, and
        // distinct blocks of one segment naturally form one contiguous
        // same-base run once produced by the allocator). But an UPSTREAM
        // double-free that reaches the magazine can hand `flush_class` a
        // batch containing the SAME pointer (or two different pointers whose
        // segment base coincides) in two SEPARATE positions, separated by a
        // pointer from a different segment — producing two runs for one
        // `base`. If the FIRST run empties the segment, `flush_run` calls
        // `release_or_pool_empty_segment(base)`, which — on the release leg —
        // decommits the payload, releases the OS reservation, and NULLs the
        // table slot; `base` is then an UNMAPPED address. The SECOND run for
        // the same `base` would still call `flush_run`, which unconditionally
        // reads/writes that segment's metadata (`SegmentMeta::new(base)`,
        // `bin_table()`, `alloc_bitmap()`, `bump_of()`, `kind_at(base)`) —
        // metadata-level use-after-free.
        //
        // Even on the POOL leg (no OS release), the segment's `bump`/
        // free-list state after the first run's `set_head` no longer matches
        // what a naively-repeated second run would assume, and per the M2
        // (double-free) discipline the safe move is uniformly "do not
        // re-touch a base this call already recycled" rather than trying to
        // distinguish pooled-safe from released-unsafe.
        //
        // Fixed-capacity array (M5: `AllocCore` allocates no `Vec`/`Box`),
        // bounded like the sibling `FLUSH_RUN_DETECT_CAP` in `flush_run`: the
        // production magazine batch is `TCACHE_CAP` (16) at most, so at most
        // 16 DISTINCT bases can appear in one legitimate call; a `flush_class`
        // slice larger than that (tests only) simply stops recording new
        // bases once the array is full (`recycled_n == CAP`) — the excess
        // just loses the double-free containment for anything beyond the
        // 16th distinct recycled base, it does not corrupt anything.
        const RECYCLED_CAP: usize = 16;
        let mut recycled_bases: [*mut u8; RECYCLED_CAP] = [core::ptr::null_mut(); RECYCLED_CAP];
        let mut recycled_n: usize = 0;

        let mut i = 0;
        while i < blocks.len() {
            let ptr = blocks[i];
            if ptr.is_null() {
                i += 1;
                continue; // defensive: skip nulls (matches per-block path)
            }
            let candidate = os::segment_base_of_ptr(ptr);
            // Detect the run of consecutive same-segment blocks starting at `i`.
            // Nulls terminate a run (they are handled by the outer loop as
            // no-ops, exactly as the per-block path skips them).
            let mut run_end = i + 1;
            while run_end < blocks.len() {
                let q = blocks[run_end];
                if q.is_null() || os::segment_base_of_ptr(q) != candidate {
                    break;
                }
                run_end += 1;
            }
            // L-4: if an EARLIER run in this call already recycled `base`,
            // skip this run entirely — `base`'s metadata may be unmapped
            // (released leg) or in a state a blind re-run must not assume
            // (pooled leg). This is the exact defensive-skip the per-block
            // `dealloc_small` path gets "for free" one block at a time (each
            // call independently re-checks `contains_base`/`magic`/bitmap
            // state); the batched run path must do it explicitly because it
            // hoists metadata reads ONCE per run, before any per-block guard
            // could observe the segment having vanished mid-batch.
            let already_recycled = recycled_bases[..recycled_n].contains(&candidate);
            if !already_recycled {
                if let Some(base) = self.table.canonical_base_of(candidate) {
                    let recycled_now = self.flush_run(class_idx, base, &blocks[i..run_end]);
                    if recycled_now && recycled_n < RECYCLED_CAP {
                        recycled_bases[recycled_n] = candidate;
                        recycled_n += 1;
                    }
                }
            }
            i = run_end;
        }
    }

    /// Flush ONE run of blocks that all share segment `base` (Э8). See
    /// `flush_class` for splice and batched-credit policy reasoning. Every
    /// block in `run` is non-null and has `segment_base_of_ptr(block) == base`.
    ///
    /// L-4 (UBFIX-11): returns `true` iff this run's flush triggered
    /// `release_or_pool_empty_segment(base)` (i.e. the segment reached
    /// `live_count == 0` and was pooled or released). `flush_class`
    /// uses this to record `base` and skip any LATER same-`base` run within
    /// the same call, instead of re-touching a segment whose metadata may now
    /// be unmapped (released leg) or whose state a blind re-run must not
    /// assume (pooled leg). Always `false` when `alloc-decommit` is off (no
    /// recycle path exists in that config).
    #[inline]
    #[must_use]
    fn flush_run(&mut self, class_idx: usize, base: *mut u8, run: &[*mut u8]) -> bool {
        let meta = SegmentMeta::new(base);
        let mut bt = meta.bin_table();
        let mut bm = meta.alloc_bitmap();
        // Hoist the `bump` LOAD once (the COMPARE stays per-block). A flush
        // never carves, so `bump` cannot advance during this run.
        let bump = meta.bump_of();
        // H-1 (UBFIX-3): hoist the payload lower bound once — every block in
        // `run` shares this `base` (see this fn's doc), so `kind`/
        // `payload_start` are run-invariant. Reject any block whose `off`
        // lands in the segment's OWN metadata region (header / page map /
        // bin table / …) instead of the payload; see `dealloc_small`'s
        // identical guard for the full rationale.
        let kind = SegmentHeader::kind_at(base);
        let payload_start = if kind == SegmentKind::Primordial {
            SegLayout::primordial_meta_end()
        } else {
            SegLayout::small_meta_end()
        };

        // Capture the segment's CURRENT freelist head ONCE — the first accepted
        // block links to this (matching the first sequential `dealloc_small`,
        // whose `old_head` is exactly this value).
        let old_head = bt.head(class_idx);
        let mut prev_off = old_head; // next-target for the next accepted block
        let mut last_accepted: Option<u32> = None;
        // Count accepted blocks for one owner-side credit retirement after `set_head`.
        let mut accepted_count: usize = 0;

        for &ptr in run {
            let off = (ptr as usize - base as usize) as u32;
            // Guard 0 (per-block): H-1 payload lower bound (`payload_start`
            // hoisted above — run-invariant). A `write_next` on an
            // in-metadata offset would clobber this segment's own header/
            // page-map/bin-table in place.
            if (off as usize) < payload_start {
                continue;
            }
            // Guard 1 (per-block): decommit stale-free `off >= bump`. M-1
            // (UBFIX-3): previously `#[cfg(feature = "alloc-decommit")]`-only
            // — non-decommit builds had no upper bound at all. Corruption
            // containment must not depend on the decommit feature;
            // unconditional now (`bump` is hoisted unconditionally above).
            if (off as usize) >= bump {
                continue;
            }
            // Guard 2 (per-block): skip a block whose current bitmap state is free.
            if bm.is_free(off) {
                continue;
            }
            // `ptr` identifies the offset only. Link through the stored
            // reservation root, which covers the entire free-list word.
            let block_nn = match NonNull::new(Node::deref(base, off as usize)) {
                Some(nn) => nn,
                None => continue,
            };
            // Link this accepted block at the head of the run-local chain: its
            // `next` is the PRIOR accepted block's off (or the captured
            // `old_head` for the first accepted). Byte-identical to the LIFO
            // push each sequential `dealloc_small` performs.
            let next_ptr = if prev_off == FREE_LIST_NULL {
                core::ptr::null_mut()
            } else {
                Node::deref(base, prev_off as usize)
            };
            Node::write_next(block_nn, next_ptr);
            bm.mark_free(off);
            prev_off = off;
            last_accepted = Some(off);
            accepted_count += 1;
        }

        // Write the new head ONCE (only if ≥1 block was accepted). Mirrors the
        // final `set_head` of the last sequential `dealloc_small` in the run.
        if let Some(off) = last_accepted {
            bt.set_head(class_idx, off);
            // R7-A2: directory bitmap maintenance — the new head is always
            // non-null (`off`), so the only transition is empty→non-empty
            // when old_head was FREE_LIST_NULL.
            #[cfg(feature = "alloc-segment-directory")]
            if old_head == FREE_LIST_NULL {
                let slot_idx = SegmentHeader::segment_id_at(base) as usize;
                self.publish_nonempty(base, class_idx, slot_idx);
            }
        }

        // E3 (task W4): retire accepted credits once, after `set_head`.
        // The following eligibility check only selects pool/release policy.
        SegmentMeta::new(base).sub_live(accepted_count as u32);
        #[cfg(feature = "alloc-decommit")]
        {
            let small_cur = self.small_cur;
            if Self::dec_live_batch_and_maybe_decommit(base, accepted_count as u32, small_cur) {
                // Mechanism 2 (task #51): pool-or-release instead of the former
                // unconditional recycle.
                let _ = self.release_or_pool_empty_segment(base);
                // L-4 (UBFIX-11): report the recycle to `flush_class` so it can
                // skip any later same-`base` run within this call.
                return true;
            }
        }
        false
    }
}
