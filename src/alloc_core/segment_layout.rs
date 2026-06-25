//! [`SegmentLayout`] — read-only access to the segment substrate's geometry
//! constants. The single public type re-exported from the `alloc_core` module
//! alongside [`AllocCore`].
//!
//! Pure compile-time constants — no state, no `unsafe`. Exposed so callers
//! (and tests) can reason about segment boundaries, page counts, and the
//! alignment mask without depending on private module paths.

/// Read-only access to the segment substrate's geometry.
///
/// All fields are `const`; this struct is a name-space for the constants. It
/// is `Copy + Clone + Default` (a zero-sized marker) so it can be passed
/// around trivially, but the constants are also accessible as associated
/// constants (`SegmentLayout::SEGMENT`) without an instance.
#[derive(Debug, Copy, Clone, Default)]
pub struct SegmentLayout;

impl SegmentLayout {
    /// The segment size and alignment, in bytes (4 MiB). Every segment handed
    /// up by the OS aperture is aligned to a multiple of this value.
    pub const SEGMENT: usize = super::os::SEGMENT;

    /// The page granularity used by the per-segment `PageMap` (4 KiB).
    pub const PAGE: usize = super::os::PAGE;

    /// The number of pages in one segment (`SEGMENT / PAGE`).
    pub const PAGES_PER_SEGMENT: usize = super::segment_header::PAGES_PER_SEGMENT;

    /// The minimum block size and fundamental small-class alignment (16 B).
    pub const MIN_BLOCK: usize = super::size_classes::MIN_BLOCK;

    /// The maximum alignment a small allocation may request and still be
    /// served by the small free-list path. Larger alignments go through the
    /// dedicated-segment (large) path.
    pub const SMALL_ALIGN_MAX: usize = super::size_classes::SMALL_ALIGN_MAX;

    /// The largest small size class. Allocations larger than this (or with
    /// alignment > `SMALL_ALIGN_MAX`) go through the large path.
    pub const SMALL_MAX: usize = super::size_classes::SMALL_MAX;

    /// Convert an address to the SEGMENT-aligned base of the segment that
    /// contains it (the O(1) owner-lookup primitive).
    #[must_use]
    pub const fn segment_base_of(addr: usize) -> usize {
        super::os::segment_base_of(addr)
    }
}
