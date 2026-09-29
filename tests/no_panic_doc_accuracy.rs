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
//! #1984 (alloc-core perf review P1-2) demoted the realloc ownership
//! re-check (former site 1) to `debug_assert!`, leaving FOUR release-surviving
//! tripwires. A later canonical-root change replaced that duplicate probe
//! with fallible `canonical_base_of(base)?`. This test pins the current
//! mechanism and the four remaining large-cache tripwires:
//!
//!   * **Code side:** the four remaining distinctive panic-message strings
//!     each appear exactly once in their expected source file, AND the
//!     former realloc site resolves the table-stored canonical root with `?`
//!     before deriving the block pointer or reading its header, and no
//!     release-surviving `assert!(` appears in that file. Removing that
//!     fallible resolution or restoring a panic fails this guard.
//!
//!   * **Doc side:** `sefer_alloc.rs`'s "No-panic" section contains the
//!     qualifying language (`rustc_nounwind`, `invariant tripwire`), states
//!     the FOUR count (not the stale five), names the fallible canonical-root
//!     replacement,
//!     and does NOT contain the old unqualified overclaim. If the section is
//!     rewritten back to "NEVER panics" without the caveat, this fails.
//!
//! R2-08 (task #2010) later found point (3) above wrong: a DIRECT trait call
//! never passes through the `#[rustc_nounwind]` std shims, and even on the
//! `#[global_allocator]` path a pre-R2-08 panic was observed to unwind
//! through `__rust_alloc` rather than abort. The doc now states the
//! normative "`GlobalAlloc` methods must not unwind — upheld at the source"
//! rule instead; `no_panic_doc_is_qualified` also pins that the old
//! "Panic-in-`GlobalAlloc` is abort, not UB" claim stays gone.
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
    // Former site 1 now uses the table's stored root as a fallible boundary:
    // a missing address returns None, and no caller-derived pointer reaches
    // the block/header reads. A release assert remains forbidden here.
    let core = read_src("alloc_core/alloc_core/mem/realloc_fastpath.rs");
    let squashed: String = core.split_whitespace().collect::<Vec<_>>().join(" ");
    let marker = "let base = self.table.canonical_base_of(base)?;";
    assert_count(&squashed, marker, 1, "fallible known-base root resolution");
    let known_base = squashed
        .split_once("pub(super) fn realloc_inplace_fast_path_known_base(")
        .expect("known-base realloc function")
        .1;
    let root = known_base.find(marker).expect("canonical root assignment");
    let block = known_base
        .find("let ptr = crate::alloc_core::node::Node::deref(base,")
        .expect("allocator-root block derivation");
    let kind = known_base
        .find("let kind = SegmentHeader::kind_at(base);")
        .expect("header read");
    assert!(
        root < block && block < kind,
        "canonical root must precede block derivation and header read"
    );
    assert!(
        !squashed.contains(" assert!("),
        "mem/realloc_fastpath.rs must contain no release-surviving `assert!(` — \
         a re-promotion (or a new release assert) regresses the no-panic \
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

    // The shim mechanism must still be named — as what the crate does NOT
    // rely on (R2-08), not as an abort guarantee.
    assert!(
        doc.contains("rustc_nounwind"),
        "sefer_alloc.rs 'No-panic' section must name the #[rustc_nounwind] shims"
    );
    // R2-08: the shim-based "abort, not UB" claim is false for a direct trait
    // call (and was observed false on the #[global_allocator] path too); it
    // must not come back, and the normative no-unwind rule must stay.
    assert!(
        !doc.contains("Panic-in-`GlobalAlloc` is abort, not UB"),
        "sefer_alloc.rs reintroduced the R2-08 overclaim that a panic escaping \
         GlobalAlloc is a guaranteed abort via the #[rustc_nounwind] shims"
    );
    assert!(
        doc.contains("`GlobalAlloc` methods must not unwind"),
        "sefer_alloc.rs must state the normative R2-08 rule that GlobalAlloc \
         methods must not unwind (upheld at the source, not by the std shims)"
    );
    assert!(
        doc.contains("DIRECT trait call"),
        "sefer_alloc.rs must say a direct GlobalAlloc trait call bypasses the \
         std shims (R2-08)"
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

    // Keep the historical demotion and current fallible replacement distinct.
    assert!(
        doc.contains("first demoted to `debug_assert!`"),
        "sefer_alloc.rs must preserve the #1984 demotion history"
    );
    assert!(
        doc.contains("fallible") && doc.contains("`canonical_base_of(base)?`"),
        "sefer_alloc.rs must describe the current no-panic canonical-root \
         resolution rather than a retained debug assertion"
    );
}
