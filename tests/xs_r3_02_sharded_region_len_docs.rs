#![cfg(feature = "experimental")]
//! Doc-only regression guard for review finding P3-1
//! (`docs/reviews/2026-09-28-232143-src-review-xs-sol-round-3.md`).
//!
//! `ShardedRegion::len`/`is_empty` sum/scan per-shard `AtomicUsize` counters
//! sequentially — not a cross-shard snapshot. With >= 2 shards a concurrent
//! cross-shard move (insert into shard A, then remove from shard B) can make
//! `is_empty()` report `true` (or `len()` report `0`) although at least one
//! entry was live at every instant. Before this task the doc comments called
//! the result a "momentary observation" and I4's module-doc bullet claimed
//! `len` is "correct under concurrent remote removal" — both readable as an
//! exactness/linearizability guarantee that does not hold. This test pins the
//! corrected wording (exact only absent concurrent mutation; approximate and
//! non-linearizable under concurrent insert/remove; not a drain-complete/
//! shutdown signal) and bans the old unqualified claims from reappearing.
//!
//! Source-text style, following `tests/no_stale_doc_references.rs`: reads
//! `src/concurrent/sharded/sharded_region.rs` at a compile-time-known path
//! and asserts on its text directly — no crate linking, so gating this behind
//! `experimental` only mirrors the sibling `tests/sharded.rs`/
//! `tests/sharded_remote.rs` convention for the file the doc lives in, not a
//! real compile dependency.
//!
//! Counterfactual (documented, not re-run by the test itself): restoring the
//! pre-fix text via `git show <pre-fix-rev>:src/concurrent/sharded/sharded_region.rs`
//! makes `new_wording_present_on_both_methods` and
//! `old_unqualified_claims_are_gone` fail RED; the current worktree content
//! makes both pass GREEN.

use std::fs;
use std::path::PathBuf;

fn sharded_region_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("concurrent")
        .join("sharded")
        .join("sharded_region.rs");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// Returns the doc-comment block (contiguous `///` lines) immediately above
/// the first line containing `anchor` (e.g. `pub fn len(`), skipping any
/// attribute lines (e.g. `#[must_use]`) directly between the doc block and
/// the anchor. Panics if the anchor or a preceding doc block is not found, so
/// an anchor rename fails loudly instead of silently vacuously passing.
fn doc_block_above(text: &str, anchor: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let anchor_idx = lines
        .iter()
        .position(|l| l.contains(anchor))
        .unwrap_or_else(|| panic!("anchor `{anchor}` not found in sharded_region.rs"));
    let mut cursor = anchor_idx;
    while cursor > 0 && lines[cursor - 1].trim_start().starts_with('#') {
        cursor -= 1;
    }
    let mut start = cursor;
    while start > 0 && lines[start - 1].trim_start().starts_with("///") {
        start -= 1;
    }
    assert!(
        start < cursor,
        "no doc comment found directly above anchor `{anchor}` (skipping attributes)"
    );
    lines[start..cursor].join("\n")
}

#[test]
fn new_wording_present_on_both_methods() {
    let text = sharded_region_source();

    let len_doc = doc_block_above(&text, "pub fn len(&self) -> usize {");
    let is_empty_doc = doc_block_above(&text, "pub fn is_empty(&self) -> bool {");

    for (name, doc) in [("len", &len_doc), ("is_empty", &is_empty_doc)] {
        assert!(
            doc.contains("Exact only when no concurrent mutation is in flight"),
            "`{name}`'s doc comment must state exactness holds only absent \
             concurrent mutation:\n{doc}"
        );
        assert!(
            doc.contains("approximate, non-linearizable observation"),
            "`{name}`'s doc comment must call the concurrent-mutation result \
             an approximate, non-linearizable observation:\n{doc}"
        );
        assert!(
            doc.to_lowercase().contains("drain-complete")
                || doc.to_lowercase().contains("shutdown signal"),
            "`{name}`'s doc comment must warn against use as a drain-complete/\
             shutdown signal:\n{doc}"
        );
    }

    // The module-doc I4 bullet gets the same treatment.
    assert!(
        text.contains("I4 — accounting:")
            && text.contains("approximate, non-linearizable observation"),
        "the I4 module-doc bullet must also state the approximate/\
         non-linearizable qualification"
    );
}

#[test]
fn old_unqualified_claims_are_gone() {
    let text = sharded_region_source();

    // Pre-fix `len`'s doc said only "Under concurrency this is a momentary
    // observation." with no exactness/non-linearizability qualifier anywhere
    // else in the same doc block — that exact unqualified sentence must not
    // reappear.
    assert!(
        !text.contains("Under concurrency this is a momentary observation."),
        "the old unqualified 'momentary observation' claim on `len` \
         reappeared — it must carry the approximate/non-linearizable \
         qualification instead"
    );

    // Pre-fix I4 module-doc bullet's sole claim: `len` is "correct under
    // concurrent remote removal" — presented with no non-linearizability
    // caveat. That exact phrase must not reappear in the I4 bullet.
    assert!(
        !text.contains("correct under concurrent remote removal"),
        "the old I4 bullet's sole claim ('correct under concurrent remote \
         removal', with no non-linearizability caveat) reappeared"
    );
}
