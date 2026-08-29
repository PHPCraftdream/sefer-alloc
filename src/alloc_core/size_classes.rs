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
//!   multiple of `MIN_BLOCK`); eight more (task B1) are explicit device-block-
//!   and page-friendly classes — 512, 1024, 2048 (sector/sub-page sizes),
//!   4096, 8192, 12288, 16384 (exact page multiples), plus 6144 (density
//!   fill, 1.5 pages) — and one more (task #145) is the exact 256 B class.
//!   All are merged into the sorted [`SIZE_CLASS_TABLE`] by the crate's
//!   `build_table`, so typical page-aligned requests (direct I/O buffers,
//!   `io_uring`, `#[repr(align(4096))]`) in the 512 B – 16 KiB range resolve to
//!   a small class instead of burning a whole ~4 MiB Large segment.
//! - **Medium classes (`#[cfg(feature = "medium-classes")]`, opt-in — NOT part
//!   of `production`):** six more EXACT classes (256 KiB … 1 MiB) merged into
//!   the SAME sorted table via the crate's `extras`, taking `SMALL_CLASS_COUNT`
//!   from 49 to 55 and `SMALL_MAX` to 1 MiB. See [`EXTRAS`].
//! - **Wide medium classes (`#[cfg(feature = "medium-classes-wide")]`, opt-in —
//!   NOT part of `production`, requires `medium-classes`):** three MORE exact
//!   classes (1.25 / 1.5 / 1.75 MiB) appended on top of the six-class medium
//!   list, taking `SMALL_CLASS_COUNT` 55 → 58 and `SMALL_MAX` 1 MiB → 1.75 MiB.
//!   The R9-4 prototype — partially closes the relocated cliff at the
//!   `medium-classes` ceiling (R8-9 §5 K7). 2 MiB itself is OUT OF SCOPE (fits
//!   only 1× per 4 MiB segment, so a class for it would carry no density win;
//!   needs a larger medium-arena / page-run layer). See [`EXTRAS`].
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
//!   `MIN_BLOCK` (a power of two) -- a STRIDE guarantee (see the crate's own
//!   `SizeClasses::class_for` doc); the crate itself has no notion of an
//!   address and cannot check the base-alignment precondition its own doc
//!   requires. Sefer satisfies it via two facts together, not the segment
//!   reservation alone: every small segment's base is `SEGMENT`-aligned
//!   (`super::os::SEGMENT`, 4 MiB -- see `os.rs`), AND `carve_block` places
//!   every block at that base plus an ABSOLUTE multiple of `block_size`
//!   (`align_up(bump, block_size)` in segment-relative coordinates --
//!   `alloc_core_small.rs`), not merely somewhere past the metadata prefix.
//!   Every `align` this scheme ever serves divides both `block_size` (the
//!   crate's own stride guarantee) and `SEGMENT`: `align > SMALL_MAX` is
//!   rejected (the crate's own large-path fallback), and every power of two
//!   `<= SMALL_MAX < SEGMENT` (4 MiB) trivially divides `SEGMENT` -- so
//!   `block_addr = base + m*block_size` is `align`-aligned. The segment base's own alignment
//!   would NOT be sufficient by itself: the metadata prefix before it is
//!   only `PAGE`-aligned (`segment_header_layout.rs`), so relying on the
//!   base alone -- without `carve_block`'s own alignment to `block_size` --
//!   would silently misalign every `align = 8192/16384` request in a
//!   default build.
//! - The smallest class is `>= NODE_SIZE` (asserted in `segment_header.rs`).

use size_classes::{size2class_len, InvalidAlign, Params, SizeClasses as SizeClassesImpl};

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
/// divisibility-jump slow path (#114/B1) whenever some class's block size is
/// a multiple of that alignment. When none is (or when `size` pushes
/// `max(size, align)` past the last such class), the request falls through
/// to the dedicated-segment large/huge path — as does any `align >
/// SMALL_MAX`.
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
/// disjoint from the geometric run. `#[cfg(feature = "medium-classes-wide")]`
/// (R9-4) further appends three exact wide-medium classes
/// (1.25 / 1.5 / 1.75 MiB) on top of the six-class medium list, in the same
/// strictly-increasing append-not-merge shape.
#[cfg(not(feature = "medium-classes"))]
const EXTRAS: &[usize] = &[256, 512, 1024, 2048, 4096, 6144, 8192, 12288, 16384];
#[cfg(all(feature = "medium-classes", not(feature = "medium-classes-wide")))]
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
#[cfg(feature = "medium-classes-wide")]
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
    1280 * 1024,
    1536 * 1024,
    1792 * 1024, // WIDE_MEDIUM_EXTRA (R9-4: 1.25 / 1.5 / 1.75 MiB)
];

/// The total number of small-class table entries in THIS build: 49 without
/// `medium-classes`, 55 with `medium-classes` (but not `medium-classes-wide`),
/// 58 with `medium-classes-wide`. Equal to `GEO_COUNT + EXTRAS.len()`.
pub(crate) const TABLE_LEN: usize = GEO_COUNT + EXTRAS.len();

/// The huge threshold: allocations of this size or larger are flagged "huge"
/// so future phases can apply distinct policy. Passed to the crate as the
/// scheme's [`Params::huge_threshold`]. `super::os::SEGMENT` — anything needing
/// a whole segment or more is "huge".
pub(crate) const HUGE_THRESHOLD: usize = super::os::SEGMENT;

/// The [`Params`] describing sefer's concrete size-class scheme — the single
/// instantiation of the crate's `const`-generic builder.
const PARAMS: Params = Params::new(MIN_BLOCK, GROWTH, GEO_COUNT, EXTRAS, HUGE_THRESHOLD);

/// Compile-time drift guard: [`SMALL_ALIGN_MAX`] is defined independently as
/// `MIN_BLOCK` above, not read off the built scheme (a `const` item cannot
/// read through a reference to the `SC` `static` -- so this re-runs `build`
/// fresh instead). Today the crate's own `build` hardcodes `small_align_max =
/// params.min_block`, so the two cannot actually drift; if the crate ever
/// grows the `small_align_max` knob its own README already anticipates, this
/// fails to compile instead of the constant silently going stale (fh
/// publication audit P4-5).
///
/// Uses a minimal 2-class probe scheme (`MIN_BLOCK`/`GROWTH`, no `extras`)
/// rather than sefer's full `PARAMS`/`TABLE_LEN`/`S2C_LEN` -- the invariant
/// this proves is a property of the crate's `build` (`small_align_max` is
/// always `params.min_block`), not of sefer's specific table, so a 2-class
/// probe proves the identical thing at ~3 const-eval buckets instead of
/// const-evaluating the real `S2C_LEN`-bucket LUT (16173/65537/114689
/// depending on feature) a second time just to read one `u32` back out.
/// `PROBE_L` is derived from the probe's own built table rather than
/// hardcoded as `MIN_BLOCK * 2` -- that shortcut only holds for
/// `GROWTH == (5, 4)` (`round_up(ceil(16*5/4), 16) == 32 == 2 * MIN_BLOCK`);
/// deriving it keeps the guard correct if `GROWTH` ever changes, instead of
/// panicking with a message that names neither `SMALL_ALIGN_MAX` nor this
/// guard (size-classes round-5 prepublish review P4-2).
const _: () = {
    const PROBE: Params = Params::new(MIN_BLOCK, GROWTH, 2, &[], HUGE_THRESHOLD);
    const PROBE_TABLE: [usize; 2] = size_classes::build_table::<2>(PROBE);
    const PROBE_L: usize = size2class_len(PROBE_TABLE[1], MIN_BLOCK);
    assert!(
        SMALL_ALIGN_MAX == SizeClassesImpl::<2, PROBE_L>::build(PROBE).small_align_max()
    );
};

/// The table of fine small size classes, in strictly increasing order — built
/// at compile time by the crate's `build_table` from [`PARAMS`]. The **single
/// source of truth** for the small-class geometry; [`SIZE2CLASS`] is derived
/// from it.
pub(crate) const SIZE_CLASS_TABLE: [usize; TABLE_LEN] =
    size_classes::build_table::<TABLE_LEN>(PARAMS);

/// Number of small size classes (length of [`SIZE_CLASS_TABLE`]).
pub(crate) const SMALL_CLASS_COUNT: usize = SIZE_CLASS_TABLE.len();

/// The largest small size class. Allocations `<=` this (with alignment `<=`
/// [`SMALL_ALIGN_MAX`]) are served by the small free-list path.
pub(crate) const SMALL_MAX: usize = SIZE_CLASS_TABLE[TABLE_LEN - 1];

/// The `SIZE2CLASS` array length: one `u8` per `MIN_BLOCK` bucket up to and
/// including `SMALL_MAX`.
const S2C_LEN: usize = size2class_len(SMALL_MAX, MIN_BLOCK);

/// Sefer's concrete size-class scheme — one const instantiation of the crate's
/// `const`-generic [`SizeClassesImpl`]. Drives every classification query.
///
/// `static`, not `const`, for the same reason as [`SIZE2CLASS`] below:
/// `SizeClassesImpl` embeds its own copy of the size2class table, and a
/// `const` this size re-materializes at every use site instead of living at
/// one fixed address. `SizeClassesImpl<N, L>` implements `Debug, Clone` —
/// plain data, no interior mutability — so `static` is sound.
static SC: SizeClassesImpl<TABLE_LEN, S2C_LEN> = SizeClassesImpl::build(PARAMS);

/// The O(1) size→class lookup table, **derived at compile time from
/// [`SIZE_CLASS_TABLE`]** by the crate's `build_size2class`. `SIZE2CLASS[k]`
/// is the index of the smallest class whose `block_size >= (k + 1) *
/// MIN_BLOCK` -- except the last entry, a harmless sentinel `class_for`
/// never actually queries (see the crate's `build_size2class` doc for why).
///
/// `static`, not `const`: a single fixed-address item shared by every
/// reference, avoiding the `.rodata` duplication a `const` this size would
/// cause at the `medium-classes` ~64 KiB size.
///
/// Task #1518: this used to be built by its OWN separate call to
/// `size_classes::build_size2class`, re-running the exact same derivation
/// [`SC`] already performs internally to populate its private `size2class`
/// field — two independent const-evaluations of the same table. This now
/// copies `SC`'s own table (`*SC.size2class()`, a `[u8; S2C_LEN]` dereference
/// of `SizeClasses::size2class`'s `&[u8; L]` accessor — sound because
/// `[u8; S2C_LEN]: Copy`) instead of rebuilding it, removing that particular
/// duplication. The `SMALL_ALIGN_MAX` drift guard above performs its own,
/// separate `build_size2class` const-evaluation (via its own
/// `SizeClassesImpl::build(PARAMS)` call, not reusing [`SC`]) purely to read
/// one `small_align_max()` field back out -- compile-time cost only, not
/// eliminated by this change.
///
/// This removes the redundant *computation*, not necessarily the redundant
/// *storage*: `SIZE2CLASS` is still its own `static` with its own address,
/// distinct from the copy inside `SC` — whether the compiler additionally
/// dedupes the two `.rodata` byte sequences (identical content, different
/// symbols) is an optimizer/linker decision this change does not control or
/// guarantee. Measured (task #1518): `examples/global_allocator` built
/// `--release --features "production internals alloc-global"` before and
/// after this change produced byte-identical executables (217088 bytes both
/// times, via `git stash`/`git stash pop` on this file) -- the MSVC linker
/// already folds the two identical `.rodata` sequences, so this crate's own
/// storage duplication happens to cost nothing extra on this toolchain
/// today. A different linker could behave differently; do not cite this
/// comment as a portable `.rodata`-savings guarantee beyond "one fewer
/// redundant `build_size2class` call at compile time." (size-classes
/// round-3 prepublish review P3-4 asked whether `SIZE2CLASS` could instead
/// be a `&'static` reference into `SC`'s own table rather than a copy --
/// given the measured zero current impact, that type change was not made;
/// revisit if a future toolchain/linker measurement shows a real cost.)
pub(crate) static SIZE2CLASS: [u8; S2C_LEN] = *SC.size2class();

/// A classifier over [`SIZE_CLASS_TABLE`]. A zero-sized forwarder to the crate
/// scheme [`SC`] — kept so the in-tree `SizeClasses::class_for(..)` /
/// `::block_size(..)` / `::is_huge(..)` call sites compile unchanged.
///
/// All methods are `const` pure arithmetic — no allocations, no panics on the
/// lookup path FOR IN-CONTRACT INPUTS (task #755's closing review, F6:
/// mirrors the same qualification task #731 already applied to the crate's
/// own doc, `size_classes::SizeClasses::class_for` — `class_for` below
/// forwards straight into a `debug_assert!` that fires for a non-power-of-two
/// `align`, and `block_size` panics on an out-of-range index in every
/// profile, not just debug — see that method's own doc below).
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
    /// `size` here is already clamped to `>= MIN_BLOCK` by every allocator
    /// entry point that calls this (`AllocCore::alloc`/realloc, the registry
    /// heap-core entry points). The `SegmentLayout::class_for` re-export
    /// forwards `size` unclamped -- clamping there is its OWN caller's
    /// documented contract, not this function's.
    #[must_use]
    pub(crate) const fn class_for(size: usize, align: usize) -> Option<usize> {
        SC.class_for(size, align)
    }

    /// The checked twin of [`class_for`](Self::class_for): rejects a
    /// non-power-of-two `align` (including `0`) with `Err(InvalidAlign)`
    /// instead of trusting it. No in-tree allocator call site needs this --
    /// every internal caller's alignment already comes from a `Layout` -- it
    /// exists solely so [`SegmentLayout::try_class_for`](super::SegmentLayout::try_class_for),
    /// a genuinely public API, has a checked forwarder to expose (size-classes
    /// round-4 prepublish review, P2-1).
    #[must_use = "this returns a Result, not just a class index -- the Err case must be handled"]
    pub(crate) const fn try_class_for(
        size: usize,
        align: usize,
    ) -> Result<Option<usize>, InvalidAlign> {
        SC.try_class_for(size, align)
    }

    /// The block size of class `idx`. Panics (all profiles) if out of range
    /// — `self.table[idx]` is a bounds-checked array index, not a
    /// `debug_assertions`-gated guard (task #755's closing review, F6: this
    /// doc previously said "(debug)", which was wrong in every profile) —
    /// the Cartographer only ever passes indices returned by `class_for`.
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
