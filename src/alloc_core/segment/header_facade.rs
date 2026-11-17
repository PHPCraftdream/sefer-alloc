//! Private compatibility adapters for root allocator code during the segment
//! crate boundary transition. Callers must pass roots established by the
//! allocator's table/route ownership protocol, never arbitrary user pointers.

use sefer_segment::segment::segment_header::{SegmentHeader, SegmentKind, SegmentMeta};
use sefer_segment::support::root_compat::traits::*;

/// Read the kind from a root already proven live and allocator-owned by caller.
#[inline(always)]
#[allow(unsafe_code)] // P2.1 tier-2 facade: forwards the segment-crate unsafe contracts.
pub(crate) fn kind_at(base: *mut u8) -> SegmentKind {
    // SAFETY: this root-only adapter is used after root callers resolve a
    // canonical registered segment under the applicable allocator lease.
    unsafe { SegmentHeader::kind_at(base) }
}

/// Copy a header from a root already proven live and allocator-owned by caller.
#[inline(always)]
#[allow(unsafe_code)] // P2.1 tier-2 facade: forwards the segment-crate unsafe contracts.
pub(crate) fn read_at(base: *mut u8) -> SegmentHeader {
    // SAFETY: this root-only adapter is used after root callers resolve a
    // canonical registered segment and retain its mapping through this read.
    unsafe { SegmentHeader::read_at(base) }
}

/// Build a metadata view for a canonical root already proven live by caller.
#[inline(always)]
#[allow(unsafe_code)] // P2.1 tier-2 facade: forwards the segment-crate unsafe contracts.
pub(crate) fn meta(base: *mut u8) -> SegmentMeta {
    // SAFETY: callers obtain this root from a live table/route entry and keep
    // the mapping live for every use of the resulting metadata view.
    unsafe { SegmentMeta::new(base) }
}

/// Initialize bins in a freshly reserved, exclusively owned segment.
#[inline(always)]
#[allow(unsafe_code)] // P2.1 tier-2 facade: forwards the segment-crate unsafe contracts.
pub(crate) fn init_bins(heads: *mut u32) {
    // SAFETY: bootstrap/reservation callers derive this address from a fresh
    // live segment and prove the full BinTable footprint is writable/exclusive.
    unsafe { sefer_segment::segment::segment_header::BinTable::init_in_place(heads) }
}

/// Write the initial header into a fresh segment before it is published.
#[inline(always)]
pub(crate) fn write_header(meta: &mut SegmentMeta, header: SegmentHeader) {
    // Fresh-segment exclusivity and writable header extent are guaranteed by
    // the caller; `compat_write_header` is a safe forwarding method.
    meta.compat_write_header(header)
}
