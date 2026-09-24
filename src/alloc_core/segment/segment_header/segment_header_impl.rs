use super::*;
use crate::alloc_core::node::Node;
use crate::alloc_core::os::PAGE;
#[cfg(feature = "hardened")]
use crate::alloc_core::os::SEGMENT;
#[cfg(feature = "hardened")]
use crate::alloc_core::size_classes::MIN_BLOCK;
// Only the `page-map-diag`-gated `PageClass` items below consume this.
#[cfg(feature = "page-map-diag")]
use crate::alloc_core::size_classes::SMALL_CLASS_COUNT;

// `allow(unused_imports)`: unlike a `const` declaration (this file's
// pre-reorg form), an equivalent re-export binding warns when unreferenced —
// and `SMALL_META_FOOTPRINT` has no consumer anywhere in the crate in any
// feature config; the re-export exists purely to keep the old
// `alloc_core::segment_header::SMALL_META_FOOTPRINT` path valid.
#[allow(unused_imports)]
pub(crate) use descriptors::SMALL_META_FOOTPRINT;
pub(crate) use descriptors::{align_up, BinTable, Layout, PageMap, SegmentMeta, FREE_LIST_NULL};
pub(crate) use layout_asserts::align_up_const;

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
#[doc(hidden)]
pub const OWNER_STATE_LIVE: u64 = 0;
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
#[doc(hidden)]
pub const OWNER_ID_NONE: u32 = 0x7FFF_FFFF;

/// Owner id of the process-global fallback heap (src/global/fallback.rs) — the
/// fallback is NOT a registry slot, so it cannot carry a slot index, and its
/// historical sentinel `u32::MAX` was NOT representable in the 31-bit id
/// field: `pack_owner`'s shift leaked bit 32 into the generation field and
/// `unpack_owner_id` returned `OWNER_ID_NONE`, so `stamp_segment_owner`'s
/// OPT-C fast-path compare (`unpack_owner_id(cur) == self.id`) could never
/// hit on the fallback heap. Chosen constraints (each pinned by the const
/// asserts below): < 2^31 so `pack_owner`/`unpack_owner_id` round-trip it
/// exactly (the property the OPT-C compare needs); `!= OWNER_ID_NONE`; and
/// >= 4096 (`MAX_HEAPS`, src/registry/bootstrap/registry.rs) so every
/// owner-id -> slot resolution (`>= MAX_HEAPS` bounds check, e.g.
/// `heap_core_xthread::ring::owner_slot_is_live`) keeps treating
/// fallback-stamped segments as out-of-range, exactly as the old
/// masks-to-OWNER_ID_NONE behaviour did.
#[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
#[doc(hidden)]
pub const OWNER_ID_FALLBACK: u32 = 0x7FFF_FFFE;

const _: () = assert!(
    (OWNER_ID_FALLBACK as u64) < (1u64 << 31),
    "OWNER_ID_FALLBACK must be representable in the 31-bit owner-id field \
     so pack_owner/unpack_owner_id round-trip it exactly"
);
const _: () = assert!(
    OWNER_ID_FALLBACK != OWNER_ID_NONE,
    "OWNER_ID_FALLBACK must not collide with the unstamped-segment sentinel"
);
const _: () = assert!(
    OWNER_ID_FALLBACK as usize >= 4096,
    "OWNER_ID_FALLBACK must stay outside every real registry slot index \
     (< MAX_HEAPS = 4096) so owner-id resolution keeps treating it as \
     out-of-range"
);

/// Pack `(state, owner_id, generation)` into one `u64` word (the layout
/// documented above the [`OWNER_STATE_LIVE`] constant). `const` so the header
/// constructors can build the initial packed word at compile time.
///
/// `owner_id` is masked to the 31-bit id field before shifting, so no input
/// can leak across a field boundary (bug #1981: the unmasked shift let
/// `u32::MAX` set bit 32 — the generation field's LSB — and round-trip as
/// [`OWNER_ID_NONE`], never `u32::MAX`). An id >= 2^31 therefore clamps
/// rather than round-trips, which is why every id actually stamped into an
/// `owner_state` word — every `HeapCore::id`, plus the
/// [`OWNER_ID_FALLBACK`] sentinel — must stay < 2^31.
#[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
#[doc(hidden)]
#[inline(always)]
pub const fn pack_owner(state: u64, owner_id: u32, generation: u32) -> u64 {
    (state & OWNER_STATE_MASK)
        | (((owner_id as u64) & ((1u64 << 31) - 1)) << OWNER_ID_SHIFT)
        | ((generation as u64) << OWNER_GEN_SHIFT)
}

/// Unpack the owner heap id from an owner-state word.
#[cfg_attr(not(feature = "alloc-global"), allow(dead_code))]
#[doc(hidden)]
#[inline(always)]
pub const fn unpack_owner_id(word: u64) -> u32 {
    ((word & OWNER_ID_MASK) >> OWNER_ID_SHIFT) as u32
}

/// The number of pages in one segment (`SEGMENT / PAGE` = 1024 for the default
/// 4 MiB / 4 KiB pair). The `PageMap` has exactly this many entries.
pub(crate) const PAGES_PER_SEGMENT: usize = crate::alloc_core::os::SEGMENT / PAGE;

/// X7 Ф1 (task #189): the byte footprint of the per-segment **generation
/// table** — the hardened remote-free staleness guard. One byte per
/// `MIN_BLOCK` granule of the WHOLE segment, so every segment-relative offset
/// `off` indexes a unique cell at `off >> MIN_BLOCK_SHIFT` without needing a
/// payload-vs-metadata bounds distinction (the metadata granules' cells are
/// simply never read/written — no block starts there, exactly like the
/// [`AllocBitmap`](crate::alloc_core::alloc_bitmap::AllocBitmap) discipline). For the
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
///
/// R12-11 (task #262): diagnostic-only — see `PageMap`'s struct doc. Gated
/// behind `page-map-diag`; unused (and its `encode_class`/`decode` companions
/// with it) without that feature.
#[cfg(feature = "page-map-diag")]
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
#[cfg(feature = "page-map-diag")]
const _: () = assert!(
    SMALL_CLASS_COUNT < 0xFE,
    "PageClass encodes class indices as a u8 sharing its value space with the \
     Free (0xFF) / Meta (0xFE) sentinels; SMALL_CLASS_COUNT must stay strictly \
     below 0xFE (254) so no real class index can collide with either sentinel"
);

#[cfg(feature = "page-map-diag")]
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
    /// [`crate::alloc_core::os`]). Recorded so `AllocCore::drop` can release the WHOLE
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
    /// Three sentinels: [`ABANDONED_TAIL`] (`u64::MAX`, "not linked into any
    /// stack" — every fresh/reclaimed segment starts here, and the
    /// double-push guard claims the link word FROM this value) and
    /// `DEFERRED_LARGE_TAIL` (`u64::MAX - 1`, "on this stack, no next" — the
    /// bottom-of-stack marker), and `DEFERRED_LARGE_PUBLISHING`
    /// (`u64::MAX - 2`, claimed/swapped but link not yet ready). Accessed atomically through
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
    /// B1 (R7 Workstream B): the owner-only, page-aligned frontier recording
    /// how far the payload has been committed. On the lazy-commit path
    /// (feature `alloc-lazy-commit`, Windows, not `numa-aware`) a fresh small
    /// segment starts with only a small initial chunk committed; the rest is
    /// reserved-but-uncommitted. `committed_payload_end` records the byte
    /// offset (from the segment base) up to which pages are committed. B2
    /// will grow this frontier on demand when a carve would exceed it.
    ///
    /// On the eager path (feature-OFF, Unix, miri, or `numa-aware`), the
    /// entire segment is committed at reservation time, so this field is set
    /// to `SEGMENT` (the full span) — every "is X above the frontier?" check
    /// is trivially false and the code behaves identically to pre-B1.
    ///
    /// **Present in EVERY build's layout** — the byte layout of
    /// `SegmentHeader` is identical regardless of feature config (same
    /// discipline as `live_count`/`node_id`/`ring_drain_head`). The field is
    /// READ and WRITTEN only under `#[cfg(any(feature = "primordial-lazy-commit",
    /// feature = "small-segment-lazy-commit"))]` (R12-9, task #260: the split
    /// sibling features of the former single `alloc-lazy-commit`, which is now
    /// a pure alias for "both together" — see the `Cargo.toml` doc); without
    /// either sub-feature it is inert dead data (lint silenced below). Adding
    /// this 8-byte field grows `size_of::<SegmentHeader>()` from 120 to 128,
    /// but `Layout::page_map_off()` (`align_up(128, 4096)`) remains 4096, so
    /// every downstream metadata offset is UNCHANGED.
    ///
    /// **Not atomic — owner-only.** Written at segment-init time and (B2,
    /// future) when the owner grows the frontier. The cross-thread freer
    /// NEVER touches this field (it pushes into the `RemoteFreeRing`; the
    /// owner grows when it drains). Accessed via the field-specific
    /// `committed_payload_end_of` / `set_committed_payload_end` accessor pair
    /// (same `offset_of!` discipline as `bump` and `live_count`).
    #[cfg_attr(
        not(any(
            feature = "primordial-lazy-commit",
            feature = "small-segment-lazy-commit"
        )),
        allow(dead_code)
    )]
    pub committed_payload_end: usize,
    /// R12-4 (feature `large-reserved-capacity`): for LARGE/huge segments
    /// only, the total VA byte span RESERVED for this segment — which may
    /// exceed `span_usable` (the COMMITTED span; `span_usable`'s own meaning
    /// and the bug #134 carry-forward discipline are UNCHANGED by this
    /// field's addition — see [`span_usable`](Self::span_usable)'s doc).
    /// `reserved_capacity >= span_usable` always.
    ///
    /// Lets a growing `realloc` (OPT-G,
    /// `AllocCore::realloc_inplace_fast_path_known_base`) commit just the
    /// missing tail `[span_usable, new_span_usable)` via
    /// [`crate::alloc_core::os::commit_pages`] and grow the pointer IN PLACE — without a
    /// copy — whenever the grown size still fits within `reserved_capacity`,
    /// instead of needing the WHOLE `reserved_capacity` pre-committed (which
    /// would defeat the RSS savings `exact-span-large` exists to provide).
    ///
    /// Set once at the segment's initial OS reservation
    /// (`alloc_large_slow`'s `large-reserved-capacity` arm, geometric
    /// `LARGE_RESERVED_CAP_GROWTH_FACTOR`x of the initial `span_usable`
    /// capped at `LARGE_RESERVED_CAP_BYTES`; **R14-6/task #291 raised the
    /// factor from 2x to 4x** — see that constant's doc in
    /// `alloc_core_large.rs` for the doubling-cadence-workload data behind
    /// the change) and carried forward
    /// verbatim on a large-cache-hit reuse — same discipline as
    /// `span_usable` (bug #134): never recomputed from `large_size`/
    /// `large_align`, because a cache-hit reuse can be smaller than the
    /// segment's actual physical capacity.
    ///
    /// **Without the feature** (or for a Small/Primordial segment, or a
    /// Large segment built before this feature existed / reused across a
    /// feature-off rebuild): equals `span_usable` — "reserved == committed",
    /// which makes every `reserved_capacity`-gated check in the OPT-G path a
    /// trivial no-op identical to pre-R12-4 behaviour (the committed-span
    /// check `payload_off + new_eff <= span_usable` already covers
    /// everything reachable).
    ///
    /// **Present in EVERY build's layout** — the byte layout of
    /// `SegmentHeader` is identical regardless of feature config (same
    /// discipline as `committed_payload_end`/`node_id`). The field is READ
    /// and WRITTEN only under `#[cfg(feature = "large-reserved-capacity")]`;
    /// without that feature it is inert dead data (lint silenced below).
    /// Adding this 8-byte field grows `size_of::<SegmentHeader>()` from 128
    /// to 136, but `Layout::page_map_off()` (`align_up(136, 4096)`) remains
    /// 4096, so every downstream metadata offset is UNCHANGED.
    ///
    /// **Not atomic — owner-only.** Written at segment-init time and grown
    /// by the owner's `realloc` path (never shrunk). The cross-thread freer
    /// never touches this field — it only reads `span_usable` (via
    /// `safe_payload_read_span`) to bound its own defensive copy length,
    /// which stays correct regardless of `reserved_capacity`'s value.
    #[cfg_attr(not(feature = "large-reserved-capacity"), allow(dead_code))]
    pub reserved_capacity: usize,
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
    /// R12-10 (task #261, `virgin-zero-skip`): owner-only flag recording
    /// whether every byte in this segment's payload `[small_meta_end, bump)`
    /// was made readable EITHER by the segment's own fresh OS reservation OR
    /// by an incremental first-time `commit_pages` grow-on-carve call, with
    /// NO in-place decommit-then-recommit cycle ever having occurred on this
    /// segment's CURRENT registration. `1` (true) means a bump-cursor CARVE
    /// on this segment is OS-zero-guaranteed and `alloc_zeroed` may skip its
    /// explicit `Node::zero` pass for the carved block; `0` (false) means the
    /// carve must be zeroed explicitly, exactly as before this feature
    /// existed.
    ///
    /// **This bit alone does not decide virginity of a SPECIFIC served
    /// block** — see `docs/perf/R11_8_SMALL_VIRGIN_ZERO_SKIP_DESIGN.md` §2's
    /// combined predicate: a block is virgin only if BOTH (a) it was served
    /// by a bump-cursor carve (`carve_block`/`carve_batch`), never a
    /// free-list pop (`pop_free`) — a pop is by construction never virgin,
    /// regardless of this bit's value — AND (b) this bit reads `true` at the
    /// moment of that carve AND (c) `cfg!(not(miri))`. `carve_block` /
    /// `carve_batch` are the only READERS; `reserve_small_segment` (sets
    /// `cfg!(not(miri))` on a genuinely fresh reservation) and
    /// `decommit_empty_segment_impl`'s `release_follows == false` retain-leg
    /// (sets `false`, defensively — see that function's doc: the leg has
    /// zero production callers today, but the bit must fail safe if a future
    /// decommit policy ever re-enables it) are the only WRITERS. Every write
    /// site and every read site runs on the segment's owning thread only —
    /// same owner-only, single-writer discipline as `bump`/`live_count`/
    /// `decommitted` (no cross-thread reader or writer ever touches this
    /// field, so a plain field read/write is race-free).
    ///
    /// **Present in EVERY build's layout** (same discipline as
    /// `committed_payload_end`/`node_id`) — the byte layout of
    /// `SegmentHeader` is identical regardless of feature config. The field
    /// is READ and WRITTEN only under `#[cfg(feature = "virgin-zero-skip")]`;
    /// without that feature it is inert dead data (lint silenced below).
    /// Stored as a `u32` (not a single bit) to match the existing
    /// `live_count`/`decommitted`/`node_id` plain-field-at-`offset_of!`
    /// accessor discipline — the 1-bit payload is not worth a bitfield here.
    #[cfg_attr(not(feature = "virgin-zero-skip"), allow(dead_code))]
    pub payload_virgin: u32,
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
            // B1 (R7 Workstream B): the committed-payload frontier. The
            // constructor sets it to 0 (placeholder); the caller
            // (reserve_small_segment) stamps the real value immediately after
            // writing the header via set_committed_payload_end. On the eager
            // path: SEGMENT (full span). On the lazy path: metadata_end +
            // first chunk.
            committed_payload_end: 0,
            // R12-4: inert for Small/Primordial segments — Large-only field
            // (see the field's doc comment).
            reserved_capacity: 0,
            // Phase B: NUMA node is unknown at construction time; the caller
            // (reserve_small_segment under numa-aware) stamps the real value
            // immediately after writing the header via set_node_id.
            node_id: NO_NODE_RAW,
            // PERF-PASS-4 (G9/C2): a fresh segment's ring starts at head == 0
            // (RemoteFreeRing::init_in_place zeroes the cursors); the cache
            // starts at the same value so the first drain guard check
            // correctly observes "nothing to drain yet".
            ring_drain_head: 0,
            // R12-10 (`virgin-zero-skip`): the constructor sets a PLACEHOLDER
            // (0/false) — the caller (`reserve_small_segment`) stamps the
            // real value (`cfg!(not(miri))`) immediately after writing the
            // header, exactly like `committed_payload_end`'s own placeholder
            // pattern above. Placeholder is `false`, not `true`: if a future
            // caller ever forgets the stamp, the fail-safe default is
            // "never skip the zero pass", not "silently return
            // uninitialised/dirty memory".
            payload_virgin: 0,
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
    ///
    /// `reserved_capacity` (R12-4) is the segment's total RESERVED VA span
    /// (`>= span_usable`) — see [`reserved_capacity`](Self::reserved_capacity)'s
    /// doc. The caller MUST pass `span_usable` itself (not a larger value)
    /// when the `large-reserved-capacity` feature is off, or when reusing a
    /// cached segment that predates the feature/was reserved without extra
    /// capacity — "reserved == committed" is always a safe, inert value.
    /// Like `span_usable`, this MUST be carried forward verbatim (never
    /// recomputed) on a cache-hit reuse — see the field's doc for the same
    /// bug-#134-shaped rationale.
    ///
    /// `#[allow(clippy::too_many_arguments)]`: this is a plain field-by-field
    /// struct constructor (like [`small`](Self::small) beside it) — every
    /// parameter maps 1:1 onto a `SegmentHeader` field with no combinable
    /// substructure, so splitting it into a builder or a parameter struct
    /// would add indirection without reducing the actual information the
    /// caller must supply. R12-4 added the 5th argument
    /// (`reserved_capacity`), crossing clippy's default 7-argument
    /// threshold at 8; the same reasoning applied at every prior addition to
    /// this constructor (R12-3's `span_usable`, before that `segment_id`
    /// etc.).
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn large(
        segment_id: u32,
        size: usize,
        align: usize,
        span_usable: usize,
        reserved_capacity: usize,
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
            // B1 (R7 Workstream B): inert for Large segments (they hold one
            // allocation and are freed wholesale — no incremental commit).
            committed_payload_end: 0,
            // R12-4: the segment's total reserved VA span (see the field doc
            // and this constructor's doc above).
            reserved_capacity,
            // Phase B: same sentinel as small(); the caller (alloc_large under
            // numa-aware) stamps the real value after writing the header.
            node_id: NO_NODE_RAW,
            // Large segments have no RemoteFreeRing (no BinTable either) —
            // inert, like live_count/decommitted above.
            ring_drain_head: 0,
            // R12-10 (`virgin-zero-skip`): inert for Large segments — the
            // virgin-carve skip is Small-only by design (Large already has
            // its own, structurally simpler, per-segment freshness skip via
            // `alloc_large`'s `(ptr, is_fresh)` return — see the design
            // docs' §1 "why Large and Small differ structurally").
            payload_virgin: 0,
        }
    }

    /// Read the header at `base` (segment base, any kind) THROUGH the node
    /// seam. Returns a copy of the header. `base` MUST be a live segment base
    /// with a valid header at offset 0.
    ///
    /// R2-06 (independent src review round 2, task #2008): `deferred_next`
    /// is the cross-thread deferred-large-free protocol's intrusive Treiber
    /// link word — a REMOTE thread may CAS/store it at any time via
    /// [`SegmentMeta::deferred_next_atomic`] (`push_large_deferred_free`),
    /// independent of whatever this snapshot's caller owns/holds. A plain
    /// full-struct load (the old implementation, `Node::read_struct`) would
    /// read those bytes non-atomically — a data race, and therefore
    /// undefined behavior, against that concurrent atomic write, regardless
    /// of whether a given call site happens to be safe in practice (several
    /// are, by protocol construction, but proving that per call site is
    /// fragile and does not scale to every current AND future caller of this
    /// function). [`Node::read_struct_with_atomic_word`] closes this
    /// structurally: it never performs a non-atomic read over
    /// `deferred_next`'s bytes at all, filling them via a real atomic load
    /// instead — so `read_at` is sound for EVERY caller, including a
    /// diagnostic/census walk over segments this thread does not own.
    pub(crate) fn read_at(base: *mut u8) -> Self {
        Node::read_struct_with_atomic_word::<SegmentHeader>(
            base as *const SegmentHeader,
            core::mem::offset_of!(SegmentHeader, deferred_next),
        )
    }
}

/// X7 Ф1 (task #189) — the generation-table byte-level accessors moved to
/// [`segment_header_gen_table`](super::super::segment_header_gen_table) (task
/// R6-CQ-7c's split); re-exported at this path (doc-hidden test-only
/// forwarder — CLAUDE.md's "one file, one export" exception category 1) so
/// existing callers of `sefer_alloc::alloc_core::segment_header::{gen_at,
/// bump_gen, GEN_TABLE_FOOTPRINT}` (e.g. `tests/regression_gen_table_layout.rs`,
/// `tests/regression_gen_table_lifecycle_seams.rs`,
/// `tests/regression_gen_wrap_boundary.rs`,
/// `tests/regression_r2_3_gen_table_index_guard.rs`) do not need to change
/// their import path.
#[cfg(feature = "hardened")]
pub use super::super::segment_header_gen_table::{bump_gen, gen_at, init_gen_table_in_place};
