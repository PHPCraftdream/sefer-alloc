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
#[cfg(feature = "hardened")]
use super::size_classes::MIN_BLOCK;
use super::size_classes::SMALL_CLASS_COUNT;

/// Magic value written to every segment header at creation. Used as a sanity
/// check that a computed segment base really is one of our segments (defence
/// against a foreign pointer being passed to `dealloc`).
pub(crate) const SEGMENT_MAGIC: u32 = 0x5E_F5_E0_01;

// ---------------------------------------------------------------------------
// Segment ownership state (the `owner_state: u64` field).
//
// Each small/primordial segment carries an `owner_state: u64` field packing:
//
//   bits [0]      : state    — 0 = LIVE (owned by a heap). Always LIVE today:
//                              the abandoned-segments / adoption substrate that
//                              wrote the `1 = ABANDONED` value was removed
//                              (task #97 / R4-5); the bit is retained in the
//                              packing for layout stability but is structurally
//                              always 0 now.
//   bits [1..32]  : owner_id — the owning heap's registry slot index
//                              (MAX_HEAPS = 4096 ≪ 2^31, so 31 bits is ample)
//   bits [32..63] : generation — the coherence key read by cross-thread free
//                                routing (a stale pointer reading an old
//                                generation is routed to the slow path).
//
// The packing is plain data (laid down / read through the `node` seam, like
// the rest of the header) so this file stays `unsafe`-free.
//
// `cfg_attr(not(alloc-global), allow(dead_code))`: the helpers below are used
// by the registry's owner-resolution path, which is `alloc-global`-gated.
// Without `alloc-global` the registry does not compile, so the helpers appear
// unused — but they are part of the segment header's documented contract (the
// fields exist in every build's layout), so we silence the dead-code lint
// rather than gate the fields themselves.
// ---------------------------------------------------------------------------

/// Owner-state bit layout.
#[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
pub(crate) const OWNER_STATE_LIVE: u64 = 0;
/// Mask for the state bit (bit 0).
const OWNER_STATE_MASK: u64 = 0x1;
/// Bit-shift for the owner heap id field (starts at bit 1).
const OWNER_ID_SHIFT: u32 = 1;
const OWNER_ID_MASK: u64 = ((1u64 << 31) - 1) << OWNER_ID_SHIFT;
/// Bit-shift for the generation field (starts at bit 32). Retained because
/// [`pack_owner`] packs a generation (always 0 now that the adoption substrate
/// that bumped it is gone — task #97 / R4-5; the field is kept for layout
/// stability).
const OWNER_GEN_SHIFT: u32 = 32;

/// Sentinel owner id meaning "not bound to any heap yet" (a freshly-reserved
/// segment before its first stamp). Distinct from a real slot index (which is
/// `< MAX_HEAPS`).
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

/// Unpack the owner heap id from an owner-state word.
#[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
#[inline(always)]
pub(crate) const fn unpack_owner_id(word: u64) -> u32 {
    ((word & OWNER_ID_MASK) >> OWNER_ID_SHIFT) as u32
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

// R6-OPT-P0-3a (correctness-surface item #4, "page map"): `PageClass` encodes
// a class index as a plain `u8` sharing its value space with the two sentinel
// discriminants `Free = 0xFF` / `Meta = 0xFE`. `encode_class`/`decode` are
// sound only while every real class index stays strictly below 0xFE (254) —
// otherwise a class-254 or class-255 page would be indistinguishable from
// `Meta`/`Free` and `decode` would silently misreport it as "not a class
// page". `medium-classes` (R6-OPT-P0-3a) grows `SMALL_CLASS_COUNT` from 49 to
// 55 — nowhere near 254 — but this pins the invariant at compile time (for
// EVERY feature configuration, not just `medium-classes`) rather than leaving
// it as an unstated assumption a much later class-count grower could violate
// silently.
const _: () = assert!(
    SMALL_CLASS_COUNT < 0xFE,
    "PageClass encodes class indices as a u8 sharing its value space with the \
     Free (0xFF) / Meta (0xFE) sentinels; SMALL_CLASS_COUNT must stay strictly \
     below 0xFE (254) so no real class index can collide with either sentinel"
);

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
/// routing), `owner_state` (owner-id compare — the state bit is
/// structurally always LIVE since the adoption substrate that wrote
/// `ABANDONED` was removed, task #97 / R4-5), `magic`
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
/// `span_usable`, `reservation`, `reservation_len`, `deferred_next`,
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
/// `reservation`, `reservation_len`, `deferred_next`, `pool_next`,
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
    /// The segment's ownership state — packed
    /// `(state, owner_heap_id, generation)` (see the [`OWNER_STATE_*`] /
    /// [`OWNER_ID_*`] / [`OWNER_GEN_*`] constants above). The state bit is
    /// structurally always `LIVE` (the abandoned-segments / adoption
    /// substrate that wrote the `ABANDONED` value was removed — task #97 /
    /// R4-5; the bit is retained only for layout stability), and
    /// `generation` is always `0` for the same reason. The LIVE value this
    /// word carries is the owning heap's slot index (`owner_heap_id`),
    /// stamped at claim time and read by cross-thread free routing to
    /// recognise ownership. Stored as a plain `u64` so the `#[repr(C)] Copy`
    /// `SegmentHeader` remains a plain bit-pattern (the bootstrap lays it down
    /// via `Node::write_struct`, and `SegmentMeta::header` reads it back as a
    /// unit). Cross-thread readers access it through the dedicated
    /// [`owner_state_atomic`](SegmentMeta::owner_state_atomic) view (`&AtomicU64`
    /// at the same fixed offset), because a plain struct-field read would
    /// race a concurrent owner-stamp store.
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
    /// `deferred_next`); it is read/mutated ONLY under `alloc-decommit`. Without
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
    /// The intrusive link for the cross-thread deferred-large-free Treiber
    /// stack (see `alloc_core::deferred_large`): while a Large segment `base`
    /// is queued for its owning heap to reclaim, this field holds the ADDRESS
    /// of the NEXT queued base (packed as an exposed-provenance `u64`), or a
    /// sentinel. Stored as a plain `u64` (not a pointer) so the field is plain
    /// `Copy` data inside the header; the address↔pointer reconstruction is
    /// done at the push/drain call sites via `expose_provenance` /
    /// `with_exposed_provenance_mut` (the crate's sanctioned exposed-provenance
    /// pairing — see `deferred_large::push`/`drain`).
    ///
    /// Two sentinels: [`ABANDONED_TAIL`] (`u64::MAX`, "not linked into any
    /// stack" — every fresh/reclaimed segment starts here, and the
    /// double-push guard claims the link word FROM this value) and
    /// `DEFERRED_LARGE_TAIL` (`u64::MAX - 1`, "on this stack, no next" — the
    /// bottom-of-stack marker). Accessed atomically through
    /// [`deferred_next_atomic`](SegmentMeta::deferred_next_atomic).
    ///
    /// Historically this field was the link for the abandoned-segments stack of
    /// the segment-transfer substrate; that substrate was removed (task #97 /
    /// R4-5) and the field was repurposed for the deferred-large stack, which
    /// is its sole current consumer. The `ABANDONED_TAIL` sentinel keeps its
    /// historical name for the same reason (it is that link's "free" marker).
    pub deferred_next: u64,
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
    /// ever touches these fields (unlike `deferred_next`, which the
    /// CROSS-THREAD deferred-large-free protocol accesses via a `&AtomicU64`
    /// view — hence THAT field stays a `u64` address/sentinel hybrid with exposed
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

/// Sentinel for the [`deferred_next`](SegmentHeader::deferred_next) link
/// word meaning "not currently linked into any stack" — the rest value
/// every fresh/reclaimed segment header starts with, and the value the
/// deferred-large double-push guard claims the link word FROM (see
/// `alloc_core::deferred_large::push`). Deliberately distinct from
/// `DEFERRED_LARGE_TAIL` (`u64::MAX - 1`, "on this stack, no next"): if the
/// two coincided, a `base` pushed onto an EMPTY deferred-large stack would
/// read back as "never pushed", silently defeating the guard the first time
/// it ran. Neither `u64::MAX` nor `u64::MAX - 1` is ever a real link value
/// (a queued base address cast to `u64` is SEGMENT-aligned and nowhere near
/// `usize::MAX`), so both are unambiguous.
///
/// The `ABANDONED_` prefix is historical: this sentinel pre-dates the
/// deferred-large repurposing of `deferred_next` (it was the abandoned-
/// segments stack tail). The name is retained; the value is now the
/// deferred-large link's "free" marker.
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
    /// (not yet stamped with a real heap id); the claim path stamps it when
    /// the segment is bound to a heap.
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
            deferred_next: ABANDONED_TAIL,
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
            deferred_next: ABANDONED_TAIL,
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
    ///
    /// R6-MS-3 (round5 memory_safety_review R5-MS-3): an out-of-range `c`
    /// (`>= SMALL_CLASS_COUNT`) is a RELEASE-MODE no-op returning
    /// `FREE_LIST_NULL`, NOT merely a `debug_assert!`. The previous guard
    /// compiled out under the `production` profile, so a caller-controlled
    /// `class_idx` (e.g. via `flush_class`/`dbg_freelist_head_for`) could raw-
    /// read `heads + c * 4` out of bounds. The check lives here, inside the
    /// lowest-level accessor, so EVERY caller (production `dealloc_small`/
    /// `flush_run`, and the doc-hidden `dbg_*` seams) is protected uniformly;
    /// the `debug_assert!` is retained as a debug-mode tripwire.
    #[inline(always)]
    pub(crate) fn head(&self, c: usize) -> u32 {
        debug_assert!(c < SMALL_CLASS_COUNT, "class index out of range");
        if c >= SMALL_CLASS_COUNT {
            return FREE_LIST_NULL;
        }
        Node::read_u32_unaligned(self.heads_at_const(c))
    }

    /// Set the head of class `c`'s free list to `off`.
    ///
    /// R6-MS-3: an out-of-range `c` (`>= SMALL_CLASS_COUNT`) is a RELEASE-MODE
    /// no-op, NOT merely a `debug_assert!` (same rationale as [`head`](Self::head)).
    #[inline(always)]
    pub(crate) fn set_head(&mut self, c: usize, off: u32) {
        debug_assert!(c < SMALL_CLASS_COUNT, "class index out of range");
        if c >= SMALL_CLASS_COUNT {
            return;
        }
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
    //   - the cross-thread-read fields split by access kind:
    //     * `kind`, `owner_thread_free` are written ONCE at init/stamp time
    //       and only read cross-thread thereafter — a plain field read of
    //       either does not race the owner's disjoint-field `bump` writes
    //       (verified R6-MS-5: no atomic writer exists for either field, so
    //       there is no plain-read-vs-atomic-store access-kind mismatch).
    //     * `magic` is the EXCEPTION — it is ALSO atomically zeroed on
    //       Large-segment recycle-to-cache (UBFIX-6), so its cross-thread
    //       read is an ATOMIC Acquire load (`magic_at` via `atomic_u32_at`),
    //       NOT a plain field read, pairing the recycler's Release store.
    //       (R6-MS-5 / U-R5-1 closed this access-kind mismatch.)
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
    // Atomic views over the owner-state / deferred_next fields. These
    // return `&AtomicU64` at the field's fixed offset so a cross-thread
    // read/store is a genuine atomic operation (NOT a non-atomic struct
    // field read, which would be a data race under concurrency). The single
    // `unsafe` dereference lives in the [`node`](super::node) seam
    // (`Node::atomic_u64_at`); the field offset is computed by the safe
    // `core::mem::offset_of!` macro on the `#[repr(C)]` header, so this file
    // stays unsafe-free, as it has been since Phase 8.
    // -------------------------------------------------------------------

    /// A `&AtomicU64` view over this segment's `owner_state` field.
    /// Cross-thread free routing uses this for a race-free read of the
    /// owning heap's id (`owner_state`'s `owner_heap_id` field); the
    /// owner-stamp path uses it for the atomic store. The view aliases the
    /// header byte range; access is atomic so there is no data race with a
    /// concurrent header read.
    ///
    /// # Caller's contract
    ///
    /// `self.base` MUST be a live small/primordial segment base with a valid
    /// header at offset 0 (the caller — cross-thread free routing / owner
    /// stamping — guarantees this; the segment is registered and has a valid
    /// header).
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    #[inline(always)]
    pub(crate) fn owner_state_atomic(&self) -> &'static core::sync::atomic::AtomicU64 {
        // `offset_of!` is a safe macro (address arithmetic on a
        // `#[repr(C)]` type); the atomic-view dereference is delegated to
        // the `node` seam.
        let off = core::mem::offset_of!(SegmentHeader, owner_state);
        Node::atomic_u64_at(self.base, off)
    }

    /// A `&AtomicU64` view over this segment's `deferred_next` intrusive-link
    /// field. Used by the deferred-large-free push (remote producer) and
    /// drain (owner) paths — see `alloc_core::deferred_large`.
    #[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
    pub(crate) fn deferred_next_atomic(&self) -> &'static core::sync::atomic::AtomicU64 {
        let off = core::mem::offset_of!(SegmentHeader, deferred_next);
        Node::atomic_u64_at(self.base, off)
    }
}

pub(crate) const fn align_up_const(n: usize, a: usize) -> usize {
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

/// X7 Ф1 (task #189) — the generation-table byte-level accessors moved to
/// [`segment_header_gen_table`](super::segment_header_gen_table) (task
/// R6-CQ-7c's split); re-exported at this path (doc-hidden test-only
/// forwarder — CLAUDE.md's "one file, one export" exception category 1) so
/// existing callers of `sefer_alloc::alloc_core::segment_header::{gen_at,
/// bump_gen, GEN_TABLE_FOOTPRINT}` (e.g. `tests/regression_gen_table_layout.rs`,
/// `tests/regression_gen_table_lifecycle_seams.rs`,
/// `tests/regression_gen_wrap_boundary.rs`,
/// `tests/regression_r2_3_gen_table_index_guard.rs`) do not need to change
/// their import path.
#[cfg(feature = "hardened")]
pub use super::segment_header_gen_table::{bump_gen, gen_at, init_gen_table_in_place};
