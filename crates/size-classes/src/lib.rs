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
#![deny(missing_docs)]
#![no_std]

/// Parameters for a size-class scheme, consumed by [`build_table`],
/// [`build_size2class`] and [`SizeClasses::build`].
///
/// All fields are plain data so the whole thing is usable in `const` context.
///
/// task #728 (rust-intel audit §C1a, MEDIUM): decided before this crate's
/// first crates.io publish (task #660) -- retrofitting `#[non_exhaustive]`
/// on an already-published all-pub-field config struct is ITSELF a breaking
/// change, so this had to be settled now, not deferred. `Params` carries
/// `#[non_exhaustive]`: adding a future policy field (plausible --
/// `small_align_max` is currently hardwired to `min_block` inside
/// `SizeClasses::build`, an obvious future knob the audit itself named) is
/// a semver-MINOR addition instead of MAJOR for every downstream struct
/// literal. Construct via [`Params::new`] (a `const fn`, so it works in the
/// same `const PARAMS: Params = ...` context struct-literal syntax did) --
/// plain `#[non_exhaustive]` alone would make this type UNCONSTRUCTABLE
/// downstream, since `const` context has no `Default`/functional-record-
/// update escape hatch, so the two halves cannot be shipped separately.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
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
    /// task #728 (rust-intel audit §B1b, INFO): `Params`'s borrowed
    /// lifetime `'a` was reviewed and is justified, not a defect — this is
    /// a `no_std`, zero-alloc, `const`-fn crate, so a borrowed slice is the
    /// only representable form for a variable-length field, and the
    /// zero-copy `const`-context design goal is documented at the
    /// crate-doc level. In typical `const` usage (a `const PARAMS: Params
    /// = Params::new(.., EXTRAS, ..)` binding to a `const`/`static` slice)
    /// `'a` resolves to `'static`; nothing about the type requires it to.
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
    /// `const fn` so this works everywhere the previous struct-literal
    /// syntax did, including `const PARAMS: Params = Params::new(..);` —
    /// the required construction path now that [`Params`] is
    /// `#[non_exhaustive]` (task #728).
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
/// task #731 (rust-intel audit §B26, INFO): this is this crate's ONE `pub`
/// function that previously had zero parameter validation -- `max_class /
/// min_block` hit an unguarded integer division for `min_block == 0`
/// (panics in every profile, but with a bare "attempt to divide by zero"
/// rather than a diagnostic naming the actual bad parameter), even though
/// every sibling entry point ([`build_table`], [`build_size2class`])
/// asserts `min_block.is_power_of_two()` with a named message. Added the
/// matching assert here for the same precondition, closing the one
/// inconsistent chokepoint.
///
/// size-classes publication audit run 1 (Sol-codex, P2-1): the trailing
/// `+ 1` was a bare add -- `size2class_len(usize::MAX, 1)` divides to
/// `usize::MAX`, and the `+ 1` then overflows. `debug` traps on this; in
/// `release` -- both a runtime call AND a `const` evaluation, since this
/// function is `pub`, not `const`-only, and const-eval overflow checks
/// follow the profile for a `const fn`'s body -- it silently wrapped to
/// `0` instead, the exact release-silent, profile-dependent bug class
/// this crate's own `checked_mul`/`checked_add` precedent in
/// [`build_table`] already exists to prevent elsewhere.
/// `checked_add` turns the wrap into the same loud, named panic in every
/// profile.
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
/// combination but also with a large enough `geo_count` alone -- e.g. at the
/// crate's own default `min_block = 16`, `growth = (5, 4)`, `geo_count = 177`
/// already overflows), or if the merged table (geometric run + `extras`) is
/// not
/// itself strictly increasing — the per-entry `extras` checks above catch
/// misshapen `extras`, but not an `extras` entry that ties or interleaves
/// with a value the geometric run also produces, which only the merged
/// table reveals.
#[must_use]
pub const fn build_table<const N: usize>(params: &Params) -> [usize; N] {
    let min_block = params.min_block;
    assert!(
        min_block.is_power_of_two(),
        "min_block must be a power of two"
    );
    assert!(params.geo_count > 0, "geo_count must be > 0");
    // task #731 (rust-intel audit §B26, INFO): `growth.1` (the denominator)
    // was never asserted non-zero -- `den == 0` reached the geometric
    // advance step below and panicked with a BARE "attempt to divide by
    // zero", inconsistent with every sibling precondition here (all of
    // which name the actual bad parameter). `growth.0 == 0` is NOT
    // rejected: it silently degrades to a linear min_block-step table via
    // the existing `next <= cur` min-step fallback rather than panicking,
    // which is an intentional (if unusual) valid scheme, not a contract
    // violation -- only the denominator has no such fallback.
    assert!(params.growth.1 > 0, "growth denominator must be > 0");
    let geo_count = params.geo_count;
    let extras = params.extras;
    // size-classes publication audit run 2 (Claude, review-2 F3): a bare `+`
    // here shares the exact overflow hazard this crate has twice already
    // named and fixed elsewhere (task #731's `den == 0`; run 1's P2-1). An
    // absurd `geo_count` (e.g. `usize::MAX` with non-empty `extras`) wraps
    // this sum in release, and the wrapped value COULD pass the check --
    // but never produces a silently-wrong table: the merge loop below still
    // runs the true (non-wrapped) `geo_count + extras.len()` iterations and
    // is guaranteed to panic on `out[oi]` with a bare "index out of bounds"
    // once `oi` exceeds `N`. `checked_add` turns that into the same named,
    // parameter-identifying diagnostic every sibling precondition here
    // already gives.
    let n_matches = match geo_count.checked_add(extras.len()) {
        Some(sum) => sum == N,
        None => false,
    };
    assert!(n_matches, "N must equal geo_count + extras.len()");

    let mask = min_block - 1;

    // `extras` preconditions (documented on `Params::extras`): each entry a
    // multiple of `min_block` (so the fast path in `SizeClasses::class_for`,
    // which skips the divisibility check entirely, stays sound), and the
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
            // size-classes publication audit run 1 (Sol-codex, P2-2): `0` is
            // "a multiple of min_block" (the check above alone accepts it),
            // but `min_block` is documented as the scheme's minimum block
            // size (see `Params::min_block`) -- an `extras` entry of `0`
            // would sort before the geometric run's own first class
            // (`min_block`) and land in the table/`size2class`/`block_size`
            // surface as a zero-sized class no `Layout` (`align >= 1`) can
            // ever resolve to. Rejected outright rather than silently
            // admitted: a caller who genuinely wants a below-`min_block`
            // tier should lower `min_block` itself, not smuggle it in via
            // `extras`.
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
                // task #701 (rust-intel audit §B26, MEDIUM): `cur * num` was
                // a bare multiply on a geometrically-ACCUMULATING value --
                // this is a library (`build_table`/`Params` are `pub`, and
                // some call sites reach this at runtime, not just in
                // `const` table construction), so per §B26 it cannot assume
                // the consumer built with `overflow-checks = true`. In a
                // release profile the multiply would silently WRAP, and the
                // `next <= cur` min-step fallback below then MASKED that
                // wrap into a valid-looking strictly-increasing table
                // (min_block steps instead of the requested geometry) --
                // `build_size2class`'s downstream monotonicity check cannot
                // catch this, since the masked table IS still strictly
                // increasing, just silently wrong. `checked_mul`/
                // `checked_add` turn a release-silent wrong-table bug into
                // an always-panicking one, with a diagnostic naming the
                // actual overflow -- in BOTH `const` and runtime contexts:
                // const-eval overflow checks follow the `overflow-checks`
                // profile for a `const fn`'s body (task #1423/#1431,
                // empirically verified), so a `release`-profile `const`
                // consumer of this fix was silently wrong before it too,
                // not just runtime call sites.
                let mut next = cur
                    .checked_mul(num)
                    .expect("geometric progression overflows usize -- reduce geo_count/growth")
                    .div_ceil(den);
                next = next
                    .checked_add(mask)
                    .expect("geometric progression overflows usize -- reduce geo_count/growth")
                    & !mask; // round up to a multiple of min_block
                if next <= cur {
                    // task #755's closing review (F4, MEDIUM): this bare `+`
                    // shares the exact overflow hazard the two `checked_*`
                    // calls above were fixed for -- #701's own commit
                    // message named this line and left it unguarded. It is
                    // reachable with a `min_block` in the 2^62+ range (an
                    // absurd but not `Params`-rejected value) and, worse,
                    // on every step of the `growth.0 == 0` linear-degradation
                    // scheme this crate's own docs bless as valid (see this
                    // function's rustdoc), since that scheme's fallback IS
                    // the only advance path. Reproduced pre-fix: a
                    // `min_block` of `1 << 62` wraps `next` to a duplicate
                    // AND a zero-sized class, not even monotone -- worse than
                    // the bug #701 fixed, since #701's masked table was at
                    // least strictly increasing.
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

    // size-classes publication audit run 1 (Sol-codex, P2-2): this rustdoc
    // promises the merged table is strictly increasing, but until this
    // check existed HERE, the only monotonicity check anywhere in the
    // crate lived downstream in `build_size2class` -- so a standalone
    // caller of `build_table` (a sanctioned use: the crate-level docs
    // describe `build_table`/`build_size2class` as independent building
    // blocks) could get back a table with a silent duplicate whenever an
    // `extras` entry ties or interleaves with the geometric run at a value
    // the per-entry checks above cannot see (each `extras` entry is only
    // checked against `min_block`-alignment and the OTHER `extras` entries,
    // never against the geometric run it is about to be merged with). The
    // exact reproduction: `min_block = 16`, `extras = [16, 32]` -- both
    // pass every check above (aligned, strictly increasing among
    // themselves) yet duplicate the geometric run's own first two classes.
    // Checking the merged `out` directly closes the contract at its own
    // function, not one layer downstream; `build_size2class`'s own
    // monotonicity check stays in place as defense-in-depth for tables a
    // caller assembles by hand rather than through `build_table`.
    {
        let mut i = 1;
        while i < N {
            assert!(
                out[i] > out[i - 1],
                "build_table: merged table must be strictly increasing -- an \
                 extras entry overlaps or interleaves with the geometric run \
                 (each extras entry is only checked against min_block-alignment \
                 and the other extras entries before merging)"
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
/// `L - 1`, whose ideal `need` (`L * min_block`) exceeds `table[N - 1]` (the
/// largest class), so no such class exists; that bucket is clamped to
/// `table[N - 1]` itself instead. This is harmless: [`SizeClasses::class_for`]
/// never queries bucket `L - 1` for any in-range `size` (its own `need >
/// small_max` early rejection catches every size that would land there), so
/// the clamped entry is an unreachable sentinel, not an observable answer.
///
/// `L` must equal [`size2class_len`]`(max_class, min_block)`, where `max_class`
/// is `table[N - 1]`.
///
/// `table` need not come from [`build_table`] -- this function is a
/// standalone building block, callable with any hand-built strictly
/// increasing array. Note, though, that [`build_table`]'s own output always
/// has every entry a multiple of `min_block`; a hand-built `table` that
/// violates that (while still passing every check below) can produce an
/// entry `class_for`'s bucket rounding never selects -- e.g. `min_block =
/// 16`, `table = [16, 24, 32]`: bucket `(16, 32]` resolves straight to `32`,
/// leaving `24` monotonicity-valid but permanently unreachable through the
/// public lookup path.
///
/// # Panics
///
/// Panics -- identically in `const` evaluation and at runtime, since this is
/// a `pub const fn` callable either way -- if the table is empty, if `L` is
/// wrong (including if computing the expected `L` via
/// [`size2class_len`]`(table[N - 1], min_block)` itself overflows `usize`),
/// if `min_block` is not a power of two, if `table.len() >= 256` (the entry
/// type is `u8`; a table beyond 255 classes would silently truncate), or if
/// `table` is not strictly increasing.
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
    // size-classes publication audit run 1 (Sol-codex, P2-1): reuse
    // `size2class_len` rather than re-deriving `small_max / min_block + 1`
    // here -- the bare add duplicated in this call site is exactly the
    // overflow hazard that function's own `checked_add` now closes, and two
    // copies of the same unchecked formula is how one of them silently
    // stays wrong.
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
        // size-classes publication audit run 1 (Sol-codex, P2-1): the
        // multiply used to run BEFORE the clamp comparison, so it could
        // overflow `usize` even though the clamped answer is always
        // `<= small_max`. `checked_mul` makes the overflow case fall
        // straight into the same clamp: if `(k + 1) * min_block` does not
        // fit in `usize`, its true mathematical value is certainly greater
        // than `small_max` (which does fit), so clamping to `small_max` is
        // exactly the answer the unchecked multiply would have produced had
        // it not wrapped.
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
/// independently). task #731 (rust-intel audit §F2, INFO): the previous
/// unqualified "no panics on the lookup path" contradicted
/// [`block_size`](Self::block_size)'s own `# Panics` section on this same
/// type (`idx >= N` panics there) -- qualified here rather than removing
/// `block_size`'s panic doc, since that panic is the correct behavior for
/// an out-of-contract `idx`.
///
/// Deliberately `Copy` (plain const data, no interior mutability, meant
/// for `const`/`static` use; removing it post-release would be breaking)
/// — the default SEFER instance is ~16 KiB, so pass it by reference.
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
    /// obligations); a mismatch panics identically in `const` evaluation
    /// (compile error) and at runtime.
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
    ///
    /// # Preconditions
    ///
    /// task #729 (rust-intel audit §F2/§B26): `align` **must be a power of
    /// two** — the same `Layout` contract Rust's own allocator API requires
    /// of its callers. This was previously stated only in an INTERNAL
    /// slow-path comment, never in this function's own public contract.
    /// For a non-power-of-two `align`, BOTH paths can silently violate the
    /// fit predicate stated above (`block_size % align == 0`): the fast
    /// path (`align <= small_align_max`) returns `seed` unconditionally,
    /// without ever checking divisibility by a non-pow2 `align`; the slow
    /// path's bitmask round-up (`(block | (align - 1)) + 1`) is only a
    /// correct "next multiple of `align`" computation for a power-of-two
    /// `align` and can overshoot for a non-pow2 one, skipping a class that
    /// would actually have fit or returning `None` where a fitting class
    /// exists. Third mode (task #1429, review-3 F2): false-ACCEPT — the
    /// slow path's P-1 mask test `block & (align - 1) == 0` is not a
    /// divisibility test for a non-pow2 `align` (on the SEFER scheme,
    /// `class_for(20, 24)` returns the block=32 seed, `32 % 24 == 8`).
    /// Neither path panics for a non-pow2 `align` — this is
    /// deliberately a `debug_assert!`, not a hard `assert!`, since the
    /// failure mode is a suboptimal/wrong CLASS CHOICE for a contract
    /// violation, not memory unsafety or table corruption (contrast task
    /// #701's geometric-overflow finding, which promoted to a release-
    /// active `assert!` because a masked wrong TABLE is a worse failure
    /// mode than a masked wrong class choice here). Most callers derive
    /// `align` from `core::alloc::Layout`, which already guarantees
    /// power-of-two by construction -- but NOT all: task #755's closing
    /// review found `tests/medium_classes_correctness.rs` (in this crate's
    /// consuming workspace) calling this function directly with a non-pow2
    /// `align` (a test bug, since fixed there), which is exactly the shape
    /// of caller this precondition exists to catch. The `debug_assert!`
    /// fires on any such violation at zero cost in release.
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
            // review-2 P-1: `align` is contractually a power of two here
            // (debug-asserted above; fast path already handles non-slow
            // aligns), so mask == is_multiple_of but skips a division;
            // matches `next_mult`'s idiom below. Measured ~24-45% faster on
            // the two affected bench rows, see task #1426 commit.
            if block & (align - 1) == 0 {
                return Some(i);
            }
            // Smallest multiple of `align` strictly greater than `block` (align
            // is a power of two, so `(block | (align - 1)) + 1` rounds up).
            //
            // size-classes publication audit run 1 (Sol-codex, P2-1): the
            // trailing `+ 1` was a bare add -- if `block | (align - 1)` is
            // already `usize::MAX` (no representable next multiple exists),
            // it wrapped to `0` in release instead of correctly falling
            // through to `None` below. `checked_add` makes "no next
            // multiple fits in usize" resolve to exactly that `None`, the
            // same outcome the `next_mult > self.small_max` clamp already
            // produces for every other out-of-range case on this path.
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
