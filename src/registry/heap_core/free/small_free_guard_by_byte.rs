//! G1-K2: the own-thread scalar free guard that takes the size class from the
//! block's state byte instead of the caller's `Layout` (non-`hardened` only).
//!
//! The state byte is the owner's record of the issued class (written at every
//! issue site, `set_state` keeps the class field), so it is the one source of
//! truth; a `Layout` naming another class no longer turns a free into a silent
//! leak — the block goes back to its true class, the same way a foreign free
//! already does (the producer publishes only the offset; reclaim uses the
//! class code the owner stored at issue). `hardened` keeps the exact
//! Layout-vs-class check of [`small_free_guard`](super::dealloc_own_base). The
//! batch free keeps it too: one class per batch.
//!
//! Order and strength of the remaining checks are unchanged: physical kind
//! first (a Large segment holds payload where a Small one holds state bytes),
//! payload lower bound, 16 B start alignment, a LIVE byte whose class is in
//! range, then the bump bound.

use super::dealloc_own_base::SmallFreeGuard;
use crate::alloc_core::segment_header::{
    Layout as SegmentLayout, SegmentHeader, SegmentKind, SegmentMeta,
};
use crate::alloc_core::size_classes::{MIN_BLOCK, SMALL_CLASS_COUNT};

/// `Ok((off, class))` for a LIVE start-of-block of the owner's Small or
/// Primordial segment; `Err(guard)` (never `Accept`) for every reject and for
/// the Large route.
#[inline(always)]
pub(super) fn small_free_guard_by_byte(
    base: *mut u8,
    ptr: *mut u8,
) -> Result<(u32, usize), SmallFreeGuard> {
    let payload_start = match SegmentHeader::kind_at(base) {
        SegmentKind::Small => SegmentLayout::small_meta_end(),
        SegmentKind::Primordial => SegmentLayout::primordial_meta_end(),
        SegmentKind::Large => {
            // SEC-03: only this already-selected Large arm reads payload geometry.
            if crate::alloc_core::AllocCore::reject_large_interior_free(base, ptr) {
                return Err(SmallFreeGuard::RejectNoOp);
            }
            return Err(SmallFreeGuard::RouteToLargeFree);
        }
        SegmentKind::Unknown => return Err(SmallFreeGuard::RejectNoOp),
    };
    let off = (ptr as usize - base as usize) as u32;
    if (off as usize) < payload_start {
        return Err(SmallFreeGuard::RejectNoOp);
    }
    // F1: an interior offset `p+1..p+15` indexes the start granule's byte.
    if (off & (MIN_BLOCK as u32 - 1)) != 0 {
        return Err(SmallFreeGuard::RejectNoOpSilent);
    }
    let meta = SegmentMeta::new(base);
    let byte = meta.state_byte().read(off);
    // LIVE is state 0, so a LIVE byte is exactly `class + 1`; one unsigned
    // compare rejects zero (never issued / interior), MAG, FREE and any class
    // field outside the table.
    let class = (byte as usize).wrapping_sub(1);
    if class >= SMALL_CLASS_COUNT {
        if byte == 0 && (off as usize) < meta.bump_of() {
            return Err(SmallFreeGuard::RejectNoOpSilent);
        }
        return Err(SmallFreeGuard::RejectNoOp);
    }
    if (off as usize) >= meta.bump_of() {
        return Err(SmallFreeGuard::RejectNoOp);
    }
    Ok((off, class))
}
