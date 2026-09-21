//! Doc-accuracy regression guard for the "No-panic" contract in
//! `src/global/sefer_alloc/mod.rs` (R34-16, release-stabilization audit F-5).
//!
//! F-5 found a divergence between the module doc's claim "Every entry point
//! here returns null on failure and NEVER panics" and the code: five
//! release-surviving (not `debug_assert!`) invariant checks are reachable
//! from the `GlobalAlloc` impl under `production`. R34-16 resolved F-5 the
//! low-risk way (option (b)): the doc was rewritten to (1) keep the accurate
//! failure-path bullets, (2) enumerate the five tripwires as "abort by
//! design" defence-in-depth, and (3) state explicitly that a panic escaping
//! `GlobalAlloc` aborts via `#[rustc_nounwind]` (not UB), independent of any
//! downstream `panic = "abort"` setting.
//!
//! #1984 (alloc-core perf review P1-2) later demoted the realloc ownership
//! re-check (former site 1) to `debug_assert!`, leaving FOUR
//! release-surviving tripwires (the large-cache slot take/set sites). This
//! test pins both sides of that resolution so neither silently regresses:
//!
//!   * **Code side:** the four remaining distinctive panic-message strings
//!     each appear exactly once in their expected source file, AND the
//!     demoted realloc site stays demoted (its message survives, attached to
//!     a `debug_assert!`, and no release-surviving `assert!(` remains in its
//!     file — the file's only one WAS that site). If a tripwire is removed,
//!     reworded, or re-promoted to a release assert, this fails and forces a
//!     conscious doc update (a removed tripwire may be a real softening the
//!     doc must reflect; a reworded one must stay in lockstep with the doc's
//!     enumeration).
//!
//!   * **Doc side:** `sefer_alloc.rs`'s "No-panic" section contains the
//!     qualifying language (`rustc_nounwind`, `invariant tripwire`), states
//!     the FOUR count (not the stale five), keeps the #1984 demotion note,
//!     and does NOT contain the old unqualified overclaim. If the section is
//!     rewritten back to "NEVER panics" without the caveat, this fails.
//!
//! Doc/source-text only: never links against the crate, so it runs in every
//! feature configuration (mirrors `tests/no_stale_doc_references.rs`).

use std::fs;
use std::path::PathBuf;

fn src_path(rel: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join(rel)
}

fn read_src(rel: &str) -> String {
    fs::read_to_string(src_path(rel)).unwrap_or_else(|e| panic!("read src/{rel}: {e}"))
}

/// Assert `needle` occurs exactly `expected` times in `haystack`.
fn assert_count(haystack: &str, needle: &str, expected: usize, ctx: &str) {
    let actual = haystack.matches(needle).count();
    assert_eq!(
        actual, expected,
        "{ctx}: expected {expected} occurrence(s) of {needle:?}, found {actual}"
    );
}

#[test]
fn four_invariant_tripwires_pinned_by_message() {
    // Former site 1 — the realloc ownership re-check, demoted to
    // `debug_assert!` by #1984 (P1-2): both callers already prove
    // `contains_base(base)` on the same path, and a release panic on the
    // alloc path violated the no-panic contract. Pinned three ways so the
    // demotion cannot silently regress in EITHER direction:
    //   * the check still exists, on the same predicate, still carrying its
    //     distinctive message exactly once (guards outright deletion);
    //   * it is attached to a `debug_assert!` (debug-only, the F12
    //     falsification-pin style);
    //   * NO release-surviving `assert!(` remains in the file — the file's
    //     only one WAS this site, so this is the exact inverse of the old
    //     pin, not a weaker cousin.
    let core = read_src("alloc_core/alloc_core/mem/realloc_fastpath.rs");
    assert_count(
        &core,
        "known-base realloc called for a segment not owned by this core",
        1,
        "mem/realloc_fastpath.rs former site 1 (message must survive the #1984 demotion)",
    );
    // Whitespace-normalized structural pins (robust to reflow/rustfmt).
    let squashed: String = core.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        squashed.contains("debug_assert!( self.table.contains_base_ro(base),"),
        "mem/realloc_fastpath.rs's realloc ownership re-check must stay a \
         `debug_assert!` on `contains_base_ro` (#1984 demotion regressed)"
    );
    assert!(
        !squashed.contains(" assert!("),
        "mem/realloc_fastpath.rs must contain no release-surviving `assert!(` — \
         its only one WAS the realloc ownership re-check, demoted by #1984; a \
         re-promotion (or a new release assert) regresses the no-panic \
         contract this guard pins"
    );

    // Sites 1–4 (renumbered from 2–5 after #1984) — large-cache slot take/set
    // helpers (alloc-decommit-gated, in `production`; gated out of some
    // configs, but the source text is feature-independent so this guard
    // still applies).
    let cache = read_src("alloc_core/large/alloc_core_large_cache.rs");
    assert_count(
        &cache,
        "large_cache_slot_take: empty base slot",
        1,
        "large_cache.rs site 1",
    );
    assert_count(
        &cache,
        "large_cache_slot_take: empty extension slot",
        1,
        "large_cache.rs site 2",
    );
    assert_count(
        &cache,
        "large_cache_slot_take: idx out of base range with extension disabled",
        1,
        "large_cache.rs site 3",
    );
    assert_count(
        &cache,
        "large_cache_slot_set: idx out of base range with extension disabled",
        1,
        "large_cache.rs site 4",
    );
}

#[test]
fn no_panic_doc_is_qualified() {
    let doc = read_src("global/sefer_alloc/mod.rs");

    // The old unqualified overclaim must be gone.
    assert!(
        !doc.contains("returns null on failure and NEVER panics"),
        "sefer_alloc.rs still carries the unqualified 'NEVER panics' overclaim (F-5 regression)"
    );

    // The panic=abort caveat must name the mechanism explicitly.
    assert!(
        doc.contains("rustc_nounwind"),
        "sefer_alloc.rs 'No-panic' section must state the #[rustc_nounwind] abort guarantee"
    );

    // The remaining four tripwires must be acknowledged as abort-by-design.
    assert!(
        doc.contains("invariant tripwire"),
        "sefer_alloc.rs 'No-panic' section must acknowledge the invariant tripwires"
    );

    // The enumeration count must stay in lockstep with the code: #1984
    // demoted the realloc site (FIVE → FOUR release-surviving tripwires), so
    // the doc must state FOUR and must not carry the stale FIVE heading.
    assert_count(
        &doc,
        "Four release-surviving invariant tripwires",
        1,
        "sefer_alloc.rs tripwire enumeration heading",
    );
    assert!(
        !doc.contains("Five release-surviving"),
        "sefer_alloc.rs still claims FIVE release-surviving tripwires — a \
         stale count (the realloc site was demoted to `debug_assert!` by \
         #1984)"
    );

    // The #1984 demotion note must stay: the demoted site's message is
    // pinned above; this keeps its doc mention from silently vanishing.
    assert!(
        doc.contains("demoted to `debug_assert!`"),
        "sefer_alloc.rs must keep the #1984 note recording the realloc \
         ownership re-check's demotion to `debug_assert!`"
    );
}
