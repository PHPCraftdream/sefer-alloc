//! R13-7 (task #277) — the `large-cache-extended` sidecar materialises only
//! once the base 8 slots (`LARGE_CACHE_SLOTS`) overflow, and correctly holds
//! entries beyond that point.
//!
//! Counterfactual: without this task's change, depositing a 9th distinct
//! (non-aliasing) Large size would find no free base slot, FIFO-evict the
//! oldest of the first 8, and release ITS reservation to the OS — the 9th
//! size would never coexist with all 8 originals. With the fix, the 9th (and
//! up to the 32nd) distinct size gets a slot in the lazily-materialised
//! extension instead, so all of them can be simultaneously resident.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-decommit",
    feature = "large-cache-extended"
))]

use core::alloc::Layout;
use sefer_alloc::{AllocCore, SegmentLayout};

fn layout(bytes: usize) -> Layout {
    Layout::from_size_align(bytes, 8).unwrap()
}

/// R13-7 follow-up correction, TWICE over (both caught during this task's
/// own zero-trust verification pass, not shipped):
///
/// 1. The ORIGINAL version of this list (`4, 16, 64, ..., 1048576` MiB —
///    i.e. up to a single 1 TiB allocation) made this test VACUOUS on any
///    machine without that much committable address space —
///    `ac.alloc(l)` returns null well before the 12th size, the test hits
///    the graceful `eprintln!(...); return;` bail-out, and reports `ok`
///    having verified NOTHING (confirmed empirically: `cargo test --
///    --nocapture` printed "OOM allocating 262144 MiB" and exited early).
/// 2. The FIRST fix (a fixed 288 KiB-397 MiB list) was itself broken under
///    `--all-features`: that combination also enables `medium-classes-wide`,
///    which raises the Small/Large boundary from ~253 KiB to ~1.75 MiB —
///    three of the ten fixed sizes (288/640/1408 KiB) then classify as
///    SMALL, not Large, and never reach the large-cache at all, so the
///    cache never overflows past 8 and the test fails for real
///    (`extension must materialise once the base 8 slots overflow`).
///
/// This version computes sizes AT RUNTIME relative to
/// [`AllocCore::dbg_small_class_count`]'s actual largest block size (the
/// REAL Small/Large boundary in THIS build, whatever feature combination is
/// active — mirrors the density-agnostic-under-`--all-features` fix task
/// #265/R12-14 already established as this project's convention for this
/// exact class of feature-interaction gap), doubling from there with a
/// strict `> 2x` margin (segment-aligned, so `SEGMENT`-quantized rounding
/// under the non-`exact-span-large` default is a provable no-op — each step
/// is already an exact multiple of `SegmentLayout::SEGMENT`). 9 sizes is the
/// minimum that provably overflows the base 8 by exactly 1.
fn large_test_sizes() -> Vec<usize> {
    let segment = SegmentLayout::SEGMENT;
    let small_max_class = AllocCore::dbg_small_class_count() - 1;
    let small_max = AllocCore::dbg_block_size(small_max_class);
    // Start at the smallest SEGMENT multiple strictly above 2x the actual
    // Small/Large boundary — safely Large under every SMALL_MAX this crate
    // ships (253 KiB default, up to 1.75 MiB under `medium-classes-wide`).
    let mut n = (2 * small_max).div_ceil(segment).max(1) * segment;
    let mut sizes = Vec::with_capacity(9);
    for _ in 0..9 {
        sizes.push(n);
        // Next size: smallest SEGMENT multiple strictly greater than 2x
        // this one — guarantees `next > 2 * n` even after any further
        // SEGMENT-quantization (both are already SEGMENT multiples).
        n = (2 * n + 1).div_ceil(segment) * segment;
    }
    sizes
}

/// Depositing 9 distinct non-aliasing sizes must NOT evict any of the first
/// 8 once the extension is available — all 9 should remain simultaneously
/// cached, and the extension sidecar must report itself materialised.
#[test]
fn overflow_past_base_eight_materialises_extension() {
    let mut ac = AllocCore::new().expect("primordial");
    ac.dbg_set_large_cache_budget(None); // isolate slot-count effect from budget

    assert!(
        !ac.dbg_large_cache_extension_materialised(),
        "extension must start unmaterialised"
    );
    assert_eq!(
        ac.dbg_large_cache_total_slots(),
        8,
        "total slots must be 8 before any overflow"
    );

    let sizes = large_test_sizes();
    let mut ptrs = Vec::with_capacity(sizes.len());
    for &bytes in &sizes {
        let l = layout(bytes);
        let p = ac.alloc(l);
        assert!(
            !p.is_null(),
            "alloc of {bytes} bytes failed -- an OOM here indicates a \
             genuinely memory-starved host, not the expected/tolerated case \
             this test's predecessor silently swallowed"
        );
        ptrs.push((p, l));
    }
    // Dealloc all 9 — each should deposit into a DISTINCT slot (sizes are
    // pairwise > 2x apart, so none is best-fit-compatible with another).
    for (p, l) in ptrs {
        // SAFETY (R6-MS-1/2): pointer returned by a prior matching alloc in
        // this test, live, freed exactly once here.
        unsafe { ac.dealloc(p, l) };
    }

    assert!(
        ac.dbg_large_cache_extension_materialised(),
        "extension must materialise once the base 8 slots overflow"
    );
    assert_eq!(
        ac.dbg_large_cache_total_slots(),
        40,
        "total slots must be 8 + 32 = 40 once the extension is materialised"
    );

    // All 9 distinct sizes must have found a home — the base 8 slots plus 1
    // in the extension.
    let base_occupied = ac
        .dbg_large_cache_slot_sizes()
        .iter()
        .filter(|s| s.is_some())
        .count();
    let ext_occupied = ac
        .dbg_large_cache_extended_slot_sizes()
        .iter()
        .filter(|s| s.is_some())
        .count();
    assert_eq!(
        base_occupied + ext_occupied,
        sizes.len(),
        "all {} distinct sizes must be simultaneously cached (base={base_occupied}, ext={ext_occupied})",
        sizes.len()
    );
    assert_eq!(base_occupied, 8, "all 8 base slots should be full");
    assert_eq!(
        ext_occupied,
        sizes.len() - 8,
        "the remaining {} sizes should have landed in the extension",
        sizes.len() - 8
    );

    // Every one of the 9 sizes must now be servable as a cache HIT (no OS
    // round-trip): re-alloc each and confirm a non-null pointer comes back
    // and the cache's total occupied count decreases by one per hit.
    for &bytes in &sizes {
        let l = layout(bytes);
        let p = ac.alloc(l);
        assert!(
            !p.is_null(),
            "re-alloc of {bytes} bytes must succeed (was cached)"
        );
        // SAFETY: freshly returned live pointer from this same call, freed
        // immediately to keep the cache populated for the next iteration's
        // hit-path check.
        unsafe { ac.dealloc(p, l) };
    }
}
