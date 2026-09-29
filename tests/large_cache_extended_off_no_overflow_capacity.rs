//! R13-7 (task #277) — with `large-cache-extended` OFF, the large-cache
//! behaves EXACTLY as it did before this task: capped at the base 8 slots,
//! no extension sidecar, extra reservations beyond 8 evict the oldest
//! rather than gaining extra headroom.
//!
//! This is the counterfactual companion to
//! `large_cache_extended_materializes_on_overflow.rs`: exercise the same
//! nine-live-reservation deposit sequence, but with the feature compiled out, and
//! confirm the cache tops out at 8 occupied slots (never 9) — proving the
//! feature controls capacity. Holding all allocations live before any free
//! prevents cache reuse without requiring exponentially increasing sizes.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    not(feature = "large-cache-extended"),
    feature = "internals"
))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

fn layout(bytes: usize) -> Layout {
    Layout::from_size_align(bytes, 8).unwrap()
}

fn large_test_sizes() -> Vec<usize> {
    let small_max_class = AllocCore::dbg_small_class_count() - 1;
    let small_max = AllocCore::dbg_block_size(small_max_class);
    vec![small_max + SegmentLayout::PAGE; 9]
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
            "bounded Large allocation of {bytes} bytes failed"
        );
        ptrs.push((p, l));
    }
    for (p, l) in ptrs {
        // SAFETY (R6-MS-1/2): pointer returned by a prior matching alloc in
        // this test, live, freed exactly once here.
        unsafe { ac.dealloc(p, l) };
    }

    // With no extension, at most 8 of the 9 independent reservations can be
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
