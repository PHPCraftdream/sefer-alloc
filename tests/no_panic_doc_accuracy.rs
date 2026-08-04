//! Doc-accuracy regression guard for the "No-panic" contract in
//! `src/global/sefer_alloc.rs` (R34-16, release-stabilization audit F-5).
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
//! This test pins both sides of that resolution so neither silently regresses:
//!
//!   * **Code side:** the five distinctive panic-message strings each appear
//!     exactly once in their expected source file. If a tripwire is removed
//!     or reworded, this fails and forces a conscious doc update (a removed
//!     tripwire may be a real softening the doc must reflect; a reworded one
//!     must stay in lockstep with the doc's enumeration).
//!
//!   * **Doc side:** `sefer_alloc.rs`'s "No-panic" section contains the
//!     qualifying language (`rustc_nounwind`, `invariant tripwire`) and does
//!     NOT contain the old unqualified overclaim. If the section is rewritten
//!     back to "NEVER panics" without the caveat, this fails.
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
fn five_invariant_tripwires_pinned_by_message() {
    // Site 1 — realloc ownership re-check (always compiled, not feature-gated).
    let core = read_src("alloc_core/alloc_core.rs");
    assert_count(
        &core,
        "known-base realloc called for a segment not owned by this core",
        1,
        "alloc_core.rs site 1",
    );

    // Sites 2–5 — large-cache slot take/set helpers (alloc-decommit-gated,
    // in `production`; gated out of some configs, but the source text is
    // feature-independent so this guard still applies).
    let cache = read_src("alloc_core/alloc_core_large_cache.rs");
    assert_count(
        &cache,
        "large_cache_slot_take: empty base slot",
        1,
        "large_cache.rs site 2",
    );
    assert_count(
        &cache,
        "large_cache_slot_take: empty extension slot",
        1,
        "large_cache.rs site 3",
    );
    assert_count(
        &cache,
        "large_cache_slot_take: idx out of base range with extension disabled",
        1,
        "large_cache.rs site 4",
    );
    assert_count(
        &cache,
        "large_cache_slot_set: idx out of base range with extension disabled",
        1,
        "large_cache.rs site 5",
    );
}

#[test]
fn no_panic_doc_is_qualified() {
    let doc = read_src("global/sefer_alloc.rs");

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

    // The five tripwires must be acknowledged as abort-by-design.
    assert!(
        doc.contains("invariant tripwire"),
        "sefer_alloc.rs 'No-panic' section must acknowledge the invariant tripwires"
    );
}
