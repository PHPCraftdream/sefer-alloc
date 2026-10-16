//! R6-T20 (review R6-CQ-22) — equivalence oracle for the medium→Large
//! promotion-reachability predicate's single source of truth
//! (`src/alloc_core/config/medium_promotion.rs`).
//!
//! After R6-T20 the predicate exists in exactly three irreducible places:
//! the `cfg!` const (`MEDIUM_PROMOTION_REACHABLE`), the `#[cfg]` generator
//! macro (`medium_promotion_reachable!` + its negated sibling), and the
//! hand-written oracle below. Everything else (`HeapCore::try_promote_to_large`,
//! its call site, the counter re-exports, `SegmentHeader::large_size_at`/
//! `large_align_at`, the branch (A)/(B) dealloc gates, ...) is GENERATED
//! from those. These tests pin the three to each other, per build:
//!
//! 1. `reachable_const_matches_hand_written_oracle` — the const side against
//!    the predicate as documented (Cargo.toml feature docs, R15-3). If the
//!    const's `cfg!` copy is edited wrongly, this fails in every CI row
//!    building the diverging polarity.
//! 2. `macro_gate_matches_reachable_const` — the macro side (via the
//!    `dbg_promotion_macro_gate` probe, whose very existence and return
//!    value are macro-generated) against the const side. If the macro's
//!    embedded predicate is edited without the const (or vice versa), this
//!    fails in the polarity where they disagree.
//!
//! Counterfactual (why this is not vacuous): before R6-T20 each of the ~16
//! hand-written copies could drift independently with no signal (the exact
//! failure mode R6-CQ-22 reports); after R6-T20 any drift between the
//! surviving copies flips one side of an `assert_eq!` here in at least one
//! per-PR CI row (`test (--features "hardened medium-classes internals")`,
//! `test (--features "production medium-classes internals")`,
//! `test (--features "production medium-classes exact-span-large internals")`,
//! and the `--all-features` suite row).
//!
//! Whole file is a no-op without `alloc-global` (HeapCore's tree),
//! `medium-classes` (the predicate's meaningful polarity range) or
//! `internals` (the doc-hidden accessors) — same file-gate shape as
//! `tests/r14_4_promotion_move_leg_reduction.rs`.

#![cfg(all(
    feature = "alloc-global",
    feature = "medium-classes",
    feature = "internals"
))]

use sefer_alloc::registry::HeapCore;

/// Independent hand-written oracle: the promotion-reachability predicate
/// EXACTLY as documented at R15-3 (task #305) — `medium-classes` minus the
/// zero-headroom `exact-span-large` exclusion (`large-reserved-capacity`
/// restores the headroom unless `numa-aware` overrides it). Deliberately NOT
/// derived from `src/`: it is the third copy that pins the other two.
const PROMOTION_REACHABLE_ORACLE: bool = cfg!(all(
    feature = "medium-classes",
    any(
        not(feature = "exact-span-large"),
        all(
            feature = "large-reserved-capacity",
            not(feature = "numa-aware")
        )
    )
));

/// The const side of the single source of truth equals the documented
/// predicate in THIS build's configuration.
#[test]
fn reachable_const_matches_hand_written_oracle() {
    assert_eq!(
        HeapCore::dbg_promotion_compiled(),
        PROMOTION_REACHABLE_ORACLE,
        "MEDIUM_PROMOTION_REACHABLE diverged from the documented predicate"
    );
}

/// The macro side (what every `#[cfg]` element gate is generated from)
/// equals the const side in THIS build's configuration. Exactly one
/// `dbg_promotion_macro_gate` definition exists per build (positive or
/// negated macro), and its constant return value is the macro's verdict.
#[test]
fn macro_gate_matches_reachable_const() {
    assert_eq!(
        HeapCore::dbg_promotion_compiled(),
        HeapCore::dbg_promotion_macro_gate(),
        "medium_promotion_reachable!'s embedded predicate diverged from \
         MEDIUM_PROMOTION_REACHABLE"
    );
}
