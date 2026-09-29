//! Structural tripwire for R5-02 (independent src review round 5,
//! `docs/reviews/2026-09-29-091221-src-review-xxs-sol-round-5.md`):
//! `EpochRegion::_set_slot_generation_for_tests` (and its sole callee,
//! `AtomicSlot::set_generation_for_tests`) must never again be reachable on a
//! plain `experimental` build without `internals`.
//!
//! Before the fix, `_set_slot_generation_for_tests` was a safe `pub fn`
//! reachable under plain `experimental` alone. Achievable without `unsafe`:
//! on a capacity-1 `EpochRegion`, insert a value and keep its handle `h`,
//! remove `h` (slot 0 now vacant), call `_set_slot_generation_for_tests(0,
//! 0)` to roll the vacant slot's generation back to a generation `h` already
//! carries, then insert another value into the same slot — the OLD,
//! already-removed `h` resolves and can delete the NEW value, breaking the
//! documented no-ABA/tombstone guarantee entirely in safe code. Separately,
//! forcing generation `u32::MAX` mints a handle `AtomicSlot::try_evict_at`
//! always rejects, permanently stranding that slot.
//!
//! Doc/source-text guard, same established pattern as
//! `tests/no_stale_doc_references.rs`: reads source text, never links the
//! crate, so it runs in every feature configuration and costs zero extra
//! `cargo` invocations.
//!
//! Counterfactual (verified manually, not by this file, since it must pass
//! unconditionally): restoring the pre-fix source text of
//! `src/concurrent/epoch/epoch_region.rs` /
//! `src/concurrent/epoch/hand.rs` (`git show HEAD~<n>:<file>`, i.e. the
//! state before this task's commit) makes both checks below fail loudly;
//! the current tree makes both pass.

use std::fs;
use std::path::{Path, PathBuf};

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Walk backward from line index `fn_line` (0-based, exclusive) through the
/// contiguous attribute/doc-comment/blank block immediately above it,
/// returning `true` if any REAL `#[cfg(...)]` attribute line (not a `//`/
/// `///`/`//!` comment merely mentioning the string) in that block names
/// `feature = "internals"`. Mirrors
/// `scripts/verify-alloc-core-dbg-internals-exhaustive.mjs`'s
/// `precedingBlockIsInternalsGated` — same walk-back shape, same
/// comment-line-is-never-a-gate rule, so a doc comment describing the gate
/// in prose can never produce a false GREEN.
fn preceding_block_is_internals_gated(lines: &[&str], fn_line: usize) -> bool {
    let mut gated = false;
    let mut j = fn_line;
    while j > 0 {
        j -= 1;
        let trimmed = lines[j].trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("#[") {
            if trimmed.contains("feature = \"internals\"") {
                gated = true;
            }
            continue;
        }
        if trimmed.starts_with("///") || trimmed.starts_with("//!") || trimmed.starts_with("//") {
            continue;
        }
        break;
    }
    gated
}

/// Find the 0-based line index whose trimmed text starts with `needle`, or
/// `None` if absent. Fails loudly (via the caller's `expect`) rather than
/// silently vacuously passing if the anchor itself has gone missing (e.g.
/// renamed or removed) — a missing anchor must not read as "gated".
fn find_line_starting_with(lines: &[&str], needle: &str) -> Option<usize> {
    lines
        .iter()
        .position(|line| line.trim_start().starts_with(needle))
}

#[test]
fn epoch_region_generation_setter_is_internals_gated() {
    let path = src_dir().join("concurrent/epoch/epoch_region.rs");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let lines: Vec<&str> = text.lines().collect();

    let fn_line = find_line_starting_with(&lines, "pub fn _set_slot_generation_for_tests(")
        .unwrap_or_else(|| {
            panic!(
                "anchor `pub fn _set_slot_generation_for_tests(` not found in {} \
             -- the setter was renamed, removed, or reformatted; update this \
             tripwire's anchor rather than treating a missing anchor as gated",
                path.display()
            )
        });

    assert!(
        preceding_block_is_internals_gated(&lines, fn_line),
        "src/concurrent/epoch/epoch_region.rs:{}: \
         `_set_slot_generation_for_tests` must be immediately preceded by \
         `#[cfg(feature = \"internals\")]` (R5-02) -- without it, this safe \
         `pub fn` is reachable on any plain `experimental` build and can \
         hand an old, already-removed handle CAS rights back to a value it \
         never minted (see this file's module doc for the exact safe-code \
         scenario)",
        fn_line + 1,
    );
}

#[test]
fn atomic_slot_generation_setter_is_internals_gated() {
    let path = src_dir().join("concurrent/epoch/hand.rs");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let lines: Vec<&str> = text.lines().collect();

    let fn_line = find_line_starting_with(&lines, "pub(crate) fn set_generation_for_tests(")
        .unwrap_or_else(|| {
            panic!(
                "anchor `pub(crate) fn set_generation_for_tests(` not found in {} \
             -- the setter was renamed, removed, or reformatted; update this \
             tripwire's anchor rather than treating a missing anchor as gated",
                path.display()
            )
        });

    assert!(
        preceding_block_is_internals_gated(&lines, fn_line),
        "src/concurrent/epoch/hand.rs:{}: \
         `AtomicSlot::set_generation_for_tests` (the only callee of \
         `EpochRegion::_set_slot_generation_for_tests`) must be immediately \
         preceded by `#[cfg(feature = \"internals\")]` (R5-02), consistent \
         with its sole caller's gate",
        fn_line + 1,
    );
}
