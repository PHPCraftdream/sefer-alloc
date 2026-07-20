//! R8-6 (task #219) — correctness tests for the split between the TIGHT
//! payload/metadata boundary (`small_meta_end`) and the runtime
//! real-OS-page-aligned decommit boundary (`small_decommit_start`).
//!
//! Two load-bearing assertions:
//!
//! 1. **Payload recovery** (Step 5.1): on a 4 KiB-page system, the tight
//!    `small_meta_end` is STRICTLY SMALLER than the old #205 over-aligned
//!    value (`align_up(small_meta_end, MAX_REALISTIC_PAGE_SIZE)`), proving the
//!    recovered payload this task delivers. Gated on the runtime page-size
//!    query.
//!
//! 2. **Decommit → recommit round-trip** (Step 5.2): a small segment reaches
//!    `live_count == 0`, decommits, then is reallocated — a write/read near
//!    the START of the payload (at the tight `small_meta_end` offset) must
//!    succeed. This is the test that would catch a wrong decommit-boundary
//!    computation: if `small_decommit_start` were too small, decommitting
//!    could clip into live metadata; if the recommit didn't bring back the
//!    right range, the first write after reuse would fault on a
//!    still-decommitted page.
//!
//! Reuses the production pattern from `tests/decommit_miri_cycle.rs` (pool
//! disabled so decommit fires deterministically), sized for miri.

#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]

use core::alloc::Layout;

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::SegmentLayout;
use sefer_alloc::{LargeCacheConfig, SmallSegmentPoolConfig};

/// Round `n` up to the next multiple of power-of-two `a`.
fn align_up_pow2(n: usize, a: usize) -> usize {
    (n + a - 1) & !(a - 1)
}

// ── Step 5.1: payload recovery ────────────────────────────────────────────

/// On a 4 KiB-page system, the tight `SMALL_META_END` is strictly smaller than
/// the value #205's `MAX_REALISTIC_PAGE_SIZE` over-alignment produced — proving
/// the recovered payload. On a 16/64 KiB-page system the decommit boundary
/// rounds up but the tight boundary stays at 4 KiB alignment; this test only
/// asserts the recovery claim where it holds (4 KiB pages).
#[test]
fn small_meta_end_recovers_payload_vs_old_over_alignment() {
    let tight = SegmentLayout::SMALL_META_END;
    // The tight boundary is PAGE-aligned (4 KiB) — the basic invariant.
    assert_eq!(
        tight % SegmentLayout::PAGE,
        0,
        "SMALL_META_END ({tight}) must be PAGE-aligned (the tight boundary invariant)"
    );
    // The value #205's over-alignment would have produced from this same
    // layout: `align_up(pre_runstack, MAX_REALISTIC_PAGE_SIZE)` is identical to
    // `align_up(align_up(pre_runstack, PAGE), MAX_REALISTIC_PAGE_SIZE)` because
    // MAX_REALISTIC_PAGE_SIZE is a multiple of PAGE — so this reconstruction is
    // exact.
    let old_over_aligned = align_up_pow2(tight, MAX_REALISTIC_PAGE_SIZE);
    // On a 4 KiB-page system, the tight value must be strictly smaller than the
    // over-aligned value — this IS the payload recovery. (On a hypothetical
    // layout that naturally lands on a 64 KiB boundary the two would be equal
    // and there would be nothing to recover; that has never been the case for
    // this layout — the non-hardened value is ~72 KiB, well between 64 KiB and
    // 128 KiB.)
    let real_page = aligned_vmem::page_size();
    if real_page == SegmentLayout::PAGE {
        assert!(
            tight < old_over_aligned,
            "SMALL_META_END ({tight}) must be strictly smaller than the old #205 over-aligned \
             value ({old_over_aligned}) on a 4 KiB-page system — otherwise no payload was recovered"
        );
        let recovered = old_over_aligned - tight;
        eprintln!(
            "R8-6 payload recovery: SMALL_META_END tight={tight}, old_over_aligned={old_over_aligned}, \
             recovered={recovered} bytes/segment ({:.1} KiB)",
            recovered as f64 / 1024.0
        );
    }
    // The decommit boundary is always <= the old over-aligned value (it rounds
    // to the REAL page, which is <= MAX_REALISTIC_PAGE_SIZE).
    let decommit = SegmentLayout::small_decommit_start();
    assert!(
        decommit <= old_over_aligned,
        "small_decommit_start ({decommit}) must be <= old over-aligned value ({old_over_aligned})"
    );
}

/// Same recovery assertion for the primordial boundary.
#[test]
fn primordial_meta_end_recovers_payload_vs_old_over_alignment() {
    let tight = SegmentLayout::PRIMORDIAL_META_END;
    assert_eq!(
        tight % SegmentLayout::PAGE,
        0,
        "PRIMORDIAL_META_END ({tight}) must be PAGE-aligned"
    );
    assert!(
        tight >= SegmentLayout::SMALL_META_END,
        "PRIMORDIAL_META_END ({tight}) must be >= SMALL_META_END ({})",
        SegmentLayout::SMALL_META_END
    );
    let old_over_aligned = align_up_pow2(tight, MAX_REALISTIC_PAGE_SIZE);
    let real_page = aligned_vmem::page_size();
    if real_page == SegmentLayout::PAGE {
        assert!(
            tight < old_over_aligned,
            "PRIMORDIAL_META_END ({tight}) must be strictly smaller than old over-aligned ({old_over_aligned})"
        );
    }
}

/// The conservative compile-time upper bound on real OS page sizes (see
/// `os::MAX_REALISTIC_PAGE_SIZE`). Hardcoded here (not imported) so the test
/// cross-checks the *value*, not the constant.
const MAX_REALISTIC_PAGE_SIZE: usize = 1 << 16;

// ── Step 5.2: decommit → recommit round-trip ─────────────────────────────

/// The segment size constant (4 MiB).
const SEGMENT: usize = SegmentLayout::SEGMENT;

/// Get the segment base of a pointer.
fn seg_base(ptr: *mut u8) -> usize {
    (ptr as usize) & !(SEGMENT - 1)
}

/// A full decommit → recommit round-trip through a small segment reaching
/// `live_count == 0`. The FIRST block carved after reuse lands at the tight
/// `small_meta_end` offset from the segment base — the exact byte that would
/// fault if the decommit boundary were miscalculated (too large relative to
/// what was committed, the recommit-on-reuse would not cover it; too small,
/// the decommit would clip into metadata). Writing and reading back a value
/// there proves the boundary is correct.
///
/// Modeled on `tests/decommit_miri_cycle.rs`'s proven round-trip pattern (pool
/// disabled so decommit fires deterministically), with the added assertion
/// that the reused block sits at the tight `meta_end` offset.
#[test]
fn decommit_recommit_round_trip_near_payload_start() {
    let before = AllocCore::dbg_decommit_count();
    // Disable the empty-small-segment pool so decommit fires deterministically
    // (with the pool ON, emptied segments are absorbed retained-comitted).
    let cfg = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(0));
    let mut ac = AllocCore::new_with_config(cfg).expect("primordial");
    // 2 KiB blocks: a few thousand per 4 MiB payload, sized for miri.
    let layout = Layout::from_size_align(2048, 8).unwrap();
    let meta_end = SegmentLayout::SMALL_META_END;

    // Two rounds: round 1 fills + empties (→ decommit); round 2 reuses.
    for round in 0..2u32 {
        let mut ptrs: Vec<(*mut u8, u8)> = Vec::new();
        // Enough to overflow the primordial into >= 2 fresh Small segments.
        let prim_base = {
            let p = ac.alloc(layout);
            assert!(!p.is_null(), "primordial alloc null round={round}");
            let b = 0xA5 ^ (round as u8);
            unsafe {
                p.write(b);
                p.add(2047).write(b);
            }
            ptrs.push((p, b));
            seg_base(p)
        };
        let mut saw_payload_start_block = false;
        for i in 1..6000usize {
            let p = ac.alloc(layout);
            assert!(!p.is_null(), "alloc null i={i} round={round}");
            let base = seg_base(p);
            // The first carved block in any fresh/reused segment sits at the
            // tight `meta_end` offset from the segment base (bump starts there;
            // 2048 divides PAGE so align_up is a no-op). This invariant holds
            // on BOTH the fresh reserve path AND the recommit-on-reuse path
            // (round 2, after decommit).
            if base != prim_base {
                let off = (p as usize) - base;
                if off == meta_end {
                    saw_payload_start_block = true;
                }
            }
            let b = ((i as u32) & 0xFF) as u8 ^ (round as u8);
            unsafe {
                p.write(b);
                p.add(2047).write(b);
            }
            ptrs.push((p, b));
        }
        // On every round we should have carved at least one block at the tight
        // payload-start offset in a non-primordial segment. This is the byte
        // that proves the decommit/recommit boundary covers the tight start.
        assert!(
            saw_payload_start_block,
            "round {round}: never observed a block at the tight SMALL_META_END offset ({meta_end}) \
             in a non-primordial segment"
        );
        // Read back (recommitted pages on reuse must read back what we wrote).
        for &(p, b) in &ptrs {
            unsafe {
                assert_eq!(p.read(), b, "byte0 readback round={round}");
                assert_eq!(p.add(2047).read(), b, "byteN readback round={round}");
            }
        }
        // Free all → empties fresh non-current Small segments → decommit.
        for &(p, _) in &ptrs {
            // SAFETY: `p` was returned by a matching alloc in this test, is live, freed once.
            unsafe { ac.dealloc(p, layout) };
        }
    }

    let after = AllocCore::dbg_decommit_count();
    assert!(
        after > before,
        "decommit hook never fired (before={before}, after={after}) — round 2 could not have \
         exercised the recommit-on-reuse path"
    );
}
