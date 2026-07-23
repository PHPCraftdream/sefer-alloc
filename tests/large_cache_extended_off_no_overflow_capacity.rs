//! R13-7 (task #277) — with `large-cache-extended` OFF, the large-cache
//! behaves EXACTLY as it did before this task: capped at the base 8 slots,
//! no extension sidecar, extra distinct sizes beyond 8 evict the oldest
//! rather than gaining extra headroom.
//!
//! This is the counterfactual companion to
//! `large_cache_extended_materializes_on_overflow.rs`: run the SAME
//! 9-distinct-size deposit sequence, but with the feature compiled out, and
//! confirm the cache tops out at 8 occupied slots (never 9) — proving the
//! feature is what changes the behaviour, not some incidental effect of the
//! sizes chosen.
//!
//! R13-7 follow-up correction (twice over — see the sibling file's module
//! doc for the full story): the size list is computed AT RUNTIME relative
//! to the actual Small/Large boundary in this build (via
//! [`AllocCore::dbg_small_class_count`]/`dbg_block_size`), not a fixed
//! constant list — a fixed list either OOM'd (the original up-to-1-TiB
//! version) or silently misclassified as Small under `--all-features`'s
//! `medium-classes-wide` (the first, still-fixed-list fix).

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    not(feature = "large-cache-extended")
))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

fn layout(bytes: usize) -> Layout {
    Layout::from_size_align(bytes, 8).unwrap()
}

/// Verbatim copy of the sibling file's `large_test_sizes` — see that file's
/// module doc for the full derivation rationale (kept duplicated rather than
/// shared, since integration tests in `tests/` cannot import from one
/// another and this project has no test-support crate for such helpers).
fn large_test_sizes() -> Vec<usize> {
    let segment = SegmentLayout::SEGMENT;
    let small_max_class = AllocCore::dbg_small_class_count() - 1;
    let small_max = AllocCore::dbg_block_size(small_max_class);
    let mut n = (2 * small_max).div_ceil(segment).max(1) * segment;
    let mut sizes = Vec::with_capacity(9);
    for _ in 0..9 {
        sizes.push(n);
        n = (2 * n + 1).div_ceil(segment) * segment;
    }
    sizes
}

#[test]
fn without_extension_feature_cache_stays_capped_at_eight() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None);

    assert_eq!(
        ac.dbg_large_cache_total_slots(),
        8,
        "without `large-cache-extended`, total slots must always be 8"
    );

    let sizes = large_test_sizes();
    let mut ptrs = Vec::with_capacity(sizes.len());
    for &bytes in &sizes {
        let l = layout(bytes);
        let p = ac.alloc(l);
        assert!(
            !p.is_null(),
            "alloc of {bytes} bytes failed -- an OOM here indicates a \
             genuinely memory-starved host"
        );
        ptrs.push((p, l));
    }
    for (p, l) in ptrs {
        // SAFETY (R6-MS-1/2): pointer returned by a prior matching alloc in
        // this test, live, freed exactly once here.
        unsafe { ac.dealloc(p, l) };
    }

    // With no extension, at most 8 of the 9 distinct sizes can be
    // simultaneously resident — the rest were FIFO-evicted (released to the
    // OS) as later deposits displaced earlier ones.
    let occupied = ac
        .dbg_large_cache_slot_sizes()
        .iter()
        .filter(|s| s.is_some())
        .count();
    assert_eq!(
        occupied, 8,
        "cache must be exactly full at 8 slots, never more, with the feature off"
    );
    assert_eq!(
        ac.dbg_large_cache_total_slots(),
        8,
        "total slots must remain 8 after overflow attempts with the feature off"
    );
}
