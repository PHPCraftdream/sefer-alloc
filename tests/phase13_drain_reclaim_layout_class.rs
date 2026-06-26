//! Task #40 — §13 regression gate for the CROSS-THREAD drain/reclaim path.
//!
//! ## Why this test exists (the §13 root cause on the drain path)
//!
//! A segment has ONE bump cursor shared by all size classes, so a single 4 KiB
//! page can host blocks of several classes. The page-dedication rule records
//! only the FIRST class to touch a page in `page_map` (`carve_block` calls
//! `set_class` only when `class_of(page).is_none()`). For any LATER block of a
//! DIFFERENT class on the same page, `page_map` returns the WRONG class.
//!
//! The OLD cross-thread drain path (`Heap::drain_thread_free` →
//! `AllocCore::dealloc_small_by_segment`) had only the block POINTER — not the
//! original `Layout` — so it re-derived the class from `page_map`. On a
//! mixed-class page that linked the freed block into the WRONG class's free
//! list: a later alloc of the page_map class would hand out a block of the
//! wrong (smaller) size, and the caller writing its full (larger) Layout size
//! would corrupt the neighbour. That latent bug is now removed: cross-thread
//! free routes `(offset, class)` through the per-segment `RemoteFreeRing`, and
//! the owner reclaims via `reclaim_offset`, which TRUSTS the carried class and
//! NEVER consults `page_map`.
//!
//! ## How this test is non-vacuous (counterfactual)
//!
//! It first EXHIBITS a mixed-class page (a block whose `page_map`-class differs
//! from its `Layout`-class). Then it drives the SAME ring → `reclaim_offset`
//! path the cross-thread freer uses (via the test-only `dbg_push_to_ring` /
//! `dbg_drain_all_rings` seam), pushing the probe block with its CORRECT
//! (Layout) class. It asserts the block resurfaces on a Layout-class alloc and
//! NOT on a page_map-class alloc.
//!
//! The counterfactual is made EXPLICIT and self-checking: the test ALSO runs
//! the buggy variant — pushing the SAME block with the `page_map` class (what
//! the removed `dealloc_small_by_segment` would have derived) — and asserts
//! that the block then resurfaces on the page_map-class alloc. That second
//! assertion is the red-on-the-bug proof: if `reclaim_offset` ignored the
//! carried class and used `page_map`, BOTH pushes would route identically and
//! the first (correct-class) assertion would be the one that could pass by
//! luck. By showing the two carried classes route DIFFERENTLY, the test proves
//! reclaim genuinely honours the carried class — i.e. the §13 fix is
//! load-bearing.

#![cfg(feature = "alloc-xthread")]

use std::alloc::Layout;
use std::ptr;

use sefer_alloc::AllocCore;

/// Force a mixed-class page into existence and return the first block whose
/// `page_map`-class differs from its `Layout`-class, along with both classes
/// and the probe layout. Mirrors `tests/phase13_3_dealloc_layout_class.rs`.
fn exhibit_mixed_class_page(
    a: &mut AllocCore,
    seed_layout: Layout,
    probe_layout: Layout,
) -> Option<(*mut u8, usize, usize, Layout)> {
    for _ in 0..32 {
        let _ = a.alloc(seed_layout);
    }
    for _ in 0..64 {
        let p = a.alloc(probe_layout);
        if p.is_null() {
            continue;
        }
        let pm = a.dbg_page_map_class_for(p)?;
        let lc = a.dbg_layout_class_for(probe_layout)?;
        if pm != lc {
            return Some((p, lc, pm, probe_layout));
        }
    }
    None
}

/// Find a small `Layout` that resolves to the given size class.
fn layout_for_class(a: &AllocCore, class: usize) -> Layout {
    for bs in 1..=4096 {
        let candidate = Layout::from_size_align(bs, 16).unwrap();
        if a.dbg_layout_class_for(candidate) == Some(class) {
            return candidate;
        }
    }
    panic!("no small layout resolves to class {class}");
}

/// Allocate up to `tries` blocks of `layout` and return true if one of them is
/// the block carrying `canary` at offset 8 (i.e. the freed block resurfaced on
/// this class's free list).
fn block_resurfaces(a: &mut AllocCore, layout: Layout, canary: u64, tries: usize) -> bool {
    for _ in 0..tries {
        let p = a.alloc(layout);
        assert!(!p.is_null(), "alloc returned null");
        // SAFETY: p is valid for layout.size() bytes (>= 16); read u64 at 8.
        let observed = unsafe { ptr::read(p.add(8) as *const u64) };
        if observed == canary {
            return true;
        }
    }
    false
}

/// The fix: reclaim routes by the CARRIED (Layout) class — the block resurfaces
/// on its Layout class and NOT on its page_map class.
#[test]
fn reclaim_uses_carried_layout_class_not_page_map() {
    let mut a = AllocCore::new().unwrap();
    let seed_layout = Layout::from_size_align(16, 16).unwrap();
    let probe_layout = Layout::from_size_align(48, 16).unwrap();

    let (block, layout_class, page_map_class, block_layout) =
        exhibit_mixed_class_page(&mut a, seed_layout, probe_layout)
            .expect("test precondition: a mixed-class page must be exhibited (non-vacuous)");
    assert_ne!(
        layout_class, page_map_class,
        "vacuous: page_map class matches Layout class"
    );

    // Canary at offset 8 (past the intrusive free-list `next` word at offset 0,
    // which reclaim overwrites when pushing onto the free list).
    let canary: u64 = 0xDE_AD_BE_EF_C0_FF_EE_11;
    // SAFETY: block is valid for block_layout.size() bytes (>= 16).
    unsafe { ptr::write(block.add(8) as *mut u64, canary) };

    // Drive the cross-thread path: push (offset, CORRECT Layout class) into the
    // segment's RemoteFreeRing, then drain via reclaim_offset.
    assert!(
        a.dbg_push_to_ring(block, layout_class),
        "ring push failed (ring full?)"
    );
    a.dbg_drain_all_rings();

    // It must NOT resurface on a page_map-class alloc...
    let pm_layout = layout_for_class(&a, page_map_class);
    assert!(
        !block_resurfaces(&mut a, pm_layout, canary, 256),
        "block resurfaced on the page_map-class free list — reclaim used \
         page_map, not the carried Layout class (the §13 bug)"
    );
    // ...and it MUST resurface on a Layout-class alloc.
    assert!(
        block_resurfaces(&mut a, block_layout, canary, 256),
        "block did NOT resurface on its Layout-class free list (routing bug)"
    );
}

/// Counterfactual anchor: pushing the SAME block with the `page_map` class (the
/// class the removed `dealloc_small_by_segment` would have derived) routes it to
/// the page_map class's free list — proving reclaim genuinely honours the
/// CARRIED class. If reclaim ignored the carried class and used page_map, this
/// test and the one above would be indistinguishable; their DIFFERING outcomes
/// prove the carried class is load-bearing.
#[test]
fn reclaim_routes_by_carried_class_counterfactual() {
    let mut a = AllocCore::new().unwrap();
    let seed_layout = Layout::from_size_align(16, 16).unwrap();
    let probe_layout = Layout::from_size_align(48, 16).unwrap();

    let (block, layout_class, page_map_class, _block_layout) =
        exhibit_mixed_class_page(&mut a, seed_layout, probe_layout)
            .expect("test precondition: a mixed-class page must be exhibited (non-vacuous)");
    assert_ne!(layout_class, page_map_class);

    let canary: u64 = 0xCA_FE_BA_BE_12_34_56_78;
    // SAFETY: block is valid for its layout size (>= 16).
    unsafe { ptr::write(block.add(8) as *mut u64, canary) };

    // Push with the WRONG (page_map) class on purpose. reclaim_offset trusts the
    // carried class, so the block lands on the page_map class's free list.
    assert!(
        a.dbg_push_to_ring(block, page_map_class),
        "ring push failed"
    );
    a.dbg_drain_all_rings();

    let pm_layout = layout_for_class(&a, page_map_class);
    assert!(
        block_resurfaces(&mut a, pm_layout, canary, 256),
        "block did NOT resurface on the carried (page_map) class — reclaim did \
         not honour the carried class"
    );
}
