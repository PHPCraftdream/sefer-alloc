//! `size-classes` — const-built size-class tables + a compile-time-derived
//! O(1) size→class lookup + an alignment-divisibility classifier.
//!
//! Every slab / pool / arena allocator reinvents the same trio: a table of
//! block sizes, an O(1) map from a requested byte size to the smallest class
//! that fits it, and a classifier that also honours alignment. This crate
//! packages that trio as a `const`-evaluated, `no_std`, zero-dependency,
//! `#![forbid(unsafe_code)]` unit — the table shape is a parameter, so a
//! consumer can bake its own scheme and still get the derived lookup and the
//! alignment-aware classifier for free.
//!
//! ## The three pieces
//!
//! - [`build_table`] — a `const fn` sorted-merge of a geometric progression
//!   (`geo_count` classes, each `round_up(prev * num / den, min_block)`) with
//!   an arbitrary sorted list of explicit `extras` (page-aligned classes, an
//!   exact size the geometric run skips, a feature-gated medium tier, …).
//! - [`build_size2class`] — derives the O(1) `size→class` lookup from a table
//!   at compile time with the monotone-pointer technique
//!   (`O(buckets + classes)` const-eval) and a compile-time `u8` pin.
//! - [`SizeClasses::class_for`] — an O(1) fast path for `align <= min_block`
//!   and a provably-equivalent *jump* slow path for larger alignments: round
//!   `block` up to the next multiple of `align` via a bitmask, re-seed through
//!   the lookup, and so skip whole runs of non-divisible classes instead of
//!   stepping by one. Without it, every `align >= 512` request silently falls
//!   through to the caller's whole-segment path (a real bug class in
//!   hand-rolled allocators).
//!
//! ## The `huge` threshold is a policy parameter
//!
//! [`SizeClasses::is_huge`] compares against a caller-supplied
//! [`Params::huge_threshold`]. The crate has no notion of an OS segment size;
//! the consumer picks the threshold that separates "large" from "huge" for its
//! own segment policy.
//!
//! ## Deriving lengths
//!
//! [`SizeClasses`] is generic over both the table length `N` (`geo_count` +
//! `extras.len()`) and the lookup length `L` (`max_class / min_block + 1`).
//! Both are pure functions of the [`Params`] — a consumer computes them as
//! `const` expressions (see [`size2class_len`]) so nothing is dynamic.

#![forbid(unsafe_code)]
#![no_std]

/// Parameters for a size-class scheme, consumed by [`build_table`],
/// [`build_size2class`] and [`SizeClasses::build`].
///
/// All fields are plain data so the whole thing is usable in `const` context.
#[derive(Debug, Clone, Copy)]
pub struct Params<'a> {
    /// The minimum block size and the fundamental small-class alignment. Must
    /// be a power of two. Every generated class is a multiple of it, so every
    /// block is naturally `min_block`-aligned.
    pub min_block: usize,
    /// The geometric growth ratio as `(num, den)` — each class after the first
    /// is `round_up(prev * num / den, min_block)`, with a minimum step of
    /// `min_block` so two adjacent classes never collide. `(5, 4)` is the
    /// classic mimalloc 1.25× small spacing.
    pub growth: (usize, usize),
    /// How many classes the geometric progression contributes (starting at
    /// `min_block`).
    pub geo_count: usize,
    /// Explicit extra classes to merge into the geometric run — a **strictly
    /// increasing** list, each entry a multiple of `min_block`, and each
    /// disjoint from the geometric run (the builder sorted-merges them; the
    /// disjointness/increasing preconditions are the caller's, matched by a
    /// consumer-side test). Typical uses: page-aligned classes, an exact size
    /// the geometric run skips, a feature-gated medium tier.
    pub extras: &'a [usize],
    /// The "huge" policy threshold: [`SizeClasses::is_huge`] reports `true` for
    /// a size `>=` this. Pure bookkeeping for the crate — the consumer decides
    /// what "huge" means for its own segment policy (guard pages, eager
    /// decommit, …).
    pub huge_threshold: usize,
}

/// The `size2class` array length for a scheme whose largest class is
/// `max_class`: one `u8` per `min_block`-sized bucket from `0` up to and
/// including `max_class`. A consumer uses this in a `const` expression to pin
/// the `L` generic of [`SizeClasses`].
#[must_use]
pub const fn size2class_len(max_class: usize, min_block: usize) -> usize {
    max_class / min_block + 1
}

/// Build the size-class table at compile time: a geometric progression merged
/// with `params.extras` in sorted order, returned as `[usize; N]` where `N`
/// must equal `params.geo_count + params.extras.len()`.
///
/// Spacing: start at `min_block`, then each next class is
/// `round_up(prev * num / den, min_block)`, with a minimum step of
/// `min_block`. The `extras` are merged in sorted order (a plain sorted-merge —
/// `const fn` cannot call `slice::sort`), keeping the combined table strictly
/// increasing and every entry a multiple of `min_block`.
///
/// # Panics
///
/// Panics in `const` evaluation if `N != geo_count + extras.len()`, if
/// `min_block` is not a power of two, or if `geo_count == 0`.
#[must_use]
pub const fn build_table<const N: usize>(params: &Params) -> [usize; N] {
    let min_block = params.min_block;
    assert!(
        min_block.is_power_of_two(),
        "min_block must be a power of two"
    );
    assert!(params.geo_count > 0, "geo_count must be > 0");
    let geo_count = params.geo_count;
    let extras = params.extras;
    assert!(
        N == geo_count + extras.len(),
        "N must equal geo_count + extras.len()"
    );
    let (num, den) = params.growth;

    let mask = min_block - 1;
    let mut out = [0usize; N];

    // Merge the geometric run (generated lazily) with `extras` (already sorted)
    // into one strictly-increasing `out`. Both sources are non-decreasing, so a
    // classic two-pointer merge settles it without an intermediate buffer.
    let mut gi = 0; // geometric index
    let mut ei = 0; // extras index
    let mut oi = 0; // output index
    let mut cur = min_block; // current geometric value (valid while gi < geo_count)
    while gi < geo_count || ei < extras.len() {
        let take_geo = if gi >= geo_count {
            false
        } else if ei >= extras.len() {
            true
        } else {
            cur < extras[ei]
        };
        if take_geo {
            out[oi] = cur;
            gi += 1;
            // Advance the geometric value for the next iteration:
            // next = round_up(ceil(cur * num / den), min_block), min step min_block.
            if gi < geo_count {
                let mut next = (cur * num).div_ceil(den);
                next = (next + mask) & !mask; // round up to a multiple of min_block
                if next <= cur {
                    next = cur + min_block; // enforce the minimum step
                }
                cur = next;
            }
        } else {
            out[oi] = extras[ei];
            ei += 1;
        }
        oi += 1;
    }
    out
}

/// Build the O(1) `size→class` lookup **from a table** at compile time — so the
/// lookup and the table cannot drift. The caller indexes it as
/// `size2class[(size - 1) >> log2(min_block)]`, so bucket `k` covers every size
/// in `(k * min_block, (k + 1) * min_block]`; `size2class[k]` is the smallest
/// class whose `block_size >= (k + 1) * min_block`.
///
/// `L` must equal [`size2class_len`]`(max_class, min_block)`, where `max_class`
/// is `table[N - 1]`.
///
/// # Panics
///
/// Panics in `const` evaluation if the table is empty, if `L` is wrong, if
/// `min_block` is not a power of two, or if `table.len() >= 256` (the entry
/// type is `u8`; a table beyond 255 classes would silently truncate).
#[must_use]
pub const fn build_size2class<const N: usize, const L: usize>(
    table: &[usize; N],
    min_block: usize,
) -> [u8; L] {
    assert!(N > 0, "table must be non-empty");
    assert!(
        min_block.is_power_of_two(),
        "min_block must be a power of two"
    );
    // The `u8` entry type is sound only while the class count < 256.
    assert!(
        N < 256,
        "size2class entries are u8; the class count must stay below 256"
    );
    let shift = min_block.trailing_zeros();
    let small_max = table[N - 1];
    assert!(
        L == small_max / min_block + 1,
        "L must equal size2class_len(max_class, min_block)"
    );
    let _ = shift; // the caller applies the shift; recorded here for clarity.

    let mut out = [0u8; L];
    let mut k = 0;
    // `class_idx` persists across `k` (monotone-pointer): both `need` and the
    // table are non-decreasing, so the answer for an earlier bucket is a valid
    // start for the next — O(buckets + classes) total.
    let mut class_idx = 0;
    while k < L {
        // The largest size mapping to bucket k via (size-1)>>shift is
        // (k+1)*min_block. Clamp to small_max so the top bucket (only ever
        // indexed by a size > small_max, which `class_for` rejects first) stays
        // in-range and resolves to the last class (a harmless sentinel).
        let need = if (k + 1) * min_block < small_max {
            (k + 1) * min_block
        } else {
            small_max
        };
        while class_idx < N {
            if table[class_idx] >= need {
                break;
            }
            class_idx += 1;
        }
        out[k] = class_idx as u8;
        k += 1;
    }
    out
}

/// A const-built size-class scheme: the sorted class table, its derived O(1)
/// `size→class` lookup, and the policy constants needed to classify a request.
///
/// - `N` — the number of classes (`geo_count + extras.len()`).
/// - `L` — the `size2class` length ([`size2class_len`]`(max_class, min_block)`).
///
/// Construct one at compile time with [`SizeClasses::build`]. All query methods
/// are `const` pure arithmetic — no allocation, no panics on the lookup path.
#[derive(Debug, Clone, Copy)]
pub struct SizeClasses<const N: usize, const L: usize> {
    table: [usize; N],
    size2class: [u8; L],
    min_block: usize,
    min_block_shift: u32,
    small_align_max: usize,
    small_max: usize,
    huge_threshold: usize,
}

impl<const N: usize, const L: usize> SizeClasses<N, L> {
    /// Build a scheme from [`Params`] at compile time. `N` and `L` must match
    /// the params (see [`build_table`] / [`build_size2class`] for the exact
    /// obligations); a mismatch is a `const`-evaluation panic (compile error).
    ///
    /// `small_align_max` — the alignment ceiling of the O(1) fast path — is set
    /// to `min_block`: every block is `min_block`-aligned by construction, so
    /// any `align <= min_block` is trivially honoured. Larger alignments take
    /// the divisibility-jump slow path in [`class_for`](Self::class_for).
    #[must_use]
    pub const fn build(params: Params) -> Self {
        let table = build_table::<N>(&params);
        let size2class = build_size2class::<N, L>(&table, params.min_block);
        let small_max = table[N - 1];
        Self {
            table,
            size2class,
            min_block: params.min_block,
            min_block_shift: params.min_block.trailing_zeros(),
            small_align_max: params.min_block,
            small_max,
            huge_threshold: params.huge_threshold,
        }
    }

    /// The class table (strictly increasing, each entry a multiple of
    /// `min_block`). The single source of truth for the scheme's geometry.
    #[must_use]
    pub const fn table(&self) -> &[usize; N] {
        &self.table
    }

    /// The derived O(1) `size→class` lookup.
    #[must_use]
    pub const fn size2class(&self) -> &[u8; L] {
        &self.size2class
    }

    /// The minimum block size / fundamental alignment (`min_block`).
    #[must_use]
    pub const fn min_block(&self) -> usize {
        self.min_block
    }

    /// `log2(min_block)` — the shift turning a byte size into a
    /// `min_block`-unit index.
    #[must_use]
    pub const fn min_block_shift(&self) -> u32 {
        self.min_block_shift
    }

    /// The alignment ceiling of the O(1) fast path (equal to `min_block`). Not
    /// the ceiling on alignments the small path can serve — see
    /// [`class_for`](Self::class_for)'s slow path.
    #[must_use]
    pub const fn small_align_max(&self) -> usize {
        self.small_align_max
    }

    /// The largest class (`table[N - 1]`). A request larger than this — or with
    /// an alignment larger than this — takes the caller's large path.
    #[must_use]
    pub const fn small_max(&self) -> usize {
        self.small_max
    }

    /// The number of classes (`N`).
    #[must_use]
    pub const fn count(&self) -> usize {
        N
    }

    /// The block size of class `idx`.
    ///
    /// # Panics
    ///
    /// Panics if `idx >= N` — the caller only ever passes indices returned by
    /// [`class_for`](Self::class_for).
    #[must_use]
    pub const fn block_size(&self, idx: usize) -> usize {
        self.table[idx]
    }

    /// Whether a `size` request is "huge" per the caller's
    /// [`Params::huge_threshold`] policy.
    #[must_use]
    pub const fn is_huge(&self, size: usize) -> bool {
        size >= self.huge_threshold
    }

    /// Resolve `(size, align)` to a class index, or `None` for the caller's
    /// large path.
    ///
    /// A class fits iff its `block_size >= max(size, align)` AND
    /// `block_size % align == 0` (so the natural offset within a
    /// `max_class`-aligned span lands on an `align`-aligned address without any
    /// per-block padding). Returns the index of the smallest such class.
    ///
    /// **Fast path (`align <= min_block`):** every block is `min_block`-aligned,
    /// so the divisibility check is trivially satisfied — one O(1) lookup.
    ///
    /// **Slow path (`align > min_block`, a power of two):** seed at the lookup
    /// entry covering `max(size, align)`, then jump forward over non-divisible
    /// classes — from a non-divisible class of block size `b`, the next class
    /// that could be a multiple of `align` is the one covering the smallest
    /// multiple of `align` strictly greater than `b` (a bitmask round-up plus
    /// one lookup). Provably equivalent to a step-by-1 walk, never more
    /// iterations, fewer whenever the seed lands in a run of non-divisible
    /// classes.
    ///
    /// `size` is expected `>= min_block` (the caller's contract); it is also
    /// well-defined for `size >= 1`, since `(size - 1) >> shift` stays in range.
    #[must_use]
    pub const fn class_for(&self, size: usize, align: usize) -> Option<usize> {
        let need = if size > align { size } else { align };
        if need > self.small_max {
            return None;
        }
        let seed = self.size2class[(need - 1) >> self.min_block_shift] as usize;
        if align <= self.small_align_max {
            return Some(seed);
        }
        // Slow path: `align > small_align_max` is a power of two (the `Layout`
        // contract). Walk forward, JUMPING over non-divisible classes via the
        // lookup rather than stepping one class at a time.
        //
        // Termination: `next_mult > block` ⟹ the looked-up class index is
        // strictly greater than `i` (the table is strictly increasing), so `i`
        // advances every iteration.
        let mut i = seed;
        while i < N {
            let block = self.table[i];
            if block.is_multiple_of(align) {
                return Some(i);
            }
            // Smallest multiple of `align` strictly greater than `block` (align
            // is a power of two, so `(block | (align - 1)) + 1` rounds up).
            let next_mult = (block | (align - 1)) + 1;
            if next_mult > self.small_max {
                return None;
            }
            i = self.size2class[(next_mult - 1) >> self.min_block_shift] as usize;
        }
        None
    }
}
