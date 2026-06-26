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
//! small and auditable, and matches §6 of `MALLOC_PLAN.md` which names exactly
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
    pub(crate) fn write_next(block: NonNull<u8>, next: *mut u8) {
        let ptr = block.as_ptr() as *mut *mut u8;
        // SAFETY: `block` is non-null (from `NonNull`), aligned to at least the
        // smallest size-class alignment (>= `NODE_SIZE`, which equals the word
        // size — see the size-class table), and the caller guarantees it is
        // exclusively owned (free, not user-visible). Casting to `*mut *mut u8`
        // and writing one word is in-bounds: the block is `>= NODE_SIZE` bytes,
        // and we write exactly `size_of::<*mut u8>()` bytes at offset 0. The
        // write is not visible to any other reference (Phase 8 is
        // single-threaded; `block` is exclusive).
        unsafe { ptr.write_unaligned(next) };
    }

    /// Load the `next` pointer stored in the first word of the free block at
    /// `block`.
    ///
    /// Returns the address previously stored by [`write_next`](Self::write_next),
    /// or null if none. The block MUST currently be a free-list node (caller's
    /// invariant).
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
        // zeroes; the range does not overlap any other live reference (Phase 8
        // is single-threaded and the block is exclusively owned at this point).
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
    pub(crate) fn write_struct<T: Copy>(dst: *mut T, value: T) {
        // SAFETY: caller guarantees `dst` is valid for `size_of::<T>()` bytes,
        // properly aligned, and exclusively owned. `T: Copy` means the write
        // is a plain bit copy (no destructor surprise). The write does not
        // alias any other live reference (Phase 8 is single-threaded and the
        // caller owns the target exclusively).
        unsafe { dst.write(value) };
    }

    /// Write a single byte `value` at `dst`. Used by the page map init.
    ///
    /// `dst` MUST be a valid writable byte inside a segment owned by us.
    pub(crate) fn write_u8(dst: *mut u8, value: u8) {
        // SAFETY: caller guarantees `dst` is a valid, exclusively-owned byte.
        // One-byte write, no alignment requirement.
        unsafe { dst.write(value) };
    }

    /// Write a single `u32` `value` at `dst` (unaligned-tolerant). Used by the
    /// bin table init.
    pub(crate) fn write_u32_unaligned(dst: *mut u32, value: u32) {
        // SAFETY: caller guarantees `dst` is valid for 4 bytes and exclusively
        // owned. `write_unaligned` tolerates any address; one 4-byte write.
        unsafe { dst.write_unaligned(value) };
    }

    /// Read a single byte from `src`. Used by the page map lookup.
    pub(crate) fn read_u8(src: *const u8) -> u8 {
        // SAFETY: caller guarantees `src` is a valid readable byte in a live
        // segment. Single-threaded → no racing write.
        unsafe { src.read() }
    }

    /// Read a single `u32` from `src` (unaligned-tolerant). Used by the bin
    /// table head lookup.
    pub(crate) fn read_u32_unaligned(src: *const u32) -> u32 {
        // SAFETY: caller guarantees `src` is valid for 4 bytes in a live
        // segment. `read_unaligned` tolerates any address.
        unsafe { src.read_unaligned() }
    }

    /// Read a typed value `*src`. The generalised hand-discipline primitive
    /// for reading metadata from segment memory.
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
    pub(crate) fn write_ptr<T>(dst: *mut *const T, value: *const T) {
        // SAFETY: caller guarantees `dst` is valid, pointer-aligned, and
        // exclusively owned (single-writer: the stamping path runs on the
        // owning thread and writes the field at most once per segment).
        unsafe { dst.write(value) };
    }

    /// Dereference a `*const AtomicPtr<u8>` to obtain a shared reference.
    /// Used by the Phase 10 cross-thread free path: a segment header stores a
    /// `*const AtomicPtr<u8>` pointing to the owning heap's thread-free stack
    /// head; the cross-thread freer reads this pointer to CAS-push onto it.
    ///
    /// # Caller's contract
    ///
    /// `ptr` must be a valid, aligned pointer to a live `AtomicPtr<u8>` that
    /// will remain valid for the duration of the returned reference's use.
    /// The pointed-to `AtomicPtr` is `Box`-allocated inside the owning `Heap`
    /// and outlives all segments that reference it (see the heap-lifetime
    /// reasoning in `ThreadFreeStack::push`).
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn deref_atomic_ptr<'a>(ptr: *const core::sync::atomic::AtomicPtr<u8>) -> &'a core::sync::atomic::AtomicPtr<u8> {
        // SAFETY: the caller guarantees `ptr` is a valid, aligned pointer to
        // a live `AtomicPtr<u8>` that will not be dropped during the returned
        // reference's lifetime. The `AtomicPtr` is `Sync`, so shared access
        // from any thread is sound. The heap-lifetime discipline (documented
        // in `ThreadFreeStack::push`) ensures the `Box<AtomicPtr>` outlives
        // all segment headers that reference it.
        unsafe { &*ptr }
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
    pub(crate) fn offset(base: *mut u8, off: usize) -> *mut u8 {
        // SAFETY: caller guarantees `off <= SEGMENT` and `base` is the start of
        // a segment of `>= SEGMENT` bytes, so `base + off` is in-bounds and
        // does not wrap (off <= SEGMENT < isize::MAX). `add` computes the
        // address without dereferencing.
        unsafe { base.add(off) }
    }

    /// Return a `&'static AtomicU32` view over the 4 aligned bytes at
    /// `base + off`. Used by the per-segment `RemoteFreeRing` (the
    /// non-intrusive cross-thread-free queue) to obtain atomic views over its
    /// in-segment slot/cursor words. Mirrors [`atomic_u64_at`]; see that fn's
    /// contract — the same segment-lifetime + alignment-by-construction
    /// reasoning applies, only the field width is 4 bytes and the alignment
    /// requirement is 4 (not 8).
    #[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
    pub(crate) fn atomic_u32_at(
        base: *mut u8,
        off: usize,
    ) -> &'static core::sync::atomic::AtomicU32 {
        let ptr = Self::offset(base, off) as *mut core::sync::atomic::AtomicU32;
        // SAFETY: caller guarantees `base` is a live segment base and `off` is
        // the offset of a properly 4-byte-aligned `AtomicU32` field within a
        // `#[repr(C)]` (or hand-laid-out) metadata region at `base`, with
        // `off + 4` in-bounds. The segment remains mapped for the process
        // lifetime (freed only at `AllocCore::drop`, after all cross-thread
        // frees have quiesced), so the `'static` lifetime is sound. `AtomicU32`
        // is `Sync`, so shared atomic access from any thread is race-free.
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
    /// - `base` MUST be a live segment base owned by this allocator (it will
    ///   remain mapped and the header byte range valid for the process
    ///   lifetime — segments are only freed at `AllocCore::drop`, which runs
    ///   after all adoption has quiesced).
    /// - `off` MUST be the offset of an 8-byte-aligned `u64`/`AtomicU64`
    ///   field within a `#[repr(C)]` header at `base`, and `off + 8` MUST be
    ///   within the segment. The caller (the segment-header module) derives
    ///   `off` via `core::mem::offset_of!` on the `#[repr(C)]` header, which
    ///   yields a properly-aligned in-layout offset — so alignment and bounds
    ///   hold by construction.
    ///
    /// The returned reference carries `'static` because the segment is never
    /// freed while adoption may be in flight (the abandon/adopt protocol
    /// completes before `AllocCore::drop`).
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    pub(crate) fn atomic_u64_at(
        base: *mut u8,
        off: usize,
    ) -> &'static core::sync::atomic::AtomicU64 {
        let ptr = Self::offset(base, off) as *mut core::sync::atomic::AtomicU64;
        // SAFETY: caller guarantees `base` is a live segment base and `off` is
        // the offset of a properly-aligned `AtomicU64` field within a
        // `#[repr(C)]` header at `base`, with `off + 8` in-bounds. The
        // segment remains mapped for the process lifetime (freed only at
        // `AllocCore::drop`, after adoption quiesces), so the `'static`
        // lifetime is sound. `AtomicU64` is `Sync`, so shared atomic access
        // from any thread is race-free.
        unsafe { &*ptr }
    }
}
