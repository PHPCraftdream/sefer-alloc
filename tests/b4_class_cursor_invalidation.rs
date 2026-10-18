#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals"
))]

//! B4 §11 (docs/perf/B4_MT_COLLAPSE_DIAGNOSIS) invalidation regression for
//! the per-class discovery cursor on the segment RELEASE leg.
//!
//! Release path (pool cap 0): a fully emptied segment goes through
//! `release_or_pool_empty_segment` → `release_empty_segment_now` → the
//! segment table's `recycle`, which NULLs the slot's canonical root (the
//! OS reservation is unmapped after recycle). The release leg calls
//! `invalidate_class_cursor_for_slot`, dropping any cursor naming the slot.
//!
//! THE key assertion: after the cursor segment S is emptied and released,
//! the class cursor must read `None`. If the release-leg invalidation is
//! removed, the cursor dangles past the release — and because the probe
//! would then touch a recycled slot, it MUST be a NULL-root check
//! (`base_at`), never a metadata deref. The final allocation proves the
//! belt-and-braces half: with a stale cursor, `cursor_probe_base` reads a
//! NULL root, clears the cursor (a counted miss — never a trusted negative)
//! and ordinary discovery takes over, so no UB and no wrong result occurs;
//! with the invalidation intact (green path) there is no cursor left to
//! probe at all and the allocation succeeds unconditionally.

use core::alloc::Layout;

use sefer_alloc::{AllocCore, LargeCacheConfig, SegmentLayout, SmallSegmentPoolConfig};

/// Smallest class whose block is ≥ 64 KiB (same geometry discipline as
/// `b4_class_cursor_discovery_burst`), falling back to the `SMALL_MAX`
/// class.
fn pick_class(core: &AllocCore) -> (usize, Layout) {
    for i in 0..AllocCore::dbg_small_class_count() {
        let size = AllocCore::dbg_block_size(i);
        if size >= 64 * 1024 {
            let layout = Layout::from_size_align(size, 8).expect("block layout valid");
            assert_eq!(core.dbg_layout_class_for(layout), Some(i));
            return (i, layout);
        }
    }
    let layout =
        Layout::from_size_align(SegmentLayout::SMALL_MAX, 8).expect("SMALL_MAX layout valid");
    (
        core.dbg_layout_class_for(layout)
            .expect("SMALL_MAX must classify to a small class"),
        layout,
    )
}

/// A core whose pool cap is 0: every emptied segment is RELEASED (and its
/// table slot recycled) immediately instead of being pooled.
fn new_core_zero_pool() -> AllocCore {
    let cfg = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(0));
    AllocCore::new_with_config(cfg).expect("AllocCore::new_with_config")
}

#[test]
fn b4_class_cursor_invalidated_when_cursor_segment_is_released() {
    let mut core = new_core_zero_pool();
    let (class_idx, layout) = pick_class(&core);

    // Every live block, whatever path issued it (fill, drain, post).
    let mut live: Vec<*mut u8> = Vec::new();
    let alloc = |core: &mut AllocCore, live: &mut Vec<*mut u8>| {
        let ptr = core.alloc(layout);
        assert!(!ptr.is_null(), "alloc must not OOM");
        live.push(ptr);
        ptr
    };

    // ── Fill ≥ 3 distinct segments with ≥ 3 live blocks each.
    let mut by_base: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for _ in 0..4000 {
        let ptr = alloc(&mut core, &mut live);
        *by_base
            .entry(SegmentLayout::segment_base_of(ptr as usize))
            .or_default() += 1;
        if by_base.values().filter(|c| **c >= 3).count() >= 3 {
            break;
        }
    }
    assert!(
        by_base.len() >= 3,
        "fill must span at least 3 segments (got {})",
        by_base.len()
    );

    // ── Free 2 blocks of one segment S. S is the LOWEST-addressed
    // non-primordial base: carve order follows slot order, so earlier
    // segments' refill leftovers were already consumed by the fill
    // (discovery always drains the earliest non-empty bin before a new
    // segment is carved) and S is neither the primordial first base nor the
    // current carving segment — S's bin ends at exactly the 2 blocks freed
    // into it and is the first non-empty bin the slot-0 scan can find.
    let first_base = SegmentLayout::segment_base_of(live[0] as usize);
    let carving_base = SegmentLayout::segment_base_of(*live.last().expect("non-empty") as usize);
    let s_base = *by_base
        .keys()
        .filter(|b| **b != first_base && **b != carving_base)
        .min_by_key(|b| **b)
        .expect("a non-primordial victim base exists");
    let in_s: Vec<usize> = live
        .iter()
        .enumerate()
        .filter(|(_, p)| SegmentLayout::segment_base_of(**p as usize) == s_base)
        .map(|(i, _)| i)
        .collect();
    assert!(by_base[&s_base] >= 3);
    for i in in_s.into_iter().take(2).rev() {
        // SAFETY: live `layout` block of `core`, freed exactly once.
        unsafe { core.dealloc(live.remove(i), layout) };
    }

    // ── Drain until the cursor lands on S: allocate (keeping every block
    // live) until `small_cur` and all refill leftovers are exhausted and
    // discovery walks the (slot-ordered) bins into S's 2-block bin.
    let s_slot_of = |core: &AllocCore, live: &[*mut u8]| -> usize {
        live.iter()
            .find(|p| SegmentLayout::segment_base_of(**p as usize) == s_base)
            .map(|p| core.dbg_segment_id_of(*p) as usize)
            .expect("S still holds live blocks")
    };
    let mut s_slot = None;
    for step in 0..512 {
        if let Some(slot) = core.dbg_class_cursor_slot(class_idx) {
            s_slot = Some(slot);
            break;
        }
        alloc(&mut core, &mut live);
        assert!(step < 511, "no class cursor was ever recorded");
    }
    let s_slot = s_slot.expect("cursor recorded");
    // The cursor names S (the first non-empty bin the slot-ordered scan
    // found is S's 2-block bin).
    assert_eq!(
        Some(s_slot),
        Some(s_slot_of(&core, &live)),
        "the recorded cursor must be on segment S"
    );
    // ── Empty S entirely: free EVERY live block of S (fill leftovers, the
    // drain allocations discovery re-issued from S's bin, …). Pool cap 0
    // means `release_or_pool_empty_segment` releases S and
    // `table.recycle` NULLs the slot — the release-leg invalidation must
    // drop the cursor naming S.
    for i in (0..live.len()).rev() {
        if SegmentLayout::segment_base_of(live[i] as usize) == s_base {
            let ptr = live.remove(i);
            // SAFETY: live `layout` block of `core`, freed exactly once.
            unsafe { core.dealloc(ptr, layout) };
        }
    }
    assert_eq!(
        core.dbg_class_cursor_slot(class_idx),
        None,
        "releasing the cursor segment must invalidate the class cursor \
         (release-leg invalidation, THE key assertion)"
    );

    // ── Alloc again: must succeed without UB. (Green path: there is no
    // cursor left to probe — the invalidation already fired. Counterfactual
    // path: the stale cursor survives the release here; the probe's
    // `base_at` NULL-root check clears it, records a counted miss and falls
    // through to discovery, so the allocation still succeeds.)
    let reused = alloc(&mut core, &mut live);
    assert!(!reused.is_null(), "alloc after release must succeed");

    // ── Cleanup the rest.
    for ptr in live {
        // SAFETY: each pointer is a live, distinct `layout` block.
        unsafe { core.dealloc(ptr, layout) };
    }
}
