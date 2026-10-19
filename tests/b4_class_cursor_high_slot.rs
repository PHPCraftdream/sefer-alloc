#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

//! B4 §11 (docs/perf/B4_MT_COLLAPSE_DIAGNOSIS) regression: the per-class
//! discovery cursor must be recordable for a segment whose table slot is
//! at or above `SMALL_CLASS_COUNT`.
//!
//! The cursor array is indexed by class and stores a segment-table SLOT.
//! A width check that compared the slot against the array length (the class
//! count) instead of the slot type's range silently refused every slot
//! ≥ `SMALL_CLASS_COUNT`, so heaps with more segments than classes never got
//! a cursor for their later segments. Counterfactual: with that check the
//! cursor stays `None` and the first assertion below fails.
//!
//! Scenario: fill more than `SMALL_CLASS_COUNT + 2` segments with the
//! `SMALL_MAX` class, free 2 blocks of the highest-slot full segment S (not
//! the carving segment), then allocate until discovery serves S. Only S and
//! the carving segment hold free blocks of the class, so the first recorded
//! cursor must name S.

use core::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

#[test]
fn b4_class_cursor_records_slots_beyond_class_count() {
    let mut core = AllocCore::dbg_new_routed_for_test().expect("routed core");
    let layout =
        Layout::from_size_align(SegmentLayout::SMALL_MAX, 8).expect("SMALL_MAX layout is valid");
    let class_idx = core
        .dbg_layout_class_for(layout)
        .expect("SMALL_MAX must classify to a small class");
    let class_count = AllocCore::dbg_small_class_count();
    let need_segments = class_count + 3;

    // ── FILL past `need_segments` distinct segments.
    let mut by_base: std::collections::HashMap<usize, Vec<*mut u8>> =
        std::collections::HashMap::new();
    let mut live: Vec<*mut u8> = Vec::new();
    for _ in 0..200_000 {
        let ptr = core.alloc(layout);
        assert!(!ptr.is_null(), "fill alloc must not OOM");
        live.push(ptr);
        by_base
            .entry(SegmentLayout::segment_base_of(ptr as usize))
            .or_default()
            .push(ptr);
        if by_base.len() > need_segments {
            break;
        }
    }
    assert!(
        by_base.len() > need_segments,
        "fill must span more than {need_segments} segments (got {})",
        by_base.len()
    );

    // ── Victim S: the highest-slot segment that is neither the carving
    // segment (base of the last allocation) nor the first one.
    let carving_base = SegmentLayout::segment_base_of(*live.last().expect("non-empty") as usize);
    let first_base = SegmentLayout::segment_base_of(live[0] as usize);
    let (s_base, s_slot) = by_base
        .iter()
        .filter(|(b, v)| **b != carving_base && **b != first_base && v.len() >= 3)
        .map(|(b, v)| (*b, core.dbg_segment_id_of(v[0]) as usize))
        .max_by_key(|(_, slot)| *slot)
        .expect("a victim segment exists");
    assert!(
        s_slot >= class_count,
        "victim slot {s_slot} must be ≥ SMALL_CLASS_COUNT ({class_count}) for this regression"
    );
    let victim = by_base.get_mut(&s_base).expect("victim present");
    for _ in 0..2 {
        let ptr = victim.pop().expect("victim has ≥ 3 live");
        live.retain(|p| *p != ptr);
        // SAFETY: live `layout` block of `core`, freed exactly once.
        unsafe { core.dealloc(ptr, layout) };
    }
    drop(by_base);

    // ── DRAIN until the first cursor is recorded; it must name S.
    let mut recorded = None;
    for _ in 0..4096 {
        let ptr = core.alloc(layout);
        assert!(!ptr.is_null(), "drain alloc must not OOM");
        live.push(ptr);
        if let Some(slot) = core.dbg_class_cursor_slot(class_idx) {
            recorded = Some(slot);
            break;
        }
    }
    assert_eq!(
        recorded,
        Some(s_slot),
        "discovery served segment S (slot {s_slot} ≥ class count {class_count}); \
         its slot must be recorded as the class cursor"
    );

    for ptr in live {
        // SAFETY: each pointer is a live, distinct `layout` block.
        unsafe { core.dealloc(ptr, layout) };
    }
}
