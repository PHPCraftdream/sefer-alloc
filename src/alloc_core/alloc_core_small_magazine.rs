//! Tcache / magazine batch operations for [`AllocCore`] (mechanical split of
//! `alloc_core_small.rs`, task R4-10).
//!
//! This file holds the `impl AllocCore { .. }` block for the magazine refill
//! and flush batch APIs (`refill_class*`, `flush_class`, `flush_run`).
//! Pure code-movement sibling of `alloc_core_small.rs`; no behavior changed.

use core::ptr::NonNull;

use super::node::{Node, NODE_SIZE};
use super::os;
use super::segment_header::{
    Layout as SegLayout, SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL,
};
use super::size_classes::SizeClasses;

use super::alloc_core::AllocCore;

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
    ///      `find_segment_with_free` (which lazily drains each owned segment's
    ///      remote-free ring, reclaiming cross-thread frees). This MUST run
    ///      before any bump-carve: if we carved first, freed blocks sitting in
    ///      the per-segment rings/BinTables would go stale, the rings would back
    ///      up (RSS drift), and the xthread ring-reclaim expectations (A1) would
    ///      break — a freed remote block must be reused, not stranded while we
    ///      grow the bump cursor.
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
    pub fn refill_class_bump(&mut self, class_idx: usize, out: &mut [*mut u8]) -> usize {
        self.refill_class_bump_impl(
            class_idx,
            out,
            #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))]
            &|_, _| false,
        )
    }

    /// Task #164: variant with magazine predicate.
    #[doc(hidden)]
    #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))]
    pub fn refill_class_bump_checked<F: Fn(*mut u8, usize) -> bool>(
        &mut self,
        class_idx: usize,
        out: &mut [*mut u8],
        is_in_magazine: &F,
    ) -> usize {
        self.refill_class_bump_impl(class_idx, out, is_in_magazine)
    }

    #[inline]
    fn refill_class_bump_impl<
        #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))] F: Fn(*mut u8, usize) -> bool,
    >(
        &mut self,
        class_idx: usize,
        out: &mut [*mut u8],
        #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))] is_in_magazine: &F,
    ) -> usize {
        let block_size = SizeClasses::block_size(class_idx);
        debug_assert!(block_size >= NODE_SIZE);
        let want = out.len();
        let mut filled = 0usize;
        // Once the whole-heap free scan (`find_segment_with_free`) reports NO
        // free block of this class anywhere AND has drained every owned
        // segment's remote-free ring, there is nothing more to reclaim for the
        // rest of THIS refill: our own frees cannot happen mid-refill, and
        // remote frees that arrive now land in the (already-scanned) rings and
        // are deferred to the NEXT refill's drain — exactly the amortisation
        // the retired `carve_block_with_refill` used (it also drained/scanned
        // once, then carved its whole batch). Latching this avoids re-running
        // the O(segments) scan + ring drain on every carved block of a cold
        // storm; correctness is unchanged because the drain still runs at
        // least once BEFORE any carve (source order preserved).
        let mut free_exhausted = false;
        while filled < want {
            // 1. FREE-DRAIN FIRST (order is non-negotiable — see doc). Prefer
            //    free blocks from the current segment, then from any owned
            //    segment (which also drains remote rings → xthread reclaim).
            //
            //    Э7 (task #161): drain the segment's freelist in ONE walk via
            //    `drain_freelist_batch` instead of one `pop_free` per block —
            //    `set_head`/`head`-read/`inc_live` are hoisted out of the
            //    per-block loop. The end-state (bitmap bits, live_count,
            //    freelist head) is byte-identical to the per-block path. Source
            //    order is UNCHANGED: current segment's freelist, then the
            //    ring-draining whole-heap scan, then bump-carve.
            //
            //    E1 (task W4): once `free_exhausted` is latched there is nothing
            //    left to reclaim for the rest of this refill (proof below), so we
            //    SKIP the per-iteration `drain_freelist_batch` re-read + subslice
            //    construction — a pure tautology after the latch — and go
            //    straight to the batched bump-carve. The head cannot become
            //    non-null mid-refill: no dealloc / reclaim / flush runs inside
            //    `refill_class_bump` after the latch, and a remote free that
            //    arrives now lands in the (already-scanned) ring, deferred to the
            //    NEXT refill's drain. So re-draining the current segment's
            //    freelist would only ever pop 0 — safe to skip.
            if !free_exhausted {
                let n = self.drain_freelist_batch(self.small_cur, class_idx, &mut out[filled..]);
                if n != 0 {
                    filled += n;
                    continue;
                }
                // `find_segment_with_free` runs the A1 ring-drain (reclaiming
                // cross-thread frees into the per-segment BinTables) BEFORE it
                // returns a base — that ordering is preserved: we call the batch
                // drain only on the base it hands back.
                // Task R1 (retro C1): wrap the caller's magazine predicate
                // with an out-membership guard. The predicate passed in from
                // `refill_magazine_slow` opens with `if k == c { return false; }`
                // (justified ONLY by the borrow-safety invariant count[c]==0),
                // which means blocks already pulled into `out[0..filled]` during
                // THIS refill call — magazine-destined but not yet stamped into
                // the magazine — are INVISIBLE to it. A stale cross-thread
                // double-free note for such a block still sitting in a ring
                // would then be reclaimed (write_next + mark_free), relinking
                // the block onto the freelist, and the SAME refill loop would
                // pull it into `out` AGAIN → P issued twice out of one refill.
                //
                // The guard closes the window for free: when the ring is empty
                // (the common case) `issued_so_far.contains` is never consulted,
                // so the Ir cost on the hot refill path is exactly zero — the
                // out-buffer is non-empty only when we have already drained at
                // least one block from the freelist AND the ring has work, and
                // even then the scan is over a CAP-bounded magazine refill batch.
                #[cfg(all(feature = "alloc-xthread", feature = "fastbin"))]
                let found_seg = {
                    let issued_so_far: &[*mut u8] = &out[..filled];
                    self.find_segment_with_free_checked(class_idx, &|ptr, k| {
                        is_in_magazine(ptr, k) || (k == class_idx && issued_so_far.contains(&ptr))
                    })
                };
                #[cfg(not(all(feature = "alloc-xthread", feature = "fastbin")))]
                let found_seg = self.find_segment_with_free(class_idx);
                if let Some(seg) = found_seg {
                    let n = self.drain_freelist_batch(seg, class_idx, &mut out[filled..]);
                    if n != 0 {
                        filled += n;
                        continue;
                    }
                }
                // Scan found nothing (and drained all rings): stop re-scanning
                // AND re-draining for the remainder of this refill; carve only.
                free_exhausted = true;
            }
            // 2. No free block anywhere: batched bump-carve DIRECTLY into `out`
            //    (E1, task W4). One `carve_batch` fills the whole remaining run
            //    from the current segment's bump in one shot — no BinTable
            //    round-trip, block live + bitmap-allocated, exactly the
            //    handed-out state (byte-identical to the per-block `carve_block`
            //    loop it replaces; see `carve_batch`).
            let n = self.carve_batch(class_idx, block_size, &mut out[filled..]);
            if n != 0 {
                filled += n;
                continue;
            }
            // 3. Current segment is full: reserve a fresh one and retry the
            //    carve. If reserve fails, stop and return what we have.
            match self.reserve_small_segment() {
                Some(_) => {
                    let n = self.carve_batch(class_idx, block_size, &mut out[filled..]);
                    if n != 0 {
                        filled += n;
                        continue;
                    }
                    // A fresh segment that cannot fit even one block indicates
                    // metadata corruption; stop gracefully rather than loop.
                    break;
                }
                None => break,
            }
        }
        filled
    }

    /// Push a batch of blocks of class `class_idx` back onto their owning
    /// segments' `BinTable`s.
    ///
    /// Each block undergoes EXACTLY the same transition as a single
    /// `dealloc_small`: off>=bump guard + `is_free` (M2 double-free) +
    /// `write_next`/`set_head` + `mark_free` + `dec_live_and_maybe_decommit`
    /// (+ `table.recycle` on decommit if fired).
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
    /// views (`SegmentMeta::new`, `bin_table`, `alloc_bitmap`, and — under
    /// decommit — the `bump_of` LOAD) ONCE and write the freelist head ONCE,
    /// instead of once per block.
    ///
    /// ### The TWO guards STAY per-block (they are NOT tautologies)
    ///
    /// 1. `is_free(off)` — a REAL guard: under the documented #164 residual, a
    ///    cross-thread free of a magazine-resident block routes via the ring →
    ///    `reclaim_offset` marks it FREE on the BinTable while it still sits in
    ///    the magazine; this flush must then SKIP it (`is_free == true`) or the
    ///    freelist gets a duplicate. So the run-local chain links ONLY blocks
    ///    that PASS `is_free`.
    /// 2. `off >= bump` (decommit stale-free) — the COMPARE stays per-block;
    ///    only the `bump_of()` LOAD is hoisted. A flush never carves, so `bump`
    ///    cannot advance during a flush; and a decommit-reset of `bump` can only
    ///    happen at the LAST accepted block of a run (see the decommit proof
    ///    below), after which there is no further block in the run to mis-judge.
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
    /// ### Decommit — deferred `dec_live`/decommit is EQUIVALENT
    ///
    /// Within a same-segment run, `live_count` starts at the segment's current
    /// count `L` and drops by one per accepted block. Every un-flushed
    /// same-segment block (still handed out to the user, still in the magazine,
    /// or later in this/another run) counts as live, so `live` reaches 0 iff the
    /// run flushes ALL `L` remaining live blocks — and then ONLY at the LAST
    /// accepted block. The per-block path likewise only decommits at the block
    /// that brings `live` to 0. So running `dec_live_and_maybe_decommit`
    /// per-accepted-block here (AFTER the run's `set_head`, matching the
    /// sequential order where each block's dec-then-decommit follows its own
    /// `set_head`) fires decommit on exactly the same block, exactly once, and
    /// `table.recycle` exactly when it fired. If decommit DOES fire at the last
    /// accepted block, `decommit_empty_segment` re-NULLs every class head
    /// (including this one) and zeroes the bitmap — wiping the chain we just
    /// spliced. That wipe is CORRECT and identical to the sequential path (whose
    /// last block's decommit does the same after its own `set_head`); there is
    /// no subsequent block in the run to be affected, since `live` can only reach
    /// 0 at the last.
    /// # Safety
    ///
    /// The caller must honour the batch-free contract for every entry in
    /// `blocks`. This is the batched analogue of [`dealloc`](AllocCore::dealloc)'s
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
    pub unsafe fn flush_class(&mut self, class_idx: usize, blocks: &[*mut u8]) {
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
            let base = os::segment_base_of_ptr(ptr);
            // Detect the run of consecutive same-segment blocks starting at `i`.
            // Nulls terminate a run (they are handled by the outer loop as
            // no-ops, exactly as the per-block path skips them).
            let mut run_end = i + 1;
            while run_end < blocks.len() {
                let q = blocks[run_end];
                if q.is_null() || os::segment_base_of_ptr(q) != base {
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
            let already_recycled = recycled_bases[..recycled_n].contains(&base);
            if !already_recycled {
                let recycled_now = self.flush_run(class_idx, base, &blocks[i..run_end]);
                if recycled_now && recycled_n < RECYCLED_CAP {
                    recycled_bases[recycled_n] = base;
                    recycled_n += 1;
                }
            }
            i = run_end;
        }
    }

    /// Flush ONE run of blocks that all share segment `base` (Э8). See
    /// `flush_class` for the byte-identical / decommit-equivalence proofs. Every
    /// block in `run` is non-null and has `segment_base_of_ptr(block) == base`.
    ///
    /// L-4 (UBFIX-11): returns `true` iff this run's flush triggered
    /// `release_or_pool_empty_segment(base)` (i.e. the segment reached
    /// `live_count == 0` and was recycled — pooled or released). `flush_class`
    /// uses this to record `base` and skip any LATER same-`base` run within
    /// the same call, instead of re-touching a segment whose metadata may now
    /// be unmapped (released leg) or whose state a blind re-run must not
    /// assume (pooled leg). Always `false` when `alloc-decommit` is off (no
    /// recycle path exists in that config).
    #[inline]
    #[must_use]
    fn flush_run(&mut self, class_idx: usize, base: *mut u8, run: &[*mut u8]) -> bool {
        // PERF-3 Ф2: under `alloc-runfreelist`, detect contiguous-accepted
        // sub-runs (offset-adjacent blocks) and encode them as compact
        // `(start_off, count)` descriptors on the per-segment `RunStack` instead
        // of writing per-block `next` pointers — later drains reconstruct
        // addresses by stride arithmetic, eliminating the dependent-load
        // pointer chase that is this arc's target (plan §1). The detection
        // strategy is SORT-then-detect: the magazine's LIFO refill returns
        // blocks in DESCENDING address order within a refill batch, so an
        // in-place scan of the flush batch finds ~0% offset-adjacent neighbours
        // (empirically measured on the `bench_direct_alloc` pattern — see the Ф2
        // design report); sorting the accepted offsets ASCENDING first turns
        // that same batch into a ~100%-contiguous ascending run. Singletons
        // (runs of 1) and runs whose `RunStack::push` overflows (the per-class
        // `RUNSTACK_CAPACITY = 8` full) fall back to the EXACT classic LIFO-
        // chain path — the linked-list representation and the run-stack coexist;
        // Ф3's drain reads both. The bitmap (`mark_free`) fires for EVERY
        // accepted block regardless of representation (plan §2.3). Under
        // `not(feature = "alloc-runfreelist")` the body is byte-identical to the
        // pre-Ф2 `flush_run` (the neutrality gate).
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

        // PERF-3 Ф2: collect the offsets of ACCEPTED blocks for the run-
        // detection pass below (under `alloc-runfreelist` only). The bound is
        // `FLUSH_RUN_DETECT_CAP`: the production magazine's physical cap is 16
        // (`TCACHE_CAP` in `registry::tcache`, not imported here to respect the
        // `alloc_core` ← `registry` layering), and the overflow-flush batch is
        // `FLUSH_N = TCACHE_CAP/2 = 8`. A same-segment run longer than 16 is a
        // structural impossibility from the magazine; tests may call
        // `flush_class` with larger slices, and those extra blocks simply stay
        // on the classic linked list (the `accepted_n < CAP` guard drops them
        // from the detection buffer — they remain correctly linked and
        // `mark_free`'d by the classic path). M5: `AllocCore` allocates NO
        // `Vec`/`Box` (the reentrancy-free invariant), so a fixed stack array
        // is the only sound choice here.
        #[cfg(feature = "alloc-runfreelist")]
        const FLUSH_RUN_DETECT_CAP: usize = 16;
        #[cfg(feature = "alloc-runfreelist")]
        let mut accepted_offs: [u32; FLUSH_RUN_DETECT_CAP] = [0; FLUSH_RUN_DETECT_CAP];
        #[cfg(feature = "alloc-runfreelist")]
        let mut accepted_n: usize = 0;

        // Capture the segment's CURRENT freelist head ONCE — the first accepted
        // block links to this (matching the first sequential `dealloc_small`,
        // whose `old_head` is exactly this value).
        let old_head = bt.head(class_idx);
        let mut prev_off = old_head; // next-target for the next accepted block
        let mut last_accepted: Option<u32> = None;
        // Track how many blocks were accepted, in source order, so the decommit
        // step can run per accepted block AFTER the run's single `set_head`.
        #[cfg(feature = "alloc-decommit")]
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
            // Guard 2 (per-block): M2 double-free — skip a block already free
            // (e.g. a ring-DF'd magazine resident marked free by reclaim).
            if bm.is_free(off) {
                continue;
            }
            let block_nn = match NonNull::new(ptr) {
                Some(nn) => nn,
                None => continue,
            };
            // PERF-3 Ф2: record the accepted offset for the run-detection pass
            // (under `alloc-runfreelist`). The guard `accepted_n < CAP` keeps
            // the fixed array in bounds; an over-long run simply skips detection
            // for the tail (those blocks stay correctly on the linked list).
            #[cfg(feature = "alloc-runfreelist")]
            if accepted_n < FLUSH_RUN_DETECT_CAP {
                accepted_offs[accepted_n] = off;
                accepted_n += 1;
            }
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
            #[cfg(feature = "alloc-decommit")]
            {
                accepted_count += 1;
            }
        }

        // PERF-3 Ф2 (under `alloc-runfreelist` only): DIVERT contiguous-accepted
        // sub-runs away from the linked list we just built, into `RunStack`
        // descriptors. This runs AFTER the classic chain is fully built, so the
        // non-feature path above is byte-identical. A run-encoded block's `next`
        // word is never read on the drain path (Ф3 reconstructs by stride
        // arithmetic), and the linked-list head is repaired below to reference
        // ONLY the blocks that remain on the linked list. The bitmap stays
        // `mark_free` for every accepted block either way (sole ground truth).
        #[cfg(feature = "alloc-runfreelist")]
        {
            // `run_member[i]` is true iff `accepted_offs[i]` was successfully
            // diverted to a `RunStack` descriptor. `linked_count` counts the
            // blocks that STAY on the linked list (the complement).
            let mut run_member = [false; FLUSH_RUN_DETECT_CAP];
            let mut linked_count = accepted_n;
            if accepted_n >= 2 {
                // Step 1 — sort: build an index permutation `idx[..accepted_n]`
                // that sorts `accepted_offs` ascending. We permute INDICES (not
                // the array itself) so `run_member` lines up with the original
                // source-order slots (which is what the rebuild walk scans).
                // Insertion sort: n ≤ 16, branch-friendly, allocation-free.
                let mut idx: [usize; FLUSH_RUN_DETECT_CAP] = [0; FLUSH_RUN_DETECT_CAP];
                let mut k = 0;
                while k < accepted_n {
                    idx[k] = k;
                    k += 1;
                }
                let mut a = 1;
                while a < accepted_n {
                    let mut b = a;
                    while b > 0 && accepted_offs[idx[b - 1]] > accepted_offs[idx[b]] {
                        idx.swap(b - 1, b);
                        b -= 1;
                    }
                    a += 1;
                }
                // Step 2 — detect: scan the SORTED order for contiguous sub-runs
                // of length ≥ 2 (offset-adjacent: `cur == prev + block_size`).
                // For each, attempt `RunStack::push`; on success mark every
                // member diverted. Overflow (push returns false) or a sub-run of
                // length 1 → those offsets stay on the linked list.
                let block_size = SizeClasses::block_size(class_idx);
                let mut i = 0;
                while i < accepted_n {
                    let mut j = i + 1;
                    while j < accepted_n {
                        let prev = accepted_offs[idx[j - 1]] as usize;
                        let cur = accepted_offs[idx[j]] as usize;
                        if cur != prev + block_size {
                            break;
                        }
                        j += 1;
                    }
                    let run_len = j - i;
                    if run_len >= 2 {
                        let start_off = accepted_offs[idx[i]];
                        // SAFETY: `base` is a live, exclusively-owned segment
                        // whose RunStack region is carved.
                        #[allow(unsafe_code)]
                        if unsafe {
                            super::run_stack::RunStack::push(
                                base,
                                class_idx,
                                start_off,
                                run_len as u16,
                            )
                        } {
                            let mut m = i;
                            while m < j {
                                run_member[idx[m]] = true;
                                m += 1;
                            }
                            linked_count -= run_len;
                        }
                        // Overflow: the whole sub-run stays linked (run_member
                        // remains false for every member) — the classic chain
                        // built in the guard pass stands unchanged for them.
                    }
                    i = j;
                }
            }

            // Step 3 — rebuild: if ANY offsets were diverted, re-link the
            // COMPLEMENT (non-diverted blocks) into a fresh LIFO chain tipped by
            // `old_head`, so the linked list references ONLY non-diverted
            // blocks. We walk `accepted_offs` in SOURCE order (index 0..n),
            // skipping diverted members; the resulting chain is a valid LIFO
            // push of the complement onto `old_head` (the order among complement
            // blocks does not matter for correctness — each becomes head in
            // turn, pointing at the prior — and Ф3's drain walks the chain via
            // `read_next`, not by offset order). If NOTHING was diverted,
            // `linked_count == accepted_n` and the already-built chain stands.
            if linked_count != accepted_n {
                prev_off = old_head;
                last_accepted = None;
                let mut m = 0;
                while m < accepted_n {
                    if !run_member[m] {
                        let off = accepted_offs[m];
                        let block_ptr = Node::deref(base, off as usize);
                        // `block_ptr` is a non-null in-segment address (it came
                        // from a real accepted pointer); `NonNull::new` always
                        // succeeds. The `None` arm is dead but handled for
                        // robustness (skip on a paradoxical null).
                        if let Some(nn) = NonNull::new(block_ptr) {
                            let next_ptr = if prev_off == FREE_LIST_NULL {
                                core::ptr::null_mut()
                            } else {
                                Node::deref(base, prev_off as usize)
                            };
                            // `mark_free` already fired in the guard pass — NOT
                            // repeated (the bitmap is already correct; sole
                            // ground truth, plan §2.3).
                            Node::write_next(nn, next_ptr);
                            prev_off = off;
                            last_accepted = Some(off);
                        }
                    }
                    m += 1;
                }
            }
            // `accepted_count` (used by the decommit pass below) counts EVERY
            // accepted block — including diverted ones — because every accepted
            // block decrements `live_count` exactly once, regardless of
            // representation. Do NOT substitute `linked_count` here.
        }

        // Write the new head ONCE (only if ≥1 block was accepted). Mirrors the
        // final `set_head` of the last sequential `dealloc_small` in the run.
        if let Some(off) = last_accepted {
            bt.set_head(class_idx, off);
        }

        // E3 (task W4): batched `dec_live` (AFTER `set_head`, matching the
        // sequential ordering). `live` can only reach 0 at the LAST accepted
        // block (see `flush_run`'s doc), so one `sub_live(accepted_count)` + a
        // single decommit check is byte-identical to the former per-accepted-block
        // `dec_live_and_maybe_decommit` loop — at most one decommit fires, on the
        // same transition, under the same proviso. Recycle the slot if it fired.
        #[cfg(feature = "alloc-decommit")]
        {
            let small_cur = self.small_cur;
            if Self::dec_live_batch_and_maybe_decommit(base, accepted_count as u32, small_cur) {
                // Mechanism 2 (task #51): pool-or-release instead of the former
                // unconditional recycle.
                self.release_or_pool_empty_segment(base);
                // L-4 (UBFIX-11): report the recycle to `flush_class` so it can
                // skip any later same-`base` run within this call.
                return true;
            }
        }
        false
    }
}
