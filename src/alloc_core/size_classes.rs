//! [`SizeClasses`] — the size-class scheme (49 fine classes to a threshold,
//! then large/huge direct segments), the safe Cartographer's classifier.
//!
//! This is **pure safe integer arithmetic** over a fixed table — it touches no
//! memory and adds zero `unsafe`. It replaces the 8-class toy of the Phase 4
//! `ByteRegion` with a mimalloc-style spacing: dense small classes (low
//! internal fragmentation) with a smooth geometric progression.
//!
//! ## Scheme
//!
//! - **Small classes (index `0..SMALL_CLASS_COUNT`):** 49 fine classes from
//!   `MIN_BLOCK` (16 B) up to `SMALL_MAX` (~253 KiB — before task B1 this was
//!   the top of a 40-entry table; B1 kept that top entry but merged in 8 more
//!   classes at the low end, and task #145 (P1) merged in one more — the exact
//!   256 B class — so `SMALL_MAX` itself is unchanged). 40 of the 49 classes
//!   form the original geometric spacing: starting at `MIN_BLOCK`
//!   and growing with a roughly 1.25× step (mimalloc's small-spacing idea),
//!   rounded to a multiple of `MIN_BLOCK` so every block stays
//!   `MIN_BLOCK`-aligned (which satisfies every alignment the small classes
//!   advertise, since `align <= size <= block` and `block` is a multiple of
//!   `MIN_BLOCK`). Eight more (task B1, the follow-up to #114) are explicit
//!   "page-aligned" classes — 512, 1024, 2048, 4096, 6144, 8192, 12288,
//!   16384 — and one more (task #145) is the exact 256 B class (the geometric
//!   progression skips 240 → 304, wasting ~18% on a 256 B request — the size
//!   where mimalloc leads on churn). All are merged into the sorted sequence
//!   so that typical page-aligned
//!   requests (direct I/O buffers, `io_uring`, `#[repr(align(4096))]`) in the
//!   512 B – 16 KiB range resolve to a small class instead of unconditionally
//!   burning a whole ~4 MiB Large segment (the pre-B1 gap: no class in the
//!   plain geometric progression is ever a multiple of 512, so every
//!   `align >= 512` request fell through `class_for`'s divisibility walk to
//!   `None`, i.e. Large, however small `size` was).
//! - **Large:** allocations whose requested size exceeds `SMALL_MAX` get a
//!   dedicated whole-segment span — one `Segment` per large allocation. No
//!   size class; the segment is sized to fit. `segment_of(ptr)` still finds
//!   the owner in O(1). Alignment alone does **not** force the Large path:
//!   `align > MIN_BLOCK` (16) is still served by a small class whenever one
//!   exists whose `block_size` is a multiple of `align` (see
//!   [`SizeClasses::class_for`]'s slow path, #114/B1) — an allocation only
//!   falls through to Large on alignment grounds when `align` exceeds every
//!   small class's block size (i.e. `align > SMALL_MAX`), since no small
//!   block can then be a multiple of it.
//! - **Huge:** allocations whose size is `>= HUGE_THRESHOLD` are also a single
//!   dedicated segment (just a bigger one — `Segment::reserve` rounds up to
//!   whole segments). Large and huge share the same path; the threshold is
//!   bookkeeping so future phases can apply a different policy (guard pages,
//!   eager decommit) to huge spans.
//!
//! ## Invariants upheld
//!
//! - **M4 (alignment & size fidelity):** for a small allocation, the chosen
//!   class's `block_size` is always `>= max(requested_size, requested_align)`,
//!   AND `block_size` is a multiple of `MIN_BLOCK` (which is a power of two),
//!   so `block_size >= requested_align` implies the block is naturally aligned
//!   to `requested_align` (because the segment base is SEGMENT-aligned and the
//!   offset is a multiple of `block_size`, hence of `requested_align`).
//! - The smallest class is `>= NODE_SIZE` (the free-list node word), so a free
//!   block always has room for the intrusive `next` pointer.

/// The minimum block size and the fundamental small-class alignment. Must be a
/// power of two `>=` [`super::node::NODE_SIZE`] (the free-list node word).
pub(crate) const MIN_BLOCK: usize = 16;

/// `log2(MIN_BLOCK)` — the shift that turns a byte size into a `MIN_BLOCK`-unit
/// index. Derived from `MIN_BLOCK` at compile time so it cannot drift if
/// `MIN_BLOCK` ever changes (the table assumes `MIN_BLOCK` is a power of two;
/// `build_table` and [`build_size2class`] rely on this).
pub(crate) const MIN_BLOCK_SHIFT: u32 = MIN_BLOCK.trailing_zeros();

/// The alignment threshold below which a small allocation is served by the
/// **fast** O(1) path. Equal to `MIN_BLOCK` (every small block is
/// `MIN_BLOCK`-aligned, so any alignment `<= MIN_BLOCK` is trivially
/// honoured — the fast path in [`SizeClasses::class_for`]). This is *not*
/// the ceiling on alignments the small path can serve: `align >
/// SMALL_ALIGN_MAX` (up to `SMALL_MAX`) still resolves to a small class via
/// the bounded divisibility-walk slow path added by #114/B1 — only
/// `align > SMALL_MAX` falls through to the dedicated-segment large/huge
/// path (which can honour arbitrary alignment up to `SEGMENT`). The name
/// predates #114/B1 and is kept for the existing `SegmentLayout` public
/// constant; read it as "small-*fast*-path alignment ceiling", not
/// "small-path alignment ceiling".
pub(crate) const SMALL_ALIGN_MAX: usize = MIN_BLOCK;

/// The table of fine small size classes, in strictly increasing order. Each
/// entry is a multiple of `MIN_BLOCK` and `>=` the previous entry. Constructed
/// at compile time by [`build_table`] so the spacing is visible as code, not
/// magic numbers. This is the **single source of truth** for the small-class
/// geometry; [`SIZE2CLASS`] is derived from it by [`build_size2class`].
pub(crate) const SIZE_CLASS_TABLE: [usize; 49] = build_table();

/// Number of small size classes (length of [`SIZE_CLASS_TABLE`]).
pub(crate) const SMALL_CLASS_COUNT: usize = SIZE_CLASS_TABLE.len();

/// The largest small size class. Allocations `<=` this (with alignment `<=`
/// [`SMALL_ALIGN_MAX`]) are served by the small free-list path.
pub(crate) const SMALL_MAX: usize = *SIZE_CLASS_TABLE.last().unwrap();

/// The O(1) size→class lookup table, **derived at compile time from
/// [`SIZE_CLASS_TABLE`]** by [`build_size2class`]. `SIZE2CLASS[k]` is the index
/// of the smallest class whose `block_size >= (k * MIN_BLOCK)` — i.e. the class
/// that fits a request of `k * MIN_BLOCK` bytes (the `(size-1) >> MIN_BLOCK_SHIFT`
/// index maps a 1-based `size` onto the `k` whose class is the smallest that
/// holds `size` bytes, matching the old linear scan exactly).
///
/// Length is `(SMALL_MAX / MIN_BLOCK) + 1`: every `MIN_BLOCK`-aligned size bucket
/// from `0` (sentinel, unused on the live path) up to and including `SMALL_MAX`.
/// Entry type is `u8` because [`SMALL_CLASS_COUNT`] (currently 49; grows as
/// the table gains classes) is far below 256; a compile-time assertion in
/// [`build_size2class`] makes that invariant explicit.
pub(crate) const SIZE2CLASS: [u8; (SMALL_MAX / MIN_BLOCK) + 1] = build_size2class();

/// The huge threshold: allocations of this size or larger are flagged "huge"
/// so future phases can apply distinct policy (guard pages, eager decommit).
/// For Phase 8 it is simply `SEGMENT / 2` — anything within one segment is
/// "large", anything needing two or more segments is "huge". Both go through
/// the dedicated-segment path; the flag is bookkeeping.
#[allow(dead_code)] // Phase 10 (M6 decommit policy) consumes this; kept for that.
pub(crate) const HUGE_THRESHOLD: usize = super::os::SEGMENT;

/// A classifier over [`SIZE_CLASS_TABLE`].
///
/// All methods are `const` pure arithmetic — no allocations, no panics on the
/// lookup path. [`class_for`](Self::class_for) returns the index of the
/// smallest class that fits `(size, align)`, or `None` if the request must go
/// through the large/huge path.
pub(crate) struct SizeClasses;

impl SizeClasses {
    /// Resolve `(size, align)` to a small-class index, or `None` for large.
    ///
    /// A small class fits iff:
    ///   * its `block_size >= max(size, align)` (M4: size & align fidelity), AND
    ///   * its `block_size % align == 0` (so the natural offset within a
    ///     SEGMENT-aligned segment is a multiple of `align`, hence the returned
    ///     pointer is `align`-aligned without any per-block alignment padding).
    ///
    /// Returns the index of the smallest such class, or `None` (→ Large path)
    /// when no small class satisfies the two predicates above (the typical
    /// case is `align > SMALL_MAX` or a large page-aligned request).
    ///
    /// **Fast path (`align <= SMALL_ALIGN_MAX`, i.e. ≤ MIN_BLOCK = 16):** every
    /// small block is `MIN_BLOCK`-aligned by construction, so the divisibility
    /// check is trivially satisfied and we resolve in O(1) via the
    /// compile-time-derived [`SIZE2CLASS`] table — the original behaviour,
    /// untouched.
    ///
    /// **Slow path (`align > SMALL_ALIGN_MAX`):** we still seed at the
    /// `SIZE2CLASS` entry that covers `max(size, align)`, then jump forward
    /// over non-divisible classes (via `SIZE2CLASS` lookups on successive
    /// multiples of `align`) to find one whose `block_size` is divisible by
    /// `align`. This is provably equivalent to a step-by-1 walk but skips the
    /// non-divisible geometric classes between multiples — e.g. for
    /// `align = 128` from the ~144 B seed class it jumps straight to the
    /// 256 B class. Without this path EVERY alloc with `align > 16` would go
    /// burning a full ~4 MiB segment + a SegmentTable slot per request — an
    /// architectural OOM source under concurrent task-spawning workloads
    /// (see task #114).
    ///
    /// `size` here is already clamped to `>= MIN_BLOCK` by the only caller
    /// ([`super::alloc_core::AllocCore::alloc`]).
    #[must_use]
    pub(crate) const fn class_for(size: usize, align: usize) -> Option<usize> {
        let need = if size > align { size } else { align };
        if need > SMALL_MAX {
            return None;
        }
        let seed = SIZE2CLASS[(need - 1) >> MIN_BLOCK_SHIFT] as usize;
        // Fast path: align ≤ MIN_BLOCK ⇒ every small block satisfies divisibility.
        if align <= SMALL_ALIGN_MAX {
            return Some(seed);
        }
        // Slow path: `align > SMALL_ALIGN_MAX` is a power of two (the `Layout`
        // contract). Walk forward, but JUMP over non-divisible classes via the
        // existing [`SIZE2CLASS`] table rather than stepping one class at a time.
        // From a non-divisible class `i` (block size `b`), the next class that
        // COULD be a multiple of `align` is the one covering the smallest
        // multiple of `align` strictly greater than `b` — a bitmask round-up
        // (align is a power of two) plus one O(1) `SIZE2CLASS` lookup. This is
        // provably equivalent to the step-by-1 walk (it finds the same first
        // divisible class — see `class_for_slow_path_matches_walk` in
        // `tests/size_classes_slow_path_equivalence.rs`) but is never more
        // iterations and is fewer whenever `seed` lands in a run of non-
        // divisible geometric classes — e.g. `align=128` from the ~144 B class
        // jumps directly to the 256 B class, skipping ~8 intervening classes.
        // Termination: `next_mult > block` ⟹ the looked-up class index is
        // strictly greater than `i` (the table is strictly increasing), so `i`
        // advances every iteration.
        let mut i = seed;
        while i < SMALL_CLASS_COUNT {
            let block = SIZE_CLASS_TABLE[i];
            if block.is_multiple_of(align) {
                return Some(i);
            }
            // Smallest multiple of `align` strictly greater than `block` (align
            // is a power of two, so `(block | (align - 1)) + 1` rounds up).
            let next_mult = (block | (align - 1)) + 1;
            if next_mult > SMALL_MAX {
                return None;
            }
            i = SIZE2CLASS[(next_mult - 1) >> MIN_BLOCK_SHIFT] as usize;
        }
        None
    }

    /// The block size of class `idx`. Panics (debug) if out of range — the
    /// Cartographer only ever passes indices returned by `class_for`.
    #[must_use]
    pub(crate) const fn block_size(idx: usize) -> usize {
        SIZE_CLASS_TABLE[idx]
    }

    /// Whether a `size` request is "huge" (gets the dedicated-segment huge
    /// policy in future phases). For Phase 8 this is purely informational.
    #[must_use]
    #[allow(dead_code)] // Phase 10 (M6) consumes this; kept for that.
    pub(crate) const fn is_huge(size: usize) -> bool {
        size >= HUGE_THRESHOLD
    }
}

/// Extra "page-aligned" classes merged into the geometric table by
/// [`build_table`] — task B1 (2026-07), the follow-up to #114.
///
/// #114 fixed `class_for` to walk forward for `align > SMALL_ALIGN_MAX` and
/// find a class whose `block_size` is divisible by `align`, closing the hole
/// for alignments up to 256. But no class in the plain 1.25×-geometric
/// progression (16, 32, 48, 64, 80, 112, 144, 192, 240, 304, ...) is ever a
/// multiple of 512/1024/2048/4096, so every page-aligned request (`align` a
/// multiple of 512 — the canonical shape for direct I/O buffers, `io_uring`,
/// or `#[repr(align(4096))]` types) still fell through the walk to `None` and
/// was routed to the dedicated-segment Large path, burning a whole ~4 MiB
/// segment per allocation — the exact `SegmentTable`-exhaustion pattern #114
/// fixed for smaller alignments, just not closed for this shape.
///
/// These 8 explicit classes plug that hole for the common small
/// page-aligned sizes (512 B – 16 KiB, the direct-I/O / io_uring buffer
/// range); requests needing bigger page-aligned blocks still go through
/// Large, which is fine — Large exists precisely for less-common bulk sizes.
const PAGE_ALIGNED_EXTRA: [usize; 8] = [512, 1024, 2048, 4096, 6144, 8192, 12288, 16384];

/// The exact 256 B class (task #145, P1). The plain geometric progression
/// jumps 240 → 304 (a 1.25× step rounded to `MIN_BLOCK`), so a 256 B request
/// — the exact size where mimalloc leads on churn — resolved to the 304 B
/// class: ~18% internal waste. 256 is disjoint from the geometric sequence
/// (which never lands on 256) AND from [`PAGE_ALIGNED_EXTRA`] (all ≥ 512), so
/// it merges cleanly into the sorted table as one new class, taking
/// `SMALL_CLASS_COUNT` from 48 to 49. It is a multiple of `MIN_BLOCK` (256 =
/// 16 × 16), so every table invariant (strictly increasing, each entry a
/// multiple of `MIN_BLOCK`) still holds. `SMALL_MAX` is unchanged (256 is far
/// below the top entry).
const EXACT_EXTRA: [usize; 1] = [256];

/// Build the small size-class table at compile time. Spacing: start at
/// `MIN_BLOCK`, then each next class is `round_up(prev * 5 / 4, MIN_BLOCK)`
/// (a 1.25× geometric step rounded to the alignment), with a minimum step of
/// `MIN_BLOCK` (so two adjacent classes never collide) — 40 classes from 16 B
/// up to ~253 KiB. [`EXACT_EXTRA`] (the exact 256 B class, task #145) and
/// [`PAGE_ALIGNED_EXTRA`] (8 more classes, each a multiple of
/// 512/1024/2048/4096 up to 16 KiB) are merged in sorted order, giving 49
/// classes total. The merge keeps the table strictly increasing (a hard
/// invariant both `SizeClasses::class_for`'s divisibility walk and
/// [`build_size2class`]'s O(1) derivation rely on) and every entry stays a
/// multiple of `MIN_BLOCK` (256 = 16×16, and all `PAGE_ALIGNED_EXTRA` values
/// are multiples of 512, hence of `MIN_BLOCK` = 16).
const fn build_table() -> [usize; 49] {
    // Build the 40-entry geometric progression first (unchanged from before
    // task B1).
    let mut geo = [0usize; 40];
    let mut cur = MIN_BLOCK;
    let mut i = 0;
    while i < 40 {
        geo[i] = cur;
        // Next: ceil(cur * 1.25), then round up to MIN_BLOCK, with a minimum
        // step of MIN_BLOCK.
        let next_raw = cur + cur.div_ceil(4);
        let mut next = next_raw;
        // Round up to a multiple of MIN_BLOCK.
        let mask = MIN_BLOCK - 1;
        next = (next + mask) & !mask;
        if next <= cur {
            next = cur + MIN_BLOCK;
        }
        cur = next;
        i += 1;
    }

    // Build the sorted 9-entry `extra` sequence = EXACT_EXTRA (256, task #145)
    // ∪ PAGE_ALIGNED_EXTRA (512..=16384, task B1). Both inputs are sorted and
    // disjoint (256 < 512), and `EXACT_EXTRA` has a single element, so a plain
    // prepend keeps `extra` strictly increasing.
    let mut extra = [0usize; 9];
    extra[0] = EXACT_EXTRA[0];
    let mut xi = 0;
    while xi < 8 {
        extra[xi + 1] = PAGE_ALIGNED_EXTRA[xi];
        xi += 1;
    }

    // Merge `geo` (40, sorted) with `extra` (9, sorted, and known at
    // construction time to be disjoint from `geo` — verified by the
    // `no_duplicate_class_sizes` test) into one sorted 49-entry table. A plain
    // sorted-merge (both inputs are already sorted), since `const fn` cannot
    // call `slice::sort` (no heap, no trait objects in const context).
    let mut out = [0usize; 49];
    let mut gi = 0; // index into geo
    let mut ei = 0; // index into extra
    let mut oi = 0; // index into out
    while gi < 40 || ei < 9 {
        let take_geo = if gi >= 40 {
            false
        } else if ei >= 9 {
            true
        } else {
            geo[gi] < extra[ei]
        };
        if take_geo {
            out[oi] = geo[gi];
            gi += 1;
        } else {
            out[oi] = extra[ei];
            ei += 1;
        }
        oi += 1;
    }
    out
}

/// Build the O(1) size→class lookup [`SIZE2CLASS`] **from
/// [`SIZE_CLASS_TABLE`]** at compile time — so the lookup and the table cannot
/// drift (one source of truth). The caller indexes it as
/// `SIZE2CLASS[(size - 1) >> MIN_BLOCK_SHIFT]`, so bucket `k` covers every size
/// in `(k * MIN_BLOCK, (k + 1) * MIN_BLOCK]`. To fit the *largest* size in that
/// bucket, `SIZE2CLASS[k]` must be the smallest class whose `block_size >=
/// (k + 1) * MIN_BLOCK` (NOT `k * MIN_BLOCK` — that would under-serve sizes
/// near the top of the bucket). The table is strictly increasing and spans
/// `[MIN_BLOCK, SMALL_MAX]`, so a linear leftward walk over it settles every
/// bucket.
///
/// The `u8` entry type is sound only while [`SMALL_CLASS_COUNT`] < 256; a
/// compile-time `assert!` pins that invariant (a future table growth beyond
/// 255 classes would fail to compile here, rather than silently truncate).
const fn build_size2class() -> [u8; (SMALL_MAX / MIN_BLOCK) + 1] {
    // The `u8` entry type is sound only while SMALL_CLASS_COUNT < 256; pin that
    // invariant at compile time (a future table growth beyond 255 classes would
    // fail to compile here, rather than silently truncate).
    const {
        assert!(
            SMALL_CLASS_COUNT < 256,
            "SIZE2CLASS entries are u8; SMALL_CLASS_COUNT must stay below 256"
        )
    };
    let len = SMALL_MAX / MIN_BLOCK + 1;
    let mut out = [0u8; (SMALL_MAX / MIN_BLOCK) + 1];
    let mut k = 0;
    // `class_idx` persists across iterations of `k` (rather than restarting
    // the scan from 0 every bucket) so the whole derivation is O(buckets +
    // classes) total instead of O(buckets * classes). Both `need` (as a
    // function of `k`) and `SIZE_CLASS_TABLE` are non-decreasing, so the
    // smallest class satisfying an earlier (smaller) `need` is always a valid
    // starting point for the next (>=) `need` — the monotone-pointer
    // technique. This is purely a compile-time-cost fix (avoids tripping
    // rustc's `long_running_const_eval` lint now that the table grew from 40
    // to 49 classes); the resolved values are identical to a from-scratch
    // linear scan per bucket.
    let mut class_idx = 0;
    while k < len {
        // The largest size that maps to bucket k via (size-1)>>shift is
        // (k+1)*MIN_BLOCK; that is the size the resolved class must cover.
        // (k == 0 ⇒ need = MIN_BLOCK ⇒ class 0; this also correctly handles the
        // size-in-(0, MIN_BLOCK] range the caller clamps into.) The top bucket
        // (k == SMALL_MAX/MIN_BLOCK) is only ever indexed by a size > SMALL_MAX,
        // which `class_for` rejects before indexing — but the array still has a
        // slot for it, so clamp `need` to SMALL_MAX to keep the compile-time walk
        // in-range (it resolves to the last class, a harmless sentinel).
        let need = if (k + 1) * MIN_BLOCK < SMALL_MAX {
            (k + 1) * MIN_BLOCK
        } else {
            SMALL_MAX
        };
        // Find the smallest class whose block_size >= need. The table is
        // sorted, so a forward walk from the previous bucket's answer
        // settles it; this runs ONCE at compile time, not per alloc. need <=
        // SMALL_MAX == table.last() always holds here, so the loop always
        // breaks in-range (no panic).
        while class_idx < SIZE_CLASS_TABLE.len() {
            if SIZE_CLASS_TABLE[class_idx] >= need {
                break;
            }
            class_idx += 1;
        }
        out[k] = class_idx as u8;
        k += 1;
    }
    out
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
