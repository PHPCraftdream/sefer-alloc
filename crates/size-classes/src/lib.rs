//! `size-classes` — const-built size-class tables + a compile-time-derived
//! O(1) size→class lookup + an alignment-divisibility classifier.
//!
//! Every slab / pool / arena allocator reinvents the same trio: a table of
//! block sizes, an O(1) map from a requested byte size to the smallest class
//! that fits it, and a classifier that also honours alignment via stride
//! divisibility. This crate packages that trio as a `const`-evaluated,
//! `no_std`, zero-dependency, `#![forbid(unsafe_code)]` unit — the table
//! shape is a parameter, so a consumer can bake its own scheme and still
//! get the derived lookup and the alignment-aware classifier for free.
//!
//! ## The three pieces
//!
//! - [`build_table`] — a `const fn` sorted-merge of a geometric progression
//!   (`geo_count` classes, each `round_up(ceil(prev * num / den), min_block)`)
//!   with a strictly increasing, `min_block`-multiple, `>= min_block` list of
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
//!   (`sefer-alloc`'s own motivating case, the allocator this crate was
//!   extracted from: `align >= 512`). The classifier picks an
//!   `align`-*divisible* stride; see [`SizeClasses::class_for`]'s
//!   `# Preconditions` for the separate base-address requirement this crate
//!   cannot check.
//!   [`SizeClasses::try_class_for`] is the checked twin -- validates `align`
//!   instead of assuming it. Use it unless `align` is already known-valid by
//!   construction (e.g. taken from a [`core::alloc::Layout`]).
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
//! `extras.len()`) and the lookup length `L` (`max_class / min_block + 1`,
//! via [`size2class_len`]). Both are pure functions of the [`Params`], but
//! `L` needs the built table's LAST entry (`max_class`) — there is no
//! shortcut around building `TABLE` once to read it; [`SizeClasses::build`]
//! then builds the same table again internally, from the same [`Params`],
//! so the two never drift apart:
//!
//! ```text
//! const PARAMS: Params = Params::new(MIN_BLOCK, (5, 4), GEO_COUNT, EXTRAS, HUGE_THRESHOLD);
//! const N: usize = GEO_COUNT + EXTRAS.len();
//! const TABLE: [usize; N] = build_table::<N>(PARAMS);
//! const L: usize = size2class_len(TABLE[N - 1], MIN_BLOCK);
//! static SC: SizeClasses<N, L> = SizeClasses::build(PARAMS);
//! ```
//!
//! (Runnable form with concrete values in the crate's `README.md`.)

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
    /// be a power of two. Every generated class is a multiple of it -- see
    /// [`SizeClasses::class_for`]'s `# Preconditions` for what that does and
    /// does not guarantee about block addresses.
    pub min_block: usize,
    /// The geometric growth ratio as `(num, den)` — each class after the first
    /// is `round_up(ceil(prev * num / den), min_block)`, with a minimum step
    /// of `min_block` so two adjacent classes never collide. `(5, 4)` is the
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
/// # Memory cost
///
/// `L` (`max_class / min_block + 1`) is the byte size of the `size2class`
/// LUT `SizeClasses` embeds, and it is NOT something a consumer picks
/// directly -- it falls out of `min_block`, `growth`, `geo_count`, and
/// `extras` together. It scales with `max_class / min_block`, not with the
/// number of classes `N`, so a scheme with FEWER classes can still produce a
/// LARGER LUT than one with more: a realistic scheme (`min_block = 16`,
/// `growth = (5, 4)`, `geo_count = 40`, nine extras up to 16 KiB; the crate
/// itself has no defaults) with 49 classes and `max_class = 258752` gives
/// `L = 16173` (`table` itself is only `N * size_of::<usize>()` = 392 bytes
/// on a 64-bit target; the LUT dominates -- `table` + `size2class` together
/// are ~16.18 KiB, and `size_of::<SizeClasses<49, 16173>>()` itself is
/// ~16.20 KiB on a 64-bit target, the difference being the struct's two
/// scalar fields plus alignment padding) — but a smaller `min_block` can
/// outweigh a smaller class count entirely: `min_block = 8` with just 24
/// classes (`growth = (3, 2)`, no `extras`) reaches `max_class = 145648` and
/// `L = 18207`, a LARGER object than the 49-class example above. Concretely,
/// for that same 49-class example the sparsity this scaling implies is
/// large: buckets `888..=16172` — 15285 of the 16173 total, 94.5% — all
/// resolve to just the 14 largest classes (indices `35..=48`), because
/// above `~14 KiB` the geometric spacing widens to `~1.25×` while the LUT's
/// own resolution stays a flat `min_block`.
///
/// # Panics
///
/// Panics -- identically in `const` evaluation and at runtime, since this is
/// a `pub const fn` callable either way -- if `min_block` is not a power of
/// two, or if `max_class / min_block + 1` overflows `usize` (reachable only
/// for `min_block == 1` and `max_class == usize::MAX`; for any `min_block >=
/// 2` the quotient cannot reach `usize::MAX`).
///
/// The `+ 1` overflow check is explicit rather than relying on the profile's
/// default: a release-profile `const` evaluation reached through a `const fn`
/// call follows the crate's `overflow-checks` setting and can silently wrap
/// to `0` otherwise (<https://github.com/rust-lang/rust/issues/74823>).
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
/// `round_up(ceil(prev * num / den), min_block)`, with a minimum step of
/// `min_block`. The `extras` are merged in sorted order (a plain sorted-merge —
/// `const fn` cannot call `slice::sort`), keeping the combined table strictly
/// increasing and every entry a multiple of `min_block`.
///
/// `growth = (num, den)` with `num <= den` (including `(0, den)`) is a
/// deliberately valid scheme, not a contract violation: a ratio `<= 1` makes
/// the geometric term always `<= prev`, so every class falls back to the
/// `min_block`-step minimum, degrading the whole run to a flat `min_block`,
/// `2 * min_block`, `3 * min_block`, … sequence.
///
/// # Panics
///
/// Panics -- identically in `const` evaluation and at runtime, since this is
/// a `pub const fn` callable either way -- if any of:
///
/// - `N != geo_count + extras.len()`;
/// - `min_block` is not a power of two;
/// - `geo_count == 0`;
/// - `params.growth.1` (the growth denominator) is `0`;
/// - any `extras` entry is not a multiple of `min_block`;
/// - any `extras` entry is less than `min_block` (the scheme's minimum
///   block size);
/// - `extras` is not strictly increasing;
/// - the geometric progression's advance step overflows `usize` (see the
///   worked example below);
/// - the merged table (geometric run + `extras`) is not itself strictly
///   increasing -- the per-entry `extras` checks above catch misshapen
///   `extras`, but not an `extras` entry that DUPLICATES a value the
///   geometric run also produces, which only the merged table reveals. An
///   `extras` entry landing strictly BETWEEN two geometric values is fine,
///   and is one of the main reasons `extras` exists.
///
/// The advance-step overflow is reachable not just with an extreme
/// `min_block`/`growth` combination but with a large enough `geo_count`
/// alone: with `min_block = 16`, `growth = (5, 4)` (this crate's own tests'
/// example scheme; the crate itself has no defaults), `geo_count = 183`
/// already overflows on a 64-bit `usize` (`84` on a 32-bit one -- the
/// boundary scales with `usize::BITS`). At the top of that range (roughly
/// the last half-dozen steps -- the intermediate `cur * num` product first
/// exceeds `usize` only once `cur > usize::MAX / num`) is exactly the
/// widened-arithmetic case this crate's `CHANGELOG.md` describes: the next
/// class fits even though the intermediate `cur * num` product does not fit
/// `usize`.
#[must_use]
pub const fn build_table<const N: usize>(params: Params) -> [usize; N] {
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
/// (its own early-rejection guard catches every size that would land there),
/// so THERE the clamped entry is an unreachable sentinel, not
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
/// path FOR IN-CONTRACT INPUTS: `need = max(size, align) >= 1` (so `size ==
/// 0` alone is fine whenever `align >= 1` -- see
/// [`class_for`](Self::class_for)'s own doc for the precise domain), a
/// power-of-two `align`, and an `idx` obtained from
/// [`class_for`](Self::class_for) rather than picked independently — an
/// out-of-range `idx` does panic, see [`block_size`](Self::block_size).
///
/// Deliberately NOT `Copy`: an instance embeds both tables (a realistic
/// scheme is ~16 KiB -- see [`size2class_len`]'s `# Memory cost` for the
/// breakdown), and `Copy` would give a full-object duplicate a `let a = b;`
/// -cheap syntax. `Clone` keeps explicit duplication available while forcing
/// the call site to say so. Intended use is a `static` referenced in place
/// (a `const` this size re-materializes at every use site, duplicating the
/// embedded tables); no method needs ownership.
///
/// `Debug` is hand-written, not derived: a derive would print both raw
/// tables (same size as above) on any accidental `{:?}`/`dbg!`, burying the
/// useful line. This prints the summary a developer actually wants; use
/// [`table`](Self::table) / [`size2class`](Self::size2class) to inspect the
/// raw arrays directly.
#[derive(Clone)]
pub struct SizeClasses<const N: usize, const L: usize> {
    table: [usize; N],
    size2class: [u8; L],
    // `min_block`, `small_align_max`, and `1 << min_block_shift` are the same
    // value by construction (see `build` below) -- storing only the shift
    // removes one hot-path field load (`small_align_max`, read once in
    // `class_for`'s fast-path check) at the cost of re-deriving `1 << shift`
    // there instead; `min_block` was already accessor-only (never read
    // directly in `class_for`). `min_block()`/`small_align_max()` re-derive it.
    min_block_shift: u32,
    huge_threshold: usize,
}

/// The error [`SizeClasses::try_class_for`] returns when `align` is not a
/// power of two (the [`core::alloc::Layout`] contract
/// [`SizeClasses::class_for`] assumes but -- on its own hot path -- only
/// `debug_assert!`s). Carries the offending value for diagnostics.
///
/// A plain tuple struct, not `#[non_exhaustive]`: match the offending value
/// directly as `Err(InvalidAlign(n))`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidAlign(pub usize);

impl core::fmt::Display for InvalidAlign {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "align ({}) must be a power of two (the Layout contract)",
            self.0
        )
    }
}

impl core::error::Error for InvalidAlign {}

impl<const N: usize, const L: usize> core::fmt::Debug for SizeClasses<N, L> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SizeClasses")
            .field("N", &N)
            .field("L", &L)
            .field("min_block", &self.min_block())
            .field("small_max", &self.small_max())
            .field("huge_threshold", &self.huge_threshold)
            .finish_non_exhaustive()
    }
}

impl<const N: usize, const L: usize> SizeClasses<N, L> {
    /// Build a scheme from [`Params`] at compile time. `N` and `L` must match
    /// the params (see [`build_table`] / [`build_size2class`] for the exact
    /// obligations); a mismatch panics identically in `const` evaluation
    /// (compile error) and at runtime.
    ///
    /// Intended for a `static` (or `const`) initializer, where this is
    /// const-evaluated and free. Nothing stops calling it at *runtime*
    /// instead: doing so materializes the whole return value -- at least `L`
    /// bytes, several KiB for a realistic scheme -- by value on the caller's
    /// stack, which matters on a small-stack `no_std` target.
    ///
    /// `small_align_max` — the alignment ceiling of the O(1) fast path — is set
    /// to `min_block`: every class size is a multiple of `min_block`, so the
    /// stride trivially satisfies divisibility for any `align <= min_block`
    /// (see [`class_for`](Self::class_for)'s `# Preconditions` for the
    /// separate base-address requirement). Larger alignments take the
    /// divisibility-jump slow path in
    /// [`class_for`](Self::class_for).
    #[must_use]
    pub const fn build(params: Params) -> Self {
        let table = build_table::<N>(params);
        let size2class = build_size2class::<N, L>(&table, params.min_block);
        let small_max = table[N - 1];
        // `build_table` already guarantees every table entry is a multiple
        // of `min_block` -- this cannot fail through the public API. Kept as
        // a cheap internal sanity check because `class_for`'s index-space
        // guard (see its own comment) depends on this equality holding.
        debug_assert!(
            small_max.is_multiple_of(params.min_block),
            "SizeClasses::build: small_max must be a multiple of min_block"
        );
        Self {
            table,
            size2class,
            min_block_shift: params.min_block.trailing_zeros(),
            huge_threshold: params.huge_threshold,
        }
    }

    /// The class table (strictly increasing, each entry a multiple of
    /// `min_block`). The single source of truth for the scheme's geometry.
    #[must_use]
    #[inline]
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
    ///   guaranteed to fit `usize` for every valid scheme). Compare `size` to
    ///   [`small_max`](Self::small_max) directly, or reason about the INDEX
    ///   instead — beyond `small_max()` the raw index is NOT uniformly
    ///   clamped: `idx == L - 1` is in-bounds and returns the clamped
    ///   sentinel (a false "fits" instead of the `None`
    ///   [`class_for`](Self::class_for) would give), while `idx >= L` is
    ///   genuinely out-of-bounds and panics.
    ///
    /// [`class_for`](Self::class_for) avoids both pitfalls (its `need =
    /// max(size, align)` is always `>= 1`, and it rejects a too-large `need`
    /// before indexing) and additionally applies the `align` predicate this
    /// raw LUT ignores. Prefer it unless you specifically need the raw LUT.
    ///
    /// **This shape is a deliberate, but not permanently promised, choice.**
    /// The LUT is today one flat `u8` per `min_block`-sized bucket over the
    /// *whole* size range (see [`size2class_len`]'s `# Memory cost`) — the
    /// simplest shape that stays O(1) for arbitrary `extras`, but a
    /// memory-hungry one for a scheme with a large `max_class`. `L` being a
    /// public const generic means a future layout change (e.g. a hybrid:
    /// an exact small-size LUT below some threshold, a computed answer
    /// above it) would very likely require a breaking release regardless.
    #[must_use]
    #[inline]
    pub const fn size2class(&self) -> &[u8; L] {
        &self.size2class
    }

    /// The minimum block size / fundamental alignment (`min_block`).
    /// Derived from [`min_block_shift`](Self::min_block_shift) (`1 <<
    /// min_block_shift`) rather than stored separately -- the two are equal
    /// by construction (see [`build`](Self::build)).
    #[must_use]
    #[inline]
    pub const fn min_block(&self) -> usize {
        1usize << self.min_block_shift
    }

    /// `log2(min_block)` — the shift turning a byte size into a
    /// `min_block`-unit index.
    #[must_use]
    #[inline]
    pub const fn min_block_shift(&self) -> u32 {
        self.min_block_shift
    }

    /// The alignment ceiling of the O(1) fast path (equal to `min_block`) --
    /// not the ceiling on alignments [`class_for`](Self::class_for) can
    /// serve at all; larger alignments take its slow path instead.
    #[must_use]
    #[inline]
    pub const fn small_align_max(&self) -> usize {
        1usize << self.min_block_shift
    }

    /// The largest class (`table[N - 1]`). A request larger than this — or with
    /// an alignment larger than this — takes the caller's large path.
    #[must_use]
    #[inline]
    pub const fn small_max(&self) -> usize {
        self.table[N - 1]
    }

    /// The number of classes (`N`).
    #[must_use]
    #[inline]
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
    #[inline]
    pub const fn block_size(&self, idx: usize) -> usize {
        self.table[idx]
    }

    /// The caller's [`Params::huge_threshold`] policy value, as built. The
    /// only `Params` field with a dedicated read-back accessor here, for a
    /// caller that needs to report or log the threshold without keeping its
    /// own separate copy of it.
    #[must_use]
    #[inline]
    pub const fn huge_threshold(&self) -> usize {
        self.huge_threshold
    }

    /// Whether a `size` request is "huge" per the caller's
    /// [`Params::huge_threshold`] policy.
    #[must_use]
    #[inline]
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
    /// guarantee — see `# Preconditions` below for what it does and does not
    /// establish about block addresses.
    ///
    /// **Fast path (`align <= min_block`):** every class SIZE is a multiple of
    /// `min_block`, which does two things: the stride divisibility check is
    /// trivially satisfied, **and** the LUT's bucket-top answer is the
    /// smallest *fitting* class (no class value can lie strictly between
    /// `need` and its bucket's top) — one O(1) lookup (same base-alignment
    /// precondition as the slow path, below).
    ///
    /// **Slow path (`align > min_block`, a power of two):** seed at the lookup
    /// entry covering `max(size, align)`, then jump forward over non-divisible
    /// classes — from a non-divisible class of block size `b`, the next class
    /// that could be a multiple of `align` is the one covering the smallest
    /// multiple of `align` strictly greater than `b` (a bitmask round-up plus
    /// one lookup). Provably equivalent to a step-by-1 walk, never more
    /// iterations, fewer whenever the jump skips at least one class.
    ///
    /// The useful domain is `size >= 1`; more precisely, what must hold is
    /// `need = max(size, align) >= 1`, so `size == 0` alone is fine whenever
    /// `align >= 1` -- `(need - 1) >> shift` never underflows in that case.
    /// Consumers commonly clamp `size` up to `min_block` before calling (the
    /// classifier has no smaller class to offer below it anyway); this
    /// function does not require that clamp.
    ///
    /// # Preconditions
    ///
    /// `align` **must be a power of two** — the same `Layout` contract the
    /// standard allocator API requires. An `align` taken from
    /// [`core::alloc::Layout`] satisfies this by construction; one computed
    /// by hand may not.
    ///
    /// A violation trips a `debug_assert!` whenever `cfg(debug_assertions)` is
    /// on (both this and the `overflow-checks` knob below track the profile
    /// `size-classes` itself is compiled with, which normally tracks the
    /// consumer's). With `debug_assertions` off, the behavior for a non-zero
    /// non-power-of-two `align` is UNSPECIFIED: an incorrect `Some`/`None`
    /// (the fast path skips the divisibility check entirely; the slow path's
    /// bitmask round-up and its `block & (align - 1) == 0` test both assume a
    /// power of two and can overshoot, under-return, or wrongly accept a
    /// non-fitting class), never memory unsafety or a corrupt table. The
    /// `align == 0, size == 0` corner does NOT panic, but only with
    /// `overflow-checks` ALSO off (a separate Cargo knob from
    /// `debug_assertions`): `need - 1` underflows to `usize::MAX`, landing on
    /// the same early `None` any other out-of-range request takes (see
    /// `class_for`'s own index-space guard comment for the proof); with
    /// `overflow-checks` on, that subtraction panics instead. Prefer
    /// [`try_class_for`](Self::try_class_for), which rejects `align == 0`
    /// before any of this arithmetic runs in every profile, over relying on
    /// this fallback behavior.
    ///
    /// **The carve base must also be `align`-aligned.** For blocks carved at
    /// `base + k * block_size`, `block_size % align == 0` gives `address(k) %
    /// align == base % align` for every `k`: the stride PRESERVES whatever
    /// alignment the carve base already has (so no per-block padding is ever
    /// needed) — it cannot CREATE alignment the base lacks. This crate
    /// computes over sizes only and never sees an address, so it CANNOT
    /// check this — unlike the power-of-two contract above, it is not even
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
    #[inline]
    pub const fn class_for(&self, size: usize, align: usize) -> Option<usize> {
        debug_assert!(
            align.is_power_of_two(),
            "class_for: align must be a power of two (the Layout contract)"
        );
        let need = if size > align { size } else { align };
        // Index-space guard, not `need > self.small_max()`: `small_max()` is
        // always `(L - 1) * min_block` (every
        // `build_table` entry is a `min_block` multiple -- see `build`'s own
        // invariant assert above), so `seed_idx >= L - 1 <=> need >
        // small_max()` exactly, and `L - 1` is a compile-time constant where
        // `small_max()` reads `self.table[N - 1]` at runtime. This lets the
        // compiler prove `seed_idx < L` and drop the bounds check
        // `self.size2class[seed_idx]` would otherwise need, instead of
        // paying that check on top of the one just performed here.
        let seed_idx = (need - 1) >> self.min_block_shift;
        if seed_idx >= L - 1 {
            return None;
        }
        let seed = self.size2class[seed_idx] as usize;
        if align <= (1usize << self.min_block_shift) {
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
            // `align` is a power of two here, so the mask is a
            // division-free `is_multiple_of`.
            if block & (align - 1) == 0 {
                return Some(i);
            }
            // `block | (align - 1)` is one below the smallest multiple of
            // `align` strictly greater than `block` (align is a power of
            // two, so that's what `+ 1` would round up to) -- exactly the
            // value the bucket index below wants, so there is no reason to
            // add 1 and then immediately subtract it again. Same
            // index-space guard as the seed above; if no next multiple
            // exists (`block | (align - 1) == usize::MAX`), the shifted
            // index is `usize::MAX >> shift >= small_max >> shift == L - 1`
            // (the same identity the seed guard relies on), so the guard
            // below returns `None` on its own -- no separate overflow check
            // needed.
            let next_idx = (block | (align - 1)) >> self.min_block_shift;
            if next_idx >= L - 1 {
                return None;
            }
            i = self.size2class[next_idx] as usize;
        }
        // Unreachable in practice: `self.size2class[..] <= N - 1` always (see
        // `build_size2class`'s own invariant), so the loop always returns
        // from inside the body at `i == N - 1` at the latest. Still needed
        // as the type-level fallthrough -- and `while i < N` is what lets
        // the compiler elide the `self.table[i]` bounds check above.
        None
    }

    /// The checked twin of [`class_for`](Self::class_for): validates `align`
    /// instead of assuming it (`Err(`[`InvalidAlign`]`)` for a non-power-of-two
    /// `align`, including `0`), then delegates. Same result on every
    /// already-valid input; the only behavior difference is on the inputs
    /// `class_for`'s own `# Preconditions` already document as
    /// contract-violating. Does strictly more work than `class_for` (the
    /// added power-of-two check).
    ///
    /// **Never panics, for any `(size, align)` pair** — this is the
    /// substantive reason to prefer it over `class_for` for an `align` that
    /// is not already known-valid: a non-power-of-two `align` (including
    /// `0`) is rejected before any arithmetic runs, so `need = max(size,
    /// align)` is always `>= 1` past that point; the seed index
    /// `(need - 1) >> min_block_shift` is compared against the compile-time
    /// bound `L - 1` **before** any indexing, so both the seed and every
    /// slow-path re-seed stay strictly inside `size2class()`, and the
    /// slow-path jump loop is bounded exactly as `class_for`'s own doc
    /// proves.
    ///
    /// Use this one unless `align` is already known-valid by construction
    /// (e.g. taken directly from a [`core::alloc::Layout`]) -- `class_for`
    /// stays the zero-validation hot-path variant for that case, matching
    /// [`Layout::from_size_align`](core::alloc::Layout::from_size_align) (checked) versus
    /// [`Layout::from_size_align_unchecked`](core::alloc::Layout::from_size_align_unchecked) (trusted) in `core::alloc`.
    #[must_use = "this returns a Result, not just a class index -- the Err case must be handled"]
    #[inline]
    pub const fn try_class_for(
        &self,
        size: usize,
        align: usize,
    ) -> Result<Option<usize>, InvalidAlign> {
        if !align.is_power_of_two() {
            return Err(InvalidAlign(align));
        }
        Ok(self.class_for(size, align))
    }
}
