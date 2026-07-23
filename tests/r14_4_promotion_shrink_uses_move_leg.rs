//! R14-4 (task #289) test (c) — shrink after promotion: a block promoted to
//! Large (by growing past `MEDIUM_REALLOC_PROMOTION_THRESHOLD`), then shrunk
//! back down BELOW the original medium range, must take the EXISTING
//! Large-to-Small move-leg path — i.e. behave exactly like an ordinary
//! Large-to-Small realloc already does today. The design explicitly does
//! NOT add an in-place Large->Small shrink fast path (see
//! `try_promote_to_large`'s doc comment and
//! `docs/perf/R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md` §3
//! "Unchanged, provably" bullet on the shrinking path).
//!
//! Oracle: pointer CHANGES on the shrink. A move-leg always allocates a
//! fresh (smaller-classified) block; an in-place shrink would keep the same
//! pointer. Asserting the pointer differs proves the move path was taken,
//! not some new special-cased in-place shrink this design does not add.
//!
//! Whole file is a no-op without `medium-classes` (see `#![cfg(...)]` below)
//! — run with:
//!   cargo test --release --features "production medium-classes" --test r14_4_promotion_shrink_uses_move_leg

#![cfg(all(feature = "alloc-global", feature = "medium-classes"))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::SeferAlloc;

const ALIGN: usize = 8;
const PROMOTION_THRESHOLD: usize = 256 * 1024;

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, ALIGN).unwrap()
}

#[test]
fn shrink_below_original_medium_range_relocates_and_preserves_prefix() {
    let a = SeferAlloc::new();

    // Start in the medium range, below the threshold.
    let old_size = 64 * 1024;
    let old_layout = layout(old_size);
    // SAFETY: valid, non-zero-size layout.
    let p0 = unsafe { a.alloc(old_layout) };
    assert!(!p0.is_null());
    // SAFETY: p0 valid for old_size bytes.
    unsafe {
        for i in 0..old_size {
            p0.add(i).write((i % 233) as u8);
        }
    }

    // Grow PAST the threshold -> promotes to Large.
    let promote_size = PROMOTION_THRESHOLD + 16 * 1024;
    // SAFETY: p0 live, old_layout matches, freed at most once on success.
    let grown = unsafe { a.realloc(p0, old_layout, promote_size) };
    assert!(!grown.is_null(), "promoting realloc failed");

    // Shrink back down BELOW the original medium range (well under 64 KiB),
    // i.e. into what would ordinarily be an unremarkable Small-class shrink
    // if the block had never been promoted — except this block genuinely IS
    // a Large segment now, so this must go through the ordinary
    // Large->Small move leg.
    let shrink_size = 8 * 1024; // far below the original 64 KiB and the threshold
    let promote_layout = layout(promote_size);
    // SAFETY: grown live, promote_layout matches, freed at most once on success.
    let shrunk = unsafe { a.realloc(grown, promote_layout, shrink_size) };
    assert!(!shrunk.is_null(), "shrink-after-promotion realloc failed");

    // The pointer MUST change: proves the move-leg path (alloc + copy +
    // dealloc), not an in-place shrink this design does not add.
    assert_ne!(
        shrunk, grown,
        "shrink after promotion must relocate (move leg) — an unchanged \
         pointer would mean an in-place Large->Small shrink fast path fired, \
         which this design explicitly does not add"
    );

    // The preserved prefix (min(promote_size, shrink_size) = shrink_size)
    // must match the ORIGINAL canary written before promotion.
    // SAFETY: shrunk valid for shrink_size bytes.
    unsafe {
        for i in 0..shrink_size {
            assert_eq!(
                shrunk.add(i).read(),
                (i % 233) as u8,
                "prefix byte {i} lost or corrupted during shrink-after-promotion"
            );
        }
    }

    let shrunk_layout = layout(shrink_size);
    // SAFETY: shrunk live, shrunk_layout matches, freed exactly once.
    unsafe { a.dealloc(shrunk, shrunk_layout) };
}

/// A shrink that lands BACK inside the medium range (still above the
/// threshold boundary conceptually, but strictly less than the promoted
/// size) also relocates — Large->Small covers this case uniformly too, not
/// just the "way back down to a tiny size" case above.
#[test]
fn shrink_back_into_medium_range_also_relocates() {
    let a = SeferAlloc::new();

    let old_size = 80 * 1024;
    let old_layout = layout(old_size);
    // SAFETY: valid, non-zero-size layout.
    let p0 = unsafe { a.alloc(old_layout) };
    assert!(!p0.is_null());
    // SAFETY: p0 valid for old_size bytes.
    unsafe {
        std::ptr::write_bytes(p0, 0x7E, old_size);
    }

    let promote_size = PROMOTION_THRESHOLD + 128 * 1024; // 384 KiB
                                                         // SAFETY: p0 live, old_layout matches, freed at most once on success.
    let grown = unsafe { a.realloc(p0, old_layout, promote_size) };
    assert!(!grown.is_null());

    // Shrink back to a size still WITHIN the medium-class range (e.g. 300
    // KiB — one of the six exact medium classes), but strictly less than
    // `promote_size` so this is genuinely a shrink.
    let shrink_size = 300 * 1024;
    assert!(shrink_size < promote_size);
    let promote_layout = layout(promote_size);
    // SAFETY: grown live, promote_layout matches, freed at most once on success.
    let shrunk = unsafe { a.realloc(grown, promote_layout, shrink_size) };
    assert!(!shrunk.is_null());

    assert_ne!(
        shrunk, grown,
        "shrink back into the medium range must still relocate (Large->Small \
         move leg) — no in-place shrink fast path exists for this design"
    );

    // SAFETY: shrunk valid for shrink_size bytes.
    unsafe {
        for i in 0..old_size.min(shrink_size) {
            assert_eq!(shrunk.add(i).read(), 0x7E, "byte {i} lost on shrink");
        }
    }

    let shrunk_layout = layout(shrink_size);
    // SAFETY: shrunk live, shrunk_layout matches, freed exactly once.
    unsafe { a.dealloc(shrunk, shrunk_layout) };
}
