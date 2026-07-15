//! X7 Ф1 (task #189) — generation-table byte-level accessors (mechanical
//! split of `segment_header.rs`, task R6-CQ-7c).
//!
//! The generation table is a standalone byte-array metadata region (like the
//! alloc bitmap), NOT a `SegmentHeader` field, so its accessors are free
//! functions (not `offset_of!`-on-header field reads). Each cell is an
//! `AtomicU8` obtained through the `node` seam (`Node::atomic_u8_at`), mirroring
//! how the atomic-view accessors (`owner_state_atomic` / `deferred_next_atomic`)
//! obtain `&AtomicU64` views over header fields.
//!
//! Memory model (X7 plan §2): owner writes Relaxed (single-writer at block issue
//! — that is Ф3, not this phase); remote reads Relaxed (also Ф3). Both orderings
//! are TSan-clean: the table is the staleness key, not a release/acquire fence.
//! Ф1 wires ONLY the byte-level read/RMW primitives — nothing in the
//! alloc/dealloc/refill/drain paths consults the table yet.
//!
//! Compiled ONLY under `#[cfg(feature = "hardened")]`.

use super::node::Node;
use super::segment_header::{Layout, GEN_TABLE_FOOTPRINT};
use super::size_classes::MIN_BLOCK_SHIFT;

/// Read the generation byte of the block at payload offset `off` in the segment
/// at `base`. `off` is a segment-relative byte offset (same convention as the
/// `payload_off` used elsewhere, e.g. `realloc_inplace_fast_path`'s OPT-G code):
/// it is shifted by `MIN_BLOCK_SHIFT` to index the table. Relaxed atomic load —
/// the remote-free drain compares this against the generation stamped in the
/// ring note (Ф3); a mismatch means the note refers to a past life of the block
/// and is dropped.
///
/// # Caller's contract
///
/// `base` MUST be a live small/primordial segment base whose generation table
/// (at [`Layout::gen_table_off`]) is carved and (under Ф2+) initialised; `off`
/// MUST be a `MIN_BLOCK`-aligned segment-relative offset of a live block (`off
/// >> MIN_BLOCK_SHIFT < GEN_TABLE_FOOTPRINT`, which holds for any `off <
/// SEGMENT`).
///
/// # Safety
///
/// `base` MUST point to a live, mapped, exclusively-owned small/primordial
/// segment whose generation table (`GEN_TABLE_FOOTPRINT` bytes at
/// [`Layout::gen_table_off`]) is carved and initialised. `off` MUST be a
/// `MIN_BLOCK`-aligned segment-relative offset of a live block. The
/// release-asserted index bound is necessary but NOT sufficient —
/// validity/lifetime/exclusivity of `base` are the caller's invariants and
/// cannot be checked by the callee. The atomic view is fabricated as
/// `&'static` from `base`; a dangling or non-segment `base` is undefined
/// behaviour.
#[cfg(feature = "hardened")]
#[doc(hidden)]
#[allow(dead_code)] // wired in Ф1; consumed by Ф2/Ф3 + the layout test
#[inline(always)]
#[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
pub unsafe fn gen_at(base: *mut u8, off: usize) -> u8 {
    let idx = off >> MIN_BLOCK_SHIFT;
    // R2-3: release-surviving index bound (replaces a debug-only debug_assert!
    // that compiled out in release, leaving the atomic load unguarded). The
    // base-validity half of the contract is expressed by the `unsafe fn`
    // boundary above (task #101 / R4-MS-3).
    assert!(
        idx < GEN_TABLE_FOOTPRINT,
        "generation-table index out of range"
    );
    let cell = Node::atomic_u8_at(base, Layout::gen_table_off() + idx);
    cell.load(core::sync::atomic::Ordering::Relaxed)
}

/// Atomically increment the generation byte of the block at payload offset `off`
/// in the segment at `base`, returning the value held BEFORE the increment (the
/// "old life"). Relaxed read-modify-write (`fetch_add(1, Relaxed)`) — the owner
/// is the single writer at block-issue time (Ф3); the increment establishes a
/// new life so any in-flight remote-free note stamped with the OLD generation
/// will mismatch and be dropped on drain.
///
/// The counter is a `u8` and WRAPS at 256 (X7 plan §2.5): after 256
/// re-issues-without-drain a stale note coincides with the current generation
/// and is wrongly honoured — a probabilistic residual, accepted by design and
/// pinned by a boundary test in Ф5.
///
/// # Caller's contract
///
/// Same as [`gen_at`]: `base` is a live segment base with a carved generation
/// table; `off` is a `MIN_BLOCK`-aligned segment-relative offset of a live
/// block.
///
/// # Safety
///
/// Same as [`gen_at`](gen_at#safety): `base` MUST be a live, mapped,
/// exclusively-owned segment with a carved generation table; `off` MUST be a
/// `MIN_BLOCK`-aligned segment-relative offset. The release-asserted index
/// bound is necessary but not sufficient.
#[cfg(feature = "hardened")]
#[doc(hidden)]
#[allow(dead_code)] // wired in Ф1; consumed by Ф2/Ф3 + the layout test
#[inline(always)]
#[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
pub unsafe fn bump_gen(base: *mut u8, off: usize) -> u8 {
    let idx = off >> MIN_BLOCK_SHIFT;
    // R2-3: release-surviving index bound (replaces a debug-only debug_assert!
    // that compiled out in release, leaving the atomic load unguarded). The
    // base-validity half of the contract is expressed by the `unsafe fn`
    // boundary above (task #101 / R4-MS-3).
    assert!(
        idx < GEN_TABLE_FOOTPRINT,
        "generation-table index out of range"
    );
    let cell = Node::atomic_u8_at(base, Layout::gen_table_off() + idx);
    cell.fetch_add(1, core::sync::atomic::Ordering::Relaxed)
}

/// X7 Ф3 — initialise the per-segment generation table to ALL ZEROS at segment
/// creation. Every byte writes `0` (the first life) via [`Node::write_u8`],
/// mirroring [`AllocBitmap::init_in_place`](super::alloc_bitmap::AllocBitmap::init_in_place)'s
/// byte-by-byte zeroing discipline. Compiled ONLY under `#[cfg(feature =
/// "hardened")]`; under any other feature config the generation table does not
/// exist and this function is absent (the non-hardened build is byte-identical).
///
/// # Why zero at init (and NOT re-zero on decommit-recycle)
///
/// The X7 plan §2.2 (decision 2) fixes the semantics: the generation table
/// "lives in segment metadata, is NOT decommitted with the payload, and its
/// numbering is CONTINUOUS across decommit-reset — old generations persist,
/// new blocks continue numbering from wherever they were." So:
///   - at FRESH segment creation (primordial bootstrap + every
///     `reserve_small_segment`): zero the table — no block has ever lived
///     here, so every cell starts at life 0. Without this, a `gen_at` /
///     `bump_gen` Relaxed load on a never-written cell is UB (miri-confirmed
///     during Ф1) — the carried-over Ф1 gap this call closes.
///   - at decommit-reset (`decommit_empty_segment`): do NOT re-zero — the plan
///     explicitly wants continuity. A stale ring note whose generation matches
///     the CURRENT (recycled) life would have to survive an entire
///     `alloc→free→drain→empty→decommit→re-carve→re-issue→drain` cycle AND
///     coincide modulo 256, which is the accepted 1/256 wrap residual (§2.5),
///     not a new hole.
///
/// `base` MUST be a live small/primordial segment base whose generation table
/// (at [`Layout::gen_table_off`], [`GEN_TABLE_FOOTPRINT`] bytes) is carved and
/// about to be consulted.
///
/// # Safety
///
/// `base` MUST point to a live, mapped, exclusively-owned small/primordial
/// segment whose generation table (`GEN_TABLE_FOOTPRINT` bytes at
/// [`Layout::gen_table_off`]) is carved and writable. The callee writes every
/// cell to zero, so a too-short, dangling or shared `base` is undefined
/// behaviour.
#[cfg(feature = "hardened")]
#[doc(hidden)]
#[allow(unsafe_code)] // task #101 / R4-MS-3: `unsafe fn` boundary.
pub unsafe fn init_gen_table_in_place(base: *mut u8) {
    let table_off = Layout::gen_table_off();
    let mut i = 0;
    while i < GEN_TABLE_FOOTPRINT {
        Node::write_u8(Node::offset(base, table_off + i), 0);
        i += 1;
    }
}
