//! Regression guard for review finding P4-1
//! (`docs/reviews/2026-09-28-232143-src-review-xs-sol-round-3.md`):
//! `HeapCore::dealloc`'s summary claimed "Foreign pointers (not a sefer
//! segment) are a safe no-op", contradicting its own `# Safety` section a few
//! lines below, which excludes foreign/unmapped/already-released pointers
//! from the contract. Under `alloc-xthread`,
//! `src/registry/heap_core_xthread/routing.rs`'s `dealloc_foreign_routing`
//! reaches `SegmentHeader::magic_at(base)`, which reads the address and can
//! fault if it is unmapped — so the blanket "safe no-op" claim was false for
//! an out-of-contract caller, not just imprecise.
//!
//! This test pins two things in `src/registry/heap_core/free/dealloc.rs`:
//! the old blanket-claim phrase is gone, and the corrected wording (null is
//! the only always-safe no-op; a foreign/unmapped pointer violates the
//! contract) is present.
//!
//! Doc-only guard: it reads source text, never links against the crate, so it
//! runs in every feature configuration.

use std::fs;
use std::path::Path;

fn dealloc_rs_text() -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("registry")
        .join("heap_core")
        .join("free")
        .join("dealloc.rs");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn old_blanket_foreign_no_op_claim_is_gone() {
    let text = dealloc_rs_text();
    assert!(
        !text.contains("Foreign pointers (not a sefer segment) are a safe"),
        "P4-1 regression: `HeapCore::dealloc`'s summary reintroduced the \
         blanket claim that foreign pointers are a safe no-op, contradicting \
         its own `# Safety` section (which excludes foreign/unmapped \
         pointers from the contract)."
    );
}

#[test]
fn summary_states_null_only_safe_no_op_contract() {
    let text = dealloc_rs_text();
    assert!(
        text.contains("Only a **null** `ptr` is always a safe no-op"),
        "P4-1 fix: `HeapCore::dealloc`'s summary must state that null is the \
         only always-safe no-op, matching the `# Safety` section below it."
    );
    assert!(
        text.contains("foreign, unmapped, or already-released `ptr` violates"),
        "P4-1 fix: `HeapCore::dealloc`'s summary must state that a foreign/ \
         unmapped/already-released `ptr` violates the contract, not that it \
         is a safe no-op."
    );
}
