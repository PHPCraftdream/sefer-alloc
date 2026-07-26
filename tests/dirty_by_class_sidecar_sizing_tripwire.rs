//! v4 tripwire (task R19-9, task #345; design doc `docs/DESIGN_stale_literal_guard.md`,
//! task #334/R18-6) for the derived-literal-goes-stale defect class this
//! project has already hit 4 times (R15-5, R16-2, R17-5, R17-6): a comment
//! that restates a number computed from named constants instead of citing the
//! constants/formula, silently going stale when one of the underlying
//! constants changes.
//!
//! This asserts the `class-aware-dirty` per-(segment,class) dirty sidecar's
//! byte footprint — re-derived HERE from the REAL compiled constants via
//! [`AllocCore::dbg_small_class_count`] / [`AllocCore::dbg_words_per_class`],
//! never hardcoded from reading `size_classes.rs`/`segment_table.rs` by eye —
//! still matches the illustrative snapshot numbers
//! `src/alloc_core/dirty_by_class.rs`'s module doc comment ("## Sizing and
//! lazy materialisation" section) cites.
//!
//! If a future change to `MAX_SEGMENTS`, `SMALL_CLASS_COUNT` (i.e. any edit to
//! `size_classes.rs`'s `SIZE_CLASS_TABLE`/`EXTRAS`), or `WORDS_PER_CLASS`'s
//! formula makes one of the `EXPECTED_BYTES_*` constants below fail, update
//! BOTH this test's constant AND `dirty_by_class.rs`'s doc comment snapshot in
//! the same change — that is the whole point of this tripwire.
//!
//! Gated on `alloc-segment-directory` — the same gate
//! [`AllocCore::dbg_words_per_class`] itself carries (the sidecar is
//! meaningless without the segment-directory feature compiled in).

#![cfg(feature = "alloc-segment-directory")]

use sefer_alloc::AllocCore;

/// Must match `dirty_by_class.rs`'s doc comment's "default 49-class table"
/// snapshot: `SMALL_CLASS_COUNT (49) * WORDS_PER_CLASS (64) * 8 bytes`.
#[cfg(not(feature = "medium-classes"))]
const EXPECTED_BYTES: usize = 25_088;

/// Must match the doc comment's "55 classes under `medium-classes`" snapshot.
#[cfg(all(feature = "medium-classes", not(feature = "medium-classes-wide")))]
const EXPECTED_BYTES: usize = 28_160;

/// Must match the doc comment's "58 classes under `medium-classes-wide`"
/// snapshot. (`medium-classes-wide` requires `medium-classes` in
/// `Cargo.toml`, so this arm always wins over the plain `medium-classes` one
/// above when both are active.)
#[cfg(feature = "medium-classes-wide")]
const EXPECTED_BYTES: usize = 29_696;

/// Recomputes the sidecar's byte footprint from the REAL compiled constants
/// (never a hardcoded byte count) and checks it against the doc comment's
/// current illustrative snapshot for THIS feature combination.
#[test]
fn sidecar_byte_footprint_matches_doc_comment_snapshot() {
    let words_per_heap = AllocCore::dbg_small_class_count() * AllocCore::dbg_words_per_class();
    let bytes_per_heap = words_per_heap * 8;
    assert_eq!(
        bytes_per_heap,
        EXPECTED_BYTES,
        "PerClassDirty sidecar footprint drifted from dirty_by_class.rs's \
         doc comment snapshot ({EXPECTED_BYTES} bytes) to {bytes_per_heap} \
         bytes (small_class_count={}, words_per_class={}) — update BOTH this \
         test's EXPECTED_BYTES constant and the \"## Sizing and lazy \
         materialisation\" doc comment in src/alloc_core/dirty_by_class.rs",
        AllocCore::dbg_small_class_count(),
        AllocCore::dbg_words_per_class(),
    );
}
