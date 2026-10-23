//! Diagnostics / test-only hooks for [`AllocCore`](crate::AllocCore) (mechanical split of
//! `alloc_core_small.rs`, task R4-10).
//!
//! This file holds the `impl AllocCore { .. }` block for the inspection and
//! corruption test hooks (`dbg_carve_batch`, `dbg_freelist_head_for`, etc.).
//! Pure code-movement sibling of `alloc_core_small.rs`; no behavior changed.
//!
//! Sol-F1 (task #563, release-readiness review finding F1): every method in
//! this file is a `dbg_*` diagnostic/test-only hook with no caller outside
//! `tests/`/`benches/`/`examples/` (verified — no production call site
//! exists). `AllocCore` is re-exported at the crate root unconditionally
//! (`pub use alloc_core::AllocCore` in `src/lib.rs`, gated only on
//! `alloc-core`), so gating the `alloc_core` MODULE PATH behind `internals`
//! (R34-3/task #522) does NOT hide these inherent methods — they stay
//! reachable as `sefer_alloc::AllocCore::dbg_*` regardless. The whole `impl`
//! block is gated `#[cfg(feature = "internals")]` directly; it also hosts
//! the internals-gated `carve_batch` wrapper (moved here from
//! `alloc_core_small_impl.rs` as pure code movement to keep that file under
//! the 1000-line structure limit); see
//! `alloc_core_core_diag.rs`'s module doc for the fuller rationale (that
//! sibling file needed a split because three of its methods DO have a
//! stable-API caller — none of this file's methods do).
//!
//! Wrapper asymmetry note: the batch drain wrapper (`drain_freelist_batch`)
//! lives next to `try_drain_freelist_batch` in `alloc_core_small_impl.rs`,
//! while the batch carve wrapper (`carve_batch`) lives here — both are pure
//! code movement under the 1000-line structure limit.

#[cfg(feature = "hardened")]
use core::ptr::NonNull;

// R6-T16: every import below feeds only the internals-gated dbg impl.
#[cfg(feature = "internals")]
use crate::alloc_core::node::Node;
#[cfg(feature = "internals")]
use crate::alloc_core::os;
#[cfg(feature = "internals")]
use crate::alloc_core::segment_header::{
    Layout as SegLayout, SegmentHeader, SegmentKind, SegmentMeta, FREE_LIST_NULL,
};
#[cfg(feature = "internals")]
use crate::alloc_core::size_classes::{SizeClasses, SMALL_CLASS_COUNT};

#[cfg(feature = "internals")]
use crate::alloc_core::alloc_core::AllocCore;
#[cfg(feature = "internals")]
use crate::alloc_core::state_byte::StateByte;

#[cfg(feature = "internals")]
impl AllocCore {
    /// E1 (task W4) — **batched bump-carve**. Carve a RUN of up to `out.len()`
    /// `block_size`-strided blocks from the current small segment's bump cursor
    /// in ONE shot, writing them into `out[..n]` and returning `n` (0 if the
    /// segment cannot fit even one block — the caller reserves a fresh segment,
    /// exactly as it does on `carve_block` → `None`).
    ///
    /// ## Byte-identical to `n` sequential `carve_block`s — what is HOISTED
    ///
    /// A run of `carve_block(class_idx, block_size)` calls, after the FIRST,
    /// always finds `bump` already `block_size`-aligned (the previous carve left
    /// `bump = aligned_prev + block_size`, and every class `block_size` is a
    /// multiple of `MIN_BLOCK`), so `align_up(bump, block_size)` is a TAUTOLOGY
    /// from the second block on. We therefore align ONCE (`aligned_start`), then
    /// stride by `block_size`. The following are hoisted across the run because
    /// none of them can change mid-run (a carve run touches only owner-only
    /// bump/live/page-map state, and no free/decommit runs between carves):
    ///   - `SegmentMeta::new` + `bump_of()` LOAD — once (bump only advances by
    ///     our own writes; we track it locally).
    ///   - `align_up` div — once (tautological after block 0).
    ///   - `set_bump` STORE — once, to `aligned_start + n*block_size` (identical
    ///     to the last sequential carve's final bump).
    ///   - `live += n` — one batched saturating add (D1: exactly `n` handed out,
    ///     byte-identical to `n` `inc_live`s; owner-only counter, intermediate
    ///     states unobservable — same argument as `drain_freelist_batch`).
    ///   - `is_decommitted()` check + recommit — once at run start (the flag is
    ///     set only in the decommit path, which cannot run mid-carve).
    ///
    /// ## What STAYS per-block (NOT tautologies)
    ///   - The page-map `class_of`/`set_class` "first class wins" marking is
    ///     applied per DISTINCT payload page: we compute the page of each block
    ///     and call `set_class` only when the page index CHANGES from the prior
    ///     block (byte-identical to `carve_block`'s per-block "mark only if
    ///     `class_of(page).is_none()`", since within a run the first block to
    ///     enter a page is the one that dedicates it, and later same-page blocks
    ///     find it already `Some` → no-op). For `block_size > PAGE` every block
    ///     lands on a fresh page, so this degrades to per-block correctly.
    ///
    /// ## D1 / boundary — preserved EXACTLY
    ///   - Start/state byte (`segment::bitmap::state_byte`): carve NEVER
    ///     touches it (a bump-carved block is allocated by construction, not
    ///     by a state-byte transition) — identical to `carve_block`.
    ///   - D1: `+n` for the `n` blocks handed out.
    ///   - Boundary: `n = min(out.len(), room)` where
    ///     `room = (SEGMENT - aligned_start) / block_size`, so
    ///     `aligned_start + n*block_size <= SEGMENT` — the same
    ///     `aligned + block_size > SEGMENT` per-block check, batched.
    // R12-11 (task #262): `class_idx` is used ONLY for the diagnostic-only
    // page-map marking below (see `PageMap`'s struct doc) — it is genuinely
    // unused without `page-map-diag`.
    #[cfg_attr(not(feature = "page-map-diag"), allow(unused_variables))]
    #[cfg(feature = "internals")]
    pub(in crate::alloc_core) fn carve_batch(
        &mut self,
        class_idx: usize,
        block_size: usize,
        out: &mut [*mut u8],
    ) -> usize {
        self.try_carve_batch(class_idx, block_size, out, StateByte::LIVE)
    }

    /// Owner-only outstanding Small/Primordial credits, independent of decommit.
    /// `ptr` supplies only an address key; metadata reads use the stored root.
    #[doc(hidden)]
    pub fn dbg_live_count_for(&self, ptr: *mut u8) -> Option<u32> {
        let key = os::segment_base_of_ptr(ptr);
        let base = self.table.canonical_base_of(key)?;
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return None;
        }
        Some(SegmentMeta::new(base).live_count_of())
    }
    /// TEST-ONLY (E1, task W4): drive [`carve_batch`](Self::carve_batch)
    /// directly (it is a private internal), so the equivalence regression test
    /// can carve a run and inspect the exact block set without going through the
    /// magazine. Returns the number of blocks carved into `out`.
    #[doc(hidden)]
    pub fn dbg_carve_batch(&mut self, class_idx: usize, out: &mut [*mut u8]) -> usize {
        let block_size = SizeClasses::block_size(class_idx);
        self.carve_batch(class_idx, block_size, out)
    }

    /// TEST-ONLY (Э7, task #161): the segment-relative offset of the head of
    /// `ptr`'s segment's `BinTable[class_idx]` free list, or `FREE_LIST_NULL`
    /// (`u32::MAX`) if the list is empty. Lets the batch-drain regression test
    /// observe `set_head`'s exact post-drain value directly (partial drain →
    /// remaining head; full drain → NULL). Also returns `FREE_LIST_NULL`
    /// for foreign or Large pointers and out-of-range class indices.
    ///
    /// R2-05 (independent src review round 2, task #2007): reads through the
    /// table's own STORED (canonical) pointer, not `ptr`'s caller-derived
    /// address — see `SegmentTable::canonical_base_of`'s doc.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_freelist_head_for(&self, ptr: *mut u8, class_idx: usize) -> u32 {
        let candidate = os::segment_base_of_ptr(ptr);
        let Some(base) = self.table.canonical_base_of(candidate) else {
            return FREE_LIST_NULL;
        };
        // R6-MS-3 (round5 memory_safety_review R5-MS-3): release-mode class-index
        // bounds guard. Belt-and-suspenders alongside `BinTable::head`'s own
        // check — this is a doc-hidden test hook taking a raw caller-controlled
        // `class_idx`, so an out-of-range value must short-circuit here (and not
        // rely on the callee) before any raw `BinTable` access. Under the old
        // `debug_assert!`-only guard this read out of bounds in `production`.
        if class_idx >= SMALL_CLASS_COUNT {
            return FREE_LIST_NULL;
        }
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return FREE_LIST_NULL;
        }
        SegmentMeta::new(base).bin_table().head(class_idx)
    }

    /// TEST-ONLY (Э7, task #161): whether `ptr`'s block's state byte is in
    /// the FREE state (on a free list) — the M2 double-free oracle, B4 (в)
    /// edition. State-only: any FREE byte (regardless of class field) is
    /// "free"; zero/LIVE/MAG bytes are "not free". Foreign and Large
    /// pointers also return `false`; this is not a Large allocation-status
    /// query. Lets the batch-drain test assert every drained block ends
    /// state-issued (not FREE), exactly as `pop_free` leaves it.
    ///
    /// R2-05 (independent src review round 2, task #2007): reads through the
    /// table's own STORED (canonical) pointer, not `ptr`'s caller-derived
    /// address — see `SegmentTable::canonical_base_of`'s doc.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_is_free_for(&self, ptr: *mut u8) -> bool {
        let candidate = os::segment_base_of_ptr(ptr);
        let Some(base) = self.table.canonical_base_of(candidate) else {
            return false;
        };
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return false;
        }
        let off = (ptr as usize - base as usize) as u32;
        // B4 (в): decode the state byte by STATE only (`byte != 0 &&
        // byte >> 6 == FREE`); class-field mismatches are not this probe's
        // concern.
        let byte = SegmentMeta::new(base).state_byte().read(off);
        byte != 0 && (byte >> 6) == crate::alloc_core::state_byte::StateByte::FREE
    }

    /// TEST-ONLY (UBFIX-7, M-3 counterfactual): overwrite the CURRENT freelist
    /// head block's intrusive `next` word for `class_idx` in `ptr`'s segment
    /// with an arbitrary raw pointer, simulating a UAF write into an
    /// already-freed block that corrupts the chain pointer the allocator will
    /// next trust. `next_raw` is written completely unvalidated (unlike the
    /// production `Node::write_next` call sites, which only ever write a
    /// pointer the allocator itself derived) — the caller is responsible for
    /// choosing an out-of-segment value to exercise the hardened guard in
    /// `pop_free`/`drain_freelist_batch`.
    ///
    /// Mirrors the established `dbg_stamp_*`-style field-corruption pattern
    /// (see `AllocCore::dbg_stamp_segment_id`/`dbg_stamp_kind_byte` in
    /// `alloc_core.rs`), applied to a freelist node's `next` word instead of a
    /// header field. Returns `false` (no-op) if the class's free list is
    /// currently empty (nothing to corrupt).
    ///
    /// # Safety
    ///
    /// `ptr` MUST be a valid, live, exclusively-owned allocation pointer whose
    /// segment base is a real, mapped segment owned by this `AllocCore`. The
    /// callee computes `base` from `ptr` and writes an arbitrary `next_raw` into
    /// the head free-list node WITHOUT a membership check; passing an invalid,
    /// interior, stale or foreign `ptr` corrupts allocator metadata or triggers
    /// undefined behaviour.
    /// `ptr` must retain allocator-issued provenance and access permission for
    /// the mapped Small/Primordial segment's initialized BinTable and the
    /// current free-list head's intrusive next word, not just `ptr`'s payload.
    /// These regions must be readable, and the head word writable, through
    /// the derived base for the call, without conflicting aliases or accesses.
    /// A pointer narrowed by a payload reference/slice or `Box` reborrow is
    /// insufficient unless its tag still authorizes those regions; matching
    /// the address does not grant access. `class_idx` must be a valid small
    /// class, and the head must be a mapped, free block of that class. The
    /// arbitrary `next_raw` is only stored, not dereferenced by this hook;
    /// restore it before any path that would trust an invalid continuation.
    #[doc(hidden)]
    #[cfg(feature = "hardened")]
    #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
    pub unsafe fn dbg_corrupt_freelist_head_next(
        &self,
        ptr: *mut u8,
        class_idx: usize,
        next_raw: *mut u8,
    ) -> bool {
        let base = os::segment_base_of_ptr(ptr);
        let head_off = SegmentMeta::new(base).bin_table().head(class_idx);
        if head_off == FREE_LIST_NULL {
            return false;
        }
        let block_ptr = Node::deref(base, head_off as usize);
        let Some(block_nn) = NonNull::new(block_ptr) else {
            return false;
        };
        Node::write_next(block_nn, next_raw);
        true
    }

    /// TEST-ONLY (Э7, task #161): drive `drain_freelist_batch` directly on
    /// `ptr`'s segment so a regression test can observe partial/full-drain
    /// behaviour (return count, resulting `set_head`, per-block bitmap state) in
    /// isolation from the surrounding `refill_class_bump` carve logic.
    ///
    /// # Safety
    ///
    /// `ptr` MUST be a valid, live, exclusively-owned allocation pointer whose
    /// segment is owned by this `AllocCore`. The callee computes `base` from
    /// `ptr` and mutates the free list WITHOUT a membership check; an invalid,
    /// interior, stale or foreign `ptr` corrupts allocator metadata or triggers
    /// undefined behaviour.
    /// `ptr` must retain allocator-issued provenance and access permission for
    /// the mapped Small/Primordial segment's initialized header, BinTable,
    /// state-byte region, and every free-list node the drain may visit. These
    /// regions must remain readable/writable through the derived base for the
    /// call, without conflicting aliases or accesses. A payload-only tag from
    /// a reference/slice or `Box` reborrow is insufficient unless it still
    /// authorizes those regions; an address match does not grant access.
    /// `class_idx` must be a valid small class; visited free-list nodes must
    /// belong to that class and remain mapped. Returned blocks become live
    /// allocations and must not also be issued or freed through another path.
    #[doc(hidden)]
    #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
    pub unsafe fn dbg_drain_freelist_batch(
        &mut self,
        ptr: *mut u8,
        class_idx: usize,
        out: &mut [*mut u8],
    ) -> usize {
        let base = os::segment_base_of_ptr(ptr);
        self.drain_freelist_batch(base, class_idx, out)
    }

    /// TEST-ONLY (B4 variant (в)): raw byte-for-byte read of the per-granule
    /// STATE-BYTE region at `Layout::state_byte_off()` — the authoritative
    /// state record (the former bitmap dump hooks were removed with the dead
    /// bitmaps). Lets tests poison-then-assert that a bogus free leaves the
    /// state bytes pristine, assert granule↔byte-index mapping at the
    /// region's word boundaries, and inspect the metadata region snapshot.
    ///
    /// # Safety
    ///
    /// `ptr` MUST be a valid, live, exclusively-owned allocation pointer
    /// whose segment is owned by this `AllocCore` and retains
    /// allocator-issued provenance and read permission for the mapped
    /// Small/Primordial segment's initialized STATE-BYTE prefix of
    /// `out.len()` bytes, reached through the derived base, for the call.
    /// A payload-only reference/slice or `Box` reborrow tag is insufficient
    /// unless it still authorizes that metadata extent; address equality
    /// does not grant access. No conflicting metadata writes may occur
    /// during the copy, `out` must not alias that source, and the prefix
    /// must not exceed `StateByte::FOOTPRINT`.
    #[doc(hidden)]
    #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
    pub unsafe fn dbg_state_bytes_for(&self, ptr: *mut u8, out: &mut [u8]) {
        assert!(
            out.len() <= crate::alloc_core::state_byte::StateByte::FOOTPRINT,
            "dbg_state_bytes_for: out.len() exceeds StateByte::FOOTPRINT"
        );
        let base = os::segment_base_of_ptr(ptr);
        let state_base = Node::offset(base, SegLayout::state_byte_off());
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = Node::read_u8(Node::offset(state_base, i));
        }
    }

    /// TEST-ONLY (B1, R7 Workstream B): the `committed_payload_end` frontier
    /// of `ptr`'s segment — the byte offset from the segment base up to which
    /// payload pages are committed. Returns `None` if `ptr` is foreign or not
    /// a small/primordial segment. Returns `SEGMENT` on the eager path
    /// (feature-OFF, Unix, miri, or `numa-aware`); on the lazy path returns
    /// the page-rounded `small_meta_end() + LAZY_FIRST_CHUNK` for a fresh
    /// segment (task #1074 — equal to the tight sum on 4 KiB-page hosts).
    ///
    /// R2-05 (independent src review round 2, task #2007): reads through the
    /// table's own STORED (canonical) pointer, not `ptr`'s caller-derived
    /// address — see `SegmentTable::canonical_base_of`'s doc.
    #[doc(hidden)]
    #[must_use]
    #[cfg(any(
        feature = "primordial-lazy-commit",
        feature = "small-segment-lazy-commit"
    ))]
    pub fn dbg_committed_payload_end_for(&self, ptr: *mut u8) -> Option<usize> {
        let candidate = os::segment_base_of_ptr(ptr);
        let base = self.table.canonical_base_of(candidate)?;
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return None;
        }
        Some(SegmentMeta::new(base).committed_payload_end_of())
    }

    /// TEST-ONLY (B2, R7 Workstream B): snapshot of the process-wide
    /// grow-commit counter. Returns the number of successful `commit_pages`
    /// calls on the grow-on-carve path since process start.
    #[doc(hidden)]
    #[must_use]
    #[cfg(any(
        feature = "primordial-lazy-commit",
        feature = "small-segment-lazy-commit"
    ))]
    pub fn dbg_grow_commit_count(&self) -> u64 {
        crate::alloc_core::alloc_core_small::GROW_COMMIT_COUNT
            .load(core::sync::atomic::Ordering::Relaxed)
    }

    /// TEST-ONLY (B2, R7 Workstream B): arm the commit-failure fault injector.
    /// The next `n` calls to the real `os::commit_pages` path will return
    /// `false` without touching the OS, simulating commit-charge exhaustion.
    /// After `n` failures subsequent calls proceed normally.
    ///
    /// CRATE-P2 follow-up: delegates to `aligned_vmem::fault_injection`'s
    /// real-path hook (absorbed from sefer's former `os::COMMIT_FAIL_ARMED`).
    /// Compiled only under the TEST-ONLY `lazy-commit-fault-injection`
    /// fixture feature (R2-19): the ordinary lazy-commit policy features no
    /// longer imply `aligned-vmem/fault-injection`, so plain `production`
    /// never compiles this hook — or vmem's `FAULT_STATE` mutex — at all.
    #[doc(hidden)]
    #[cfg(feature = "lazy-commit-fault-injection")]
    pub fn dbg_arm_commit_fail(&self, n: u32) {
        aligned_vmem::fault_injection::arm_fail_next(n);
    }

    /// TEST-ONLY (B4, R7 Workstream B): arm the "fail the k-th commit" fault
    /// injector. The k-th `commit_pages` call from now (1-based) returns
    /// `false`; all other calls succeed normally. One-shot: after firing, the
    /// hook disarms itself. `k == 0` disarms without firing.
    ///
    /// ADDITIVE to `dbg_arm_commit_fail`: B2's "fail next N" hook is checked
    /// first (and has priority); B4's "fail the k-th" is checked second. Both
    /// can be armed simultaneously (B2 fails the first N, then B4 fails the
    /// k-th of the remaining calls).
    ///
    /// CRATE-P2 follow-up: delegates to `aligned_vmem::fault_injection`'s
    /// real-path hook (absorbed from sefer's former `os::COMMIT_FAIL_AT_*`).
    /// Compiled only under the TEST-ONLY `lazy-commit-fault-injection`
    /// fixture feature (R2-19): the ordinary lazy-commit policy features no
    /// longer imply `aligned-vmem/fault-injection`, so plain `production`
    /// never compiles this hook — or vmem's `FAULT_STATE` mutex — at all.
    #[doc(hidden)]
    #[cfg(feature = "lazy-commit-fault-injection")]
    pub fn dbg_arm_commit_fail_at(&self, k: u32) {
        aligned_vmem::fault_injection::arm_fail_at(k);
    }

    /// TEST-ONLY (P2-C vacuum guard, `r11_ph3a_magazine_refill_partial`):
    /// cumulative count of real commit calls the armed injector actually
    /// forced to fail since process start (both `dbg_arm_commit_fail` and
    /// `dbg_arm_commit_fail_at` tiers; monotonic, never reset). Read-only
    /// `Relaxed` load of `aligned_vmem::fault_injection`'s counter — lets a
    /// test prove a refusal was actually consumed instead of passing
    /// vacuously because the armed commit was never reached. Compiled only
    /// under the TEST-ONLY `lazy-commit-fault-injection` fixture feature
    /// (R2-19), same gating as [`Self::dbg_arm_commit_fail`].
    #[doc(hidden)]
    #[must_use]
    #[cfg(feature = "lazy-commit-fault-injection")]
    pub fn dbg_commit_fail_count(&self) -> u64 {
        aligned_vmem::fault_injection::commit_failure_count()
    }

    /// TEST-ONLY (B2, R7 Workstream B): the `GROW_CHUNK` constant (bytes).
    /// Exposed so tests can compute expected frontier values without
    /// hardcoding the crate's private constant.
    #[doc(hidden)]
    #[must_use]
    #[cfg(any(
        feature = "primordial-lazy-commit",
        feature = "small-segment-lazy-commit"
    ))]
    pub fn dbg_grow_chunk(&self) -> usize {
        crate::alloc_core::alloc_core_small::GROW_CHUNK
    }

    /// TEST-ONLY (B3, R7 Workstream B): the `LAZY_FIRST_CHUNK` constant
    /// (bytes). Exposed so tests can compute the initial lazy frontier
    /// without hardcoding the crate's private constant.
    #[doc(hidden)]
    #[must_use]
    #[cfg(any(
        feature = "primordial-lazy-commit",
        feature = "small-segment-lazy-commit"
    ))]
    pub fn dbg_lazy_first_chunk(&self) -> usize {
        crate::alloc_core::alloc_core_small::LAZY_FIRST_CHUNK
    }

    /// TEST-ONLY (UBFIX-3, H-1/M-1 counterfactual): the segment-relative
    /// payload lower bound for `ptr`'s segment — the same `payload_start`
    /// (`Layout::primordial_meta_end()` for a primordial segment, else
    /// `Layout::small_meta_end()`) checked by `dealloc_small`,
    /// `reclaim_sidecar_record`, and `flush_run` before payload writes.
    /// Exposed so a regression test can construct a metadata-region
    /// address (`base + k` for `k < payload_start`) without hardcoding the
    /// crate's private layout constants (`segment_header::Layout` is
    /// `pub(crate)`, unreachable from `tests/`).
    ///
    /// # Safety
    ///
    /// `ptr` MUST be a valid, live, exclusively-owned allocation pointer whose
    /// segment is owned by this `AllocCore`. The callee reads the segment kind
    /// byte at the computed `base` WITHOUT a membership check; a dangling or
    /// foreign `ptr` triggers undefined behaviour.
    /// `ptr` must retain allocator-issued provenance and read permission for
    /// the initialized kind byte at the mapped segment header, reached through
    /// the derived base, for the call. A payload-only tag from a reference,
    /// slice or `Box` reborrow is insufficient unless it still authorizes that
    /// header access; matching the address does not grant access. No
    /// conflicting header write or segment release may occur during the read.
    #[doc(hidden)]
    #[must_use]
    #[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
    pub unsafe fn dbg_payload_start_for(&self, ptr: *mut u8) -> usize {
        let base = os::segment_base_of_ptr(ptr);
        if SegmentHeader::kind_at(base) == SegmentKind::Primordial {
            SegLayout::primordial_meta_end()
        } else {
            SegLayout::small_meta_end()
        }
    }

    /// TEST-ONLY (B4 (в), F11): the raw STATE nibble (`byte >> 6`) of
    /// `ptr`'s block's state byte — `Some(LIVE|MAG|FREE)` for a granule
    /// whose byte is nonzero, `Some(0)`-mapped as `None`-adjacent zero is
    /// returned as `Some(0)` (never-issued/interior granule); `None` for a
    /// foreign or non-Small/Primordial pointer. Read-only safe observer in
    /// the `dbg_is_free_for` mold: resolves the segment through the table's
    /// STORED canonical root (never the caller-derived address) and reads
    /// ONE byte through the existing `Node::read_u8` seam.
    #[doc(hidden)]
    #[must_use]
    pub fn dbg_state_byte_state_for(&self, ptr: *mut u8) -> Option<u8> {
        let candidate = os::segment_base_of_ptr(ptr);
        let base = self.table.canonical_base_of(candidate)?;
        if !matches!(
            SegmentHeader::kind_at(base),
            SegmentKind::Small | SegmentKind::Primordial
        ) {
            return None;
        }
        let off = (ptr as usize - base as usize) as u32;
        let byte = SegmentMeta::new(base).state_byte().read(off);
        Some(byte >> 6)
    }
}
