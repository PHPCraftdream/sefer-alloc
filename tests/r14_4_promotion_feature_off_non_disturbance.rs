//! R14-4 (task #289) test (d) — feature-OFF non-disturbance: without
//! `medium-classes`, the promotion code (`try_promote_to_large` and its
//! `#[cfg(feature = "medium-classes")]`-gated call site in
//! `HeapCore::realloc`, `src/registry/heap_core_free.rs`) compiles out
//! entirely, and realloc behaviour for sizes that WOULD have been medium
//! under that feature is unchanged: without `medium-classes` such sizes
//! already classify Large from the start (there is no medium ladder to
//! divert away from), so growing/shrinking across the "would-be medium"
//! range must behave exactly like ordinary Large realloc always has.
//!
//! This file is the DUAL of the other three `r14_4_*` test files: THEY are
//! gated `#[cfg(feature = "medium-classes")]` (a no-op without it); THIS file
//! is gated the OPPOSITE way — `#[cfg(not(feature = "medium-classes"))]` — so
//! between the two, every feature configuration this crate ships gets
//! coverage from exactly one side. Run with:
//!   cargo test --release --features production --test r14_4_promotion_feature_off_non_disturbance

#![cfg(all(feature = "alloc-global", not(feature = "medium-classes")))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::SeferAlloc;

const ALIGN: usize = 8;

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, ALIGN).unwrap()
}

/// Without `medium-classes`, 256 KiB (the medium-classes promotion
/// threshold) is ALREADY Large (`SMALL_MAX` is ~253 KiB in the base table).
/// Growing across it must behave like ordinary Large realloc: no promotion
/// concept applies because there is nothing to promote FROM (the block was
/// already Large before the grow).
#[test]
fn growth_across_the_would_be_medium_threshold_is_ordinary_large_realloc() {
    let a = SeferAlloc::new();

    let old_size = 260 * 1024; // already Large without medium-classes
    let old_layout = layout(old_size);
    // SAFETY: valid, non-zero-size layout.
    let p = unsafe { a.alloc(old_layout) };
    assert!(!p.is_null());
    // SAFETY: p valid for old_size bytes.
    unsafe {
        for i in 0..old_size {
            p.add(i).write((i % 241) as u8);
        }
    }

    // Grow further, still comfortably within one 4 MiB Large segment's span
    // — this should hit the EXISTING OPT-G in-place grow fast path, exactly
    // as it always has (medium-classes or not, this code path is untouched).
    let new_size = old_size + 512 * 1024;
    // SAFETY: p live, old_layout matches, freed at most once on success.
    let grown = unsafe { a.realloc(p, old_layout, new_size) };
    assert!(!grown.is_null());
    assert_eq!(
        grown, p,
        "growing an already-Large block within its committed span must hit \
         OPT-G in-place, unaffected by medium-classes being off"
    );

    // SAFETY: grown valid for old_size bytes (the preserved prefix).
    unsafe {
        for i in 0..old_size {
            assert_eq!(grown.add(i).read(), (i % 241) as u8, "byte {i} lost");
        }
    }

    let new_layout = layout(new_size);
    // SAFETY: grown live, new_layout matches, freed exactly once.
    unsafe { a.dealloc(grown, new_layout) };
}

/// A genuinely small allocation (well under any medium-range size) growing
/// PAST what would be the 256 KiB promotion threshold under `medium-classes`
/// must still relocate via the ordinary Small->Large move leg — there is no
/// promotion shortcut to take (the `#[cfg(feature = "medium-classes")]` call
/// site in `HeapCore::realloc` is entirely absent from this build), so this
/// is just the pre-existing, unmodified move-leg behaviour.
#[test]
fn small_to_large_growth_without_medium_classes_is_the_ordinary_move_leg() {
    let a = SeferAlloc::new();

    let old_size = 4096;
    let old_layout = layout(old_size);
    // SAFETY: valid, non-zero-size layout.
    let p = unsafe { a.alloc(old_layout) };
    assert!(!p.is_null());
    // SAFETY: p valid for old_size bytes.
    unsafe {
        for i in 0..old_size {
            p.add(i).write((i % 199) as u8);
        }
    }

    // 256 KiB + slop: past the `medium-classes` promotion threshold, but that
    // feature and its promotion code do not exist in this build at all.
    let new_size = 256 * 1024 + 4096;
    // SAFETY: p live, old_layout matches, freed at most once on success.
    let grown = unsafe { a.realloc(p, old_layout, new_size) };
    assert!(!grown.is_null(), "small->large realloc failed");

    // SAFETY: grown valid for old_size bytes (the preserved prefix).
    unsafe {
        for i in 0..old_size {
            assert_eq!(grown.add(i).read(), (i % 199) as u8, "byte {i} lost");
        }
    }

    let new_layout = layout(new_size);
    // SAFETY: grown live, new_layout matches, freed exactly once.
    unsafe { a.dealloc(grown, new_layout) };
}
