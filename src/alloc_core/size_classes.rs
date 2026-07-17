//! [`SizeClasses`] — the size-class scheme (49 fine classes to a threshold,
//! then large/huge direct segments), the safe Cartographer's classifier.
//!
//! **Thin compat shim over the [`size_classes`](size_classes) crate.** The
//! const-built class table, the compile-time-derived O(1) `size→class` lookup,
//! and the alignment-divisibility classifier all live in the standalone
//! `size-classes` crate now (extracted verbatim — see `crates/size-classes`);
//! this module wires sefer's one concrete instantiation of that crate's
//! `const`-generic builder and re-exports the exact surface every in-tree
//! `super::size_classes::*` call site already uses, so nothing else changed.
//!
//! Two couplings the crate cannot see are cut here, sefer-side:
//! - `HUGE_THRESHOLD` is passed to the crate as [`Params::huge_threshold`] (a
//!   policy parameter of the scheme) — it is `super::os::SEGMENT`.
//! - the `MIN_BLOCK >= node::NODE_SIZE` invariant is a caller-side `const`
//!   assert (see `segment_header.rs`), since the crate cannot reference
//!   `node::NODE_SIZE`.
//!
//! ## Scheme
//!
//! - **Small classes (index `0..SMALL_CLASS_COUNT`):** 49 fine classes from
//!   `MIN_BLOCK` (16 B) up to `SMALL_MAX` (~253 KiB). 40 of the 49 classes form
//!   the geometric spacing (start at `MIN_BLOCK`, grow ~1.25×, rounded to a
//!   multiple of `MIN_BLOCK`); eight more (task B1) are explicit page-aligned
//!   classes — 512 … 16384 — and one more (task #145) is the exact 256 B class.
//!   All are merged into the sorted [`SIZE_CLASS_TABLE`] by the crate's
//!   `build_table`, so typical page-aligned requests (direct I/O buffers,
//!   `io_uring`, `#[repr(align(4096))]`) in the 512 B – 16 KiB range resolve to
//!   a small class instead of burning a whole ~4 MiB Large segment.
//! - **Medium classes (`#[cfg(feature = "medium-classes")]`, opt-in — NOT part
//!   of `production`):** six more EXACT classes (256 KiB … 1 MiB) merged into
//!   the SAME sorted table via the crate's `extras`, taking `SMALL_CLASS_COUNT`
//!   from 49 to 55 and `SMALL_MAX` to 1 MiB. See [`EXTRAS`].
//! - **Large:** allocations larger than `SMALL_MAX` get a dedicated
//!   whole-segment span. Alignment alone does not force Large: `align >
//!   MIN_BLOCK` is still served by a small class whenever one exists whose
//!   `block_size` is a multiple of `align` (the crate's divisibility slow path,
//!   #114/B1).
//! - **Huge:** allocations `>= HUGE_THRESHOLD` are also a dedicated segment.
//!
//! ## Invariants upheld
//!
//! - **M4 (alignment & size fidelity):** the chosen class's `block_size` is
//!   always `>= max(requested_size, requested_align)` AND a multiple of
//!   `MIN_BLOCK` (a power of two), so a `block_size >= requested_align` block is
//!   naturally aligned.
//! - The smallest class is `>= NODE_SIZE` (asserted in `segment_header.rs`).

use size_classes::{size2class_len, Params, SizeClasses as SizeClassesImpl};

/// The minimum block size and the fundamental small-class alignment. Must be a
/// power of two `>=` [`super::node::NODE_SIZE`] (the free-list node word — the
/// `const` assert lives in `segment_header.rs`).
pub(crate) const MIN_BLOCK: usize = 16;

/// `log2(MIN_BLOCK)` — the shift that turns a byte size into a `MIN_BLOCK`-unit
/// index. Derived from `MIN_BLOCK` at compile time so it cannot drift.
pub(crate) const MIN_BLOCK_SHIFT: u32 = MIN_BLOCK.trailing_zeros();

/// The alignment threshold below which a small allocation is served by the
/// **fast** O(1) path. Equal to `MIN_BLOCK`. This is *not* the ceiling on
/// alignments the small path can serve: `align > SMALL_ALIGN_MAX` (up to
/// `SMALL_MAX`) still resolves to a small class via the crate's bounded
/// divisibility-jump slow path (#114/B1); only `align > SMALL_MAX` falls
/// through to the dedicated-segment large/huge path.
pub(crate) const SMALL_ALIGN_MAX: usize = MIN_BLOCK;

/// The geometric growth ratio (mimalloc's 1.25× small spacing): each class
/// after the first is `round_up(prev * 5 / 4, MIN_BLOCK)`.
const GROWTH: (usize, usize) = (5, 4);

/// The number of classes contributed by the geometric progression (unchanged
/// from before task B1).
const GEO_COUNT: usize = 40;

/// The exact 256 B class (task #145) plus the 8 page-aligned classes (task B1,
/// 512 … 16384) — the sorted, geometric-disjoint `extras` merged into the
/// table. `#[cfg(feature = "medium-classes")]` additionally appends the six
/// exact medium classes (256 KiB … 1 MiB, R6-OPT-P0-3a) — all `>` the top of
/// the 49-entry table, so the combined slice stays strictly increasing and
/// disjoint from the geometric run.
#[cfg(not(feature = "medium-classes"))]
const EXTRAS: &[usize] = &[256, 512, 1024, 2048, 4096, 6144, 8192, 12288, 16384];
#[cfg(feature = "medium-classes")]
const EXTRAS: &[usize] = &[
    256,
    512,
    1024,
    2048,
    4096,
    6144,
    8192,
    12288,
    16384, // task #145 + B1
    256 * 1024,
    320 * 1024,
    384 * 1024,
    512 * 1024,
    768 * 1024,
    1024 * 1024, // MEDIUM_EXTRA (R6-OPT-P0-3a)
];

/// The total number of small-class table entries in THIS build: 49 without
/// `medium-classes`, 55 with it. Equal to `GEO_COUNT + EXTRAS.len()`.
pub(crate) const TABLE_LEN: usize = GEO_COUNT + EXTRAS.len();

/// The huge threshold: allocations of this size or larger are flagged "huge"
/// so future phases can apply distinct policy. Passed to the crate as the
/// scheme's [`Params::huge_threshold`]. `super::os::SEGMENT` — anything needing
/// a whole segment or more is "huge".
#[allow(dead_code)] // Phase 10 (M6 decommit policy) consumes this; kept for that.
pub(crate) const HUGE_THRESHOLD: usize = super::os::SEGMENT;

/// The [`Params`] describing sefer's concrete size-class scheme — the single
/// instantiation of the crate's `const`-generic builder.
const PARAMS: Params = Params {
    min_block: MIN_BLOCK,
    growth: GROWTH,
    geo_count: GEO_COUNT,
    extras: EXTRAS,
    huge_threshold: HUGE_THRESHOLD,
};

/// The table of fine small size classes, in strictly increasing order — built
/// at compile time by the crate's `build_table` from [`PARAMS`]. The **single
/// source of truth** for the small-class geometry; [`SIZE2CLASS`] is derived
/// from it.
pub(crate) const SIZE_CLASS_TABLE: [usize; TABLE_LEN] =
    size_classes::build_table::<TABLE_LEN>(&PARAMS);

/// Number of small size classes (length of [`SIZE_CLASS_TABLE`]).
pub(crate) const SMALL_CLASS_COUNT: usize = SIZE_CLASS_TABLE.len();

/// The largest small size class. Allocations `<=` this (with alignment `<=`
/// [`SMALL_ALIGN_MAX`]) are served by the small free-list path.
pub(crate) const SMALL_MAX: usize = SIZE_CLASS_TABLE[TABLE_LEN - 1];

/// The `SIZE2CLASS` array length: one `u8` per `MIN_BLOCK` bucket up to and
/// including `SMALL_MAX`.
const S2C_LEN: usize = size2class_len(SMALL_MAX, MIN_BLOCK);

/// The O(1) size→class lookup table, **derived at compile time from
/// [`SIZE_CLASS_TABLE`]** by the crate's `build_size2class`. `SIZE2CLASS[k]` is
/// the index of the smallest class whose `block_size >= (k + 1) * MIN_BLOCK`.
///
/// `static`, not `const`: a single fixed-address item shared by every
/// reference, avoiding the `.rodata` duplication `clippy::large_const_arrays`
/// flags at the `medium-classes` ~64 KiB size.
pub(crate) static SIZE2CLASS: [u8; S2C_LEN] =
    size_classes::build_size2class::<TABLE_LEN, S2C_LEN>(&SIZE_CLASS_TABLE, MIN_BLOCK);

/// Sefer's concrete size-class scheme — one const instantiation of the crate's
/// `const`-generic [`SizeClassesImpl`]. Drives every classification query.
const SC: SizeClassesImpl<TABLE_LEN, S2C_LEN> = SizeClassesImpl::build(PARAMS);

/// A classifier over [`SIZE_CLASS_TABLE`]. A zero-sized forwarder to the crate
/// scheme [`SC`] — kept so the in-tree `SizeClasses::class_for(..)` /
/// `::block_size(..)` / `::is_huge(..)` call sites compile unchanged.
///
/// All methods are `const` pure arithmetic — no allocations, no panics on the
/// lookup path.
pub(crate) struct SizeClasses;

impl SizeClasses {
    /// Resolve `(size, align)` to a small-class index, or `None` for large.
    ///
    /// A small class fits iff its `block_size >= max(size, align)` AND
    /// `block_size % align == 0`. Returns the index of the smallest such class,
    /// or `None` (→ Large path) otherwise. See the crate's
    /// `SizeClasses::class_for` for the fast/slow (#114/B1 divisibility-jump)
    /// path detail.
    ///
    /// `size` here is already clamped to `>= MIN_BLOCK` by the only caller
    /// ([`super::alloc_core::AllocCore::alloc`]).
    #[must_use]
    pub(crate) const fn class_for(size: usize, align: usize) -> Option<usize> {
        SC.class_for(size, align)
    }

    /// The block size of class `idx`. Panics (debug) if out of range — the
    /// Cartographer only ever passes indices returned by `class_for`.
    #[must_use]
    pub(crate) const fn block_size(idx: usize) -> usize {
        SC.block_size(idx)
    }

    /// Whether a `size` request is "huge" (gets the dedicated-segment huge
    /// policy in future phases). For Phase 8 this is purely informational.
    #[must_use]
    #[allow(dead_code)] // Phase 10 (M6) consumes this; kept for that.
    pub(crate) const fn is_huge(size: usize) -> bool {
        SC.is_huge(size)
    }
}

/// The kind of an allocation, decided by the Cartographer. Determines which
/// substrate path serves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AllocKind {
    /// A small allocation served by the per-segment free-list path. Carries
    /// the resolved size-class index.
    Small { class_idx: usize },
    /// A large or huge allocation served by a dedicated whole-segment span.
    Large,
}
