//! The allocator-internal residue of the OS aperture: the R7-A1 directory
//! sidecar trio (`reserve_directory_sidecar`, `read_directory_class_words`,
//! `read_directory_node_bucket`), which depends on the allocator-internal
//! `SegmentDirectory` / `sidecar` / `segment_header` types and therefore stayed
//! behind when the SEGMENT-aligned OS aperture itself (`Segment`, the
//! `SEGMENTS_*` counters, `SEGMENT`/`PAGE`, the reserve/release/commit/decommit
//! wrappers and the depot-overflow storage) moved to the `sefer-platform` crate
//! by plan step P1.4 (docs/plans/2026-11-15-workspace-restructure-plan.md).

// The crate is `#![deny(unsafe_code)]` with `alloc-core` on; this is one of
// the documented `unsafe` seams. `allow` lifts the crate-level `deny` for the
// by-value sidecar readers below (no reference to the sidecar escapes them).
#![allow(unsafe_code)]

// P1.4 conduit: the moved surface is re-exported verbatim under its historical
// `alloc_core::os` path, so every `crate::alloc_core::os::*` call site
// (including `SegmentLayout`'s geometry constants and the `SEGMENTS_*_TOTAL`
// accounting statics) is unchanged. Pure move — same items, same cfg gates,
// same accounting.
pub(crate) use sefer_platform::os::*;

// P1.4 wrappers: the moved functions that take raw pointers are `pub unsafe fn`
// in `sefer-platform` (R25-1 class: no safe public raw-pointer API). These
// local `pub(crate)` safe wrappers keep the pre-extraction signatures, so no
// call site changes; they shadow the glob re-export above by name. The caller
// contract of each is documented on the `sefer-platform` function.

pub(crate) fn release_segment(reservation: *mut u8, reservation_len: usize) {
    // SAFETY: P1.4: wrapper preserves the pre-extraction pub(crate) safe signature; the
    // caller contract is unchanged (see the documented contract in sefer-platform).
    unsafe { sefer_platform::os::release_segment(reservation, reservation_len) }
}

#[cfg(all(feature = "alloc-decommit", not(feature = "numa-aware")))]
pub(crate) fn try_release_segment(
    reservation: *mut u8,
    reservation_len: usize,
) -> Result<(), aligned_vmem::VmemError> {
    // SAFETY: P1.4: wrapper preserves the pre-extraction pub(crate) safe signature; the
    // caller contract is unchanged (see the documented contract in sefer-platform).
    unsafe { sefer_platform::os::try_release_segment(reservation, reservation_len) }
}

#[cfg(feature = "alloc-decommit")]
pub(crate) fn decommit_pages(base: *mut u8, start_offset: usize, end_offset: usize) {
    // SAFETY: P1.4: wrapper preserves the pre-extraction pub(crate) safe signature; the
    // caller contract is unchanged (see the documented contract in sefer-platform).
    unsafe { sefer_platform::os::decommit_pages(base, start_offset, end_offset) }
}

#[must_use]
#[cfg(any(
    feature = "primordial-lazy-commit",
    feature = "small-segment-lazy-commit",
    feature = "large-reserved-capacity"
))]
pub(crate) fn commit_pages(base: *mut u8, start_offset: usize, end_offset: usize) -> bool {
    // SAFETY: P1.4: wrapper preserves the pre-extraction pub(crate) safe signature; the
    // caller contract is unchanged (see the documented contract in sefer-platform).
    unsafe { sefer_platform::os::commit_pages(base, start_offset, end_offset) }
}

#[must_use]
#[cfg(feature = "alloc-decommit")]
#[cfg_attr(feature = "small-segment-lazy-commit", allow(dead_code))]
pub(crate) fn recommit_pages(base: *mut u8, start_offset: usize, end_offset: usize) -> bool {
    // SAFETY: P1.4: wrapper preserves the pre-extraction pub(crate) safe signature; the
    // caller contract is unchanged (see the documented contract in sefer-platform).
    unsafe { sefer_platform::os::recommit_pages(base, start_offset, end_offset) }
}

#[must_use]
#[cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "alloc-decommit"
))]
pub(crate) fn commit_depot_overflow(base: *mut u8, start: usize, end: usize) -> bool {
    // SAFETY: P1.4: wrapper preserves the pre-extraction pub(crate) safe signature; the
    // caller contract is unchanged (see the documented contract in sefer-platform).
    unsafe { sefer_platform::os::commit_depot_overflow(base, start, end) }
}

#[cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    feature = "alloc-decommit"
))]
pub(crate) fn decommit_depot_overflow(base: *mut u8, start: usize, end: usize) {
    // SAFETY: P1.4: wrapper preserves the pre-extraction pub(crate) safe signature; the
    // caller contract is unchanged (see the documented contract in sefer-platform).
    unsafe { sefer_platform::os::decommit_depot_overflow(base, start, end) }
}

// ── R7-A1: directory sidecar VM reservation ────────────────────────────────
//
// The owner-only `SegmentDirectory` uses `reserve_zeroed_with` and an
// `AccountedSidecar` token that releases its VM reservation with `AllocCore`.
// Only the owning thread dereferences it; no cross-thread CAS publication
// is needed for this directory pointer.

/// Reserve and construct a [`crate::alloc_core::segment_directory::SegmentDirectory`] sidecar. Returns
/// `Some((ptr, sidecar))` on success — `ptr` is the fully-valid initial
/// state and `sidecar` (the [`crate::alloc_core::sidecar::AccountedSidecar`] token) OWNS the
/// span's VM reservation, released exactly once when the caller drops the
/// token (for this crate's single caller: when the owning `AllocCore` drops
/// its `directory_sidecar_vm` field — R2-12; the span is NO longer leaked
/// for the process lifetime), or `None` on OOM (sidecar OOM is NOT allocator
/// OOM — the mechanism simply stays off and the linear scan fallback is
/// used).
///
/// Uses [`crate::alloc_core::sidecar::reserve_zeroed_with`] rather than [`crate::alloc_core::sidecar::reserve`]:
/// `SegmentDirectory`'s bitmap fields (`class_nonempty_by_node`,
/// `active_bits_by_node`) are valid at all-zero (every bit/count clear is a
/// real, intentional initial state), but under `numa-aware` the `node_ids`
/// table is NOT (`0` is a real OS node id, so leaving it OS-zeroed would
/// misread as "node 0 already claimed bucket 0" — see that field's own doc
/// comment); the fixup closure runs the same `init_node_ids_raw` repair the
/// pre-R14-9 code ran as a separate step right after reservation — R17-1
/// (task #318): raw-pointer variant, never materialising a `&mut
/// SegmentDirectory` over the not-yet-fully-valid span (see
/// `reserve_zeroed_with`'s `# Safety` contract). Moving the whole (up to
/// ~56 KiB under `numa-aware`) `SegmentDirectory` through a by-value
/// [`crate::alloc_core::sidecar::reserve`] call would risk an avoidable stack copy; the
/// in-place fixup avoids it.
///
/// The caller stores the pointer in `AllocCore::directory_sidecar` (and the
/// token in `AllocCore::directory_sidecar_vm`) and dereferences it via
/// [`crate::alloc_core::sidecar::deref`] / [`crate::alloc_core::sidecar::deref_mut`].
#[cfg(feature = "alloc-segment-directory")]
pub(crate) fn reserve_directory_sidecar() -> Option<(
    *mut crate::alloc_core::segment_directory::SegmentDirectory,
    crate::alloc_core::sidecar::AccountedSidecar,
)> {
    // R2-12: the accounting key. Under `numa-aware` the span is multiplied
    // by `NODE_BITMAPS` and routes into the NUMA-flavored counter;
    // non-numa builds route into the plain directory counter — exactly one
    // of the two flavors is live per build.
    #[cfg(feature = "numa-aware")]
    let kind = crate::alloc_core::sidecar::SidecarKind::NumaDirectory;
    #[cfg(not(feature = "numa-aware"))]
    let kind = crate::alloc_core::sidecar::SidecarKind::Directory;
    // SAFETY: `SegmentDirectory`'s bitmap fields (`class_nonempty_by_node`,
    // `active_bits_by_node`) are the only fields `init_node_ids_raw` leaves
    // untouched, and both are valid at all-zero (every bit/count clear is a
    // real, intentional initial state — "no class non-empty yet" / "zero
    // active bits yet"). The one field that is NOT valid at all-zero
    // (`node_ids` under `numa-aware`: `0` is a real OS node id) is exactly
    // the field `init_node_ids_raw` repairs, through raw-pointer writes only
    // (never materialising a reference over the not-yet-valid value — see
    // that function's own `# Safety` doc), before this function's caller can
    // observe the pointer. Same owner-only single-writer discipline as every
    // other sidecar in this module (the pointer is stored in
    // `AllocCore::directory_sidecar` and dereferenced only via
    // `sidecar::deref`/`deref_mut`).
    unsafe {
        crate::alloc_core::sidecar::reserve_zeroed_with(kind, |p| {
            crate::alloc_core::segment_directory::SegmentDirectory::init_node_ids_raw(p);
        })
    }
}

/// Read ONE `class_nonempty_by_node[node_bucket][class_idx]` word-array out
/// of the directory sidecar BY VALUE, without materialising any `&`/`&mut
/// SegmentDirectory` reference (task #252 / R12-1).
///
/// # Why this exists
///
/// `find_segment_with_free_impl`'s directory-driven scan (in
/// `alloc_core_small.rs`) used to hold a live `&'static SegmentDirectory`
/// (from `deref_directory_sidecar`, removed by the R14-9 sidecar-seam
/// migration — today's seam is `crate::alloc_core::sidecar::deref[_mut]`)
/// across calls to
/// `validate_directory_candidate`, which can itself call
/// `deref_directory_sidecar_mut` (likewise removed; today's seam is
/// `crate::alloc_core::sidecar::deref_mut`) (via `publish_empty` /
/// `sync_directory_for_segment_classes`) on the SAME allocation while the
/// shared reference was still lexically live. That is aliasing UB under
/// Stacked/Tree Borrows — `&T` and `&mut T` simultaneously live over one
/// allocation — regardless of the fact that the single-threaded owner
/// discipline makes it data-race-free. This accessor breaks the shared
/// reference's lifetime: it copies the one word-array the scan loop needs
/// into a local `[u64; WORDS_PER_CLASS]` and returns, so no reference to the
/// sidecar survives past this call. The scan loop iterates over the local
/// copy; any candidate found is re-validated (base non-null, kind, BinTable
/// head) by `validate_directory_candidate` before use, so a value read
/// slightly stale (relative to a mutation `validate_directory_candidate`
/// performs on a LATER candidate in the same word) is exactly the same
/// "candidate, not fact" contract the directory already documents (see the
/// R7-A3 module doc: every set bit is validated, never trusted blindly).
///
/// # Safety (caller contract — upheld by `AllocCore` owner-only discipline)
///
/// `p` must be non-null and was returned by [`reserve_directory_sidecar`].
/// The calling thread is the sole owner (`AllocCore` is single-writer), so a
/// racing writer is impossible; this function itself never overlaps its own
/// raw read with any live reference because it materialises none.
#[cfg(feature = "alloc-segment-directory")]
#[inline]
pub(crate) unsafe fn read_directory_class_words(
    p: *const crate::alloc_core::segment_directory::SegmentDirectory,
    node_bucket: usize,
    class_idx: usize,
) -> [u64; crate::alloc_core::segment_directory::WORDS_PER_CLASS] {
    debug_assert!(!p.is_null(), "read_directory_class_words: null pointer");
    // SAFETY: `p` is non-null, PAGE-aligned, valid for
    // `size_of::<SegmentDirectory>()` bytes, alive until the owning
    // `AllocCore` drops the sidecar's `AccountedSidecar` reservation token
    // (R2-12; the pointer must not be used after that). `addr_of!`
    // forms a raw pointer to the target field WITHOUT creating any
    // intermediate `&SegmentDirectory`, and `.read()` performs a single
    // valid, properly-aligned, non-overlapping copy of a plain-`u64` array
    // (no interior padding/niche/Drop concerns) into an owned local — no
    // reference to the sidecar escapes this function.
    unsafe {
        let field = core::ptr::addr_of!((*p).class_nonempty_by_node[node_bucket][class_idx]);
        field.read()
    }
}

/// R12-2: read the calling thread's own directory bucket index for `node_id`
/// OUT OF the directory sidecar BY VALUE, without materialising any
/// `&SegmentDirectory` reference — same discipline as
/// [`read_directory_class_words`] (task #252 / R12-1), for the same reason:
/// this is called once, right before the scan-order bucket list is built,
/// and must not leave any reference live across the loop that follows (which
/// calls `validate_directory_candidate`, itself able to materialise a
/// `&mut SegmentDirectory` on the same allocation to self-heal a stale bit).
///
/// Read-only lookup (does NOT register a new node): mirrors
/// `SegmentDirectory::node_bucket`, not `node_bucket_mut`. A node id with no
/// claimed bucket yet (e.g. the calling thread's own node has no segments
/// registered) resolves to the unknown bucket — correct, since there is
/// nothing local to prefer yet.
///
/// # Safety (caller contract — upheld by `AllocCore` owner-only discipline)
///
/// `p` must be non-null and was returned by [`reserve_directory_sidecar`].
/// The calling thread is the sole owner (`AllocCore` is single-writer), so a
/// racing writer is impossible; this function itself never overlaps its own
/// raw read with any live reference because it materialises none.
#[cfg(all(feature = "alloc-segment-directory", feature = "numa-aware"))]
#[inline]
pub(crate) unsafe fn read_directory_node_bucket(
    p: *const crate::alloc_core::segment_directory::SegmentDirectory,
    node_id: u32,
) -> usize {
    debug_assert!(!p.is_null(), "read_directory_node_bucket: null pointer");
    if node_id == crate::alloc_core::segment_header::NO_NODE_RAW {
        return crate::alloc_core::segment_directory::MAX_NODES;
    }
    // SAFETY: `p` is non-null, PAGE-aligned, valid for
    // `size_of::<SegmentDirectory>()` bytes, alive until the owning
    // `AllocCore` drops the sidecar's `AccountedSidecar` reservation token
    // (R2-12; the pointer must not be used after that). `addr_of!` forms
    // a raw pointer to the `node_ids` field WITHOUT creating any intermediate
    // `&SegmentDirectory`, and `.read()` performs a single valid, properly-
    // aligned, non-overlapping copy of a plain-`u32` array (no interior
    // padding/niche/Drop concerns) into an owned local — no reference to the
    // sidecar escapes this function.
    let node_ids: [u32; crate::alloc_core::segment_directory::MAX_NODES] = unsafe {
        let field = core::ptr::addr_of!((*p).node_ids);
        field.read()
    };
    node_ids
        .iter()
        .position(|&n| n == node_id)
        .unwrap_or(crate::alloc_core::segment_directory::MAX_NODES)
}
