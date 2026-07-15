//! [`RegistryChunk`] — a lazily-materialised, fixed-size shard of the
//! registry's slot array (R6-OPT-P0-2, round 1).
//!
//! ## Why chunk the slot array
//!
//! `HeapSlot` (`heap_slot.rs`) inline-holds a `MaybeUninit<HeapCore>` whose
//! size is feature-dependent — from ~104 B (bare `alloc-global` +
//! `alloc-xthread`) up to tens of KiB under `production` (the `fastbin`
//! magazine + `alloc-decommit` large-cache state, PLUS, under
//! `alloc-xthread`, a full inline `HeapOverflow` — 24 KiB of that per slot;
//! that inline cost is round 2's target, untouched here). A single monolithic
//! `[HeapSlot; MAX_HEAPS]` (`MAX_HEAPS = 4096`) is therefore large enough
//! that the WHOLE registry has to be materialised in one `aligned_vmem::
//! reserve_aligned` call the moment ANY heap is claimed — even a process that
//! only ever needs one or two heaps pays the full commit floor.
//!
//! The fix: split the slot array into [`NUM_CHUNKS`] chunks of
//! [`CHUNK_SLOTS`] slots each, and materialise each chunk lazily, on first
//! touch of an index that falls inside it. A process that claims one heap
//! (chunk 0 only) now pays for `CHUNK_SLOTS` slots, not `MAX_HEAPS`.
//!
//! ## Protocol
//!
//! Identical in SHAPE to `bootstrap.rs`'s existing `Registry`-level
//! `UNINIT → INITIALIZING → READY` pointer state-machine, applied per-chunk
//! instead of once globally — see [`super::bootstrap::Registry::slot`] (the
//! single place that resolves a slot index to a `&'static HeapSlot`, and the
//! only code in the crate allowed to dereference chunk memory) for the CAS +
//! spin + publish sequence. This module owns only the chunk's LAYOUT and
//! sizing constants; the state machine lives with `Registry` in
//! `bootstrap.rs` (it needs `Registry::chunks` to drive the CAS, so keeping
//! the state machine there mirrors the existing whole-registry code instead
//! of splitting one atomic protocol across two files).
//!
//! ## Never freed, never moved
//!
//! Like the old monolithic registry, a materialised `RegistryChunk` lives for
//! the process lifetime: `bootstrap.rs`'s per-chunk `ensure_slow` reserves it
//! via `aligned_vmem::reserve_aligned` and `mem::forget`s the reservation.
//! This is load-bearing for `heap_registry::bind_slot_counters`, which plants
//! `&'static` references into slot fields (`&slot.remote.thread_free`,
//! `&slot.overflow`) — those references stay valid only because the chunk
//! backing them is never freed or moved.

// This file uses `unsafe` for exactly one thing: computing chunk byte-size
// and slot-array layout via `core::mem::size_of`/`align_of`, which needs no
// `unsafe` at all — actually this module has NO unsafe of its own; the
// `unsafe` operations (raw-pointer field init, dereferencing a published
// chunk pointer) live in `bootstrap.rs`'s `Registry::slot`/`ensure_chunk_slow`,
// which already carries the crate's `#![allow(unsafe_code)]` seam. This file
// stays plain safe Rust (no `#![allow(unsafe_code)]` needed).

use super::heap_slot::HeapSlot;

/// Number of slots per chunk. 64 is a compromise: small enough that a
/// single-heap process's commit floor (`CHUNK_SLOTS * size_of::<HeapSlot>()`)
/// stays a small, bounded multiple of one `HeapSlot`, large enough that a
/// realistic multi-threaded process (dozens to low hundreds of live heaps)
/// touches only a handful of chunks rather than re-paying the CAS+reserve
/// protocol on every few claims.
pub(crate) const CHUNK_SLOTS: usize = 64;

/// Number of chunks spanning [`MAX_HEAPS`](super::bootstrap::MAX_HEAPS). Kept
/// as a `pub(crate) const` (not merely a local computation) so `bootstrap.rs`
/// can size `Registry::chunks` with it and so a future `MAX_HEAPS` change
/// that does not divide evenly into `CHUNK_SLOTS` fails the compile-time
/// assert below rather than silently truncating the slot space.
pub(crate) const NUM_CHUNKS: usize = super::bootstrap::MAX_HEAPS / CHUNK_SLOTS;

const _: () = assert!(
    NUM_CHUNKS * CHUNK_SLOTS == super::bootstrap::MAX_HEAPS,
    "MAX_HEAPS must be an exact multiple of CHUNK_SLOTS so every slot index \
     0..MAX_HEAPS maps to exactly one (chunk_idx, slot_in_chunk) pair with no \
     remainder slots left unreachable"
);

/// One lazily-materialised shard of the registry's slot array: [`CHUNK_SLOTS`]
/// contiguous [`HeapSlot`]s. Heap-allocated (via `aligned_vmem::
/// reserve_aligned`, the same M5-clean direct-syscall path
/// `bootstrap::ensure_slow` already uses for the top-level `Registry` — NOT
/// `std::alloc`), constructed in-place by `bootstrap.rs`'s per-chunk
/// materialisation path, and never freed or moved for the process lifetime.
///
/// `repr(C)` so the byte layout is deterministic (needed for the raw
/// field-by-field in-place init in `bootstrap.rs`, which writes each slot's
/// non-zero fields directly through pointer arithmetic rather than
/// constructing a `RegistryChunk` value and `ptr::write`-ing it whole — the
/// same reasoning `Registry`'s own in-place init documents: a `CHUNK_SLOTS`-
/// element array of `HeapSlot` can be tens to hundreds of KiB, too large to
/// safely materialise as a stack/const temporary).
#[repr(C)]
pub(crate) struct RegistryChunk {
    pub(crate) slots: [HeapSlot; CHUNK_SLOTS],
}

/// Byte size of one [`RegistryChunk`], rounded up to a multiple of
/// `aligned_vmem::PAGE` (4 KiB) — `reserve_aligned` requires `size` to be a
/// non-zero multiple of `PAGE`.
pub(crate) const CHUNK_SIZE: usize = {
    let raw = core::mem::size_of::<RegistryChunk>();
    let page = aligned_vmem::PAGE;
    (raw + page - 1) & !(page - 1)
};

/// Alignment for the chunk's `reserve_aligned` call. `RegistryChunk`'s
/// natural alignment is `HeapSlot`'s (64 bytes, `#[repr(align(64))]`), well
/// under a page; `reserve_aligned` requires `align >= PAGE`, so we use `PAGE`
/// directly — the chunk occupies whole pages anyway.
pub(crate) const CHUNK_ALIGN: usize = aligned_vmem::PAGE;
