//! [`Node`] — the **intrusive free-list node seam**: the second confined
//! `unsafe` module of the Phase 8 substrate.
//!
//! This generalizes the `concurrent::hand` discipline to raw byte spans. A
//! freed block stores its free-list `next` pointer **inside itself** (the
//! first word of the freed block), so the allocator's free list needs zero
//! out-of-band metadata — the free block IS the node. This is exactly how
//! mimalloc / jemalloc / dlmalloc implement their free lists; the cost is one
//! irreducible pointer write/read that lives here, behind a safe membrane.
//!
//! ## What this module IS and is NOT
//!
//! - IS: a safe-to-use membrane over the two operations a free list needs:
//!   [`Node::write_next`] (store a `*mut u8` into the first word of a free
//!   block) and [`Node::read_next`] (load it back), plus [`Node::deref`] (the
//!   `(segment, offset) → *mut u8` handoff — turning an integer offset into
//!   the writable pointer the caller receives). Every `unsafe` block carries a
//!   `// SAFETY:` proof.
//! - IS NOT: a way to read or write the *payload* of a live allocation. Live
//!   blocks are owned by the caller; we touch memory only at free-list node
//!   time (when the block is in the free list) and at the handoff (handing the
//!   freshly-allocated pointer to the caller, who then owns it).
//!
//! ## Why a SEPARATE seam (and not folded into `os`)
//!
//! The two seams have different proof obligations: `os` proves OS-allocation
//! soundness (the span was reserved by the kernel and is owned by us); `node`
//! proves bounds + alignment soundness (the offset is within a live segment and
//! the write does not escape the block). Keeping them separate keeps each proof
//! small and auditable, and matches §6 of `ALLOC_PLAN.md` which names exactly
//! these two seams.

// The crate is `#![deny(unsafe_code)]` with `alloc-core` on; this is one of the
// TWO documented `unsafe` seams of the Phase 8 substrate (the other is
// [`super::os`]). `allow` lifts the crate-level `deny` for this file only —
// `unsafe` anywhere else in the crate is a hard error.
#![allow(unsafe_code)]

use core::mem::size_of;
use core::ptr::NonNull;

/// A free-list node: the first word of a freed block stores the `*mut u8` of
/// the NEXT free block (or null for the tail). This constant is the number of
/// bytes at the start of a free block that the allocator owns for the node —
/// the minimum block size MUST be `>=` this (enforced by the size-class table).
///
/// We store a full pointer rather than an offset so the read path needs no
/// segment-relative arithmetic (faster) and the write path is one store.
pub(crate) const NODE_SIZE: usize = size_of::<*mut u8>();

/// The intrusive free-list node membrane.
///
/// All functions are pure transforms on `*mut u8` and `NonNull<u8>` — they take
/// the address of a block (already known to be valid for at least
/// [`NODE_SIZE`] bytes by the caller) and read or write its first word. The
/// proofs rest on the caller's bounds guarantee; this module never decides
/// WHICH block is free — that is the safe Cartographer's job.
pub(crate) struct Node;

impl Node {
    /// Store `next` into the first word of the free block at `block`.
    ///
    /// After this, [`read_next`](Self::read_next)`(block)` returns `next`.
    ///
    /// # Safety contract for callers (this fn is safe; the contract is the
    /// caller's invariant)
    ///
    /// `block` MUST point to the start of a free block of size `>=`
    /// [`NODE_SIZE`] that the caller exclusively owns (it is in a free list,
    /// not handed to the user). The block MUST lie within a segment owned by
    /// this allocator. `next` is either null (tail) or another such free
    /// block's address.
    #[inline(always)]
    pub(crate) fn write_next(block: NonNull<u8>, next: *mut u8) {
        let ptr = block.as_ptr() as *mut *mut u8;
        // SAFETY: `block` is non-null (from `NonNull`), aligned to at least the
        // smallest size-class alignment (>= `NODE_SIZE`, which equals the word
        // size — see the size-class table), and the caller guarantees it is
        // exclusively owned (free, not user-visible). Casting to `*mut *mut u8`
        // and writing one word is in-bounds: the block is `>= NODE_SIZE` bytes,
        // and we write exactly `size_of::<*mut u8>()` bytes at offset 0. The
        // write does not alias any other live reference under the SINGLE-WRITER
        // invariant: a free-list node is touched only by the segment's OWNER
        // thread (owner-only free-list discipline), and a cross-thread ("remote")
        // free NEVER writes the body of a block — it enqueues `(offset, class)`
        // into the per-segment ring, leaving the block bytes untouched (see
        // `registry::heap_core::dealloc_routing`, "block bytes untouched" /
        // Variant-2 ring). So while `block` is in this thread's free list, no
        // remote path writes these bytes and `block` is exclusively the owner's.
        unsafe { ptr.write_unaligned(next) };
    }

    /// Load the `next` pointer stored in the first word of the free block at
    /// `block`.
    ///
    /// Returns the address previously stored by [`write_next`](Self::write_next),
    /// or null if none. The block MUST currently be a free-list node (caller's
    /// invariant).
    #[inline(always)]
    pub(crate) fn read_next(block: NonNull<u8>) -> *mut u8 {
        let ptr = block.as_ptr() as *mut *mut u8;
        // SAFETY: same bounds/alignment/exclusivity proof as `write_next`. The
        // load is unaligned-tolerant (`read_unaligned`) so a block whose base
        // is only `NODE_SIZE`-aligned (not `align_of::<*mut u8>()`-aligned on
        // platforms where those differ) is still sound. We read exactly one
        // word; the block is `>= NODE_SIZE` bytes.
        unsafe { ptr.read_unaligned() }
    }

    /// The `(segment_base, offset) → *mut u8` handoff: turn a resolved offset
    /// within a known-live segment into the writable pointer the caller
    /// receives.
    ///
    /// `segment_base` MUST be the SEGMENT-aligned base of a segment owned by
    /// this allocator (returned by [`super::os::Segment::as_ptr`]); `offset`
    /// MUST be `< segment_len` (the caller — the safe Cartographer — guarantees
    /// this by construction). The resulting pointer is valid for whatever block
    /// size the Cartographer carved at that offset.
    #[inline(always)]
    pub(crate) fn deref(segment_base: *mut u8, offset: usize) -> *mut u8 {
        // SAFETY: `segment_base` is the start of an OS-reserved span owned by
        // this allocator (the `Segment` is alive — the safe Cartographer holds
        // a borrow of the segment table that owns it). `offset < segment_len`
        // by the caller's contract, so `segment_base + offset` lies within the
        // span. `add` on a raw pointer computes the address (no dereference),
        // which is always sound; the resulting pointer is dereferenced later
        // only by the user, who owns the block by then.
        unsafe { segment_base.add(offset) }
    }

    /// Zero `len` bytes starting at `ptr`. Used by `alloc_zeroed`.
    ///
    /// `ptr` MUST be valid for `len` bytes (caller's contract — typically a
    /// freshly-allocated block whose full size is `>= len`).
    pub(crate) fn zero(ptr: *mut u8, len: usize) {
        // SAFETY: caller guarantees `[ptr, ptr+len)` is a valid writable range
        // (a freshly-reserved or free block). `write_bytes(0)` fills it with
        // zeroes; the range does not overlap any other live reference under the
        // single-writer invariant — the block is owned by this (owner) thread at
        // this point and a remote free never writes a block's body (it enqueues
        // `(offset, class)` into the per-segment ring; see
        // `registry::heap_core::dealloc_routing`, "block bytes untouched").
        unsafe { core::ptr::write_bytes(ptr, 0, len) };
    }

    /// Copy `count` non-overlapping bytes from `src` to `dst` (the realloc
    /// move). Both ranges MUST be valid for `count` bytes and not overlap.
    pub(crate) fn copy_nonoverlapping(src: *const u8, dst: *mut u8, count: usize) {
        // SAFETY: caller guarantees both ranges are valid for `count` bytes and
        // do not overlap (they are distinct allocations: `src` is the old
        // block, `dst` is the freshly-allocated new block). `copy_nonoverlapping`
        // is a memmove-free byte copy; UB only if the validity or non-overlap
        // contract is violated, which the caller upholds.
        unsafe { core::ptr::copy_nonoverlapping(src, dst, count) };
    }

    /// Write a typed value `value` at `dst`. The generalised hand-discipline
    /// primitive for laying down metadata structures (segment headers, page
    /// maps, bin tables) in segment memory.
    ///
    /// # Safety contract for callers (this fn is safe; the contract is the
    /// caller's invariant)
    ///
    /// `dst` MUST point to `size_of::<T>()` writable bytes inside a segment
    /// owned by this allocator, properly aligned for `T`. The caller must
    /// exclusively own the target range (single-threaded Phase 8 guarantees
    /// this for any metadata write). `T: Copy` so the write is a pure bit
    /// pattern (no drop glue to mis-handle).
    #[inline(always)]
    pub(crate) fn write_struct<T: Copy>(dst: *mut T, value: T) {
        // SAFETY: caller guarantees `dst` is valid for `size_of::<T>()` bytes,
        // properly aligned, and exclusively owned. `T: Copy` means the write
        // is a plain bit copy (no destructor surprise). The write does not
        // alias any other live reference under the single-writer invariant: a
        // metadata field is written only by its owning thread, and a remote free
        // never writes segment/block bodies (it enqueues `(offset, class)` into
        // the per-segment ring; see `registry::heap_core::dealloc_routing`,
        // "block bytes untouched"). Fields read cross-thread are either
        // owner-only (written once, then read-only) or atomic — see the
        // field-specific accessors below.
        unsafe { dst.write(value) };
    }

    /// Write a single byte `value` at `dst`. Used by the page map init.
    ///
    /// `dst` MUST be a valid writable byte inside a segment owned by us.
    #[inline(always)]
    pub(crate) fn write_u8(dst: *mut u8, value: u8) {
        // SAFETY: caller guarantees `dst` is a valid, exclusively-owned byte.
        // One-byte write, no alignment requirement.
        unsafe { dst.write(value) };
    }

    /// Write a single `u32` `value` at `dst` (unaligned-tolerant). Used by the
    /// bin table init.
    #[inline(always)]
    pub(crate) fn write_u32_unaligned(dst: *mut u32, value: u32) {
        // SAFETY: caller guarantees `dst` is valid for 4 bytes and exclusively
        // owned. `write_unaligned` tolerates any address; one 4-byte write.
        unsafe { dst.write_unaligned(value) };
    }

    /// Read a single byte from `src`. Used by the page map lookup.
    #[inline(always)]
    pub(crate) fn read_u8(src: *const u8) -> u8 {
        // SAFETY: caller guarantees `src` is a valid readable byte in a live
        // segment. Single-threaded → no racing write.
        unsafe { src.read() }
    }

    /// Read a single `u32` from `src` (unaligned-tolerant). Used by the bin
    /// table head lookup.
    #[inline(always)]
    pub(crate) fn read_u32_unaligned(src: *const u32) -> u32 {
        // SAFETY: caller guarantees `src` is valid for 4 bytes in a live
        // segment. `read_unaligned` tolerates any address.
        unsafe { src.read_unaligned() }
    }

    /// Read a typed value `*src`. The generalised hand-discipline primitive
    /// for reading metadata from segment memory.
    #[inline(always)]
    pub(crate) fn read_struct<T: Copy>(src: *const T) -> T {
        // SAFETY: caller guarantees `src` is valid for `size_of::<T>()` bytes,
        // properly aligned, in a live segment. `T: Copy` → plain bit copy.
        unsafe { src.read() }
    }

    /// Read a single `usize` from `src` (aligned). Used by the field-specific
    /// segment-header accessors (e.g. reading the owner-only `bump` cursor).
    /// A field read is a single word load — it does NOT race with a concurrent
    /// field write of a DIFFERENT field (the struct-read/struct-write RMW of
    /// `read_struct`/`write_struct` would, because it touches every field).
    ///
    /// `src` MUST be valid for `size_of::<usize>()` bytes, properly aligned for
    /// `usize`, in a live segment.
    #[inline(always)]
    pub(crate) fn read_usize(src: *const usize) -> usize {
        // SAFETY: caller guarantees `src` is valid, aligned, in a live segment.
        // One word load.
        unsafe { src.read() }
    }

    /// Write a single `usize` `value` at `dst` (aligned). Used by the
    /// field-specific segment-header accessor for the owner-only `bump` cursor
    /// (written on every `carve_block`); writing only this field avoids the
    /// full-struct RMW that raced with cross-thread field reads.
    ///
    /// `dst` MUST be valid for `size_of::<usize>()` bytes, properly aligned for
    /// `usize`, and (for fields read cross-thread under the single-writer
    /// discipline) the field must be owner-only or atomic.
    #[inline(always)]
    pub(crate) fn write_usize(dst: *mut usize, value: usize) {
        // SAFETY: caller guarantees `dst` is valid, aligned, exclusively owned
        // (single-writer: the owning thread is the sole writer of its segments'
        // bump cursors). One word store.
        unsafe { dst.write(value) };
    }

    /// Read a single `u32` from `src` (aligned). Used by the field-specific
    /// segment-header accessor for `magic` (the sanity word written once at
    /// segment init and only read thereafter — a field read does not race with
    /// the owner's `bump` field writes because they touch disjoint bytes).
    ///
    /// `src` MUST be valid for 4 bytes, 4-byte aligned, in a live segment.
    // Used by the alloc-xthread cross-thread path; under `alloc-core` alone
    // (no xthread) the call sites are gated out and these helpers look dead.
    #[allow(dead_code)]
    #[inline(always)]
    pub(crate) fn read_u32(src: *const u32) -> u32 {
        // SAFETY: caller guarantees `src` is valid, 4-byte aligned, in a live
        // segment. One 4-byte load.
        unsafe { src.read() }
    }

    /// Read a `*const T` pointer from `src` (aligned, word-sized). Used by the
    /// field-specific segment-header accessor for `owner_thread_free` (a
    /// pointer written ONCE at stamp time and only read cross-thread
    /// thereafter — a field read does not race with the owner's `bump` writes).
    ///
    /// `src` MUST be valid for `size_of::<*const T>()` bytes, properly aligned
    /// for a pointer, in a live segment.
    #[allow(dead_code)]
    #[inline(always)]
    pub(crate) fn read_ptr<T>(src: *const *const T) -> *const T {
        // SAFETY: caller guarantees `src` is valid, pointer-aligned, in a live
        // segment. One word load.
        unsafe { src.read() }
    }

    /// Write a `*const T` pointer `value` at `dst` (aligned, word-sized). Used
    /// by the field-specific segment-header accessor that stamps
    /// `owner_thread_free` ONCE per segment (owner-only write under the
    /// single-writer discipline; cross-thread readers use [`read_ptr`] on the
    /// same field — a single-word write does not race with single-word reads of
    /// disjoint fields the way a full-struct `write_struct` RMW does).
    ///
    /// `dst` MUST be valid for `size_of::<*const T>()` bytes, properly aligned
    /// for a pointer, and exclusively owned.
    #[allow(dead_code)]
    #[inline(always)]
    pub(crate) fn write_ptr<T>(dst: *mut *const T, value: *const T) {
        // SAFETY: caller guarantees `dst` is valid, pointer-aligned, and
        // exclusively owned (single-writer: the stamping path runs on the
        // owning thread and writes the field at most once per segment).
        unsafe { dst.write(value) };
    }

    /// Read a `*mut T` pointer from `src` (aligned, word-sized). RAD-3 (E2,
    /// task #56): the mutable-pointer counterpart of [`read_ptr`] — used by
    /// the field-specific segment-header accessors for the empty-small-segment
    /// pool's intrusive `pool_next`/`pool_prev` links, which are OWNER-ONLY
    /// (mutated on every pool admit/remove, unlike `owner_thread_free`'s
    /// write-once discipline) `*mut u8` fields, not `*const u8`.
    ///
    /// `src` MUST be valid for `size_of::<*mut T>()` bytes, properly aligned
    /// for a pointer, in a live segment.
    #[allow(dead_code)]
    #[inline(always)]
    pub(crate) fn read_ptr_mut<T>(src: *const *mut T) -> *mut T {
        // SAFETY: caller guarantees `src` is valid, pointer-aligned, in a live
        // segment. One word load.
        unsafe { src.read() }
    }

    /// Write a `*mut T` pointer `value` at `dst` (aligned, word-sized). RAD-3
    /// (E2, task #56): the mutable-pointer counterpart of [`write_ptr`]; see
    /// [`read_ptr_mut`] for why the pool's link fields need this variant
    /// rather than the `*const T` one.
    ///
    /// `dst` MUST be valid for `size_of::<*mut T>()` bytes, properly aligned
    /// for a pointer, and exclusively owned (owner-only: only the segment's
    /// owning thread's pool bookkeeping ever writes `pool_next`/`pool_prev`).
    #[allow(dead_code)]
    #[inline(always)]
    pub(crate) fn write_ptr_mut<T>(dst: *mut *mut T, value: *mut T) {
        // SAFETY: caller guarantees `dst` is valid, pointer-aligned, and
        // exclusively owned by the calling (owner) thread.
        unsafe { dst.write(value) };
    }

    /// Compute `base + off` as a `*mut u8` — the address-arithmetic primitive
    /// the safe Cartographer needs to address in-segment metadata. Wrapping
    /// `*mut u8::add` (an `unsafe fn` because a bad offset could wrap or
    /// escape the allocation) in the seam, with the segment-bounds contract
    /// documented on the caller.
    ///
    /// # Caller's contract
    ///
    /// `off` must be `<= SEGMENT` and `base + off` must lie within a single
    /// segment owned by this allocator. The Cartographer only ever passes
    /// offsets derived from the fixed [`super::segment_header::Layout`] or the
    /// bump cursor (both bounded by `SEGMENT`), so this holds by construction.
    #[inline(always)]
    pub(crate) fn offset(base: *mut u8, off: usize) -> *mut u8 {
        // SAFETY: caller guarantees `off <= SEGMENT` and `base` is the start of
        // a segment of `>= SEGMENT` bytes, so `base + off` is in-bounds and
        // does not wrap (off <= SEGMENT < isize::MAX). `add` computes the
        // address without dereferencing.
        unsafe { base.add(off) }
    }

    /// Return a `&'static AtomicU8` view over the single byte at `base + off`.
    /// X7 Ф1 (task #189): used by the per-segment generation table — one
    /// `AtomicU8` per `MIN_BLOCK` granule, the hardened remote-free staleness
    /// guard. The table lives in segment metadata under `#[cfg(feature =
    /// "hardened")]`; the owner writes Relaxed (single-writer at issue time —
    /// Ф3), remote reads Relaxed (also Ф3). Mirrors [`atomic_u32_at`]; the
    /// segment-lifetime reasoning is identical, only the field width is 1 byte
    /// (and `AtomicU8` has no alignment requirement beyond 1).
    #[cfg_attr(not(feature = "hardened"), allow(dead_code))]
    #[allow(dead_code)] // wired in X7 Ф1; consumed by Ф2/Ф3 + the gen-table layout test
    #[inline(always)]
    pub(crate) fn atomic_u8_at(base: *mut u8, off: usize) -> &'static core::sync::atomic::AtomicU8 {
        let ptr = Self::offset(base, off) as *mut core::sync::atomic::AtomicU8;
        // SAFETY: caller guarantees `base` is a live segment base and `off` is
        // the offset of a byte within a metadata region at `base`, with
        // `off + 1` in-bounds. LIFETIME (see the `'static` note on
        // [`atomic_u64_at`] for the full argument): the `'static` here is NOT
        // "the segment is mapped for the whole process" — Large segments are
        // released mid-process (`AllocCore::reclaim_large_segment` /
        // large-cache eviction → `os::release_segment`), and no `HeapCore` is
        // ever dropped. The reference is valid only WHILE `base`'s segment is
        // registered in its owning heap's table; the CALLER must supply the
        // per-path liveness argument that the segment cannot be released under
        // this access (for the remote-free paths: the double-push guard in
        // `alloc_core::deferred_large::push` and the "(a)/(b) indistinguishable,
        // dangling free → fault" reasoning in
        // `registry::heap_core::dealloc_routing`). `AtomicU8` is `Sync`, so
        // shared atomic access from any thread is race-free.
        unsafe { &*ptr }
    }

    /// Return a `&'static AtomicU32` view over the 4 aligned bytes at
    /// `base + off`. Used by the per-segment `RemoteFreeRing` (the
    /// non-intrusive cross-thread-free queue) to obtain atomic views over its
    /// in-segment slot/cursor words. Mirrors [`atomic_u64_at`]; see that fn's
    /// contract — the same segment-lifetime + alignment-by-construction
    /// reasoning applies, only the field width is 4 bytes and the alignment
    /// requirement is 4 (not 8).
    #[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn atomic_u32_at(
        base: *mut u8,
        off: usize,
    ) -> &'static core::sync::atomic::AtomicU32 {
        let ptr = Self::offset(base, off) as *mut core::sync::atomic::AtomicU32;
        // SAFETY: caller guarantees `base` is a live segment base and `off` is
        // the offset of a properly 4-byte-aligned `AtomicU32` field within a
        // `#[repr(C)]` (or hand-laid-out) metadata region at `base`, with
        // `off + 4` in-bounds. LIFETIME (see the `'static` note on
        // [`atomic_u64_at`] for the full argument): the `'static` is NOT "mapped
        // for the whole process" — Large segments are released mid-process
        // (`AllocCore::reclaim_large_segment` / large-cache eviction →
        // `os::release_segment`). The reference is valid only WHILE `base`'s
        // segment is registered in its owning heap's table; the CALLER must
        // supply the per-path liveness argument that the segment cannot be
        // released under this access (for the remote-free ring paths: the
        // double-push guard in `alloc_core::deferred_large::push` and the
        // "(a)/(b) indistinguishable, dangling free → fault" reasoning in
        // `registry::heap_core::dealloc_routing`). `AtomicU32` is `Sync`, so
        // shared atomic access from any thread is race-free.
        unsafe { &*ptr }
    }

    /// Write a single `u32` `value` at `dst` (aligned — used by the ring init).
    /// Same contract as [`write_u32_unaligned`] but requires 4-byte alignment
    /// (the ring slots are 4-aligned by the Layout).
    #[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
    pub(crate) fn write_u32(dst: *mut u32, value: u32) {
        // SAFETY: caller guarantees `dst` is valid for 4 bytes, 4-byte aligned,
        // and exclusively owned (bootstrap-time init). One 4-byte write.
        unsafe { dst.write(value) };
    }

    /// Return a `&'static AtomicU64` view over the 8 aligned bytes at
    /// `base + off`. Used by the Phase 12.4 adoption path to obtain an atomic
    /// view over a segment header field (the `owner_state` CAS is the M9
    /// linearization point; a plain struct-field read would be a data race).
    ///
    /// # Caller's contract
    ///
    /// - `base` MUST be a live segment base owned by this allocator, and the
    ///   caller MUST hold a liveness argument that it STAYS live (registered in
    ///   its owning heap's segment table) for the duration of the access — see
    ///   the LIFETIME note below.
    /// - `off` MUST be the offset of an 8-byte-aligned `u64`/`AtomicU64`
    ///   field within a `#[repr(C)]` header at `base`, and `off + 8` MUST be
    ///   within the segment. The caller (the segment-header module) derives
    ///   `off` via `core::mem::offset_of!` on the `#[repr(C)]` header, which
    ///   yields a properly-aligned in-layout offset — so alignment and bounds
    ///   hold by construction.
    ///
    /// ## LIFETIME — why `'static`, and what actually backs it
    ///
    /// The `'static` here is a SEAM convenience (it lets `registry::heap_core`,
    /// which is `#![deny(unsafe_code)]`, hold an `&AtomicU64` into segment
    /// metadata without its own pointer seam), NOT a claim that the segment
    /// lives for the whole process. The old "segments are only freed at
    /// `AllocCore::drop`, after all cross-thread frees/adoption have quiesced"
    /// wording was FALSE on both halves and is removed:
    ///
    /// - Large segments are released MID-process —
    ///   `AllocCore::reclaim_large_segment` → `os::release_segment`, plus the
    ///   several `alloc-decommit`/large-cache eviction release sites in
    ///   `alloc_core.rs`. So a segment header CAN become unmapped while the
    ///   process runs.
    /// - "after cross-thread frees have quiesced" was an argument for the
    ///   long-removed public alloc façade (task #17); today nothing enforces it
    ///   and — because registry `HeapCore`s are never dropped — nothing needs it.
    ///
    /// The real safety of remote accesses rests on THIN per-path liveness
    /// arguments living in OTHER files, which the caller is obliged to carry:
    /// the double-push guard in `alloc_core::deferred_large::push`
    /// (`push_large_deferred_free`, "claim once") and the honest
    /// "(a) live-foreign / (b) already-released are O(1)-indistinguishable;
    /// a dangling free into a released segment is fundamentally UB" reasoning in
    /// `registry::heap_core::dealloc_routing`. This same class of "dormant"
    /// substrate hazard is what the REACTIVATION HAZARD note in
    /// `registry::heap_registry` warns about: a future decommit-when-empty
    /// policy that releases segments mid-process would invalidate any naive
    /// blanket-`'static` read here. Treat the returned reference as valid ONLY
    /// while `base`'s segment is registered in its owning heap's table.
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn atomic_u64_at(
        base: *mut u8,
        off: usize,
    ) -> &'static core::sync::atomic::AtomicU64 {
        let ptr = Self::offset(base, off) as *mut core::sync::atomic::AtomicU64;
        // SAFETY: caller guarantees `base` is a live segment base and `off` is
        // the offset of a properly-aligned `AtomicU64` field within a
        // `#[repr(C)]` header at `base`, with `off + 8` in-bounds. The `'static`
        // is sound only WHILE `base`'s segment is registered in its owning
        // heap's table; the caller carries the per-path liveness argument (see
        // the LIFETIME note in this fn's doc — segments ARE released mid-process,
        // so this is not a whole-process mapping guarantee). `AtomicU64` is
        // `Sync`, so shared atomic access from any thread is race-free.
        unsafe { &*ptr }
    }

    /// Return a `&'static AtomicPtr<u8>` view over the pointee of `ptr`.
    /// 0.3.0 (task A1): used by [`HeapCore::push_large_deferred_free`]
    /// (`registry::heap_core`) to deref a REMOTE `owner_thread_free_head()`
    /// pointer (a `*const AtomicPtr<u8>` obtained from another heap's segment
    /// header stamp) so the cross-thread Large-segment reclaim path can CAS
    /// onto it without an `unsafe` block outside the allowed seam list —
    /// `registry::heap_core` is `#![forbid]`/`#![deny(unsafe_code)]` (it is
    /// NOT one of the whitelisted seam modules in `src/lib.rs`), so the
    /// pointer-to-reference conversion is centralised here instead.
    ///
    /// 0.3.x task #132: the cross-thread Large-segment reclaim path uses this
    /// SAME accessor for the identical purpose, deref'ing a REMOTE
    /// `owner_thread_free_at(base)` stamp that points at another owner's
    /// thread-free-stack head atomic. Post-W3 (task #13) that head is a
    /// slot-resident `'static` field (`HeapSlot::thread_free`) or the fallback
    /// process-`'static` `FALLBACK_TFS` — NOT a leaked `Box` — so its address
    /// is stable for the process lifetime (see the caller's contract below).
    ///
    /// [`HeapCore::push_large_deferred_free`]: crate::registry::heap_core::HeapCore
    ///
    /// # Caller's contract
    ///
    /// `ptr` MUST be the address of a live, process-`'static` `AtomicPtr<u8>`
    /// that is NOT an inline field of any `HeapCore`. task H1 moved the
    /// cross-thread free-stack head OUT of `HeapCore` for exactly this reason
    /// (the head is CASed by remotes while the owner holds a protected `&mut
    /// HeapCore`, so it must not lie inside that struct's byte range). Today
    /// the two live pointees are:
    ///
    /// - (a) a registry slot's
    ///   [`HeapSlot::thread_free`](crate::registry::HeapSlot) field — the slot
    ///   array is `'static`; or
    /// - (b) the fallback heap's `FALLBACK_TFS` `static AtomicPtr<u8>`
    ///   (`global::fallback`) — a process-`'static` standalone atomic.
    ///
    /// Both callers derive `ptr` exclusively from
    /// `thread_free_head()`/`owner_thread_free_at`, which only ever produce such
    /// addresses (the stamp site writes the slot-field / fallback-static
    /// address). In both cases the pointee is a `'static` never dropped for the
    /// process lifetime, so the returned `'static` reference's lifetime is
    /// sound. NOTE — unlike the segment-metadata accessors above
    /// ([`atomic_u64_at`] et al., whose `'static` is only valid WHILE the
    /// segment stays registered because segments ARE released mid-process), the
    /// two pointees here are GENUINELY process-`'static`: a registry slot's
    /// `thread_free` (the slot array never shrinks) and `FALLBACK_TFS` (a
    /// program-lifetime static). So this accessor's `'static` needs no per-path
    /// liveness argument — the exhaustive (a)/(b) enumeration above IS the
    /// liveness proof, and keeping it exhaustive is load-bearing. `AtomicPtr` is
    /// `Sync`, so shared atomic access from any thread is race-free. Because the
    /// pointee is outside every `&mut HeapCore` retag
    /// range, a remote write here also does not conflict with the owner's
    /// `alloc(&mut self)` protector (the H1 fix — see `HeapCore::thread_free`).
    #[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn atomic_ptr_ref(
        ptr: *const core::sync::atomic::AtomicPtr<u8>,
    ) -> &'static core::sync::atomic::AtomicPtr<u8> {
        // Task #142 (cross-thread aliasing soundness): materialize the shared
        // atomic through EXPOSED provenance (a "wildcard" pointer), NOT a
        // reference-derived provenance the stamp inherited from the owner. The
        // stamp site (`stamp_owner_thread_free` callers) took `ptr` from the
        // owning thread; if a REMOTE thread reconstructed `&*ptr` under a
        // reference tag and wrote (the deferred-free Treiber
        // `compare_exchange`), that write would be "foreign" to the stamp's tag
        // and DISABLE it, so a SECOND remote reading through a sibling `&*ptr`
        // would hit UB (Stacked/Tree Borrows). The stamp sites therefore
        // `expose_provenance()` the atomic; reconstructing here via
        // `with_exposed_provenance_mut` yields a pointer that is NOT a child of
        // any owner-rooted borrow tree, so concurrent cross-thread
        // interior-mutable access by multiple remotes no longer disables a
        // shared parent tag. `AtomicPtr` is `Sync` + interior-mutable, so
        // shared atomic writes through the resulting `&AtomicPtr` are sound.
        //
        // task H1: the pointee is now a slot-resident / fallback-`static`
        // `AtomicPtr` OUTSIDE any `HeapCore` (see the caller's-contract note
        // above), so the remote write also cannot conflict with the owner's
        // `alloc(&mut self)` protector — the remote-vs-remote AND the
        // remote-vs-owner conflict classes are both closed.
        let exposed =
            core::ptr::with_exposed_provenance_mut::<core::sync::atomic::AtomicPtr<u8>>(ptr.addr());
        // SAFETY: caller's contract above — `ptr`'s address is the stable
        // address of a live, process-`'static` `AtomicPtr<u8>` (a registry
        // slot's `thread_free` field, or the fallback's `FALLBACK_TFS` static)
        // whose provenance the stamp site exposed, so the wildcard pointer
        // validly covers it.
        unsafe { &*exposed }
    }
}
