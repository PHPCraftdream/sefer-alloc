use core::mem::size_of;

use crate::alloc_core::os::PAGE;

use super::{Layout, SegmentHeader, NO_NODE_RAW};

pub(crate) const fn align_up_const(n: usize, a: usize) -> usize {
    let mask = a - 1;
    (n + mask) & !mask
}

// Compile-time sanity: the metadata footprints must fit in one segment with
// room for at least one payload page, and the smallest size class must hold a
// free-list node.
const _: () = assert!(Layout::primordial_meta_end() + PAGE <= crate::alloc_core::os::SEGMENT);
const _: () = assert!(Layout::small_meta_end() + PAGE <= crate::alloc_core::os::SEGMENT);
// R7-B6 (primordial lazy commit): mirrors `alloc_core_small.rs`'s identical
// `small_meta_end() + LAZY_FIRST_CHUNK` assert, for the primordial
// segment's (larger) metadata footprint. `bootstrap::primordial` commits
// the page-rounded `[0, primordial_meta_end() + LAZY_FIRST_CHUNK)` under
// `primordial-lazy-commit` (task #1074: rounded UP to the runtime OS page
// size — `Layout::lazy_initial_commit`); this pins that sum PLUS one
// MAX_REALISTIC_PAGE_SIZE of rounding slack within one segment at compile
// time so a future metadata-region growth (e.g. a wider registry/hash
// table) fails the build here rather than overflowing the payload at
// runtime.
// R12-9 (task #260): gated on `primordial-lazy-commit` specifically (not the
// shared-mechanism `any(...)` condition) — this assert is about the
// PRIMORDIAL reservation's own initial-commit size, which only
// `primordial-lazy-commit` controls. `LAZY_FIRST_CHUNK` itself (imported
// below) is defined under `alloc_core_small.rs`'s shared-mechanism gate, so
// it is still visible whenever either sub-feature is on; this assert simply
// only NEEDS to hold when the primordial policy is active.
#[cfg(feature = "primordial-lazy-commit")]
const _: () = assert!(
    Layout::primordial_meta_end()
        + crate::alloc_core::alloc_core_small::LAZY_FIRST_CHUNK
        + crate::alloc_core::os::MAX_REALISTIC_PAGE_SIZE
        <= crate::alloc_core::os::SEGMENT
);
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
const _: () = assert!(Layout::small_meta_end() + PAGE <= crate::alloc_core::os::SEGMENT);
const _: () =
    assert!(crate::alloc_core::size_classes::MIN_BLOCK >= crate::alloc_core::node::NODE_SIZE);
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

/// F12 (task #498): exact-size compile-time pin. `size_of::<SegmentHeader>()`
/// has drifted three times already with nothing catching the exact value
/// (see the RAD-3/E2, `committed_payload_end`, and `reserved_capacity` doc
/// comments above tracking 104 -> 120 -> 128 -> 136 -> current) — only the
/// coarser `size_of::<SegmentHeader>() <= PAGE` bound (below) has ever
/// guarded it, which would not catch a field ADDITION that silently changes
/// which fields the large-cache hit arm's targeted field-wise write
/// (`AllocCore::alloc_large`, `alloc_core_large.rs`) needs to cover. This
/// assert pins the CURRENT value (144 bytes, confirmed via a `[u8; 0] =
/// [0u8; size_of::<SegmentHeader>()]` mismatched-array-length compiler-error
/// probe under `--features production` and `--all-features`) so a future
/// field addition/removal fails the build here instead of silently.
/// Deliberately an exact `==` pin (unlike the coarser `<=PAGE` bound below):
/// this is the value the F12 targeted-write optimization's correctness
/// argument was verified against, not a "stays under a budget" bound.
///
/// R2-17 (docs/reviews/2026-09-22-120730-src-review-xa-round-2.md §R2-17): this pin is a 64-bit truth by virtue of the crate-root R2-17
/// target gate in `src/lib.rs`, which rejects any `alloc-core`-built (and
/// therefore any allocator-feature) build on a `target_pointer_width != 64`
/// target outright — so 144 is only ever evaluated where `*mut u8`/`usize`
/// are 8 bytes wide and the `repr(C)` arithmetic the F12 argument rests on
/// actually holds. The pin itself stays UNCONDITIONAL on purpose: if the
/// crate-root gate is ever removed or weakened, this assert must keep
/// failing loudly on any target whose ABI breaks the 144-byte premise,
/// rather than silently letting a mismatched header layout through.
const _: () = assert!(size_of::<SegmentHeader>() == 144);

/// R34-14 (task #533): exhaustive field-classification compile-time pin for
/// the large-cache hit path's carry-forward invariant. Adding a new field to
/// `SegmentHeader` without classifying it here is a COMPILE ERROR: the
/// exhaustive destructuring below (no `..`) names every field, so a new
/// field makes this not compile until it is added AND classified into one
/// of the carry-forward groups documented in `AllocCore::alloc_large`'s hit
/// path (`alloc_core_large.rs`, R34-14 comment block).
///
/// Each field's classification (what the large-cache hit path does with it):
///
/// **Written before register (7 fields)** — actively stored by the hit path
/// before `register()` publishes the segment (F12 targeted writes +
/// R34-14 owner/deferred resets):
/// - `magic` -> `SEGMENT_MAGIC` (F12)
/// - `large_size` -> new size (F12)
/// - `large_align` -> new align (F12)
/// - `bump` -> new bump cursor (F12)
/// - `owner_state` -> `OWNER_ID_NONE` (R34-14, was carried forward — stale
///   value is correct for same-heap reuse but widens the register-to-stamp
///   defensive window for stale/invalid frees)
/// - `owner_thread_free` -> null (R34-14, was carried forward — stale value
///   prevents `stamp_segment_owner` from re-stamping and widens the window)
/// - `deferred_next` -> `ABANDONED_TAIL` (R34-14, was carried forward — a
///   non-`ABANDONED_TAIL` value causes `push_large_deferred_free`'s CAS to
///   fail, silently dropping a subsequent cross-thread free -> permanent
///   leak)
///
/// **Patched after register (1 field)** — written after `register()` returns
/// the real slot index:
/// - `segment_id` -> registry-assigned id (pre-existing 1-word patch)
///
/// **Carried forward, debug-asserted (4 fields)** — verified byte-identical
/// to what the old full-struct write would have written (F12
/// debug_assert_eq! in the hit path):
/// - `span_usable`, `reserved_capacity`, `reservation`, `reservation_len`
///
/// **Carried forward, inert for Large (9 fields)** — Large segments never
/// use these; the stale values are never read:
/// - `kind` (already `Large`; cache only holds former Large segments)
/// - `live_count`, `decommitted` (Large has no small-segment decommit)
/// - `ring_drain_head` (Large has no `RemoteFreeRing`)
/// - `pool_next`, `pool_prev` (Large never joins the small-segment pool)
/// - `committed_payload_end` (Large has no lazy-commit frontier)
/// - `node_id` (re-stamped under `numa-aware`; otherwise inert)
/// - `payload_virgin` (Large has no virgin-zero-skip)
const _: () = {
    let SegmentHeader {
        bump,
        owner_thread_free,
        owner_state,
        magic,
        live_count,
        decommitted,
        ring_drain_head,
        kind,
        large_size,
        large_align,
        span_usable,
        reservation,
        reservation_len,
        deferred_next,
        pool_next,
        pool_prev,
        committed_payload_end,
        reserved_capacity,
        segment_id,
        node_id,
        payload_virgin,
    } = SegmentHeader::large(0, 0, 0, 0, 0, 0, core::ptr::null_mut(), 0);
    let _ = (
        bump,
        owner_thread_free,
        owner_state,
        magic,
        live_count,
        decommitted,
        ring_drain_head,
        kind,
        large_size,
        large_align,
        span_usable,
        reservation,
        reservation_len,
        deferred_next,
        pool_next,
        pool_prev,
        committed_payload_end,
        reserved_capacity,
        segment_id,
        node_id,
        payload_virgin,
    );
};
