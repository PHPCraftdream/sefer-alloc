//! [`SizeClasses`] — the size-class scheme (~40 fine classes to a threshold,
//! then large/huge direct segments), the safe Cartographer's classifier.
//!
//! This is **pure safe integer arithmetic** over a fixed table — it touches no
//! memory and adds zero `unsafe`. It replaces the 8-class toy of the Phase 4
//! `ByteRegion` with a mimalloc-style spacing: dense small classes (low
//! internal fragmentation) with a smooth geometric progression.
//!
//! ## Scheme
//!
//! - **Small classes (index `0..SMALL_CLASS_COUNT`):** ~40 fine classes from
//!   `MIN_BLOCK` (16 B) up to `SMALL_MAX` (a few KiB). The spacing starts at
//!   `MIN_BLOCK` and grows with a roughly 1.25× step (mimalloc's small-spacing
//!   idea), rounded to a multiple of `MIN_BLOCK` so every block stays
//!   `MIN_BLOCK`-aligned (which satisfies every alignment the small classes
//!   advertise, since `align <= size <= block` and `block` is a multiple of
//!   `MIN_BLOCK`).
//! - **Large:** allocations whose requested size exceeds `SMALL_MAX` (or whose
//!   alignment exceeds `MIN_BLOCK`) get a dedicated whole-segment span — one
//!   `Segment` per large allocation. No size class; the segment is sized to
//!   fit. `segment_of(ptr)` still finds the owner in O(1).
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

/// The maximum alignment a small allocation may request and still be served by
/// a small size class. Equal to `MIN_BLOCK` (every small block is
/// `MIN_BLOCK`-aligned, so any alignment `<= MIN_BLOCK` is honoured). Larger
/// alignments go through the large/huge path (a dedicated segment, which can
/// honour arbitrary alignment up to `SEGMENT`).
pub(crate) const SMALL_ALIGN_MAX: usize = MIN_BLOCK;

/// The table of fine small size classes, in strictly increasing order. Each
/// entry is a multiple of `MIN_BLOCK` and `>=` the previous entry. Constructed
/// at compile time by [`build_table`] so the spacing is visible as code, not
/// magic numbers. This is the **single source of truth** for the small-class
/// geometry; [`SIZE2CLASS`] is derived from it by [`build_size2class`].
pub(crate) const SIZE_CLASS_TABLE: [usize; 40] = build_table();

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
/// Entry type is `u8` because [`SMALL_CLASS_COUNT`] (40) is far below 256; a
/// compile-time assertion in [`build_size2class`] makes that invariant explicit.
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
    /// A small class fits iff its `block_size >= max(size, align)` AND
    /// `align <= SMALL_ALIGN_MAX`. Returns the index of the smallest such
    /// class. The small path is **O(1)** — two comparisons plus one
    /// [`SIZE2CLASS`] load (no loop) — using the compile-time-derived lookup
    /// table. `size` here is already clamped to `>= MIN_BLOCK` by the only
    /// caller ([`super::alloc_core::AllocCore::alloc`]); the `(size-1) >>
    /// MIN_BLOCK_SHIFT` index maps a 1-based size onto its bucket exactly as
    /// the former linear scan did (verified by `tests/size_classes_lookup.rs`).
    #[must_use]
    pub(crate) const fn class_for(size: usize, align: usize) -> Option<usize> {
        if align > SMALL_ALIGN_MAX || size > SMALL_MAX {
            return None;
        }
        // `(size - 1) >> MIN_BLOCK_SHIFT`: 1-based size → 0-based MIN_BLOCK-unit
        // index. size==1 → 0, size==MIN_BLOCK → 0 (same class), size==MIN_BLOCK+1
        // → 1, … size==SMALL_MAX → SMALL_MAX/MIN_BLOCK. This is exactly the
        // smallest class `>= size`, because SIZE2CLASS was built so that entry
        // `k` is the smallest class whose block_size covers `k*MIN_BLOCK` bytes,
        // and `(size-1) >> shift` is the `k` for which `k*MIN_BLOCK < size <=
        // (k+1)*MIN_BLOCK` — i.e. the first class that can hold `size`.
        Some(SIZE2CLASS[(size - 1) >> MIN_BLOCK_SHIFT] as usize)
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

/// Build the small size-class table at compile time. Spacing: start at
/// `MIN_BLOCK`, then each next class is `round_up(prev * 5 / 4, MIN_BLOCK)`
/// (a 1.25× geometric step rounded to the alignment), with a minimum step of
/// `MIN_BLOCK` (so two adjacent classes never collide). Yields 40 classes from
/// 16 B up to ~30 KiB.
const fn build_table() -> [usize; 40] {
    let mut t = [0usize; 40];
    let mut cur = MIN_BLOCK;
    let mut i = 0;
    while i < 40 {
        t[i] = cur;
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
    t
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
    const { assert!(
        SMALL_CLASS_COUNT < 256,
        "SIZE2CLASS entries are u8; SMALL_CLASS_COUNT must stay below 256"
    ) };
    let len = SMALL_MAX / MIN_BLOCK + 1;
    let mut out = [0u8; (SMALL_MAX / MIN_BLOCK) + 1];
    let mut k = 0;
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
        // Find the smallest class whose block_size >= need. The table is sorted,
        // so a linear walk settles it; this runs ONCE at compile time, not per
        // alloc. need <= SMALL_MAX == table.last() always holds here, so the loop
        // always breaks in-range (no panic).
        let mut class_idx = 0;
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
