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

    /// `log2(MIN_BLOCK)` — the shift turning a byte size into a `MIN_BLOCK`-unit
    /// index (used by the O(1) size-class lookup). Exposed so tests can re-derive
    /// the lookup arithmetic independently of the crate internals.
    pub const MIN_BLOCK_SHIFT: u32 = super::size_classes::MIN_BLOCK_SHIFT;

    /// The alignment threshold below which a small allocation is served by
    /// the **fast** O(1) small-class lookup. This is *not* the ceiling on
    /// alignments the small path can serve: an alignment above this value
    /// (and up to [`SMALL_MAX`](Self::SMALL_MAX)) still resolves to a small
    /// class via a bounded divisibility-jump slow path (see
    /// [`class_for`](Self::class_for), #114/B1) — whenever some class's
    /// block size is a multiple of that alignment. When none is (or when
    /// `size` pushes `max(size, align)` past the last such class), the
    /// request falls through to the dedicated-segment large path — as does
    /// any alignment greater than `SMALL_MAX`.
    pub const SMALL_ALIGN_MAX: usize = super::size_classes::SMALL_ALIGN_MAX;

    /// The largest small size class. Allocations larger than this go through
    /// the large path; so does an allocation whose alignment exceeds this
    /// value, since no small class's block size can then be a multiple of
    /// it (see [`class_for`](Self::class_for)).
    pub const SMALL_MAX: usize = super::size_classes::SMALL_MAX;

    /// The fine small size-class table — the single source of truth for the
    /// small-class geometry. `SIZE2CLASS` is derived from it at compile time.
    /// Exposed so tests can re-run the linear scan independently and assert the
    /// O(1) lookup never drifts from it.
    ///
    /// Exposed as a slice (not a fixed-size array) so that tuning the number
    /// of small classes stays semver-compatible: the array length grew
    /// silently from 40 to 49 in 0.3.0, which — had this constant been public
    /// at the time — would have been a breaking type change (`[usize; 40]` →
    /// `[usize; 49]`). A slice view has no length in its type, so future
    /// re-tuning of the class count is not a breaking change.
    pub const SIZE_CLASS_TABLE: &'static [usize] = &super::size_classes::SIZE_CLASS_TABLE;

    /// The O(1) size→class lookup table (compile-time-derived from
    /// [`SIZE_CLASS_TABLE`](Self::SIZE_CLASS_TABLE)). `SIZE2CLASS[k]` is the
    /// smallest class index whose block size is `>= (k + 1) * MIN_BLOCK`
    /// bytes -- except the last entry, a harmless sentinel `class_for` never
    /// actually queries (see the crate's `build_size2class` doc for why).
    ///
    /// Exposed as a slice for the same semver reason as
    /// [`SIZE_CLASS_TABLE`](Self::SIZE_CLASS_TABLE): its length is derived
    /// from `SMALL_MAX`/`MIN_BLOCK` and would otherwise bake a fixed array
    /// length into the public type.
    pub const SIZE2CLASS: &'static [u8] = &super::size_classes::SIZE2CLASS;

    /// Resolve `(size, align)` to a small-class index, or `None` for the large
    /// path. The crate's own well-defined domain is `size >= 1` given
    /// `align >= 1` (`need = max(size, align) >= 1`); `size >= MIN_BLOCK` is
    /// the public allocator entry points' own clamping CONVENTION, not a
    /// requirement `class_for` itself imposes -- a test driving this forwarder
    /// directly with `size < MIN_BLOCK` (e.g. to force a specific slow-path
    /// seed) is in-contract. This is the O(1) lookup
    /// (`SIZE2CLASS[(size-1) >> MIN_BLOCK_SHIFT]`) — exposed so tests can drive
    /// it directly and compare against an independent linear-scan reference.
    ///
    /// `align` must be a power of two: the underlying `debug_assert!` panics
    /// whenever `cfg(debug_assertions)` is on; with `debug_assertions` off
    /// AND `overflow-checks` off (the default release profile) the result
    /// for a violation is unspecified, not a panic; but with
    /// `debug_assertions` off and `overflow-checks` ON (a separate Cargo
    /// knob a caller can enable in release), `size == align == 0` panics on
    /// the underlying `need - 1` subtraction -- see the crate's own
    /// `SizeClasses::class_for` doc for the full three-profile breakdown.
    /// The parenthetical formula above is the fast path only — `align >
    /// min_block` takes the divisibility-jump slow path (see the crate's
    /// `SizeClasses::class_for`).
    #[must_use]
    pub const fn class_for(size: usize, align: usize) -> Option<usize> {
        super::size_classes::SizeClasses::class_for(size, align)
    }

    /// The checked twin of [`class_for`](Self::class_for): rejects a
    /// non-power-of-two `align` (including `0`) with
    /// `Err(size_classes::InvalidAlign(align))` before any arithmetic runs,
    /// instead of assuming `align` is valid. Never panics, for any `(size,
    /// align)` pair. The crate root re-exports this error type as
    /// `sefer_alloc::InvalidAlign`, so a caller need not add `size-classes`
    /// as their own direct dependency just to name/match it.
    ///
    /// Every in-tree allocator call site uses [`class_for`](Self::class_for)
    /// directly, because their alignments already come from a `Layout` and so
    /// are trusted by construction. This exists for `SegmentLayout`'s OTHER
    /// audience: `SegmentLayout` is re-exported from the crate root
    /// (`sefer_alloc::SegmentLayout`) as a public introspection surface, and
    /// an external caller of `class_for` can pass an arbitrary `usize` --
    /// size-classes round-4 prepublish review P2-1 flagged that the crate's
    /// most discoverable public forwarder had no equivalent to the
    /// `size-classes` crate's own recommended-by-default `try_class_for`
    /// (see that crate's `SizeClasses::try_class_for` doc). Use this one
    /// unless `align` is already known-valid by construction.
    #[must_use = "this returns a Result, not just a class index -- the Err case must be handled"]
    pub const fn try_class_for(
        size: usize,
        align: usize,
    ) -> Result<Option<usize>, size_classes::InvalidAlign> {
        super::size_classes::SizeClasses::try_class_for(size, align)
    }

    /// The end of the small-segment metadata region (page-aligned past the
    /// last metadata structure). Payload carving begins at this offset.
    /// Exposed so tests can reason about the metadata/payload boundary without
    /// depending on the private `segment_header::Layout` module.
    ///
    /// R8-6 (task #219): this is the **TIGHT** metadata boundary — aligned
    /// only to `PAGE` (4 KiB). The decommit/recommit-safe boundary is
    /// [`small_decommit_start`](Self::small_decommit_start) (a runtime value,
    /// real-OS-page-aligned). See that method's doc for the full rationale.
    pub const SMALL_META_END: usize = super::segment_header::Layout::small_meta_end();

    /// The end of the primordial segment's metadata region (page-aligned past
    /// the free-list top counter). The primordial segment additionally carries
    /// the registry array + hash table + free-list stack past the small-segment
    /// metadata, so this is `>=` [`SMALL_META_END`](Self::SMALL_META_END).
    ///
    /// R8-6 (task #219): like [`SMALL_META_END`](Self::SMALL_META_END), this is
    /// the **TIGHT** metadata boundary (4 KiB aligned); the
    /// decommit/recommit-safe boundary is
    /// [`primordial_decommit_start`](Self::primordial_decommit_start).
    pub const PRIMORDIAL_META_END: usize = super::segment_header::Layout::primordial_meta_end();

    /// R8-6 (task #219): the real, runtime-determined decommit/recommit safe
    /// boundary for a small segment — [`SMALL_META_END`](Self::SMALL_META_END)
    /// rounded UP to the actual OS page size (`aligned_vmem::page_size()`).
    /// Called only by the actual `os::decommit_pages`/`os::recommit_pages`
    /// syscall sites. Always `>= SMALL_META_END`; on a 4 KiB-page system this
    /// returns EXACTLY `SMALL_META_END` (no waste); on a 16/64 KiB-page system
    /// it returns the real-page-aligned value. Test-only public surface
    /// (`#[doc(hidden)]` convention — see `lib.rs`); not stable public API.
    #[doc(hidden)]
    #[must_use]
    pub fn small_decommit_start() -> usize {
        super::segment_header::Layout::small_decommit_start()
    }

    /// R8-6 (task #219): same as [`small_decommit_start`](Self::small_decommit_start)
    /// for the primordial segment. Always `>= PRIMORDIAL_META_END`; on a 4
    /// KiB-page system returns EXACTLY `PRIMORDIAL_META_END`. The primordial
    /// segment is never decommitted in the current codebase, so this has no
    /// live production call site — it exists for symmetry and the alignment
    /// sanity tests. Test-only public surface; not stable public API.
    #[doc(hidden)]
    #[must_use]
    pub fn primordial_decommit_start() -> usize {
        super::segment_header::Layout::primordial_decommit_start()
    }

    /// Task #1074 test-only forwarder: the ORDINARY small segment's lazy
    /// `initial_commit` for a HYPOTHETICAL page size (tests table-drive
    /// 4/16/64 KiB; the real call sites pass `aligned_vmem::page_size()`).
    /// Exposes `Layout::lazy_initial_commit(SMALL_META_END, page_size)`.
    /// Test-only public surface (`#[doc(hidden)]` convention — see
    /// `lib.rs`); not stable public API.
    #[doc(hidden)]
    #[must_use]
    #[cfg(any(
        feature = "primordial-lazy-commit",
        feature = "small-segment-lazy-commit"
    ))]
    pub fn small_lazy_initial_commit(page_size: usize) -> usize {
        super::segment_header::Layout::lazy_initial_commit(Self::SMALL_META_END, page_size)
    }

    /// Task #1074 test-only forwarder: the PRIMORDIAL segment's lazy
    /// `initial_commit` for a HYPOTHETICAL page size — exposes
    /// `Layout::lazy_initial_commit(PRIMORDIAL_META_END, page_size)`. See
    /// [`small_lazy_initial_commit`](Self::small_lazy_initial_commit).
    /// Test-only public surface; not stable public API.
    #[doc(hidden)]
    #[must_use]
    #[cfg(any(
        feature = "primordial-lazy-commit",
        feature = "small-segment-lazy-commit"
    ))]
    pub fn primordial_lazy_initial_commit(page_size: usize) -> usize {
        super::segment_header::Layout::lazy_initial_commit(Self::PRIMORDIAL_META_END, page_size)
    }

    /// Convert an address to the SEGMENT-aligned base of the segment that
    /// contains it (the O(1) owner-lookup primitive).
    #[must_use]
    pub const fn segment_base_of(addr: usize) -> usize {
        super::os::segment_base_of(addr)
    }
}
