#![cfg(all(
    feature = "production",
    feature = "internals",
    feature = "bench-internals",
    not(feature = "numa-aware")
))]

//! B4 §11 (docs/perf/B4_MT_COLLAPSE_DIAGNOSIS) counterfactual for the
//! per-class discovery cursor: after a successful discovery
//! (`find_segment_with_free`), the serving segment's table slot is
//! remembered in `AllocCore::small_class_cursor[class_idx]`; on the next
//! bin miss the scalar path first pops one block from the cursor segment's
//! bin (`pop_from_class_cursor`), and the discovery `linear_scan` STARTS ONE
//! SLOT AFTER the cursor (`class_cursor_scan_start`), wrapping once so every
//! active slot is still visited exactly once. The cursor is a hint, never a
//! trusted negative: `cursor_probe_base` re-validates the slot (non-null
//! root, Small/Primordial kind, header `segment_id` == slot, != `small_cur`)
//! and an empty cursor bin falls through to a normal discovery.
//!
//! ## Counterfactual arithmetic
//!
//! Deterministic fragmentation: M = 8 segments each hold exactly 2 freed
//! blocks of the class in their `BinTable`. A drain phase allocates until
//! `small_cur` and all refill leftovers are exhausted — observable as the
//! FIRST class-cursor recording (discovery just served the first non-empty
//! bin). The measured window then runs K = 2·M allocations, each of which
//! must come out of one of the M 2-block victim bins.
//!
//! WITH the cursor: the cursor segment is drained block-by-block via the
//! probe (0 discovery calls per block), and only when a bin runs empty does
//! discovery fire to find the next 2-block segment — one discovery per
//! 2-block segment, i.e. ≤ M (+ slack 4) discovery calls in the window; the
//! linear scan also starts after the cursor, so a re-discovery never
//! re-scans already-drained segments. WITHOUT the cursor (the revert below
//! neutralises `cursor_probe_base`, so BOTH probes die, and
//! `record_class_cursor`, so no cursor — and no scan start — is ever
//! recorded): the drain phase never records a cursor at all (explicit
//! panic), and a window allocation would full-scan from slot 0 every time —
//! discovery delta ≈ K = 2·M, far above M + 4.
//!
//! Observers: `dbg_b4_collapse_counters()[0]` = `B4_FIND_SEGMENT_CALLS`
//! (discovery entries), `dbg_b4_cursor_counters()` = [probe hits, probe
//! misses]. A miss is only counted when a cursor is recorded but does not
//! serve (stale slot or drained bin) — the drained cursor bins supply the
//! ≥ 1 miss.
//!
//! Segment count stays BELOW the directory materialisation threshold (32):
//! ≥ 64 KiB blocks at ~64 blocks per 4 MiB segment mean ~12 segments — the
//! per-class directory never materialises and the linear scan (not a
//! directory bit hit) is the only discovery mechanism under test.

use core::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

/// Smallest class whose block is ≥ 64 KiB (bounds memory: ~64 blocks per
/// 4 MiB segment), falling back to the `SMALL_MAX` class (the t10 geometry)
/// if no class is that large.
fn pick_class(core: &AllocCore) -> (usize, Layout) {
    for i in 0..AllocCore::dbg_small_class_count() {
        let size = AllocCore::dbg_block_size(i);
        if size >= 64 * 1024 {
            let layout = Layout::from_size_align(size, 8).expect("block layout valid");
            assert_eq!(
                core.dbg_layout_class_for(layout),
                Some(i),
                "block-size layout must classify back to its own class"
            );
            return (i, layout);
        }
    }
    let layout =
        Layout::from_size_align(SegmentLayout::SMALL_MAX, 8).expect("SMALL_MAX layout is valid");
    let class = core
        .dbg_layout_class_for(layout)
        .expect("SMALL_MAX must classify to a small class");
    (class, layout)
}

#[test]
fn b4_class_cursor_serves_bin_misses_without_full_discovery_scans() {
    let mut core = AllocCore::dbg_new_routed_for_test().expect("routed core");
    let (class_idx, layout) = pick_class(&core);

    // ── FILL: allocate until ≥ 10 distinct segment bases hold ≥ 3 live
    // blocks each. This walks well past most segments' 31-block refill
    // leftovers; the drain phase below consumes the rest.
    let mut by_base: std::collections::HashMap<usize, Vec<*mut u8>> =
        std::collections::HashMap::new();
    let mut live: Vec<*mut u8> = Vec::new();
    for _ in 0..4000 {
        let ptr = core.alloc(layout);
        assert!(!ptr.is_null(), "fill alloc must not OOM");
        live.push(ptr);
        by_base
            .entry(SegmentLayout::segment_base_of(ptr as usize))
            .or_default()
            .push(ptr);
        if by_base.values().filter(|v| v.len() >= 3).count() >= 10 {
            break;
        }
    }
    assert!(
        by_base.values().filter(|v| v.len() >= 3).count() >= 10,
        "fill cap of 4000 allocations must not be hit"
    );

    // ── FREE: M = 8 distinct bases with ≥ 3 live each, preferring
    // non-primordial bases (skip the base of the FIRST ever allocation —
    // the primordial segment hosts the registry and holds fewer blocks).
    let first_base = SegmentLayout::segment_base_of(live[0] as usize);
    let mut victims: Vec<usize> = by_base
        .keys()
        .copied()
        .filter(|b| *b != first_base)
        .collect();
    if victims.len() < 8 {
        let extra: Vec<usize> = by_base
            .keys()
            .copied()
            .filter(|b| !victims.contains(b))
            .take(8 - victims.len())
            .collect();
        victims.extend(extra);
    }
    victims.sort_unstable();
    victims.truncate(8);
    assert_eq!(victims.len(), 8, "need 8 distinct bases with ≥ 3 live each");

    for base in &victims {
        let slots = by_base.get_mut(base).expect("victim base present");
        // Free exactly 2 live blocks of this segment. Its bin now holds
        // exactly 2 blocks (all its refill leftovers were consumed before
        // later segments were carved).
        for _ in 0..2 {
            let ptr = slots.pop().expect("victim has ≥ 3 live");
            // Drop it from `live` too, so cleanup never frees it a second time.
            live.retain(|p| *p != ptr);
            // SAFETY: ptr is a live `layout` block of `core`, freed exactly
            // once here and never reused as a live pointer below.
            unsafe { core.dealloc(ptr, layout) };
        }
    }
    drop(by_base);

    // ── DRAIN: allocate until the class cursor is FIRST recorded — i.e.
    // until `small_cur` and every refill leftover are exhausted and
    // discovery has just served the first non-empty bin (a victim's 2-block
    // bin, or the last refill leftovers). The recording allocation itself
    // popped one block from the recorded segment.
    let cursor_counters_start = sefer_alloc::SeferAlloc::dbg_b4_cursor_counters();
    let mut drain: Vec<*mut u8> = Vec::new();
    let recording = loop {
        let ptr = core.alloc(layout);
        assert!(!ptr.is_null(), "drain alloc must not OOM");
        drain.push(ptr);
        if let Some(slot) = core.dbg_class_cursor_slot(class_idx) {
            break slot;
        }
        assert!(
            drain.len() < 1024,
            "no class cursor was ever recorded — the cursor feature is absent \
             or the scenario did not exhaust `small_cur` (counterfactual red)"
        );
    };
    assert_eq!(
        core.dbg_segment_id_of(*drain.last().expect("non-empty")) as usize,
        recording,
        "the recording discovery must have served its block from the \
         recorded segment"
    );

    // ── MEASURE: K = 2·M allocations, all served from the M freed 2-block
    // bins (plus any remaining refill leftovers) without a discovery per
    // allocation.
    let m = victims.len();
    let k = 2 * m;
    let discovery_before = sefer_alloc::SeferAlloc::dbg_b4_collapse_counters()[0];
    let mut measured: Vec<*mut u8> = Vec::with_capacity(k);
    for _ in 0..k {
        let ptr = core.alloc(layout);
        assert!(!ptr.is_null(), "measured alloc must not OOM");
        measured.push(ptr);
    }
    let discovery_delta = sefer_alloc::SeferAlloc::dbg_b4_collapse_counters()[0] - discovery_before;
    let cursor_after = sefer_alloc::SeferAlloc::dbg_b4_cursor_counters();
    let hits = cursor_after[0] - cursor_counters_start[0];
    let misses = cursor_after[1] - cursor_counters_start[1];

    assert!(
        discovery_delta <= (m as u64) + 4,
        "with the class cursor, K = 2M bin-miss allocations must cause \
         ≤ M+4 discovery calls (got {discovery_delta}, M = {m}); without the \
         cursor this is ≈ 2M — see the module doc's counterfactual arithmetic"
    );
    assert!(
        hits >= 1,
        "cursor probe must serve blocks from the recorded segment (hits = {hits})"
    );
    assert!(
        misses >= 1,
        "the drained 2-block cursor bins must record probe misses \
         (misses = {misses}) — an empty cursor bin is never a trusted negative"
    );

    // The recorded cursor names a segment that actually served one of the
    // drain or measured allocations (the last discovery-hit segment).
    let cursor_slot = core
        .dbg_class_cursor_slot(class_idx)
        .expect("a cursor must still be recorded after the window");
    assert!(
        drain
            .iter()
            .chain(&measured)
            .any(|p| core.dbg_segment_id_of(*p) as usize == cursor_slot),
        "cursor slot {cursor_slot} must equal the segment of a drain or \
         measured allocation (the last served segment)"
    );

    // The directory must never have materialised: ~12 segments < threshold.
    assert!(
        !core.dbg_directory_is_materialised(),
        "this scenario must stay below DIRECTORY_MATERIALIZE_THRESHOLD (32)"
    );

    // ── Cleanup: free every remaining live pointer.
    for ptr in live.into_iter().chain(drain).chain(measured) {
        // SAFETY: each pointer is a live, distinct `layout` block.
        unsafe { core.dealloc(ptr, layout) };
    }
}
