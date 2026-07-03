//! [`large_layout_consistent`] — 0.3.0 (task #138): post-reuse defensive
//! mitigation for the cross-thread Large-free routing paths.
//!
//! ## The gap this narrows (NOT closes)
//!
//! A1's deferred-free stack (`push`/`drain` in this module) has a
//! fundamental, documented limit: a double-free of a Large block whose
//! segment has ALREADY been reclaimed and reused for a new allocation is, by
//! address alone, indistinguishable from a legitimate free of the NEW
//! occupant — the stale free arrives after `reclaim_large_segment` has
//! handed the same `base` to a fresh caller, and the segment header now
//! describes that new allocation (inherited `kind == Large`, a fresh
//! `large_size`/`owner_thread_free` stamp). This is fundamentally unsolvable
//! from inside `dealloc` alone (the allocator has no way to know the caller's
//! `Layout` is stale) — it is the same class of limit every allocator has for
//! a use-after-free-shaped double-free. The PRE-reuse window (before any
//! reclaim/reuse happens) is airtight — see `push_large_deferred_free`'s
//! double-push guard (CAS on `next_abandoned` from `ABANDONED_TAIL`) and the
//! `magic == 0` check on release; this module does not touch that guarantee.
//!
//! ## The mitigation
//!
//! Before treating a cross-thread free as a Large-segment free (queuing it
//! onto the owner's deferred-free stack), check that the CALLER's `Layout`
//! is consistent with what the segment header currently claims about its
//! occupant: `layout.size() == SegmentHeader::large_size_at(base)`.
//! `large_size` is the EXACT requested size recorded at allocation time (for
//! both a fresh reservation and a large-cache-hit reuse — see
//! `SegmentHeader::large`'s doc comment), and `GlobalAlloc`'s contract
//! requires the caller to pass back the identical `Layout` it allocated
//! with, so for a LEGITIMATE free this is an exact match, not a heuristic
//! range check.
//!
//! If the segment has been reclaimed and reused since the stale pointer was
//! captured, the NEW occupant's `large_size` will, in the overwhelming
//! majority of cases, differ from the stale caller's `layout.size()` (a
//! reuse that happens to request the bit-identical size is the one residual
//! case this mitigation cannot catch — see the module doc above; that
//! residual is accepted, not hidden).
//!
//! On a mismatch: drop the free as a no-op (the M2 degrade-safely
//! discipline used everywhere else in this crate for a suspicious
//! cross-thread free) instead of queuing it for reclaim. This trades a
//! (already-UB, already-permanently-leaked-until-this-mitigation-existed)
//! stale double-free for a safe no-op rather than corrupting the reused
//! segment's deferred-free stack or double-queuing it for reclaim.
use crate::alloc_core::segment_header::SegmentHeader;

/// Returns `true` if `layout`'s size matches the CURRENT occupant's
/// `large_size` as recorded in the segment header at `base`. `base` MUST
/// already be confirmed live (`magic_at(base) == SEGMENT_MAGIC`) and
/// `SegmentKind::Large` by the caller — this function only adds the
/// size-consistency check on top of that.
///
/// `layout_size` is the caller's RAW `layout.size()`. The alloc path clamps
/// every request to `MIN_BLOCK` before it reaches `alloc_large`
/// (`AllocCore::alloc` does `layout.size().max(MIN_BLOCK)`), so the header's
/// `large_size` is the CLAMPED size. The comparison must therefore clamp the
/// caller's size the same way — otherwise a legitimate cross-thread free of a
/// tiny-but-huge-aligned block (`size < MIN_BLOCK`, `align > SMALL_MAX` — a
/// valid `Layout` via the raw alloc API) would compare `raw != clamped`,
/// be dropped as "inconsistent", and permanently leak the segment + its
/// `SegmentTable` slot (the #114/#130 leak-to-abort class). Found by the
/// full 0.3.0 review; the clamp lives HERE (the single shared point) so both
/// faces' call sites stay symmetric with the alloc path by construction.
#[cfg(feature = "alloc-xthread")]
#[inline(always)]
pub(crate) fn large_layout_consistent(base: *mut u8, layout_size: usize) -> bool {
    let clamped = layout_size.max(crate::alloc_core::size_classes::MIN_BLOCK);
    SegmentHeader::large_size_at(base) == clamped
}
