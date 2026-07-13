//! [`SegmentHeader`] — the per-segment metadata block that lives at offset 0
//! of every segment, and [`PageMap`] / [`BinTable`] — the per-segment page
//! descriptors and per-size-class free bins, all carved from segment memory.
//!
//! These structures are the **self-hosted metadata** of the Phase 8 substrate
//! (§3 / §5 P8 of `ALLOC_PLAN.md`): they live INSIDE the segments they
//! describe, not in a `Vec`/`HashSet` on the global allocator. This is the
//! Membrane Inversion — the safe slot-table discipline governs OS memory
//! instead of consuming `std` collections.
//!
//! ## This file is PURE SAFE DATA + ARITHMETIC
//!
//! Every raw memory touch goes through the [`node`](super::node) seam. This
//! file declares only `#[repr(C)]` struct layouts, `const` offsets, and
//! methods that compute indices / route reads & writes through `Node`. There
//! is NO `unsafe` here — so the crate's structural promise ("`unsafe` lives
//! ONLY in `os` + `node`") is upheld by the compiler.
//!
//! ## Layout of a small segment
//!
//! ```text
//!   SEGMENT-aligned base
//!   ┌─────────────────────────────────────────────────────────────┐
//!   │ SegmentHeader (fixed-size, page-0)                          │
//!   │  • magic, kind, segment_id                                  │
//!   │  • bump cursor (next uncarved page offset, in bytes)        │
//!   │  • BinTable:  per-class free-list head OFFSETS (u32 each)   │
//!   │  • PageMap:    per-page descriptor (which class, or free)   │
//!   ├─────────────────────────────────────────────────────────────┤
//!   │ payload pages (carved bump-allocated into class runs)       │
//!   └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! `segment_of(ptr)` masks the low bits of `ptr` to find the segment base in
//! O(1); the header at offset 0 then tells the Cartographer everything about
//! that segment. Large/huge segments carry only `(size, align)` of their
//! single allocation — no page map (one allocation per segment).

use core::mem::size_of;

use super::node::Node;
use super::os::PAGE;
#[cfg(feature = "hardened")]
use super::os::SEGMENT;
use super::size_classes::SMALL_CLASS_COUNT;
#[cfg(feature = "hardened")]
use super::size_classes::{MIN_BLOCK, MIN_BLOCK_SHIFT};

/// Magic value written to every segment header at creation. Used as a sanity
/// check that a computed segment base really is one of our segments (defence
/// against a foreign pointer being passed to `dealloc`).
pub(crate) const SEGMENT_MAGIC: u32 = 0x5E_F5_E0_01;

// ---------------------------------------------------------------------------
// Phase 12.4 — segment ownership state (the M9 adoption linearization point).
//
// Each small/primordial segment carries an `owner_state: u64` field packing:
//
//   bits [0]      : state    — 0 = LIVE (owned by a heap), 1 = ABANDONED
//   bits [1..32]  : owner_id — the owning heap's registry slot index
//                              (MAX_HEAPS = 4096 ≪ 2^31, so 31 bits is ample)
//   bits [32..63] : generation — bumped on each adopt; the M9 coherence key
//                                (a stale pointer reading an old generation
//                                refuses — see §2.4 / §2.6 M9)
//
// The Abandoned→Live CAS on `owner_state` is the SINGLE linearization point
// of adoption (M9): exactly one adopter wins per generation. The packing is
// plain data (laid down / read through the `node` seam, like the rest of the
// header) so this file stays `unsafe`-free.
//
// `cfg_attr(not(alloc-global), allow(dead_code))`: the helpers below are used
// by the registry's abandon/adopt path, which is `alloc-global`-gated. Without
// `alloc-global` the registry does not compile, so the helpers appear unused —
// but they are part of the segment header's documented contract (the fields
// exist in every build's layout), so we silence the dead-code lint rather than
// gate the fields themselves.
// ---------------------------------------------------------------------------

/// Owner-state bit layout.
#[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
pub(crate) const OWNER_STATE_LIVE: u64 = 0;
#[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
pub(crate) const OWNER_STATE_ABANDONED: u64 = 1;
/// Mask for the state bit (bit 0).
const OWNER_STATE_MASK: u64 = 0x1;
/// Bit-shift for the owner heap id field (starts at bit 1).
const OWNER_ID_SHIFT: u32 = 1;
const OWNER_ID_MASK: u64 = ((1u64 << 31) - 1) << OWNER_ID_SHIFT;
/// Bit-shift for the generation field (starts at bit 32).
const OWNER_GEN_SHIFT: u32 = 32;
#[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
const OWNER_GEN_MASK: u64 = (u32::MAX as u64) << OWNER_GEN_SHIFT;

/// Sentinel owner id meaning "not bound to any heap yet" (a freshly-reserved
/// segment before its first stamp). Distinct from a real slot index (which is
/// `< MAX_HEAPS`); adoption skips such segments.
pub(crate) const OWNER_ID_NONE: u32 = 0x7FFF_FFFF;

/// Pack `(state, owner_id, generation)` into one `u64` word (the layout
/// documented above the [`OWNER_STATE_LIVE`] constant). `const` so the header
/// constructors can build the initial packed word at compile time.
#[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
#[inline(always)]
pub(crate) const fn pack_owner(state: u64, owner_id: u32, generation: u32) -> u64 {
    (state & OWNER_STATE_MASK)
        | ((owner_id as u64) << OWNER_ID_SHIFT)
        | ((generation as u64) << OWNER_GEN_SHIFT)
}

/// Unpack the state bit (0 = LIVE, 1 = ABANDONED) from an owner-state word.
#[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
#[inline(always)]
pub(crate) const fn unpack_owner_state(word: u64) -> u64 {
    word & OWNER_STATE_MASK
}

/// Unpack the owner heap id from an owner-state word.
#[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
#[inline(always)]
pub(crate) const fn unpack_owner_id(word: u64) -> u32 {
    ((word & OWNER_ID_MASK) >> OWNER_ID_SHIFT) as u32
}

/// Unpack the generation from an owner-state word.
#[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
pub(crate) const fn unpack_owner_gen(word: u64) -> u32 {
    ((word & OWNER_GEN_MASK) >> OWNER_GEN_SHIFT) as u32
}

/// The number of pages in one segment (`SEGMENT / PAGE` = 1024 for the default
/// 4 MiB / 4 KiB pair). The `PageMap` has exactly this many entries.
pub(crate) const PAGES_PER_SEGMENT: usize = super::os::SEGMENT / PAGE;

/// X7 Ф1 (task #189): the byte footprint of the per-segment **generation
/// table** — the hardened remote-free staleness guard. One byte per
/// `MIN_BLOCK` granule of the WHOLE segment, so every segment-relative offset
/// `off` indexes a unique cell at `off >> MIN_BLOCK_SHIFT` without needing a
/// payload-vs-metadata bounds distinction (the metadata granules' cells are
/// simply never read/written — no block starts there, exactly like the
/// [`AllocBitmap`](super::alloc_bitmap::AllocBitmap) discipline). For the
/// default 4 MiB / 16 B pair this is `4 MiB / 16 = 262 144` bytes = 256 KiB
/// (64 pages) — the ~6–7% metadata overhead the X7 plan §1/§2.1 budgets.
///
/// Computed from the constants (not a hardcoded literal) so it cannot drift if
/// `SEGMENT` / `MIN_BLOCK` change. `MIN_BLOCK` divides `SEGMENT` (both are
/// powers of two), so the division is exact — no rounding is needed.
///
/// Compiled ONLY under `#[cfg(feature = "hardened")]`; outside that feature the
/// generation table does not exist and the segment byte layout is unchanged.
#[cfg(feature = "hardened")]
#[doc(hidden)]
#[allow(dead_code)] // wired in Ф1; consumed by Ф2/Ф3 + the layout test
pub const GEN_TABLE_FOOTPRINT: usize = SEGMENT / MIN_BLOCK;

/// Kind of a segment. Lives in the header so `segment_of(ptr)` immediately
/// tells the Cartographer how to handle a pointer into this segment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum SegmentKind {
    /// The primordial segment: hosts the global `SegmentTable` registry in
    /// its early bytes (after the header). Behaves as a small segment for the
    /// remaining payload.
    Primordial = 0,
    /// A small-segment: serves small size-class allocations via per-class free
    /// lists + a bump cursor over its payload pages.
    Small = 1,
    /// A large/huge segment: holds ONE allocation of arbitrary size/align. No
    /// page map; the header records the allocation's layout.
    Large = 2,
    /// L-5 (UBFIX-11): NOT a real segment kind ever written by
    /// `SegmentHeader::small`/`large` — a REJECT sentinel returned by
    /// [`SegmentHeader::kind_at`] when the raw `kind` byte is anything other
    /// than the three legitimate discriminants (0/1/2). Exists so a
    /// corrupted/garbled `kind` byte (e.g. a wild write from an unrelated
    /// heap-overflow, or the aftermath of an H-1-class defect before its fix)
    /// is CONTAINED rather than silently amplified into a specific wrong
    /// kind. See `kind_at`'s doc for the full rationale: every caller of
    /// `kind_at` uses `==`/`matches!` against a SPECIFIC expected kind (never
    /// an exhaustive match with a catch-all), so `Unknown` naturally fails
    /// every such check and each call site's existing "not this kind" branch
    /// becomes the safe no-op/reject path for free — no caller needed to
    /// change to benefit from this guard, except the one exhaustive `match`
    /// in `AllocCore::dealloc`, which gained an explicit `Unknown => no-op`
    /// arm.
    Unknown = 0xFF,
}

/// Per-page descriptor: the FIRST size class carved into a page, or `Free` if
/// the page is still uncarved. Encoded as a `u8` (~49 small classes + the
/// `Free`/`Meta` sentinels below).
///
/// NOTE: under this substrate's shared-bump-cursor model a page is
/// **mixed-class** — one segment-wide bump cursor interleaves blocks of
/// different classes, so consecutive carves of different classes are adjacent
/// and share pages. `set_class` records only the FIRST class to touch a page
/// (the "first class wins" rule, applied in `carve_block`/`carve_batch`); later
/// blocks of other classes landing on the same page are NOT re-recorded.
/// `PageMap` is therefore NOT a reliable class oracle — no production `dealloc`
/// path derives a block's class from it (see the [`PageMap`] struct doc and §13
/// of `RACE_DRAIN_RECLAIM.md`). This deliberately differs from mimalloc's "page
/// is owned by one size class" model, which would require a per-class bump
/// cursor.
pub(crate) enum PageClass {
    /// The page is uncarved (still part of the bump region).
    Free = 0xFF,
    /// The page is metadata (the header / page map / bin table).
    Meta = 0xFE,
}

impl PageClass {
    /// Encode a small-class index as a `PageClass::Class(c)` byte.
    pub(crate) const fn encode_class(c: usize) -> u8 {
        debug_assert!(c < SMALL_CLASS_COUNT, "class_idx out of range");
        c as u8
    }
    /// Decode a page-map byte. Returns `Some(class_idx)` for a class page,
    /// `None` for `Free` / `Meta`.
    pub(crate) fn decode(b: u8) -> Option<usize> {
        match b {
            0xFF | 0xFE => None,
            c => {
                debug_assert!((c as usize) < SMALL_CLASS_COUNT, "corrupt page map entry");
                Some(c as usize)
            }
        }
    }
}

/// A fixed-size `SegmentHeader` laid down at offset 0 of every segment.
///
/// `#[repr(C)]` so the layout is deterministic and the bootstrap can compute
/// the page-map / bin-table offsets after it.
///
/// ## PERF-PASS-5 (G7, task #53) — field order is cache-line-aware
///
/// Field DECLARATION order here is the PHYSICAL byte order (guaranteed by
/// `#[repr(C)]`, unlike `AllocCore`'s `repr(Rust)`) — and `#[repr(C)]` does
/// NOT reorder for padding the way `repr(Rust)` does, so the declaration
/// order must ALREADY be alignment-descending within each hot/cold group to
/// avoid re-introducing padding gaps (a naive "hot fields first, in prose
/// order" declaration measurably grew this struct to 112 bytes — verified
/// with `-Zprint-type-sizes` while designing this reorder — because e.g.
/// `magic: u32` immediately followed by `bump: usize` forces a 4-byte gap to
/// re-align `bump` to 8). The layout actually used below is
/// alignment-descending within each group (8-byte fields, then 4-byte
/// fields, then the 1-byte `kind`), which packs with ZERO internal padding.
///
/// The small-segment per-operation hot set — `bump` (refill/carve cursor,
/// rewritten on every `carve_block`), `owner_thread_free` (cross-thread free
/// routing), `owner_state` (adoption CAS + owner-id compare), `magic`
/// (dealloc-routing base validation), `live_count` / `decommitted` (M6
/// decommit bookkeeping, touched on every own-thread free/carve under
/// `alloc-decommit`), `ring_drain_head` (task #52's drain-guard cache,
/// read/written on every refill-miss free-list scan), and `kind`
/// (dealloc-routing dispatch) — is declared FIRST: 8+8+8 (three 8-byte
/// fields, offsets 0/8/16) + 4+4+4+4 (four 4-byte fields, offsets
/// 24/28/32/36) + 1 (`kind`, offset 40) = 41 bytes, all naturally aligned
/// with no INTERNAL gaps, so the whole hot set occupies bytes 0..41 —
/// comfortably inside the first 64-byte cache line. The Large-only /
/// teardown-only / unregister-only cold fields (`large_size`, `large_align`,
/// `span_usable`, `reservation`, `reservation_len`, `next_abandoned`,
/// `pool_next`, `pool_prev`, `segment_id`, `node_id`) are declared AFTER,
/// likewise alignment-descending; a 7-byte tail-alignment gap after `kind`
/// (offset 41..48, needed to re-align the first cold 8-byte field,
/// `large_size`, to its natural 8-byte boundary) pushes the cold set to bytes
/// 48..120 — measured via `-Zprint-type-sizes` (see the task's verification
/// notes). That single unavoidable gap is the ONLY padding in the whole
/// struct; the hot set itself (bytes 0..41) has zero internal padding.
///
/// ## RAD-3 (E2, task #56) — `size_of::<SegmentHeader>()` grew 104 → 120 bytes
///
/// Two new 8-byte pointer fields (`pool_next`, `pool_prev` — the intrusive
/// doubly-linked list for the empty-small-segment hysteresis pool, replacing
/// the old fixed `[*mut u8; POOL_MAX_SLOTS]` array that lived in `AllocCore`
/// and scaled with `MAX_HEAPS`) were appended to the cold set. The 7-byte
/// tail-alignment gap after `kind` (bytes 41..48) is UNCHANGED — it exists to
/// re-align the cold set's first 8-byte field, independent of how many 8-byte
/// fields follow. `size_of::<SegmentHeader>()` is confirmed by the
/// field-by-field accounting: 3×8 + 4×4 + 1 + 7 pad (hot set, bytes 0..48) +
/// 8×8 + 2×4 (cold set: `large_size`, `large_align`, `span_usable`,
/// `reservation`, `reservation_len`, `next_abandoned`, `pool_next`,
/// `pool_prev`, then `segment_id`, `node_id`) = 48 + 72 = 120, verified via
/// `-Zprint-type-sizes` while adding these fields. `Layout::page_map_off()`
/// (`align_up(size_of::<SegmentHeader>(), PAGE)`) is `align_up(120, 4096) ==
/// 4096` — byte-identical to the pre-RAD-3 value (both 104 and 120 round up
/// to one page), so every downstream metadata offset
/// (`bin_table_off`/`alloc_bitmap_off`/`remote_ring_off`/`small_meta_end`/…)
/// is UNCHANGED — this growth is fully absorbed by the header's own
/// sub-page padding and does not ripple into the rest of the segment layout.
/// The `size_of::<SegmentHeader>() <= PAGE` / `Layout::page_map_off() ==
/// PAGE` const-asserts at the bottom of this file are a coarser compile-time
/// sanity bound (they would also pass at, say, 128 bytes), not a byte-exact
/// pin; they still catch any REGRESSION that pushes the header past a full
/// page, which is the invariant they exist to guard.
#[repr(C)]
#[derive(Clone, Copy)]
pub(crate) struct SegmentHeader {
    // ── Hot set: bytes 0..41 (one cache line, alignment-descending: the
    // three 8-byte fields first, then the four 4-byte fields, then the
    // 1-byte `kind` — zero internal padding under `#[repr(C)]`) ────────────
    /// For small/primordial segments: the bump cursor, in BYTES from the
    /// segment base, of the next uncarved payload byte. The bootstrap sets it
    /// to the end of the metadata region (header + page map + bin table).
    /// Rewritten on every `carve_block` — the single hottest owner-write in
    /// the refill path.
    pub bump: usize,
    /// Phase 10: a stable pointer to the owning heap's thread-free stack head
    /// (`*const AtomicPtr<u8>`). A cross-thread freer reads this from the
    /// segment header after `segment_base_of(ptr)` and CAS-pushes the freed
    /// block onto the Treiber stack. `null` for segments not yet bound to a
    /// heap (Phase 8 `AllocCore`-only segments). The pointer is stable because
    /// it addresses a process-`'static` head: a registry-slot-resident
    /// `HeapSlot::thread_free` field (the slot array is `'static`) or the
    /// fallback `FALLBACK_TFS` static atomic (post-W3, task #13 — no longer a
    /// `Box`).
    pub owner_thread_free: *const core::sync::atomic::AtomicPtr<u8>,
    /// Phase 12.4: the segment's ownership state — packed
    /// `(state, owner_heap_id, generation)` (see the [`OWNER_STATE_*`] /
    /// [`OWNER_ID_*`] / [`OWNER_GEN_*`] constants above). The
    /// Abandoned→Live CAS on this word is the SINGLE linearization point of
    /// adoption (M9). Stored as a plain `u64` so the `#[repr(C)] Copy`
    /// `SegmentHeader` remains a plain bit-pattern (the bootstrap lays it down
    /// via `Node::write_struct`, and `SegmentMeta::header` reads it back as a
    /// unit). The adoption path accesses it through the dedicated
    /// [`owner_state_atomic`](SegmentMeta::owner_state_atomic) view (`&AtomicU64`
    /// at the same fixed offset), because a plain struct-field read would be a
    /// non-atomic data race under the concurrent adoption CAS.
    pub owner_state: u64,
    /// Sanity magic — every segment starts with this. A computed segment base
    /// that does not have this magic is not one of our segments (foreign ptr).
    /// Read on every cross-thread dealloc-routing base validation.
    pub magic: u32,
    /// Phase 35 (M6 decommit): the **owner-only** count of live (carved-and-not-
    /// free) blocks in this small/primordial segment. Incremented when a block
    /// is handed to the caller (`pop_free` / `carve_block`), decremented when a
    /// block is freed (`dealloc_small` / `reclaim_offset`). When it reaches zero
    /// the segment is empty and (under `alloc-decommit`) its payload pages are
    /// returned to the OS.
    ///
    /// **Not atomic — owner-only.** Every mutation runs on the segment's owner:
    /// own-thread alloc/free AND the owner-side ring drain (`reclaim_offset`).
    /// The cross-thread freer NEVER touches this field (it pushes an offset into
    /// the `RemoteFreeRing`; the owner decrements when it drains). So a plain
    /// `u32` field, accessed through its `offset_of!` offset like `bump`, is
    /// race-free under the single-writer discipline (see §2 of the Phase 35
    /// design and the `bump_of`/`set_bump` precedent).
    ///
    /// The field is present in EVERY build's layout (so the header byte layout
    /// is stable regardless of feature config — like `owner_state`/
    /// `next_abandoned`); it is read/mutated ONLY under `alloc-decommit`. Without
    /// that feature it is dead data (silenced below).
    pub live_count: u32,
    /// Phase 35 (M6 decommit): owner-only flag (0 / 1) recording whether this
    /// segment's payload pages are currently DECOMMITTED (returned to the OS).
    /// Set when `live_count` hits zero and the payload is decommitted+reset;
    /// cleared when the segment is reselected for carving and the payload is
    /// recommitted. Present in every layout, used only under `alloc-decommit`.
    pub decommitted: u32,
    /// PERF-PASS-4 (G9/C2, task #52): the owner's cached copy of the
    /// `RemoteFreeRing`'s `head` cursor, as last observed by THIS segment's
    /// `find_segment_with_free_impl` drain guard. Lets the guard skip a
    /// `RemoteFreeRing::drain` call (and its unconditional `head.store(_,
    /// Release)`) when the ring's `tail` has not advanced past this cached
    /// value since the last drain — i.e. the ring is provably empty of
    /// anything new, without touching the ring's `head` atomic at all.
    ///
    /// **Not atomic — owner-only**, identical discipline to `bump` /
    /// `live_count`: the segment's owning thread is the ONLY reader/writer
    /// (the drain guard runs exclusively on the owner, exactly like the
    /// `RemoteFreeRing::drain` call it gates). A plain `u32` field, accessed
    /// through its `offset_of!` offset, is race-free under the same
    /// single-writer argument `bump_of`/`set_bump` document.
    ///
    /// **Why this lives in the segment header, not `SegmentTable`:** the
    /// cache must travel with SEGMENT identity, not with a `SegmentTable`
    /// slot INDEX. A `SegmentTable` slot index is reused across
    /// register/recycle for a completely different segment (task #60 slot
    /// recycle), so an index-keyed cache would need explicit invalidation at
    /// reuse — exactly the "stale cache surviving a re-claim" hazard this
    /// task's spec calls out. The header field instead lives inside the very
    /// segment memory it describes: a fresh segment always gets a fresh
    /// header via `SegmentHeader::small(..)` (see [`small`](Self::small),
    /// which zero-inits this field), so there is no way to observe a stale
    /// value from a PRIOR segment's ring occupying the same virtual address
    /// or the same table slot — the field's lifetime is the segment's
    /// lifetime, exactly like `bump`/`live_count`/the ring itself.
    ///
    /// **Present in EVERY build's layout** (same discipline as
    /// `live_count`/`node_id`): read/written only under
    /// `#[cfg(feature = "alloc-xthread")]`, but the byte layout of
    /// `SegmentHeader` does not otherwise shift across feature configs.
    /// Starts at 0 (matching a freshly-initialised ring's `head == 0`), so
    /// the FIRST scan of a brand-new segment correctly treats "cached head ==
    /// real head == 0" as "nothing to drain" until a real push moves `tail`.
    #[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
    pub ring_drain_head: u32,
    /// The segment kind (primordial / small / large). Decides dealloc routing.
    /// Read on every cross-thread dealloc-routing dispatch.
    pub kind: SegmentKind,

    // ── Cold set: bytes 41.. (Large-only / teardown-only / unregister-only,
    // alignment-descending: 8-byte fields, then the 4-byte `segment_id` /
    // `node_id`) ─────────────────────────────────────────────────────────
    /// For large/huge segments: the size (bytes) of the single allocation.
    /// Unused for small/primordial (zero).
    pub large_size: usize,
    /// For large/huge segments: the alignment of the single allocation.
    pub large_align: usize,
    /// For large/huge segments: the PHYSICAL committed usable span of this
    /// segment (`n_segments * SEGMENT`, computed once from the ORIGINAL OS
    /// reservation). Set exactly once — at the segment's initial OS
    /// reservation (`alloc_large_slow`) or when a cached segment is reused
    /// for a smaller request on a cache HIT (`alloc_large`'s hit path, where
    /// it is carried forward verbatim from the cached slot's `usable_size`,
    /// i.e. the physical span of the segment being reused) — and NEVER
    /// recomputed from `large_size`/`large_align`.
    ///
    /// This exists because `large_size`/`large_align` describe the CURRENT
    /// allocation living in the segment, which on a cache hit can be smaller
    /// than the segment's actual physical footprint (the OS reservation is
    /// reused as-is; only the header's logical size/align shrink to fit the
    /// new request). Recomputing "usable size" from `large_size`/`large_align`
    /// at deposit time (as an earlier version did — bug #134) therefore
    /// UNDER-reports the physical span for a reused-and-shrunk segment,
    /// corrupting the `large_cache` byte-budget accounting (`
    /// large_cache_used_bytes` and the cache-hit size-ratio matching) and
    /// causing unbounded RSS amplification. `span_usable` is the single
    /// stable source of truth for "how many bytes of OS memory does this
    /// segment actually occupy" across the segment's whole cache lifetime
    /// (fresh-reserve → N× cache-hit-reuse → deposit).
    ///
    /// Unused for small/primordial (zero — inert, like `large_size`).
    pub span_usable: usize,
    /// The start of the OS reservation that produced this segment (may differ
    /// from the segment base due to the over-reserve + trim technique — see
    /// [`super::os`]). Recorded so `AllocCore::drop` can release the WHOLE
    /// reservation by walking the registry (no `Vec<Segment>` needed — this is
    /// part of the self-hosting discipline).
    pub reservation: *mut u8,
    /// The full size of the OS reservation (head + usable + tail). Paired with
    /// `reservation` for the OS free call.
    pub reservation_len: usize,
    /// Phase 12.4: the intrusive link for the global abandoned-segments
    /// Treiber stack. While a segment is ABANDONED and on the stack, this
    /// holds the segment-relative OFFSET of the NEXT abandoned segment's base
    /// (or [`ABANDONED_TAIL`] if this is the stack tail). Stored as an offset
    /// (not a pointer) so the field is plain `Copy` data inside the header.
    /// Live (non-abandoned) segments carry [`ABANDONED_TAIL`] here ("not on
    /// the stack"). Accessed atomically through
    /// [`next_abandoned_atomic`](SegmentMeta::next_abandoned_atomic) on the
    /// abandon/adopt path.
    pub next_abandoned: u64,
    /// RAD-3 (E2, task #56) — the intrusive DOUBLY-linked list link to the
    /// NEXT more-recently-pooled segment in the empty-small-segment
    /// hysteresis pool (Mechanism 2), or `null` if this is the pool's HEAD
    /// (the warmest / most-recently-emptied entry) or the segment is not
    /// currently pooled. `AllocCore` keeps only `pool_head`/`pool_tail`/
    /// `pooled_count`/`pool_cap` — the list itself lives entirely in these two
    /// header fields, so the pool's storage cost does NOT scale with
    /// `POOL_MAX_SLOTS` (removed) x `MAX_HEAPS` the way the old fixed
    /// `[*mut u8; POOL_MAX_SLOTS]` array did (see the removed field's history
    /// in git blame / `docs/perf/PERF_PLAN_2026-07-10-radical-audit-
    /// implementation-plan.md` §E2 for the RSS-per-registry-slot rationale).
    ///
    /// **Owner-only, plain pointer (not atomic).** The pool is exclusively
    /// single-threaded bookkeeping — every push/pop/remove happens on the
    /// segment's owning thread inside `AllocCore`'s pool methods (mirroring
    /// `bump`/`live_count`'s owner-only discipline); no cross-thread reader
    /// ever touches these fields (unlike `next_abandoned`, which the
    /// CROSS-THREAD abandon/adopt protocol accesses via a `&AtomicU64` view —
    /// hence THAT field stays a `u64` offset/pointer hybrid with exposed
    /// provenance, while these can be plain `*mut u8` pointers accessed
    /// through ordinary field-specific reads/writes, see
    /// [`SegmentMeta::pool_next_of`]/[`SegmentMeta::set_pool_next`]).
    ///
    /// List order: HEAD = most-recently-pooled (`pop_pooled_segment` pops the
    /// head in O(1) — the "warmest" reuse the old max-seq scan achieved by
    /// comparison, now achieved for free by insertion order). TAIL =
    /// least-recently-pooled (the decay tick evicts the tail in O(1) — the
    /// "coldest" segment, mirroring the old min-seq scan).
    pub pool_next: *mut u8,
    /// RAD-3 (E2, task #56) — the intrusive link to the PREVIOUS
    /// (more-recently-pooled, i.e. closer to the list head) segment. Needed
    /// for O(1) removal from the MIDDLE of the list — `unpool_if_present`
    /// removes an arbitrary segment (the one just reused via
    /// `find_segment_with_free`, which is essentially never the coldest tail
    /// entry), which a singly-linked list could only do in O(n). `null` for
    /// the pool's TAIL entry or a non-pooled segment. Same owner-only, plain
    /// pointer discipline as [`pool_next`](Self::pool_next).
    pub pool_prev: *mut u8,
    /// The segment's index in the global registry. `u32::MAX` until registered
    /// (the primordial segment is index 0). Unregister/recycle-only read.
    pub segment_id: u32,
    /// Phase B (numa-aware): the NUMA node on which this segment's physical
    /// pages were allocated. `NO_NODE_RAW` (`u32::MAX`) means "unknown / not
    /// bound to any NUMA node" (the sentinel used on all platforms and when
    /// `numa-aware` is OFF).
    ///
    /// **Present in EVERY build's layout** — the byte layout of `SegmentHeader`
    /// is identical regardless of feature config (same discipline as
    /// `live_count`/`decommitted`). The field is READ and WRITTEN only under
    /// `#[cfg(feature = "numa-aware")]`; without that feature it is inert dead
    /// data (lint silenced below). This keeps the header's `size_of` — and all
    /// downstream offsets (`page_map_off`, `bin_table_off`, etc.) — feature-
    /// invariant, so serialised segment headers can be re-read regardless of
    /// which feature set was active when they were written.
    ///
    /// **Not atomic — owner-only.** Written once at segment-init time
    /// (`reserve_small_segment` / `alloc_large`), never mutated thereafter.
    /// Cross-thread readers never touch this field (it is not part of the
    /// dealloc-routing hot path); the `decommit_empty_segment` reset also
    /// leaves it intact (the physical NUMA binding does not change on
    /// decommit/recommit). Accessed via the field-specific `node_id_of` /
    /// `set_node_id` accessor pair (same `offset_of!` discipline as `bump` and
    /// `live_count`).
    #[cfg_attr(not(feature = "numa-aware"), allow(dead_code))]
    pub node_id: u32,
}

/// Sentinel for "no next abandoned segment" (the intrusive stack tail) AND
/// "this segment is not currently on the abandoned stack". A real
/// segment-relative offset is always `< SEGMENT` (`1 << 22`), so `u64::MAX`
/// is unambiguous.
#[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
pub(crate) const ABANDONED_TAIL: u64 = u64::MAX;

/// Sentinel for `SegmentHeader::node_id`: "no NUMA node / feature disabled /
/// unsupported platform". Mirrors `alloc_core::numa::NO_NODE` (`u32::MAX`),
/// but declared here (safe code) so the constructors can use it without a
/// conditional import of the `numa` module (which only compiles under
/// `feature = "numa-aware"`). The two constants are by definition equal; a
/// compile-time assert in `SegmentMeta::node_id_of` enforces this.
pub(crate) const NO_NODE_RAW: u32 = u32::MAX;

impl SegmentHeader {
    /// Build a fresh small-segment header value (does NOT write it — the
    /// bootstrap writes it through [`Node::write_struct`]). `bump` is where
    /// payload carving may begin (just past the metadata region).
    ///
    /// The segment starts in the LIVE owner-state bound to `OWNER_ID_NONE`
    /// (not yet stamped with a real heap id); the abandonment/adopt path
    /// stamps it when the segment is bound to a heap.
    pub(crate) const fn small(
        segment_id: u32,
        bump: usize,
        reservation: *mut u8,
        reservation_len: usize,
    ) -> Self {
        Self {
            magic: SEGMENT_MAGIC,
            kind: SegmentKind::Small,
            segment_id,
            bump,
            large_size: 0,
            large_align: 0,
            span_usable: 0,
            reservation,
            reservation_len,
            owner_thread_free: core::ptr::null(),
            owner_state: pack_owner(OWNER_STATE_LIVE, OWNER_ID_NONE, 0),
            next_abandoned: ABANDONED_TAIL,
            // RAD-3 (E2): a fresh segment is not on the empty-segment pool's
            // list yet — both links start null (the "not pooled" sentinel).
            pool_next: core::ptr::null_mut(),
            pool_prev: core::ptr::null_mut(),
            // A fresh small segment has no live blocks and a committed payload.
            live_count: 0,
            decommitted: 0,
            // Phase B: NUMA node is unknown at construction time; the caller
            // (reserve_small_segment under numa-aware) stamps the real value
            // immediately after writing the header via set_node_id.
            node_id: NO_NODE_RAW,
            // PERF-PASS-4 (G9/C2): a fresh segment's ring starts at head == 0
            // (RemoteFreeRing::init_in_place zeroes the cursors); the cache
            // starts at the same value so the first drain guard check
            // correctly observes "nothing to drain yet".
            ring_drain_head: 0,
        }
    }

    /// Build a large/huge header value. The single allocation will live at
    /// the first page-aligned offset past the header.
    ///
    /// `span_usable` is the segment's PHYSICAL committed usable span
    /// (`n_segments * SEGMENT`) — the caller MUST pass the true physical span
    /// of the underlying OS reservation being used: for a freshly-reserved
    /// segment this is the just-computed `usable`; for a cache-hit reuse of
    /// an existing segment (a smaller request landing in a larger cached
    /// span) this MUST be the cached slot's own `usable_size` (the ORIGINAL
    /// physical span), never recomputed from `size`/`align` (see the field's
    /// doc comment on `SegmentHeader` — bug #134).
    pub(crate) const fn large(
        segment_id: u32,
        size: usize,
        align: usize,
        span_usable: usize,
        bump: usize,
        reservation: *mut u8,
        reservation_len: usize,
    ) -> Self {
        Self {
            magic: SEGMENT_MAGIC,
            kind: SegmentKind::Large,
            segment_id,
            bump,
            large_size: size,
            large_align: align,
            span_usable,
            reservation,
            reservation_len,
            owner_thread_free: core::ptr::null(),
            owner_state: pack_owner(OWNER_STATE_LIVE, OWNER_ID_NONE, 0),
            next_abandoned: ABANDONED_TAIL,
            // RAD-3 (E2): Large segments never join the small-segment pool —
            // inert, like live_count/decommitted below.
            pool_next: core::ptr::null_mut(),
            pool_prev: core::ptr::null_mut(),
            // Large segments do not use the small-segment decommit bookkeeping
            // (they hold one allocation and are freed wholesale at Drop); these
            // are inert for a Large header.
            live_count: 0,
            decommitted: 0,
            // Phase B: same sentinel as small(); the caller (alloc_large under
            // numa-aware) stamps the real value after writing the header.
            node_id: NO_NODE_RAW,
            // Large segments have no RemoteFreeRing (no BinTable either) —
            // inert, like live_count/decommitted above.
            ring_drain_head: 0,
        }
    }

    /// Read the header at `base` (segment base, any kind) THROUGH the node
    /// seam. Returns a copy of the header. `base` MUST be a live segment base
    /// with a valid header at offset 0.
    pub(crate) fn read_at(base: *mut u8) -> Self {
        Node::read_struct::<SegmentHeader>(base as *const SegmentHeader)
    }

    /// Read the header's `kind` field only (field-specific read: a single
    /// byte load at the field's offset, NOT a full-struct read). The
    /// dealloc-routing hot path needs just this together with `magic` and
    /// `owner_thread_free`; reading each field individually avoids the
    /// full-struct `read_at` that raced with the owner's `bump` field writes
    /// (the §11 root cause — `kind`/`magic`/`owner_thread_free` are written
    /// once at init/stamp time and only read cross-thread thereafter, so a
    /// field read of any of them does not race with the owner's `bump` writes
    /// on a disjoint field).
    #[allow(dead_code)] // Used by Phase 9+ cross-thread routing; kept for that.
    #[inline(always)]
    pub(crate) fn kind_at(base: *mut u8) -> SegmentKind {
        let off = core::mem::offset_of!(SegmentHeader, kind);
        // The `SegmentKind` discriminant is one byte at `base + off`; read it
        // via the node seam and transcribe the raw byte back to the enum.
        let b = Node::read_u8(Node::offset(base, off) as *const u8);
        // L-5 (UBFIX-11): STRICT decode — the byte was laid down by
        // `SegmentHeader::small`/`large` as a valid `SegmentKind` discriminant
        // (`#[repr(u8)]`) and the header is otherwise immutable in this
        // field, so in the well-formed case the byte is always one of
        // {0,1,2}. Previously any OTHER byte (a corrupted/garbled kind — a
        // wild write from an unrelated bug, or the aftermath of an
        // H-1-class defect before its fix) fell through to a `_ => Small`
        // default: a corrupt/unexpected byte was silently treated as a
        // VALID, specific segment kind — amplifying the corruption instead
        // of containing it (e.g. a Large segment with a corrupted kind byte
        // would be misrouted onto the Small free path, and a
        // Small-specific free would write a BinTable/free-list header into
        // a live Large payload). `magic_at` (checked by the caller first on
        // the cross-thread path) rejects a non-sefer BASE, but does nothing
        // to validate the `kind` BYTE of a base that IS ours but has been
        // corrupted in place — so this decode must reject on its own.
        // Every unexpected byte now maps to `SegmentKind::Unknown`, a
        // sentinel no constructor ever writes; every caller of `kind_at`
        // tests for a SPECIFIC expected kind via `==`/`matches!` (never an
        // exhaustive match with an implicit catch-all — see the callers
        // inventory in this task's audit), so `Unknown` naturally fails
        // every such check and is routed to that call site's existing
        // "not this kind" no-op/reject branch.
        match b {
            0 => SegmentKind::Primordial,
            1 => SegmentKind::Small,
            2 => SegmentKind::Large,
            _ => SegmentKind::Unknown,
        }
    }

    /// Read the header's `magic` field only (field-specific `u32` load). Used
    /// by the cross-thread dealloc-routing path to validate the segment base
    /// without reading the whole mutable header. `magic` is written once at
    /// segment init and only read thereafter, so this field read does not race
    /// with the owner's `bump` field writes.
    #[cfg(feature = "alloc-xthread")]
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn magic_at(base: *mut u8) -> u32 {
        let off = core::mem::offset_of!(SegmentHeader, magic);
        Node::read_u32(Node::offset(base, off) as *const u32)
    }

    /// Read the header's `owner_thread_free` field only (field-specific pointer
    /// load). Used by the cross-thread dealloc-routing path to find the owning
    /// heap's TFS head without reading the whole mutable header. The field is
    /// written ONCE at stamp time (by the owning thread) and only read
    /// cross-thread thereafter, so a field read does not race with the owner's
    /// `bump` writes on a disjoint field.
    #[cfg(feature = "alloc-xthread")]
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn owner_thread_free_at(base: *mut u8) -> *const core::sync::atomic::AtomicPtr<u8> {
        let off = core::mem::offset_of!(SegmentHeader, owner_thread_free);
        Node::read_ptr(Node::offset(base, off) as *const *const core::sync::atomic::AtomicPtr<u8>)
    }

    /// Read the header's `large_size` field only (field-specific `usize`
    /// load). 0.3.0 (task #138, A1 post-reuse mitigation): used by the
    /// cross-thread Large-free routing paths (`HeapCore::dealloc_routing`,
    /// `Heap::dealloc_any_thread`) to sanity-check that the freeing layout is
    /// consistent with the CURRENT occupant of the segment before queuing it
    /// onto the owner's deferred-free stack — see the mitigation's doc
    /// comment on [`push_large_deferred_free`](crate::alloc_core::deferred_large::push_large_deferred_free)
    /// for the full rationale and its documented residual limit.
    ///
    /// `large_size` is written once at segment construction
    /// (`SegmentHeader::large`) and on every large-cache-hit reuse
    /// (`AllocCore::alloc_large`'s hit path rewrites the WHOLE header via
    /// `Node::write_struct` before the segment is handed to a new caller —
    /// never mutated in place field-by-field), so a field-specific read here
    /// does not race the owner's disjoint `bump` writes (same discipline as
    /// `kind_at`/`magic_at`/`owner_thread_free_at`). It IS, by design, able to
    /// observe a DIFFERENT value than the one the freeing thread's stale
    /// `Layout` was allocated against, if the segment has already been
    /// reclaimed and reused for a new allocation between the free and this
    /// read — that race is exactly what this check exists to catch (a
    /// mismatch here means "this is not a free of the CURRENT occupant").
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn large_size_at(base: *mut u8) -> usize {
        let off = core::mem::offset_of!(SegmentHeader, large_size);
        Node::read_usize(Node::offset(base, off) as *const usize)
    }

    /// Read the header's `segment_id` field only (field-specific `u32` load).
    /// Used by [`SegmentTable::unregister`](super::segment_table::SegmentTable::unregister)
    /// / [`SegmentTable::recycle`](super::segment_table::SegmentTable::recycle)
    /// (task #135) to locate a segment's registry slot index in O(1), instead
    /// of scanning the table for a matching base pointer. `segment_id` is
    /// written ONCE, at registration time, as part of the freshly-built header
    /// value passed to a full-struct `Node::write_struct` (`alloc_large_slow`,
    /// the large-cache-hit path, `register_segment`'s caller) — never mutated
    /// in place thereafter — so a field read here does not race with the
    /// owner's `bump` field writes on a disjoint field (same discipline as
    /// `magic_at`/`kind_at`). Present in EVERY build's layout (like `magic`),
    /// so this accessor is not feature-gated.
    #[cfg_attr(
        not(any(feature = "alloc-decommit", feature = "alloc-xthread")),
        allow(dead_code)
    )]
    #[inline(always)]
    pub(crate) fn segment_id_at(base: *mut u8) -> u32 {
        let off = core::mem::offset_of!(SegmentHeader, segment_id);
        Node::read_u32(Node::offset(base, off) as *const u32)
    }

    /// Read the header's `span_usable` field only (field-specific `usize`
    /// load). For large segments this is the PHYSICAL committed usable span
    /// (the full OS reservation rounded to whole segments). Used by
    /// `AllocCore::realloc` to decide whether an in-place Large grow fits
    /// without reallocation.
    ///
    /// `span_usable` is written once at segment construction
    /// (`SegmentHeader::large`) or carried forward verbatim on a cache-hit
    /// reuse — never mutated in place field-by-field — so a field-specific
    /// read here does not race with the owner's disjoint `bump` writes (same
    /// discipline as `kind_at`/`large_size_at`).
    #[inline(always)]
    pub(crate) fn span_usable_at(base: *mut u8) -> usize {
        let off = core::mem::offset_of!(SegmentHeader, span_usable);
        Node::read_usize(Node::offset(base, off) as *const usize)
    }

    /// Overwrite the header's `large_size` field only (field-specific `usize`
    /// store, mirroring `large_size_at`'s read). Used by `AllocCore::realloc`
    /// to update the logical allocation size after an in-place Large grow
    /// (the segment's physical span is unchanged — only the recorded size
    /// advances).
    ///
    /// Safety discipline: called ONLY by the owning thread's `realloc` path,
    /// which is the single writer for this segment. The field sits at a fixed
    /// offset disjoint from `bump` / `owner_state`, so no cross-field race.
    #[inline(always)]
    pub(crate) fn set_large_size_at(base: *mut u8, size: usize) {
        let off = core::mem::offset_of!(SegmentHeader, large_size);
        Node::write_usize(Node::offset(base, off) as *mut usize, size);
    }

    /// TEST-ONLY (task #135): overwrite the header's `segment_id` field only
    /// (field-specific write, mirroring `segment_id_at`'s read). Used by
    /// `AllocCore::dbg_stamp_segment_id` to exercise `SegmentTable::unregister`'s
    /// defensive `slots[id] == base` guard against a corrupted `segment_id`
    /// (see `tests/segment_table_o1.rs`). Never called on any production path.
    #[allow(dead_code)]
    pub(crate) fn set_segment_id_at(base: *mut u8, id: u32) {
        let off = core::mem::offset_of!(SegmentHeader, segment_id);
        Node::write_u32(Node::offset(base, off) as *mut u32, id);
    }
}

/// Round `n` up to the next multiple of `a`. Works for ANY `a > 0` (not just
/// powers of two) — the size-class table uses 1.25× spacing (rounded to
/// `MIN_BLOCK`), so most block sizes are NOT powers of two. Pure safe integer
/// arithmetic; the `debug_assert` catches a zero/misuse.
pub(crate) fn align_up(n: usize, a: usize) -> usize {
    debug_assert!(a > 0, "align must be non-zero");
    // Ceiling division: `ceil(n / a) * a`. Avoids overflow vs `n + a - 1`.
    let q = n.div_ceil(a);
    q * a
}

/// The per-segment page descriptor table. `PAGES_PER_SEGMENT` entries of one
/// byte each, carved from the segment right after the header.
///
/// Each entry is a [`PageClass`] discriminant byte telling which size class
/// owns the page (or `Free` / `Meta`).
///
/// NOTE (post-Phase 13.3): this table is **NOT load-bearing for class routing**;
/// do NOT derive block classes from it. No production `dealloc` path derives a
/// freed block's class from `PageMap` — the class is carried authoritatively by
/// the caller's `Layout` (own-thread) or stamped into the `RemoteFreeRing` entry
/// (cross-thread). Deriving a class here would reintroduce the mixed-class /
/// stale-cursor drain-reclaim bug fixed in §13 of `RACE_DRAIN_RECLAIM.md`.
pub(crate) struct PageMap {
    /// Absolute address of the first entry (we store the absolute `*mut u8`
    /// so reads need no segment-base arithmetic).
    entries: *mut u8,
}

impl PageMap {
    /// Number of bytes the page map occupies in a segment. Fixed and known at
    /// compile time so the bootstrap can carve it deterministically.
    pub(crate) const FOOTPRINT: usize = PAGES_PER_SEGMENT * size_of::<u8>();

    /// Construct the view over an already-laid-down page map at `entries`.
    /// The bootstrap calls this AFTER writing the entries via [`init_in_place`].
    pub(crate) fn new(entries: *mut u8) -> Self {
        Self { entries }
    }

    /// Initialise a fresh page map at `entries`, marking `meta_pages` low
    /// pages `Meta` and the rest `Free`. Routes every byte write through
    /// [`Node::write_u8`].
    ///
    /// `entries` MUST point to `Self::FOOTPRINT` writable bytes inside the
    /// segment being initialised (caller's contract — the bootstrap).
    pub(crate) fn init_in_place(entries: *mut u8, meta_pages: usize) {
        for p in 0..PAGES_PER_SEGMENT {
            let byte = if p < meta_pages {
                PageClass::Meta as u8
            } else {
                PageClass::Free as u8
            };
            Node::write_u8(Node::offset(entries, p), byte);
        }
    }

    /// Read the class of page `p` (decoded). Panics (debug) if
    /// `p >= PAGES_PER_SEGMENT`.
    pub(crate) fn class_of(&self, p: usize) -> Option<usize> {
        debug_assert!(p < PAGES_PER_SEGMENT, "page index out of range");
        let byte = Node::read_u8(self.entries_at_const(p));
        PageClass::decode(byte)
    }

    /// Mark page `p` as owned by size-class `class_idx`.
    pub(crate) fn set_class(&mut self, p: usize, class_idx: usize) {
        debug_assert!(p < PAGES_PER_SEGMENT, "page index out of range");
        Node::write_u8(self.entries_at_const(p), PageClass::encode_class(class_idx));
    }

    /// Mark page `p` as `Free` (uncarved). Phase 35: used by the M6 decommit
    /// reset to return an emptied segment's payload pages to the bump region.
    #[cfg(feature = "alloc-decommit")]
    pub(crate) fn set_free(&mut self, p: usize) {
        debug_assert!(p < PAGES_PER_SEGMENT, "page index out of range");
        Node::write_u8(self.entries_at_const(p), PageClass::Free as u8);
    }

    /// Pointer to entry `p`. Caller guarantees `p < PAGES_PER_SEGMENT`.
    fn entries_at_const(&self, p: usize) -> *mut u8 {
        // Routed through the `node` seam (`add` is unsafe; the seam documents
        // the in-bounds contract).
        Node::offset(self.entries, p)
    }
}

/// The per-segment per-class free-list head table. One `u32` OFFSET per small
/// class — the segment-relative offset of the head free block of that class,
/// or `FREE_LIST_NULL` if the class's free list is empty.
///
/// Storing offsets (not pointers) keeps the table compact (40 × 4 B = 160 B)
/// and lets the Cartographer reason entirely in safe integers; the conversion
/// to a pointer happens only at the `node` seam when popping.
pub(crate) struct BinTable {
    /// Absolute address of the first `u32` head. `SMALL_CLASS_COUNT` entries.
    heads: *mut u32,
}

/// Sentinel value for "this class's free list is empty". A real offset is
/// always `< SEGMENT`, so `u32::MAX` is unambiguous.
pub(crate) const FREE_LIST_NULL: u32 = u32::MAX;

impl BinTable {
    /// Footprint of the bin table in a segment. Fixed so the bootstrap can
    /// carve it deterministically.
    pub(crate) const FOOTPRINT: usize = SMALL_CLASS_COUNT * size_of::<u32>();

    /// Construct the view over an already-laid-down bin table at `heads`.
    #[inline(always)]
    pub(crate) fn new(heads: *mut u32) -> Self {
        Self { heads }
    }

    /// Initialise a fresh empty bin table at `heads`. Every write routed
    /// through [`Node::write_u32_unaligned`]. `heads` MUST point to
    /// `Self::FOOTPRINT` writable bytes.
    pub(crate) fn init_in_place(heads: *mut u32) {
        for c in 0..SMALL_CLASS_COUNT {
            Node::write_u32_unaligned(
                Node::offset(heads as *mut u8, c * size_of::<u32>()) as *mut u32,
                FREE_LIST_NULL,
            );
        }
    }

    /// The segment-relative offset of the head free block of class `c`, or
    /// `FREE_LIST_NULL` if empty.
    #[inline(always)]
    pub(crate) fn head(&self, c: usize) -> u32 {
        debug_assert!(c < SMALL_CLASS_COUNT, "class index out of range");
        Node::read_u32_unaligned(self.heads_at_const(c))
    }

    /// Set the head of class `c`'s free list to `off`.
    #[inline(always)]
    pub(crate) fn set_head(&mut self, c: usize, off: u32) {
        debug_assert!(c < SMALL_CLASS_COUNT, "class index out of range");
        Node::write_u32_unaligned(self.heads_at_const(c), off);
    }

    #[inline(always)]
    fn heads_at_const(&self, c: usize) -> *mut u32 {
        Node::offset(self.heads as *mut u8, c * size_of::<u32>()) as *mut u32
    }
}

/// The metadata footprint of a small segment: header + page map + bin table,
/// each laid out at fixed offsets (see [`Layout::small`]). This does NOT
/// include the registry array (which lives only in the primordial segment).
#[allow(dead_code)] // Compile-time sanity only; consumed by the `const _` asserts below.
pub(crate) const SMALL_META_FOOTPRINT: usize = Layout::small_meta_end();

/// The fixed layout of in-segment metadata: offsets of header / page map /
/// bin table. Centralised so the bootstrap and `SegmentMeta` agree.
pub(crate) struct Layout;
impl Layout {
    /// Offset of the page map (page-aligned past the header).
    pub(crate) const fn page_map_off() -> usize {
        align_up_const(size_of::<SegmentHeader>(), PAGE)
    }
    /// Offset of the bin table (right after the page map).
    pub(crate) const fn bin_table_off() -> usize {
        Self::page_map_off() + PageMap::FOOTPRINT
    }
    /// Offset of the per-segment [`AllocBitmap`](super::alloc_bitmap::AllocBitmap)
    /// — the O(1) double-free guard (Phase 13.4a), one bit per `MIN_BLOCK` slot
    /// of the whole segment. Placed AFTER **two** `BinTable::FOOTPRINT`s, 8-byte
    /// aligned: the second `BinTable` footprint is the slot Phase 13.4b's
    /// two-list (`free` + `local_free`) will occupy. Reserving it now means
    /// 13.4b adds its second head array in place WITHOUT shifting the bitmap /
    /// ring / registry offsets again (the spec's "compute the layout with the
    /// doubled BinTable up front" requirement — §1.2 / §2).
    pub(crate) const fn alloc_bitmap_off() -> usize {
        align_up_const(Self::bin_table_off() + BinTable::FOOTPRINT * 2, 8)
    }
    /// RAD-5 (E4) GO/NO-GO EXPERIMENT — offset of the per-segment
    /// [`MagazineBitmap`](super::magazine_bitmap::MagazineBitmap), an
    /// orthogonal second bitmap recording magazine residency (see its module
    /// doc). Placed immediately after `AllocBitmap`, 8-byte aligned, same
    /// `FOOTPRINT` geometry (one bit per `MIN_BLOCK` slot). See
    /// `docs/perf/IAI_BASELINE.md`'s RAD-5 entry for the measured verdict
    /// before assuming this offset is load-bearing for any shipped feature.
    pub(crate) const fn magazine_bitmap_off() -> usize {
        align_up_const(
            Self::alloc_bitmap_off() + super::alloc_bitmap::AllocBitmap::FOOTPRINT,
            8,
        )
    }
    /// Offset of the per-segment `RemoteFreeRing` (the non-intrusive
    /// cross-thread-free MPSC queue of `u32` block-offsets). Lives in segment
    /// metadata right after the magazine bitmap (RAD-5), 4-byte aligned (each
    /// ring slot is a `u32`). Carved alongside the bin table at bootstrap. See
    /// [`crate::alloc_core::remote_free_ring::RemoteFreeRing`] for the protocol.
    #[cfg_attr(not(feature = "alloc-xthread"), allow(dead_code))]
    pub(crate) const fn remote_ring_off() -> usize {
        align_up_const(
            Self::magazine_bitmap_off() + super::magazine_bitmap::MagazineBitmap::FOOTPRINT,
            4,
        )
    }
    /// Offset of the per-segment **generation table** (X7 Ф1, task #189) — the
    /// hardened remote-free staleness guard: one `AtomicU8` per `MIN_BLOCK`
    /// granule of the segment, recording the current "life number" of the block
    /// at that granule. Lives in segment metadata right after the remote-free
    /// ring, 1-byte aligned (it is a byte array). Compiled ONLY under
    /// `#[cfg(feature = "hardened")]`; under any other feature config the
    /// generation table does not exist and [`small_meta_end`] is byte-identical
    /// to the pre-X7 layout (the production-judge-neutrality requirement — see
    /// the layout assertions at the bottom of this file). See
    /// [`GEN_TABLE_FOOTPRINT`] / [`gen_at`] / [`bump_gen`].
    #[cfg(feature = "hardened")]
    pub(crate) const fn gen_table_off() -> usize {
        // 1-byte aligned (the table is a byte array). `remote_ring_off` is
        // already 4-byte aligned and the ring footprint is a multiple of 4, so
        // this offset is at least 4-aligned — trivially ≥ 1-aligned.
        Self::remote_ring_off() + super::remote_free_ring::FOOTPRINT
    }
    /// End of the small-segment metadata (page-aligned past the last metadata
    /// region). Payload carving begins here.
    ///
    /// Stacks up to THREE conditional metadata regions in a fixed order, each
    /// composing cleanly on top of the prior so all feature combinations
    /// produce a correct, non-overlapping layout:
    ///
    /// 1. **base** (always): header + page map + bin table + alloc bitmap +
    ///    remote ring (see [`remote_ring_off`] /
    ///    [`super::remote_free_ring::FOOTPRINT`]).
    /// 2. **generation table** (X7 Ф1, `#[cfg(feature = "hardened")]` only):
    ///    one byte per `MIN_BLOCK` granule, 1-byte aligned, immediately after
    ///    the remote ring — see [`gen_table_off`] / [`GEN_TABLE_FOOTPRINT`].
    /// 3. **run-encoded freelist stack** (PERF-3 Ф1, `#[cfg(feature =
    ///    "alloc-runfreelist")]` only): one `RunDesc` row per small class,
    ///    8-byte aligned, immediately after the generation table (or, if
    ///    `hardened` is off, immediately after the remote ring) — see
    ///    [`run_stack_off`] / [`super::run_stack::FOOTPRINT`].
    ///
    /// The final value is page-aligned past the last present region. Under
    /// every feature config the layout is the byte-for-byte composition of the
    /// regions that exist in that config — verified by the ungated
    /// `small_meta_end() + PAGE <= SEGMENT` const assert at the bottom of this
    /// file, which re-compiles under each cfg combination (X7-Ф1's
    /// neutrality argument, extended here to the runfreelist axis).
    pub(crate) const fn small_meta_end() -> usize {
        // PERF-3 Ф1: under `alloc-runfreelist` the RunStack stacks on top of
        // whatever the pre-runstack end is (base alone, OR base + gen-table
        // under hardened). 8-byte aligned start, page-aligned end. This
        // branch composes with the hardened branch above by going through
        // `small_meta_end_pre_runstack()` (which already branches on
        // hardened), NOT by replacing the hardened logic.
        #[cfg(feature = "alloc-runfreelist")]
        {
            align_up_const(Self::run_stack_off() + super::run_stack::FOOTPRINT, PAGE)
        }
        // Non-runfreelist: page-aligned past the pre-runstack end ONLY. This
        // branch is the sole compiled branch when `alloc-runfreelist` is off,
        // so the byte layout is byte-identical to the pre-PERF-3 build under
        // every `hardened` setting (the const assert at the bottom of this
        // file pins the value under each cfg combination).
        #[cfg(not(feature = "alloc-runfreelist"))]
        {
            align_up_const(Self::small_meta_end_pre_runstack(), PAGE)
        }
    }
    /// PERF-3 Ф1 (task #208): the end of the small-segment metadata BEFORE the
    /// RunStack region and BEFORE final page-alignment — i.e. the unaligned
    /// byte offset just past the last pre-runstack metadata region (the
    /// remote-free ring under non-hardened; the generation table under
    /// `hardened`). This is the anchor [`run_stack_off`] aligns up from, and
    /// the value [`small_meta_end`] page-aligns when `alloc-runfreelist` is
    /// off. Private to this module; the public surface is [`small_meta_end`].
    const fn small_meta_end_pre_runstack() -> usize {
        #[cfg(feature = "hardened")]
        {
            Self::gen_table_off() + GEN_TABLE_FOOTPRINT
        }
        #[cfg(not(feature = "hardened"))]
        {
            Self::remote_ring_off() + super::remote_free_ring::FOOTPRINT
        }
    }
    /// PERF-3 Ф1 (task #208): offset of the per-segment **run-encoded freelist
    /// stack** (`RunStack`) — compact `(start_off, count)` descriptors for
    /// contiguous freed-block runs, the storage phase of the run-encoded
    /// freelist arc (docs/design/RUN_ENCODED_FREELIST_PLAN.md). 8-byte aligned
    /// (each `RunDesc` is 8 bytes and the metadata region requires 8-byte
    /// record alignment — plan §2.1/§2.2), placed immediately after the
    /// pre-runstack metadata (remote ring under non-hardened; generation table
    /// under `hardened`). Compiled ONLY under
    /// `#[cfg(feature = "alloc-runfreelist")]`; under any other feature config
    /// the `RunStack` does not exist and [`small_meta_end`] is byte-identical
    /// to the pre-PERF-3 layout (plan §2.8 — production-judge-neutrality gate).
    /// See [`super::run_stack::RunStack`] / [`super::run_stack::FOOTPRINT`] /
    /// [`super::run_stack::RUNSTACK_CAPACITY`].
    #[cfg(feature = "alloc-runfreelist")]
    pub(crate) const fn run_stack_off() -> usize {
        // 8-byte aligned (each RunDesc is 8 bytes). The pre-runstack end is
        // at least 4-byte aligned (remote_ring_off is 4-aligned, ring FP is a
        // multiple of 4; gen_table_off is 1-aligned but its FP is a whole
        // number of bytes that is a multiple of 16 — MIN_BLOCK — so the
        // post-gen-table end is also 16-aligned, trivially ≥ 8-aligned).
        align_up_const(Self::small_meta_end_pre_runstack(), 8)
    }
    /// Offset of the registry array in the primordial segment (page-aligned
    /// past ALL small-segment metadata — header + page map + bin table + alloc
    /// bitmap + remote ring + [gen table under `hardened`] + [run-stack under
    /// `alloc-runfreelist`]). The registry is primordial-only; it sits after
    /// `small_meta_end()` so it never overlaps any small-segment metadata
    /// region.
    ///
    /// **X7 Ф3 (task #191) fix:** under `hardened`, `small_meta_end()` includes
    /// the generation table (~256 KiB). The pre-Ф3 code computed this offset
    /// from `remote_ring_off + FOOTPRINT` directly, which SKIPPED the gen table
    /// — so under hardened the registry/hash/free-list were carved ON TOP OF the
    /// gen table region, silently corrupting each other. Using `small_meta_end()`
    /// (which already accounts for the gen table under hardened, and is identical
    /// to the old computation under non-hardened) fixes the overlap. The
    /// non-hardened value is byte-identical to the pre-Ф3 layout (both compute
    /// `align_up(remote_ring_off + FOOTPRINT, PAGE)` — `small_meta_end` IS that
    /// value when the gen table is absent).
    pub(crate) const fn primordial_registry_off() -> usize {
        Self::small_meta_end()
    }
    /// Offset of the open-addressing hash table in the primordial segment
    /// (immediately after the registry array, 8-byte aligned).
    pub(crate) const fn primordial_hash_off() -> usize {
        align_up_const(
            Self::primordial_registry_off() + super::segment_table::REGISTRY_FOOTPRINT,
            8,
        )
    }
    /// Offset of the free-list index-stack array (task #135, Part 1) —
    /// immediately after the hash table, 4-byte aligned (the array holds
    /// `u32` indices).
    pub(crate) const fn primordial_free_list_off() -> usize {
        align_up_const(
            Self::primordial_hash_off() + super::segment_table::HASH_FOOTPRINT,
            4,
        )
    }
    /// Offset of the free-list top-of-stack counter (a single `u32`),
    /// immediately after the free-list array.
    pub(crate) const fn primordial_free_top_off() -> usize {
        Self::primordial_free_list_off() + super::segment_table::FREE_LIST_FOOTPRINT
    }
    /// End of the primordial metadata (page-aligned past the free-list top
    /// counter).
    pub(crate) const fn primordial_meta_end() -> usize {
        align_up_const(Self::primordial_free_top_off() + 4, PAGE)
    }
    /// Number of metadata pages in a small segment.
    pub(crate) const fn small_meta_pages() -> usize {
        Self::small_meta_end() / PAGE
    }
    /// Number of metadata pages in the primordial segment.
    pub(crate) const fn primordial_meta_pages() -> usize {
        Self::primordial_meta_end() / PAGE
    }
}

/// Accessor triple for the in-segment metadata of a small/primordial segment.
/// The bootstrap / `AllocCore` use this to obtain typed views over the header,
/// page map, and bin table of a segment given its base pointer.
pub(crate) struct SegmentMeta {
    pub base: *mut u8,
}

impl SegmentMeta {
    /// Construct the metadata view for a small/primordial segment whose base
    /// is `base` and whose header / page map / bin table are laid down at
    /// their [`Layout`] offsets.
    #[inline(always)]
    pub(crate) fn new(base: *mut u8) -> Self {
        Self { base }
    }

    /// Read the segment header (a copy).
    pub(crate) fn header(&self) -> SegmentHeader {
        SegmentHeader::read_at(self.base)
    }

    /// Write the segment header through the node seam.
    pub(crate) fn write_header(&mut self, hdr: SegmentHeader) {
        Node::write_struct(self.base as *mut SegmentHeader, hdr);
    }

    // -------------------------------------------------------------------
    // Field-specific header accessors (task #33 root-cause fix).
    //
    // The Phase-12 `SegmentHeader` packs an owner-mutated field (`bump`,
    // rewritten on every `carve_block`) alongside cross-thread-read fields
    // (`magic`, `kind`, `owner_thread_free`). A full-struct `read_at` /
    // `write_header` RMW of the whole header therefore races a Remote's
    // non-atomic struct read with the Owner's `bump`-touching struct write —
    // a data race and UB (see docs/RACE_DRAIN_RECLAIM.md §11).
    //
    // These accessors touch a SINGLE field via its `offset_of!` offset:
    //   - `bump_of` / `set_bump` — owner-only (the Owner is the sole writer
    //     and the sole reader of `bump`; no Remote ever reads it), so a plain
    //     field read/write is race-free.
    //   - the cross-thread-read fields (`magic`, `kind`,
    //     `owner_thread_free`) are written ONCE at init/stamp time and only
    //     read cross-thread thereafter, so a field read of any of them does
    //     not race with the owner's disjoint-field `bump` writes.
    // -------------------------------------------------------------------

    /// Read the owner-only `bump` cursor (the next uncarved payload byte
    /// offset). Owner-only: the owning thread is the sole reader/writer of
    /// `bump`; a plain field read is race-free (no Remote ever reads it).
    #[inline(always)]
    pub(crate) fn bump_of(&self) -> usize {
        let off = core::mem::offset_of!(SegmentHeader, bump);
        Node::read_usize(Node::offset(self.base, off) as *const usize)
    }

    /// Write the owner-only `bump` cursor. Replaces the full-struct
    /// `write_header` on the `carve_block` hot path: writing only this field
    /// avoids rewriting the cross-thread-read header fields, so it cannot race
    /// with a Remote's field read of `magic`/`kind`/`owner_thread_free`.
    /// Owner-only (the Owner is the sole writer of `bump`).
    #[inline(always)]
    pub(crate) fn set_bump(&mut self, value: usize) {
        let off = core::mem::offset_of!(SegmentHeader, bump);
        Node::write_usize(Node::offset(self.base, off) as *mut usize, value);
    }

    // -------------------------------------------------------------------
    // Phase 35 (M6 decommit) — field-specific owner-only accessors for the
    // `live_count` and `decommitted` fields. Identical discipline to
    // `bump_of`/`set_bump`: a single-word load/store at the field's
    // `offset_of!` offset through the `node` seam, so this file stays
    // `unsafe`-free. Owner-only (the owning thread is the sole mutator of
    // both fields — own-thread alloc/free and the owner-side ring drain;
    // the cross-thread freer never touches them), so a plain field
    // read/write is race-free, exactly as for `bump`.
    // -------------------------------------------------------------------

    /// Read the owner-only `live_count` (number of carved-and-not-free blocks).
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn live_count_of(&self) -> u32 {
        let off = core::mem::offset_of!(SegmentHeader, live_count);
        Node::read_u32(Node::offset(self.base, off) as *const u32)
    }

    /// Write the owner-only `live_count`.
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    fn set_live_count(&mut self, value: u32) {
        let off = core::mem::offset_of!(SegmentHeader, live_count);
        Node::write_u32(Node::offset(self.base, off) as *mut u32, value);
    }

    /// Increment `live_count` (a block was handed to the caller). Saturating so
    /// a corrupt/overflowed counter never wraps to zero and spuriously triggers
    /// a decommit of a non-empty segment (defence-in-depth; a real `live_count`
    /// is bounded by `SEGMENT / MIN_BLOCK` ≪ `u32::MAX`).
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn inc_live(&mut self) {
        let v = self.live_count_of();
        self.set_live_count(v.saturating_add(1));
    }

    /// Add `n` to `live_count` in ONE load+store (E1, task W4 — batched carve).
    /// Equivalent to `n` sequential [`inc_live`](Self::inc_live) calls: the
    /// counter is owner-only (single-writer), so the intermediate per-block
    /// values are unobservable and collapsing them to one saturating add is
    /// byte-identical in the final state — the same D1-equivalence argument
    /// `drain_freelist_batch` uses for its batched `inc_live`. Saturating for
    /// the same defence-in-depth reason as `inc_live`.
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn add_live(&mut self, n: u32) {
        let v = self.live_count_of();
        self.set_live_count(v.saturating_add(n));
    }

    /// Decrement `live_count` (a block was freed) and return the NEW value.
    /// Saturating at zero: a decrement below zero would indicate a double-free
    /// that slipped past the bitmap guard (it cannot, since the caller checks
    /// `is_free` first), but saturating keeps the counter sane rather than
    /// wrapping to `u32::MAX` and permanently suppressing decommit.
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn dec_live(&mut self) -> u32 {
        let v = self.live_count_of();
        let new = v.saturating_sub(1);
        self.set_live_count(new);
        new
    }

    /// Subtract `n` from `live_count` in ONE load+store and return the NEW
    /// value (E3, task W4 — batched flush). Equivalent to `n` sequential
    /// [`dec_live`](Self::dec_live) calls: the counter is owner-only, so the
    /// intermediate per-block values are unobservable and collapsing them to one
    /// saturating sub is byte-identical in the final value. Saturating at zero
    /// for the same defence-in-depth reason as `dec_live` (a real flush never
    /// removes more live blocks than exist).
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn sub_live(&mut self, n: u32) -> u32 {
        let v = self.live_count_of();
        let new = v.saturating_sub(n);
        self.set_live_count(new);
        new
    }

    /// Read the owner-only `decommitted` flag (true ⟺ payload pages are
    /// currently returned to the OS).
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn is_decommitted(&self) -> bool {
        let off = core::mem::offset_of!(SegmentHeader, decommitted);
        Node::read_u32(Node::offset(self.base, off) as *const u32) != 0
    }

    /// Set/clear the owner-only `decommitted` flag.
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn set_decommitted(&mut self, value: bool) {
        let off = core::mem::offset_of!(SegmentHeader, decommitted);
        Node::write_u32(Node::offset(self.base, off) as *mut u32, u32::from(value));
    }

    // -------------------------------------------------------------------
    // RAD-3 (E2, task #56) — field-specific owner-only accessors for the
    // empty-small-segment hysteresis pool's intrusive doubly-linked list
    // (`pool_next`/`pool_prev`). Identical discipline to `live_count`/
    // `decommitted`: a single-word load/store at the field's `offset_of!`
    // offset through the `node` seam. Owner-only: only `AllocCore`'s pool
    // methods (`alloc_core_small_pool.rs`), running exclusively on the
    // segment's owning thread, ever read or write these fields.
    // -------------------------------------------------------------------

    /// Read the owner-only `pool_next` link (the next more-recently-pooled
    /// segment towards the pool's HEAD, or `null` if this is the head / the
    /// segment is not pooled).
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn pool_next_of(&self) -> *mut u8 {
        let off = core::mem::offset_of!(SegmentHeader, pool_next);
        Node::read_ptr_mut(Node::offset(self.base, off) as *const *mut u8)
    }

    /// Write the owner-only `pool_next` link.
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn set_pool_next(&mut self, value: *mut u8) {
        let off = core::mem::offset_of!(SegmentHeader, pool_next);
        Node::write_ptr_mut(Node::offset(self.base, off) as *mut *mut u8, value);
    }

    /// Read the owner-only `pool_prev` link (the previous — closer to the
    /// pool's TAIL — pooled segment, or `null` if this is the tail / the
    /// segment is not pooled).
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn pool_prev_of(&self) -> *mut u8 {
        let off = core::mem::offset_of!(SegmentHeader, pool_prev);
        Node::read_ptr_mut(Node::offset(self.base, off) as *const *mut u8)
    }

    /// Write the owner-only `pool_prev` link.
    #[cfg(feature = "alloc-decommit")]
    #[inline(always)]
    pub(crate) fn set_pool_prev(&mut self, value: *mut u8) {
        let off = core::mem::offset_of!(SegmentHeader, pool_prev);
        Node::write_ptr_mut(Node::offset(self.base, off) as *mut *mut u8, value);
    }

    // -------------------------------------------------------------------
    // Phase B (numa-aware) — field-specific owner-only accessor for the
    // `node_id` field. Identical discipline to `live_count`/`decommitted`:
    // a single-word load/store at the field's `offset_of!` offset through
    // the `node` seam, keeping this file `unsafe`-free. Owner-only (written
    // once at segment init, never mutated thereafter; no cross-thread reader
    // ever touches it). Both accessors are gated on `numa-aware` — without
    // the feature the field is inert dead data.
    //
    // The sentinel value (`NO_NODE_RAW`) matches `numa::NO_NODE` (both are
    // `u32::MAX`); the compile-time assert below enforces this identity so
    // callers can compare `node_id_of(base) != numa::NO_NODE` without risk
    // of the two constants diverging.
    // -------------------------------------------------------------------

    /// Read the NUMA node stored in this segment's header (`NO_NODE_RAW` if
    /// the segment was not bound to a specific node).
    #[cfg(feature = "numa-aware")]
    pub(crate) fn node_id_of(&self) -> u32 {
        let off = core::mem::offset_of!(SegmentHeader, node_id);
        Node::read_u32(Node::offset(self.base, off) as *const u32)
    }

    /// Write the NUMA node into this segment's header.  Called once, at
    /// segment-init time, immediately after the full header is written.
    #[cfg(feature = "numa-aware")]
    pub(crate) fn set_node_id(&mut self, node: u32) {
        let off = core::mem::offset_of!(SegmentHeader, node_id);
        Node::write_u32(Node::offset(self.base, off) as *mut u32, node);
    }

    // -------------------------------------------------------------------
    // PERF-PASS-4 (G9/C2, task #52) — field-specific owner-only accessor for
    // the `ring_drain_head` drain-guard cache. Identical discipline to
    // `live_count`/`decommitted`/`node_id`: a single-word load/store at the
    // field's `offset_of!` offset through the `node` seam. Owner-only: only
    // `find_segment_with_free_impl`'s drain guard (running exclusively on the
    // segment's owning thread, the same thread that calls
    // `RemoteFreeRing::drain`) reads or writes this field.
    // -------------------------------------------------------------------

    /// Read the owner-cached `RemoteFreeRing` head, as of the last drain (or
    /// segment init, if never drained).
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn ring_drain_head_of(&self) -> u32 {
        let off = core::mem::offset_of!(SegmentHeader, ring_drain_head);
        Node::read_u32(Node::offset(self.base, off) as *const u32)
    }

    /// Write the owner-cached `RemoteFreeRing` head. Called after a drain (real
    /// or skipped) to record the ring's current `head` for the next guard
    /// check.
    #[cfg(feature = "alloc-xthread")]
    #[inline(always)]
    pub(crate) fn set_ring_drain_head(&mut self, value: u32) {
        let off = core::mem::offset_of!(SegmentHeader, ring_drain_head);
        Node::write_u32(Node::offset(self.base, off) as *mut u32, value);
    }

    /// Stamp the `owner_thread_free` field ONLY (not a full-struct
    /// `write_header`). The stamping path runs on the owning thread and writes
    /// the field at most once per segment (when it transitions null → the
    /// heap's inline TFS head address); cross-thread readers use the
    /// field-specific [`SegmentHeader::owner_thread_free_at`]. A single-word
    /// field write here cannot race with a Remote's single-word field read of
    /// a disjoint header field.
    #[cfg(feature = "alloc-xthread")]
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    pub(crate) fn stamp_owner_thread_free(
        &mut self,
        head: *const core::sync::atomic::AtomicPtr<u8>,
    ) {
        let off = core::mem::offset_of!(SegmentHeader, owner_thread_free);
        Node::write_ptr(
            Node::offset(self.base, off) as *mut *const core::sync::atomic::AtomicPtr<u8>,
            head,
        );
    }

    /// The page-map view.
    pub(crate) fn page_map(&self) -> PageMap {
        PageMap::new(Node::offset(self.base, Layout::page_map_off()))
    }

    /// The bin-table view.
    #[inline(always)]
    pub(crate) fn bin_table(&self) -> BinTable {
        BinTable::new(Node::offset(self.base, Layout::bin_table_off()) as *mut u32)
    }

    /// The alloc-bitmap view (the Phase 13.4a O(1) double-free guard). The
    /// bitmap bytes are carved at [`Layout::alloc_bitmap_off`] and zeroed at
    /// bootstrap; this returns the typed view over them.
    #[inline(always)]
    pub(crate) fn alloc_bitmap(&self) -> super::alloc_bitmap::AllocBitmap {
        super::alloc_bitmap::AllocBitmap::new(Node::offset(self.base, Layout::alloc_bitmap_off()))
    }

    /// RAD-5 (E4) GO/NO-GO EXPERIMENT — the magazine-residency bitmap view.
    /// The bitmap bytes are carved at [`Layout::magazine_bitmap_off`] and
    /// zeroed at bootstrap (mirroring `alloc_bitmap`); this returns the typed
    /// view over them. See `docs/perf/IAI_BASELINE.md`'s RAD-5 entry for the
    /// measured verdict.
    #[inline(always)]
    pub(crate) fn magazine_bitmap(&self) -> super::magazine_bitmap::MagazineBitmap {
        super::magazine_bitmap::MagazineBitmap::new(Node::offset(
            self.base,
            Layout::magazine_bitmap_off(),
        ))
    }

    /// The per-segment `RemoteFreeRing` view (the non-intrusive cross-thread
    /// free queue). The ring metadata is carved at [`Layout::remote_ring_off`]
    /// at bootstrap; this returns the typed view over it.
    #[cfg(feature = "alloc-xthread")]
    pub(crate) fn remote_ring(&self) -> super::remote_free_ring::RemoteFreeRing {
        super::remote_free_ring::RemoteFreeRing::at(self.base, Layout::remote_ring_off())
    }

    // -------------------------------------------------------------------
    // Phase 12.4 — atomic views over the owner-state / next_abandoned
    // fields. These return `&AtomicU64` at the field's fixed offset so the
    // adoption CAS is a genuine atomic operation (NOT a non-atomic struct
    // field read, which would be a data race under concurrency). The single
    // `unsafe` dereference lives in the [`node`](super::node) seam
    // (`Node::atomic_u64_at`); the field offset is computed by the safe
    // `core::mem::offset_of!` macro on the `#[repr(C)]` header, so this file
    // stays unsafe-free, as it has been since Phase 8.
    // -------------------------------------------------------------------

    /// A `&AtomicU64` view over this segment's `owner_state` field. The
    /// adoption path uses this for the Abandoned→Live CAS (the M9
    /// linearization point). The view aliases the header byte range; access
    /// is atomic so there is no data race with a concurrent header read.
    ///
    /// # Caller's contract
    ///
    /// `self.base` MUST be a live small/primordial segment base with a valid
    /// header at offset 0 (the caller — the abandon/adopt path — guarantees
    /// this; the segment is registered and has a valid header).
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn owner_state_atomic(&self) -> &'static core::sync::atomic::AtomicU64 {
        // `offset_of!` is a safe macro (address arithmetic on a
        // `#[repr(C)]` type); the atomic-view dereference is delegated to
        // the `node` seam.
        let off = core::mem::offset_of!(SegmentHeader, owner_state);
        Node::atomic_u64_at(self.base, off)
    }

    /// A `&AtomicU64` view over this segment's `next_abandoned` intrusive-link
    /// field. Used by the abandon (push) and adopt (pop chain-walk) paths.
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    pub(crate) fn next_abandoned_atomic(&self) -> &'static core::sync::atomic::AtomicU64 {
        let off = core::mem::offset_of!(SegmentHeader, next_abandoned);
        Node::atomic_u64_at(self.base, off)
    }
}

// ---------------------------------------------------------------------------
// X7 Ф1 (task #189) — generation-table byte-level accessors.
//
// The generation table is a standalone byte-array metadata region (like the
// alloc bitmap), NOT a `SegmentHeader` field, so its accessors are free
// functions (not `offset_of!`-on-header field reads). Each cell is an
// `AtomicU8` obtained through the `node` seam (`Node::atomic_u8_at`), mirroring
// how the adoption path obtains `&AtomicU64` views over header fields.
//
// Memory model (X7 plan §2): owner writes Relaxed (single-writer at block issue
// — that is Ф3, not this phase); remote reads Relaxed (also Ф3). Both orderings
// are TSan-clean: the table is the staleness key, not a release/acquire fence.
// Ф1 wires ONLY the byte-level read/RMW primitives — nothing in the
// alloc/dealloc/refill/drain paths consults the table yet.
//
// Compiled ONLY under `#[cfg(feature = "hardened")]`.
// ---------------------------------------------------------------------------

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

const fn align_up_const(n: usize, a: usize) -> usize {
    let mask = a - 1;
    (n + mask) & !mask
}

// Compile-time sanity: the metadata footprints must fit in one segment with
// room for at least one payload page, and the smallest size class must hold a
// free-list node.
const _: () = assert!(Layout::primordial_meta_end() + PAGE <= super::os::SEGMENT);
const _: () = assert!(Layout::small_meta_end() + PAGE <= super::os::SEGMENT);
// X7 Ф1 (task #189): under `hardened` the generation table (~256 KiB / 64 pages)
// is carved into segment metadata, shifting `small_meta_end` up by that much.
// This is exactly the capacity risk the X7 plan §4 "Risks" calls out ("Ёмкость
// сегмента под hardened меняет геометрию"). The assertion above (ungated) already
// re-checks under every feature config, but this hardened-only assert pins the
// LARGER value explicitly — load-bearing, not decorative: if a future change to
// `GEN_TABLE_FOOTPRINT` or the upstream layout pushed the hardened
// `small_meta_end` past `SEGMENT`, the crate would fail to compile under
// `--features hardened` here rather than silently overflowing the payload.
#[cfg(feature = "hardened")]
const _: () = assert!(Layout::small_meta_end() + PAGE <= super::os::SEGMENT);
// PERF-3 Ф1 (task #208): under `alloc-runfreelist` the RunStack (~3 KiB / ≤ 1
// page once page-aligned) is carved into segment metadata, shifting
// `small_meta_end` up by that much. The RunStack is dramatically smaller than
// the gen-table, so the capacity risk is negligible (<0.1% of the 4 MiB
// segment), but the assertion above (ungated) already re-checks under every
// feature config; this runfreelist-only assert pins the LARGER value explicitly
// — load-bearing, not decorative: if a future change to `RunStack::FOOTPRINT`
// or the upstream layout pushed the runfreelist `small_meta_end` past `SEGMENT`
// (e.g. someone raises `RUNSTACK_CAPACITY` by 1000×), the crate would fail to
// compile under `--features alloc-runfreelist` here rather than silently
// overflowing the payload. Mirrors the X7-Ф1 hardened-only pinning above.
#[cfg(feature = "alloc-runfreelist")]
const _: () = assert!(Layout::small_meta_end() + PAGE <= super::os::SEGMENT);
const _: () = assert!(super::size_classes::MIN_BLOCK >= super::node::NODE_SIZE);
// Phase 35: adding the `live_count` / `decommitted` fields must NOT push the
// header past one page, or `Layout::page_map_off()` (= `align_up(sizeof header,
// PAGE)`) would shift and break every downstream offset / the M9 abandoned-stack
// layout. The header is ~96 bytes ≪ PAGE (4 KiB); this asserts it stays so, so
// the byte layout is identical to the pre-Phase-35 build (the fields land in the
// header's existing sub-page padding).
const _: () = assert!(size_of::<SegmentHeader>() <= PAGE);
const _: () = assert!(Layout::page_map_off() == PAGE);
// Phase B: `NO_NODE_RAW` (declared here, in safe code) and `numa::NO_NODE`
// (declared in the confined-unsafe seam) must be identical so comparisons
// like `node_id_of(base) != numa::NO_NODE` are consistent without coupling
// this safe file to the conditionally-compiled `numa` module.
const _: () = assert!(NO_NODE_RAW == u32::MAX);
