//! `size-classes` — const-built size-class tables + a compile-time-derived
//! O(1) size→class lookup + an alignment-divisibility classifier.
//!
//! Every slab / pool / arena allocator reinvents the same trio: a table of
//! block sizes, an O(1) map from a requested byte size to the smallest class
//! that fits it, and a classifier that also honours alignment via stride
//! divisibility. This crate
//! packages that trio as a `const`-evaluated, `no_std`, zero-dependency,
//! `#![forbid(unsafe_code)]` unit — the table shape is a parameter, so a
//! consumer can bake its own scheme and still get the derived lookup and the
//! alignment-aware classifier for free.
//!
//! ## The three pieces
//!
//! - [`build_table`] — a `const fn` sorted-merge of a geometric progression
//!   (`geo_count` classes, each `round_up(prev * num / den, min_block)`) with
//!   a strictly increasing, `min_block`-multiple, `>= min_block` list of
//!   explicit `extras` (page-aligned classes, an exact size the geometric
//!   run skips, a feature-gated medium tier, …).
//! - [`build_size2class`] — derives the O(1) `size→class` lookup from a table
//!   at compile time with the monotone-pointer technique
//!   (`O(buckets + classes)` const-eval) and a compile-time `u8` pin.
//! - [`SizeClasses::class_for`] — an O(1) fast path for `align <= min_block`
//!   and a provably-equivalent *jump* slow path for larger alignments: round
//!   `block` up to the next multiple of `align` via a bitmask, re-seed through
//!   the lookup, and so skip whole runs of non-divisible classes instead of
//!   stepping by one. Without it, a request whose `align` exceeds what the
//!   caller's classifier happens to handle silently falls through to the
//!   caller's whole-segment path — a real bug class in hand-rolled allocators
//!   (SEFER's own motivating case: `align >= 512`). The classifier picks an
//!   align-*divisible* stride; align-aligned block *addresses* additionally
//!   require the caller's carve base to be `align`-aligned — a documented
//!   precondition of `class_for`, which this crate (sizes only, no
//!   addresses) cannot check.
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
#![deny(missing_docs)]
#![no_std]

/// Parameters for a size-class scheme, consumed by [`build_table`],
/// [`build_size2class`] and [`SizeClasses::build`].
///
/// All fields are plain data so the whole thing is usable in `const` context.
///
/// `#[non_exhaustive]`, so a future policy field is a semver-minor addition
/// rather than a breaking one. Construct with [`Params::new`] — a `const fn`,
/// since `const` context has no functional-record-update escape hatch.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub struct Params<'a> {
    /// The minimum block size and the fundamental small-class alignment. Must
    /// be a power of two. Every generated class SIZE is a multiple of it, so
    /// every stride PRESERVES whatever `min_block`-alignment the caller's
    /// carve base already has -- it does not by itself make block ADDRESSES
    /// `min_block`-aligned (see [`SizeClasses::class_for`]'s base-alignment
    /// precondition).
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
    /// increasing** list, each entry a multiple of `min_block` and `>=
    /// min_block` (the builder sorted-merges them). All three preconditions
    /// are **machine-checked**: a non-`min_block`-multiple entry, an entry
    /// below `min_block` (rejects the degenerate `0` "class"), or a
    /// non-strictly-increasing entry panics identically in `const` evaluation
    /// (compile error) and at runtime in [`build_table`], which also checks
    /// disjointness from the geometric run at its own chokepoint (the merged
    /// table must itself be strictly increasing); [`build_size2class`] keeps
    /// the same check as defense-in-depth for a hand-built table that
    /// bypasses [`build_table`] entirely. Typical uses: page-aligned classes,
    /// an exact size the geometric run skips, a feature-gated medium tier.
    ///
    /// Borrowed rather than owned because this is a `no_std`, zero-alloc
    /// crate; in the usual `const PARAMS: Params = Params::new(.., EXTRAS,
    /// ..)` form `'a` resolves to `'static`, but nothing requires that.
    pub extras: &'a [usize],
    /// The "huge" policy threshold: [`SizeClasses::is_huge`] reports `true` for
    /// a size `>=` this. Pure bookkeeping for the crate — the consumer decides
    /// what "huge" means for its own segment policy (guard pages, eager
    /// decommit, …).
    pub huge_threshold: usize,
}

impl<'a> Params<'a> {
    /// Construct a [`Params`] from its component fields.
    ///
    /// `const fn`, so it works in `const PARAMS: Params = Params::new(..);`
    /// — the construction path for a `#[non_exhaustive]` type.
    #[must_use]
    pub const fn new(
        min_block: usize,
        growth: (usize, usize),
        geo_count: usize,
        extras: &'a [usize],
        huge_threshold: usize,
    ) -> Self {
        Self {
            min_block,
            growth,
            geo_count,
            extras,
            huge_threshold,
        }
    }
}

/// The `size2class` array length for a scheme whose largest class is
/// `max_class`: one `u8` per `min_block`-sized bucket from `0` up to and
/// including `max_class`. A consumer uses this in a `const` expression to pin
/// the `L` generic of [`SizeClasses`].
///
/// # Panics
///
/// Panics -- identically in `const` evaluation and at runtime, since this is
/// a `pub const fn` callable either way -- if `min_block` is not a power of
/// two, or if `max_class / min_block + 1` overflows `usize` (only reachable
/// with `max_class` within `min_block` of `usize::MAX`).
///
/// Both are checked, not merely documented: an unchecked `+ 1` here would
/// wrap to `0` in a release build — including a release-profile `const`
/// evaluation, since const-eval overflow checks follow the `overflow-checks`
/// profile for a `const fn`'s body — silently yielding an empty lookup.
#[must_use]
pub const fn size2class_len(max_class: usize, min_block: usize) -> usize {
    assert!(
        min_block.is_power_of_two(),
        "size2class_len: min_block must be a power of two"
    );
    match (max_class / min_block).checked_add(1) {
        Some(len) => len,
        None => panic!("size2class_len: max_class / min_block + 1 overflows usize"),
    }
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
/// `growth = (0, den)` is a deliberately valid scheme, not a contract
/// violation: a zero numerator makes the geometric term always `<= prev`, so
/// every class falls back to the `min_block`-step minimum, degrading the
/// whole run to a flat `min_block`, `2 * min_block`, `3 * min_block`, …
/// sequence.
///
/// # Panics
///
/// Panics -- identically in `const` evaluation and at runtime, since this is
/// a `pub const fn` callable either way -- if `N != geo_count + extras.len()`,
/// if `min_block` is not a power of two, if `geo_count == 0`, if
/// `params.growth.1` (the growth denominator) is `0`, if any `extras`
/// entry is not a multiple of `min_block`, if any `extras` entry is less
/// than `min_block` (the scheme's minimum block size), if `extras` is not
/// strictly increasing, if the geometric progression's advance step
/// overflows `usize` (reachable not just with an extreme `min_block`/`growth`
/// combination but also with a large enough `geo_count` alone -- e.g. with
/// `min_block = 16`, `growth = (5, 4)` (this crate's own tests' example
/// scheme; the crate itself has no defaults), `geo_count = 183` already
/// overflows on a 64-bit `usize` (84 on a 32-bit one -- the boundary scales
/// with `usize::BITS`; `geo_count` up to `182` is exactly the widened-
/// arithmetic case this crate's `CHANGELOG.md` describes: the next class
/// fits even though the intermediate `cur * num` product does not fit
/// `usize`)), or if the merged table (geometric run + `extras`) is not
/// itself strictly increasing — the per-entry `extras` checks above catch
/// misshapen `extras`, but not an `extras` entry that DUPLICATES a value
/// the geometric run also produces, which only the merged table reveals.
/// An `extras` entry landing strictly BETWEEN two geometric values is
/// fine, and is one of the main reasons `extras` exists.
#[must_use]
pub const fn build_table<const N: usize>(params: &Params) -> [usize; N] {
    let min_block = params.min_block;
    assert!(
        min_block.is_power_of_two(),
        "min_block must be a power of two"
    );
    assert!(params.geo_count > 0, "geo_count must be > 0");
    // Only the DENOMINATOR is rejected: `num == 0` degrades to a linear
    // min_block-step table via the min-step fallback below (a valid scheme,
    // see this function's rustdoc), but `den == 0` has no such fallback and
    // would otherwise surface as a bare "attempt to divide by zero".
    assert!(params.growth.1 > 0, "growth denominator must be > 0");
    let geo_count = params.geo_count;
    let extras = params.extras;
    // `checked_add` for the diagnostic, not for soundness: a wrapped sum
    // could pass this check, but the merge below still runs the true
    // iteration count and would panic on `out[oi]` with a bare index error.
    // This names the actual bad parameter instead.
    let n_matches = match geo_count.checked_add(extras.len()) {
        Some(sum) => sum == N,
        None => false,
    };
    assert!(n_matches, "N must equal geo_count + extras.len()");

    let mask = min_block - 1;

    // `extras` preconditions (documented on `Params::extras`): each entry a
    // multiple of `min_block` (so the fast path's stride-divisibility
    // predicate, implicit for `align <= min_block`, stays valid -- whether
    // the resulting ADDRESS is aligned is a separate, caller-owned
    // precondition; see `SizeClasses::class_for`'s doc), and the
    // list strictly increasing (so the sorted-merge below actually produces
    // a sorted table instead of silently reordering). Checked here — not
    // just at the merged-table monotonicity checks below (this function's
    // own, or `build_size2class`'s downstream defense-in-depth one) —
    // because a non-multiple-of-`min_block` extra can still land in
    // strictly increasing position relative to the geometric run
    // (misalignment alone does not always break monotonicity), so global
    // monotonicity is not sufficient to catch it.
    {
        let mut i = 0;
        while i < extras.len() {
            assert!(
                extras[i] & mask == 0,
                "Params::extras: every entry must be a multiple of min_block"
            );
            // Catches `0`, which the multiple-of check above accepts (0 is a
            // multiple of everything) but which would land in the table as a
            // zero-sized class no `Layout` can resolve to. A caller wanting a
            // smaller tier should lower `min_block`, not smuggle it in here.
            assert!(
                extras[i] >= min_block,
                "Params::extras: every entry must be >= min_block (min_block is the \
                 scheme's minimum block size)"
            );
            if i > 0 {
                assert!(
                    extras[i] > extras[i - 1],
                    "Params::extras: must be strictly increasing"
                );
            }
            i += 1;
        }
    }

    let (num, den) = params.growth;

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
                // Widened to u128 so only the ACTUAL next class has to fit
                // `usize`: guarding `cur * num` instead would reject schemes
                // whose product overflows but whose quotient does not (e.g.
                // min_block = 2^62, growth = (3, 3) at cur = 2^63).
                //
                // Checked at all because `cur` accumulates geometrically and
                // this is a library: an unchecked wrap would be masked by the
                // min-step fallback below into a valid-looking but silently
                // wrong table, in release builds and release-profile const
                // evaluation alike. `checked_mul`/`checked_add` (rather than a
                // hand-proved "cannot overflow" comment) so the bound is
                // enforced, not just argued -- for every `usize <= 64` target
                // this crate supports today. A hypothetical 128-bit `usize`
                // would need a genuinely overflow-free multiply/divide here
                // (`cur * num` can then overflow `u128` even when the true
                // quotient still fits `usize`); `checked_mul` alone would
                // reproduce the same class of bug one width higher.
                let scaled = match (cur as u128).checked_mul(num as u128) {
                    Some(p) => p.div_ceil(den as u128),
                    None => panic!("geometric progression: cur * num overflows u128"),
                };
                // Round up to a multiple of min_block, still in u128.
                let rounded = match scaled.checked_add(mask as u128) {
                    Some(v) => v & !(mask as u128),
                    None => panic!("geometric progression: scaled + mask overflows u128"),
                };
                assert!(
                    rounded <= usize::MAX as u128,
                    "geometric progression overflows usize -- reduce geo_count/growth"
                );
                let mut next = rounded as usize;
                if next <= cur {
                    // Checked for the same reason as the widened math above,
                    // and reached far more often: under `growth.0 == 0` this
                    // fallback is the ONLY advance path, so every step goes
                    // through it. Unchecked, a `min_block` near 2^62 wraps to
                    // a duplicate zero-sized class -- not even monotone.
                    next = cur
                        .checked_add(min_block)
                        .expect("geometric progression overflows usize -- reduce geo_count/growth");
                }
                cur = next;
            }
        } else {
            out[oi] = extras[ei];
            ei += 1;
        }
        oi += 1;
    }

    // The merged table is where an extra/geometric DUPLICATE first becomes
    // visible: the per-entry checks above compare each extra only against
    // `min_block` and the other extras, never against the run it is about to
    // merge with. `min_block = 16, extras = [16, 32]` passes every check
    // above yet duplicates the run's first two classes. Checked here so a
    // standalone `build_table` caller gets the guarantee its rustdoc
    // promises, rather than discovering it in `build_size2class` downstream.
    {
        let mut i = 1;
        while i < N {
            assert!(
                out[i] > out[i - 1],
                "build_table: merged table must be strictly increasing -- an \
                 extras entry duplicates a value the geometric run also \
                 produces (each extras entry is only checked against \
                 min_block-alignment and the other extras entries before \
                 merging; an extra landing strictly BETWEEN two geometric \
                 values is fine)"
            );
            i += 1;
        }
    }

    out
}

/// Build the O(1) `size→class` lookup **from a table** at compile time — so the
/// lookup and the table cannot drift. The caller indexes it as
/// `size2class[(size - 1) >> log2(min_block)]`, so bucket `k` covers every size
/// in `(k * min_block, (k + 1) * min_block]`; `size2class[k]` is the smallest
/// class whose `block_size >= (k + 1) * min_block` -- EXCEPT the top bucket
/// `L - 1`, whose ideal `need` (`L * min_block` mathematically -- NOT
/// guaranteed to fit `usize` even for a valid scheme, e.g. `min_block =
/// 1 << 62, L = 4`; the builder computes it as `(k + 1).checked_mul(min_block)`
/// and folds that same overflow into the clamp below, never evaluating the
/// unrepresentable product) exceeds `table[N - 1]` (the
/// largest class), so no such class exists; that bucket is clamped to
/// `table[N - 1]` itself instead. For [`SizeClasses::class_for`] specifically
/// this is harmless: it never queries bucket `L - 1` for any in-range `size`
/// (its own `need > small_max` early rejection catches every size that would
/// land there), so THERE the clamped entry is an unreachable sentinel, not
/// an observable answer. A caller driving this array directly (bypassing
/// `class_for`) can still observe it -- and for a hand-built `table` whose
/// `small_max` is not a multiple of `min_block`, bucket `L - 1` need not even
/// be a sentinel: it can be the correct, reachable answer for sizes in
/// `((L - 1) * min_block, small_max]`.
///
/// `L` must equal [`size2class_len`]`(max_class, min_block)`, where `max_class`
/// is `table[N - 1]`.
///
/// `table` need not come from [`build_table`] -- this function is a
/// standalone building block, callable with any hand-built strictly
/// increasing array. Note, though, that [`build_table`]'s own output always
/// has every entry a multiple of `min_block`; a hand-built `table` that
/// violates that (while still passing every check below) can produce an
/// entry the documented bucket lookup never selects -- e.g. `min_block =
/// 16`, `table = [16, 24, 32]`: bucket `(16, 32]` resolves straight to `32`,
/// leaving `24` monotonicity-valid but permanently unreachable through the
/// public lookup path. (There is no public constructor that feeds a
/// hand-built `table` into [`SizeClasses::class_for`] -- [`SizeClasses::build`]
/// always derives its table from [`build_table`] -- so this is a property of
/// the derived LUT itself, not of `class_for`.)
///
/// # Panics
///
/// Panics -- identically in `const` evaluation and at runtime, since this is
/// a `pub const fn` callable either way -- if the table is empty, if `L` is
/// wrong (including if computing the expected `L` via
/// [`size2class_len`]`(table[N - 1], min_block)` itself overflows `usize`),
/// if `min_block` is not a power of two, if `table.len() > 256` (entries are
/// `u8` CLASS INDICES, so the largest representable table has 256 classes,
/// indices `0..=255`; a 257th class would silently truncate), or if `table`
/// is not strictly increasing.
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
    // Entries are `u8` class INDICES, so the bound is on the largest index
    // (`N - 1`), not on the count: `N == 256` yields indices `0..=255`, all
    // representable. `class_idx` never reaches `N` (the `need` clamp below
    // guarantees `table[N - 1] >= need`, so the inner scan always breaks),
    // so 256 classes cannot truncate; 257 would.
    assert!(
        N <= u8::MAX as usize + 1,
        "size2class entries are u8 class indices; the class count must not exceed 256"
    );
    // Global monotonicity of `table` — the monotone-pointer algorithm below
    // *depends* on it. `build_table` already rejects a `Params::extras`
    // overlap with the geometric run at its own chokepoint, so a table
    // reaching this check via `SizeClasses::build` is already known-good;
    // this stays as defense-in-depth for a hand-built table that bypasses
    // `build_table` entirely (e.g. a `const` array literal) — an overlap
    // there collapses two table slots to equal values, which is a
    // duplicate, not a strict increase, and would otherwise leave the
    // colliding slot silently unreachable.
    {
        let mut i = 1;
        while i < N {
            assert!(
                table[i] > table[i - 1],
                "table must be strictly increasing (hand-built tables must \
                 satisfy this directly -- Params-driven tables already do, \
                 via build_table's own check)"
            );
            i += 1;
        }
    }
    let small_max = table[N - 1];
    // Reuse `size2class_len` rather than re-deriving its formula: a second
    // copy is how one of the two silently drifts out of sync (and loses the
    // overflow check).
    assert!(
        L == size2class_len(small_max, min_block),
        "L must equal size2class_len(max_class, min_block)"
    );
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
        //
        // `checked_mul` folds overflow into that same clamp: if the product
        // does not fit `usize`, its true value certainly exceeds `small_max`
        // (which does fit), so `small_max` is exactly the answer an
        // unwrapped multiply would have given.
        let need = match (k + 1).checked_mul(min_block) {
            Some(v) if v < small_max => v,
            _ => small_max,
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
/// are `const` pure arithmetic — no allocation, and no panics on the lookup
/// path FOR IN-CONTRACT INPUTS (`size >= 1`, a power-of-two `align`, and an
/// `idx` obtained from [`class_for`](Self::class_for) rather than picked
/// independently — an out-of-range `idx` does panic, see
/// [`block_size`](Self::block_size)).
///
/// Deliberately NOT `Copy`: an instance embeds both tables, so a realistic
/// scheme is ~16 KiB, and `Copy` would give that a `let a = b;`-cheap
/// syntax. `Clone` keeps explicit duplication available while forcing the
/// call site to say so. Intended use is a `static` referenced in place (a
/// `const` this size re-materializes at every use site, duplicating the
/// embedded tables -- see `clippy::large_const_arrays`); no method needs
/// ownership.
#[derive(Debug, Clone)]
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
    /// obligations); a mismatch panics identically in `const` evaluation
    /// (compile error) and at runtime.
    ///
    /// `small_align_max` — the alignment ceiling of the O(1) fast path — is set
    /// to `min_block`: every class SIZE is a multiple of `min_block`, so the
    /// stride trivially satisfies divisibility for any `align <= min_block`
    /// (block ADDRESSES are aligned only if the caller's carve base is --
    /// see [`class_for`](Self::class_for)'s base-alignment precondition).
    /// Larger alignments take the divisibility-jump slow path in
    /// [`class_for`](Self::class_for).
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

    /// The derived O(1) `size→class` lookup, as built by [`build_size2class`]
    /// -- see that function's doc for the indexing formula and the `L - 1`
    /// top-bucket clamp.
    ///
    /// LOW-LEVEL: unlike [`class_for`](Self::class_for), this accessor does
    /// not itself validate a raw caller's `size`. The documented formula is
    /// `size2class()[(size - 1) >> min_block_shift()]`, which has TWO
    /// preconditions this array does not enforce:
    ///
    /// - **`size >= 1`** — `size - 1` underflows for `size == 0`; guard it
    ///   with `size.checked_sub(1)` if `size` may be `0`.
    /// - **`size <= small_max()`** for a genuine classification. Do NOT
    ///   derive this bound as a byte size (`L * min_block()` is NOT
    ///   guaranteed to fit `usize`, even for a fully valid scheme — e.g.
    ///   `min_block = 1 << 62`, `L = 4` gives `L * min_block() == 2^64`,
    ///   already exercised by this crate's own `extreme64_overflow` test
    ///   fixture). Compare `size` to [`small_max`](Self::small_max)
    ///   directly, or reason about the INDEX instead — beyond
    ///   `small_max()` the raw index is NOT uniformly clamped:
    ///   - `idx == L - 1` IS in-bounds and returns the clamped sentinel —
    ///     the LAST class index, a false "fits" instead of the `None`
    ///     [`class_for`](Self::class_for) would give;
    ///   - `idx >= L` is genuinely OUT-OF-BOUNDS array indexing and panics,
    ///     not a sentinel.
    ///
    /// [`class_for`](Self::class_for) avoids both pitfalls: it indexes by
    /// `need = max(size, align)`, which is always `>= 1` given `align`'s
    /// own power-of-two contract (so the `size - 1` underflow above never
    /// applies to it — `size` itself is never validated, `need` just
    /// happens to always be in range), and it rejects `need > small_max`
    /// before ever indexing — plus applies the `align` predicate this raw
    /// LUT ignores entirely. Prefer it unless you specifically need the
    /// raw LUT and are prepared to enforce both preconditions yourself.
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
    /// `block_size % align == 0`. Returns the index of the smallest such class.
    ///
    /// The divisibility conjunct is a STRIDE property, not an address
    /// guarantee. For blocks carved at `base + k * block_size`, `block_size %
    /// align == 0` gives `address(k) % align == base % align` for every `k`:
    /// the stride PRESERVES whatever alignment the carve base already has (so
    /// no per-block padding is ever needed) — it cannot CREATE alignment the
    /// base lacks. Block addresses are `align`-aligned iff the carve base is
    /// — see the base-alignment precondition below.
    ///
    /// **Fast path (`align <= min_block`):** every class SIZE is a multiple of
    /// `min_block`, so the stride divisibility check is trivially satisfied —
    /// one O(1) lookup. (Block ADDRESSES are `min_block`-aligned only if the
    /// carve base is — same base-alignment precondition as the slow path,
    /// below.)
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
    /// well-defined for `size >= 1`, since `(size - 1) >> shift` stays in range
    /// -- more precisely, what must hold is `need = max(size, align) >= 1`, so
    /// `size == 0` alone is fine whenever `align >= 1`.
    ///
    /// # Preconditions
    ///
    /// `align` **must be a power of two** — the same `Layout` contract the
    /// standard allocator API requires. An `align` taken from
    /// [`core::alloc::Layout`] satisfies this by construction; one computed
    /// by hand may not.
    ///
    /// A violation trips a `debug_assert!` and is otherwise silent: it can
    /// only produce a wrong CLASS CHOICE, never memory unsafety or a
    /// corrupt table, so it is not worth a release-active check on this hot
    /// path. Concretely, all three of the fit predicate's failure modes
    /// become reachable for a non-pow2 `align`:
    ///
    /// - the fast path (`align <= min_block`) returns its seed without
    ///   checking divisibility at all;
    /// - the slow path's bitmask round-up computes the wrong "next multiple
    ///   of `align`", so it can overshoot a class that would have fit, or
    ///   return `None` when one exists;
    /// - the slow path's `block & (align - 1) == 0` test is not a
    ///   divisibility test for a non-pow2 `align`, so it can ACCEPT a class
    ///   that does not fit — e.g. `class_for(20, 24)` returning a 32-byte
    ///   block, where `32 % 24 == 8`.
    ///
    /// **The carve base must also be `align`-aligned.** This crate computes
    /// over sizes only and never sees an address, so it CANNOT check this —
    /// unlike the power-of-two contract above, it is not even
    /// `debug_assert`-able here. The caller must place block `0` of the run
    /// serving a returned class at an address `base` with `base % align ==
    /// 0` for every `align` it resolves through this scheme (the address
    /// that matters is block `0`'s, not the span's OS reservation base, if
    /// the two differ). Carving every run from a base whose power-of-two
    /// alignment is `>=` the largest `align` the scheme will ever serve
    /// satisfies this for every smaller `align` too.
    ///
    /// A violation cannot corrupt this crate's own scheme or cause UB INSIDE
    /// IT (pure arithmetic over sizes, no addresses touched) — it yields
    /// blocks whose SIZE is `align`-divisible but whose ADDRESSES are all
    /// congruent to the same `base % align != 0`. But an allocator built on
    /// top of this crate that returns such a misaligned pointer for a
    /// request with that `align` violates ITS OWN `Layout` contract with its
    /// caller — the downstream consequence is safety-critical even though
    /// this crate cannot detect or cause it directly.
    #[must_use]
    pub const fn class_for(&self, size: usize, align: usize) -> Option<usize> {
        debug_assert!(
            align.is_power_of_two(),
            "class_for: align must be a power of two (the Layout contract)"
        );
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
            // `align` is contractually a power of two here (debug-asserted
            // above), so the mask is equivalent to `is_multiple_of` but skips
            // a division — measured ~24-45% faster on the slow-path benches,
            // and it matches `next_mult`'s idiom just below.
            if block & (align - 1) == 0 {
                return Some(i);
            }
            // Smallest multiple of `align` strictly greater than `block` (align
            // is a power of two, so `(block | (align - 1)) + 1` rounds up).
            //
            // `checked_add` because `block | (align - 1)` can already be
            // `usize::MAX`, meaning no next multiple exists -- which is
            // exactly the `None` the `> small_max` clamp below yields for
            // every other out-of-range case. Unchecked, it wrapped to `0`.
            let next_mult = match (block | (align - 1)).checked_add(1) {
                Some(v) => v,
                None => return None,
            };
            if next_mult > self.small_max {
                return None;
            }
            i = self.size2class[(next_mult - 1) >> self.min_block_shift] as usize;
        }
        None
    }
}
