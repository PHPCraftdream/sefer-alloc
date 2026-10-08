use core::mem::size_of;

use crate::alloc_core::os::PAGE;

use super::{Layout, SegmentHeader, NO_NODE_RAW};
use super::{LARGE_STATE_OFF, REMOTE_HEAD_OFF, TERMINAL_WORDS_OFF};

pub(crate) const fn align_up_const(n: usize, a: usize) -> usize {
    let mask = a - 1;
    (n + mask) & !mask
}

// Compile-time sanity: the metadata footprints must fit in one segment with
// room for at least one payload page, and the smallest size class must hold a
// free-list node.
const _: () = assert!(Layout::primordial_meta_end() + PAGE <= crate::alloc_core::os::SEGMENT);
const _: () = assert!(Layout::small_meta_end() + PAGE <= crate::alloc_core::os::SEGMENT);
const _: () = assert!(Layout::primordial_active_kind_off().is_multiple_of(8));
const _: () =
    assert!(Layout::primordial_active_kind_off() >= Layout::primordial_free_top_off() + 4);
const _: () = assert!(
    Layout::primordial_active_kind_end()
        <= align_up_const(Layout::primordial_free_top_off() + 4, PAGE)
);
const _: () = assert!(Layout::primordial_active_kind_end() <= Layout::primordial_meta_end());
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
const _: () = assert!(TERMINAL_WORDS_OFF >= size_of::<SegmentHeader>());
const _: () =
    assert!(REMOTE_HEAD_OFF.is_multiple_of(core::mem::align_of::<core::sync::atomic::AtomicU32>()));
const _: () =
    assert!(LARGE_STATE_OFF.is_multiple_of(core::mem::align_of::<core::sync::atomic::AtomicU64>()));
const _: () = assert!(REMOTE_HEAD_OFF + size_of::<u32>() <= LARGE_STATE_OFF);
const _: () = assert!(LARGE_STATE_OFF + size_of::<u64>() <= Layout::page_map_off());
// Phase B: `NO_NODE_RAW` (declared here, in safe code) and `numa::NO_NODE`
// (declared in the confined-unsafe seam) must be identical so comparisons
// like `node_id_of(base) != numa::NO_NODE` are consistent without coupling
// this safe file to the conditionally-compiled `numa` module.
const _: () = assert!(NO_NODE_RAW == u32::MAX);

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
/// - `payload_offset` -> new explicit payload start
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
/// - `pool_next`, `pool_prev` (Large never joins the small-segment pool)
/// - `committed_payload_end` (Large has no lazy-commit frontier)
/// - `node_id` (re-stamped under `numa-aware`; otherwise inert)
/// - `payload_virgin` (Large has no virgin-zero-skip)
const _: () = {
    let SegmentHeader {
        bump,
        owner_state,
        magic,
        live_count,
        decommitted,
        kind,
        large_size,
        large_align,
        payload_offset,
        span_usable,
        reservation,
        reservation_len,
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
        owner_state,
        magic,
        live_count,
        decommitted,
        kind,
        large_size,
        large_align,
        payload_offset,
        span_usable,
        reservation,
        reservation_len,
        pool_next,
        pool_prev,
        committed_payload_end,
        reserved_capacity,
        segment_id,
        node_id,
        payload_virgin,
    );
};
