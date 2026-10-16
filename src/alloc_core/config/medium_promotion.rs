//! R6-T20 (review R6-CQ-22): single source of truth for the medium→Large
//! promotion-reachability predicate
//! `all(feature = "medium-classes", any(not(feature = "exact-span-large"),
//! all(feature = "large-reserved-capacity", not(feature = "numa-aware"))))`,
//! formerly hand-duplicated across the `alloc_core` and `registry` trees
//! (R19-8 canonicalized the `free/` split's four copies into a macro; R6-T20
//! extends that to the whole crate). A wrong copy silently compiles code out
//! instead of failing the build, so every copy that CAN share one text now
//! does.
//!
//! Three views of the one predicate live here, because Rust cannot collapse
//! them further: a `#[cfg(...)]` attribute cannot take a macro invocation as
//! its argument, and a `cfg!` value cannot be produced by an attribute —
//! - `MEDIUM_PROMOTION_REACHABLE` — the `cfg!` value, for in-function
//!   branching and for the test-side forwarder;
//! - `medium_promotion_reachable!` — generates `#[cfg(<predicate>)]` for
//!   element/statement gates;
//! - `medium_promotion_unreachable!` — generates `#[cfg(not(<predicate>))]`
//!   for the negated gates.
//!
//! These copies are pinned to each other and to the documented predicate by
//! `tests/r6_t20_medium_promotion_predicate.rs`, which CI runs in both
//! predicate polarities (`hardened medium-classes internals`,
//! `production medium-classes internals`,
//! `production medium-classes exact-span-large internals`, and
//! `--all-features`). `cfg_aliases`/`build.rs` would remove even these three
//! copies but were rejected as an owner decision (R6-T20): a new build
//! script or a new build dependency.
//!
//! Feature independence: the file holds only the `cfg!` const and the two
//! `macro_rules!` definitions — no imports — so its `mod` declaration in
//! `config/mod.rs` is deliberately UNGATED (the other children there carry
//! per-child gates only because their files import gated items). Every
//! consumer tree (`alloc_core`, `registry`) compiles only when `alloc-core`
//! does, and so does this module.

/// Single source of truth for the medium→Large promotion-reachability
/// predicate (R6-T20). `true` iff the promotion mechanism
/// (`HeapCore::try_promote_to_large` and its `realloc` call site) is
/// compiled into this build: `medium-classes` on, and not the zero-headroom
/// combination (`exact-span-large` without `large-reserved-capacity`
/// headroom, or with `numa-aware` overriding that headroom) — the full
/// derivation is `tests/r14_4_promotion_move_leg_reduction.rs`'s module doc.
/// For element-level `#[cfg]` gates use `medium_promotion_reachable!` so the
/// two forms cannot diverge. Gated to its one consumer
/// (`HeapCore::dbg_promotion_compiled`) so no build carries it unused.
#[cfg(all(feature = "alloc-global", feature = "internals"))]
pub(crate) const MEDIUM_PROMOTION_REACHABLE: bool = cfg!(all(
    feature = "medium-classes",
    any(
        not(feature = "exact-span-large"),
        all(
            feature = "large-reserved-capacity",
            not(feature = "numa-aware")
        )
    )
));

/// R19-8 (task #344), R6-T20: element/statement gate generator for the
/// promotion-reachability predicate — expands to `#[cfg(<predicate>)]`
/// followed by the captured tokens, so every gated site shares ONE textual
/// copy of the predicate.
///
/// Arms, matched on a literal leading token rather than on the `item`/`stmt`
/// fragment specifiers directly, for a real reason discovered while building
/// this macro in R19-8 (not a style preference): a naive two-arm design —
/// `($item:item) => {...}; ($stmt:stmt) => {...};` — breaks no matter which
/// arm is listed first.
///
///   - `item` first: `macro_rules!` tries arms in order, and an
///     `item`-fragment parse that fails (e.g. against a leading `{` or `if`,
///     as at the statement call sites) is a HARD parse error ("expected an
///     item keyword"), not a graceful fall-through to the next arm — it
///     aborts the whole match instead of trying the `stmt` arm that would
///     have succeeded.
///   - `stmt` first (or a single `stmt`-only arm): the `stmt` fragment
///     grammar DOES accept a bare item (a `const`/`fn`/`use` declaration is
///     a valid "item statement"), so it matches the item call sites too —
///     but then splicing `#[cfg(...)] $stmt` back in at ITEM position (a
///     `const` at module scope, an `fn` inside `impl HeapCore`) fails
///     ("expected item after attributes"): an interpolated `stmt` fragment
///     is opaque, and an outer attribute directly preceding it cannot be
///     re-attached to the item hiding inside it.
///
/// The fix: capture the `const`/`fn`/`use` sites via `$(#[$m:meta])*
/// $vis:vis const/fn/use $($rest:tt)*` — plain token-tree matching, which
/// is transparent (no fragment-opacity restriction) and never commits to
/// full item/stmt grammar before falling through, so a literal-keyword
/// mismatch (e.g. `const` against a `{`-led block) is a graceful "try the
/// next arm", not a hard error. The generic `$stmt:stmt` arm last catches
/// the block / bare `if` call sites, where the fragment IS spliced back
/// into genuine statement position, so no opacity issue arises there.
/// (`$vis:vis` matches an absent visibility too, so private sites are
/// unchanged; the `fn`/`use` binders were added in R6-T20 — R19-8's `fn`
/// arm had none because its only fn site was private.)
///
/// Sites that combine the predicate with MORE cfg terms write those terms as
/// ordinary `#[cfg(...)]` attributes INSIDE the invocation — multiple `cfg`
/// attributes on one item AND together, so the compiled configuration set is
/// identical to a single `#[cfg(all(...))]` spelling:
///
/// ```text
/// medium_promotion_reachable! {
///     #[cfg(feature = "alloc-global")]
///     #[cfg(feature = "fastbin")]
///     use crate::alloc_core::segment_header::SegmentHeader;
/// }
/// ```
///
/// Does NOT cover `cfg_attr` forms (a `cfg_attr` argument cannot invoke a
/// macro — the one surviving hand-written `cfg_attr` copy is
/// `SmallFreeGuard::RouteToLargeFree`'s dead-code allow, which only toggles
/// a lint, never code inclusion) or a predicate under `any(...)`/`not(...)`
/// combined with foreign terms at the TOP level — for the negation use
/// `medium_promotion_unreachable!`.
macro_rules! medium_promotion_reachable {
    ($(#[$m:meta])* $vis:vis const $($rest:tt)*) => {
        $(#[$m])*
        #[cfg(all(
            feature = "medium-classes",
            any(
                not(feature = "exact-span-large"),
                all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
            )
        ))]
        $vis const $($rest)*
    };
    ($(#[$m:meta])* $vis:vis fn $($rest:tt)*) => {
        $(#[$m])*
        #[cfg(all(
            feature = "medium-classes",
            any(
                not(feature = "exact-span-large"),
                all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
            )
        ))]
        $vis fn $($rest)*
    };
    ($(#[$m:meta])* $vis:vis use $($rest:tt)*) => {
        $(#[$m])*
        #[cfg(all(
            feature = "medium-classes",
            any(
                not(feature = "exact-span-large"),
                all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
            )
        ))]
        $vis use $($rest)*
    };
    ($stmt:stmt) => {
        #[cfg(all(
            feature = "medium-classes",
            any(
                not(feature = "exact-span-large"),
                all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
            )
        ))]
        $stmt
    };
}

/// R6-T20: negated sibling of `medium_promotion_reachable!` — expands to
/// `#[cfg(not(<predicate>))]` followed by the captured tokens, for the
/// hand-written negation sites (`dealloc_own_base.rs`'s Ph3b pre-route and
/// hardened branch (B), and the `HARDENED_LARGE_NOOP_COUNT` import's
/// `hardened ∪ promotion` union spelled as two mutually exclusive imports).
/// A single token-tree arm: the extra cfg terms ride INSIDE the invocation
/// as ordinary `#[cfg(...)]` attributes, exactly like the positive macro.
///
/// ```text
/// medium_promotion_unreachable! {
///     #[cfg(feature = "hardened")]
///     {
///         ... promotion-off hardened reject branch ...
///     }
/// }
/// ```
macro_rules! medium_promotion_unreachable {
    ($($rest:tt)*) => {
        #[cfg(not(all(
            feature = "medium-classes",
            any(
                not(feature = "exact-span-large"),
                all(feature = "large-reserved-capacity", not(feature = "numa-aware"))
            )
        )))]
        $($rest)*
    };
}

// Path re-exports so other modules can `use` the macros/const by path
// (macro_rules! items are path-nameable only through a `use`). Each is
// cfg-gated to its consumer set, mirroring `alloc_core/mod.rs`'s own gated
// re-export discipline (the const above is re-exported straight from
// `alloc_core/mod.rs`: a same-module `use` of it would clash with its own
// definition): an ungated `pub(crate) use` with no live consumer in
// a build (e.g. `alloc-core`-only, where the `registry` tree does not
// compile) is an unused-import error under `-D warnings`.
pub(crate) use medium_promotion_reachable;
#[cfg(feature = "alloc-global")]
pub(crate) use medium_promotion_unreachable;
