//! R14-4 (task #289) test (c) — shrink after promotion: a block grown past
//! `MEDIUM_REALLOC_PROMOTION_THRESHOLD`, then shrunk back down, must take the
//! EXISTING move-leg path (alloc a fresh, smaller-classified block, copy,
//! free the old one) — never an in-place shrink. The design explicitly does
//! NOT add an in-place shrink fast path for a promoted block (see
//! `try_promote_to_large`'s doc comment and
//! `docs/perf/R11_3_REALLOC_SMALL_TO_LARGE_PROMOTION_DESIGN.md` §3
//! "Unchanged, provably" bullet on the shrinking path).
//!
//! Oracle: pointer CHANGES on the shrink. A move-leg always allocates a
//! fresh (smaller-classified) block; an in-place shrink would keep the same
//! pointer. Asserting the pointer differs proves the move path was taken,
//! not some new special-cased in-place shrink this design does not add.
//!
//! ## `HAS_PROMOTION` (R16-1, task #311, P2-1 — review finding on R15-3)
//!
//! R15-3 (task #305) gated `try_promote_to_large` itself behind an extended
//! `#[cfg]` predicate (see `tests/r14_4_promotion_move_leg_reduction.rs`'s
//! module doc for the full derivation, mirrored here as `HAS_PROMOTION`).
//! When `HAS_PROMOTION` is `false`, the growing step below that used to
//! "promote to Large" instead falls through to the ordinary medium-ladder
//! move-leg — under plain `medium-classes` (not `-wide`), `SMALL_MAX` is
//! 1 MiB, and both this file's `promote_size` values (272 KiB, 384 KiB) sit
//! well under that ceiling, so the grown block in that configuration is
//! genuinely still medium-classified, not Large, going into the shrink step.
//!
//! This does NOT make the shrink assertion vacuous either way: whichever
//! kind the grown block is (Large under `HAS_PROMOTION`, still-medium
//! otherwise), the subsequent shrink lands in a strictly smaller size class
//! than the grown block occupies, so `try_realloc_inplace_known_base`'s
//! same-class check (OPT-F for Small/medium, OPT-G growth-only for Large)
//! cannot fire either way and the move-leg is the only path — `assert_ne!`
//! genuinely distinguishes "moved" from "stayed in place" under both
//! configurations. `HAS_PROMOTION` is kept here (rather than omitted) only
//! to make that reasoning explicit and keep the doc comments accurate about
//! WHICH kind (Large vs. medium) the pre-shrink block actually is.
//!
//! Whole file is a no-op without `medium-classes` (see `#![cfg(...)]` below)
//! — run with:
//!   cargo test --release --features "production medium-classes" --test r14_4_promotion_shrink_uses_move_leg
//!   cargo test --release --features "production medium-classes exact-span-large" --test r14_4_promotion_shrink_uses_move_leg

#![cfg(all(feature = "alloc-global", feature = "medium-classes"))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::SeferAlloc;

const ALIGN: usize = 8;
const PROMOTION_THRESHOLD: usize = 256 * 1024;

/// Mirrors `tests/r14_4_promotion_move_leg_reduction.rs`'s constant of the
/// same name byte-for-byte (kept in sync manually — see that file's doc for
/// the full derivation). `true` iff `try_promote_to_large` is compiled in
/// for this build, i.e. the grow step below actually promotes to Large
/// rather than staying on the medium ladder.
const HAS_PROMOTION: bool = !cfg!(feature = "exact-span-large")
    || (cfg!(feature = "large-reserved-capacity") && !cfg!(feature = "numa-aware"));

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

    // Grow PAST the threshold. When `HAS_PROMOTION` this promotes to Large;
    // otherwise (R15-3's zero-headroom exclusion) it stays medium-classified
    // (272 KiB < `SMALL_MAX` == 1 MiB under plain `medium-classes`) and takes
    // the ordinary cross-class medium-ladder move-leg instead — either way
    // the block ends up in a different, larger size class than `old_size`.
    let promote_size = PROMOTION_THRESHOLD + 16 * 1024;
    // SAFETY: p0 live, old_layout matches, freed at most once on success.
    let grown = unsafe { a.realloc(p0, old_layout, promote_size) };
    assert!(!grown.is_null(), "promoting realloc failed");

    // Shrink back down BELOW the original medium range (well under 64 KiB),
    // i.e. into what would ordinarily be an unremarkable Small-class shrink
    // if the block had never been grown — except this block is now in a
    // strictly larger size class (Large under `HAS_PROMOTION`, a bigger
    // medium class otherwise — see `HAS_PROMOTION`'s doc above), so this
    // must go through the ordinary move leg (Large->Small OPT-G's growth-only
    // check can't fire on a shrink; medium OPT-F requires the SAME class,
    // which this shrink is not) rather than any in-place fast path.
    let shrink_size = 8 * 1024; // far below the original 64 KiB and the threshold
    let promote_layout = layout(promote_size);
    // SAFETY: grown live, promote_layout matches, freed at most once on success.
    let shrunk = unsafe { a.realloc(grown, promote_layout, shrink_size) };
    assert!(!shrunk.is_null(), "shrink-after-promotion realloc failed");

    // The pointer MUST change: proves the move-leg path (alloc + copy +
    // dealloc), not an in-place shrink this design does not add — true
    // whether the pre-shrink block is Large (`HAS_PROMOTION`) or still
    // medium-classified (`!HAS_PROMOTION`, see module doc).
    assert_ne!(
        shrunk, grown,
        "shrink after growth must relocate (move leg) — an unchanged \
         pointer would mean an in-place shrink fast path fired, \
         which this design explicitly does not add (HAS_PROMOTION={HAS_PROMOTION})"
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
/// threshold boundary conceptually, but strictly less than the grown size)
/// also relocates — covers this case uniformly too (whether the pre-shrink
/// block is Large or still medium-classified, see `HAS_PROMOTION`'s doc
/// above), not just the "way back down to a tiny size" case above.
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

    // 384 KiB: promotes to Large under `HAS_PROMOTION`; otherwise stays
    // medium-classified (still < `SMALL_MAX` == 1 MiB) but in a strictly
    // larger class than `old_size`'s.
    let promote_size = PROMOTION_THRESHOLD + 128 * 1024; // 384 KiB
                                                         // SAFETY: p0 live, old_layout matches, freed at most once on success.
    let grown = unsafe { a.realloc(p0, old_layout, promote_size) };
    assert!(!grown.is_null());

    // Shrink back to a size still WITHIN the medium-class range (e.g. 300
    // KiB — one of the six exact medium classes), but strictly less than
    // `promote_size` so this is genuinely a shrink, and in a different medium
    // class than `promote_size` either way.
    let shrink_size = 300 * 1024;
    assert!(shrink_size < promote_size);
    let promote_layout = layout(promote_size);
    // SAFETY: grown live, promote_layout matches, freed at most once on success.
    let shrunk = unsafe { a.realloc(grown, promote_layout, shrink_size) };
    assert!(!shrunk.is_null());

    assert_ne!(
        shrunk, grown,
        "shrink back into the medium range must still relocate (move leg) — \
         no in-place shrink fast path exists for this design \
         (HAS_PROMOTION={HAS_PROMOTION})"
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
