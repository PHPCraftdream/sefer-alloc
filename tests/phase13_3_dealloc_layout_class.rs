//! Phase 13.3 — counterfactual gate: own-thread `dealloc` MUST derive the size
//! class from the caller-supplied `Layout`, NOT from the segment's `page_map`.
//!
//! ## Why this test exists (the §13 root cause)
//!
//! A segment has ONE bump cursor shared by all size classes, so a single page
//! can host blocks of several classes. The page-dedication rule records only
//! the FIRST class to touch a page in `page_map` (see `carve_block`: it calls
//! `set_class` only when `class_of(page).is_none()`). For any LATER block of a
//! DIFFERENT class in the same page, `page_map` therefore returns the WRONG
//! class. Deriving the class from `page_map` on the own-thread free path would
//! link the freed block into the wrong class's free list; a subsequent alloc
//! of the (page_map) class would hand out a block of the wrong (Layout) size —
//! corrupting a neighbour when the caller writes the full (mismatched) size.
//!
//! The own-thread freer HAS the original `Layout`, so classifying from it is
//! both cheaper (no page_map load) AND correct. `RACE_DRAIN_RECLAIM.md` §13
//! established this for the cross-thread drain path; Phase 13.3 makes the
//! own-thread path consistent with it.
//!
//! ## How this test is non-vacuous (counterfactual)
//!
//! It first EXHIBITS a mixed-class page (a block whose `page_map`-class
//! differs from its `Layout`-class — such pages exist because the shared bump
//! cursor lets a second class land on a page the first class dedicated). Then
//! it frees the block with the correct (Layout) class and proves the block
//! returns to the LAYOUT class's free list (a same-class realloc reuses it),
//! NOT to the page_map class's free list. Under a (buggy) page_map derivation
//! the block would resurface on a page_map-class alloc — caught here as a
//! canary reappearing on the wrong-class alloc.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;
use std::ptr;

use sefer_alloc::AllocCore;

/// Force a mixed-class page into existence and return the first block whose
/// `page_map`-class differs from its `Layout`-class, along with both classes.
///
/// Strategy: allocate a run of one size to dedicate several pages to its
/// class, then interleave a different size; the shared bump cursor eventually
/// places the second size on a page the first size dedicated (page_map says
/// class A, the block is class B). We detect this via the test-only
/// `dbg_page_map_class_for` / `dbg_layout_class_for` accessors.
fn exhibit_mixed_class_page(
    a: &mut AllocCore,
    seed_layout: Layout,
    probe_layout: Layout,
) -> Option<(*mut u8, usize, usize, Layout)> {
    // Fill a few pages with the seed class so several page_map entries record
    // it. (Refill batches mean even one alloc carves many same-class blocks.)
    for _ in 0..32 {
        let _ = a.alloc(seed_layout);
    }
    // Now sprinkle the probe class; bump-order keeps carving into pages whose
    // page_map may already record the seed class (mixed-class) until a fresh
    // page is reached.
    for _ in 0..64 {
        let p = a.alloc(probe_layout);
        if p.is_null() {
            continue;
        }
        let pm = match a.dbg_page_map_class_for(p) {
            Some(c) => c,
            None => continue,
        };
        let lc = match a.dbg_layout_class_for(probe_layout) {
            Some(c) => c,
            None => continue,
        };
        if pm != lc {
            return Some((p, lc, pm, probe_layout));
        }
    }
    None
}

#[test]
fn dealloc_uses_layout_class_not_page_map_on_mixed_class_page() {
    let mut a = AllocCore::new().unwrap();
    // Two distinct small classes. 16 B (class 0) and 48 B (a few steps up).
    // Both well below SMALL_MAX, so both take the small free-list path.
    let seed_layout = Layout::from_size_align(16, 16).unwrap();
    let probe_layout = Layout::from_size_align(48, 16).unwrap();

    let (block, layout_class, page_map_class, block_layout) =
        match exhibit_mixed_class_page(&mut a, seed_layout, probe_layout) {
            Some(x) => x,
            None => {
                // No mixed-class page arose in this run. That itself is a problem
                // for the gate (a non-vacuous test needs the precondition to hold
                // reliably); fail loudly so the test is not silently vacuous.
                panic!(
                    "test precondition failed: no mixed-class page exhibited; \
                 the counterfactual cannot be exercised. The bump/refill \
                 geometry changed and this test needs a new seeding strategy."
                );
            }
        };
    assert_ne!(
        layout_class, page_map_class,
        "vacuous: page_map class matches Layout class"
    );

    // Stamp the block with a unique canary at offset 8 (past the intrusive
    // free-list `next` word at offset 0, which `dealloc_small` overwrites when
    // pushing onto the free list — so offset 0 cannot carry a survivor tag).
    // SAFETY: block is valid for block_layout.size() bytes (>= 16, so offset 8
    // + size_of::<u64>() = 16 is in-bounds).
    let canary: u64 = 0xDE_AD_BE_EF_C0_FF_EE_11;
    unsafe { ptr::write(block.add(8) as *mut u64, canary) };

    // Free the block with its CORRECT (Layout) class. Under Phase 13.3 this
    // routes via Layout → layout_class free list. Under a (buggy) page_map
    // derivation it would route via page_map_class free list.
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { a.dealloc(block, block_layout) };

    // Counterfactual probe A — the block must NOT resurface on a page_map-class
    // alloc. We allocate several page_map-class blocks; if any of them IS our
    // `block` (recognised by the canary), dealloc mis-routed it.
    //
    // Reconstruct a layout that resolves to `page_map_class`: the block size
    // of class C is SIZE_CLASS_TABLE[C]; a layout of exactly that size (and
    // align <= SMALL_ALIGN_MAX) resolves to class C. We don't have the table
    // in the test, but `dbg_layout_class_for` lets us VERIFY a candidate
    // layout resolves to page_map_class before using it.
    let mut pm_layout = None;
    for bs in 1..=4096 {
        let candidate = Layout::from_size_align(bs, 16).unwrap();
        if a.dbg_layout_class_for(candidate) == Some(page_map_class) {
            pm_layout = Some(candidate);
            break;
        }
    }
    let pm_layout = pm_layout.expect("a layout resolving to page_map_class exists in small range");
    for _ in 0..256 {
        let p = a.alloc(pm_layout);
        assert!(!p.is_null(), "page_map-class alloc returned null");
        // SAFETY: p is valid for pm_layout.size() bytes (>= 16); read one u64
        // at offset 8.
        let observed = unsafe { ptr::read(p.add(8) as *const u64) };
        assert_ne!(
            observed, canary,
            "page_map-class alloc returned the Layout-class block — dealloc \
             derived the class from page_map (the §13 mixed-class bug)"
        );
    }

    // Counterfactual probe B — the block MUST resurface on a Layout-class
    // alloc (it went to the correct bin). We allocate several block-layout
    // blocks; one of them must be `block` (recognised by the canary). This
    // is the positive side: it anchors that the block was reclaimable via
    // its Layout class.
    let mut found_reuse = false;
    for _ in 0..256 {
        let p = a.alloc(block_layout);
        assert!(!p.is_null(), "Layout-class alloc returned null");
        // SAFETY: p is valid for block_layout.size() bytes (>= 16); read one
        // u64 at offset 8.
        let observed = unsafe { ptr::read(p.add(8) as *const u64) };
        if observed == canary {
            found_reuse = true;
            break;
        }
    }
    assert!(
        found_reuse,
        "the freed block did NOT resurface on a Layout-class alloc — dealloc \
         did not place it in the Layout class's free list (routing bug)"
    );
}

/// Variant that exercises mixed classes in the opposite direction (big first,
/// then tiny on a big-dedicated page) to cover both orderings of the
/// page-dedication rule.
#[test]
fn dealloc_uses_layout_class_big_then_tiny_mixed_page() {
    // A mixed-class page (big page_map, tiny Layout) can only arise when the
    // big class's `block_size` does NOT evenly divide the page: only then is
    // there a leftover < big-block gap at the end of a big-dedicated page for
    // the shared bump cursor to place a later tiny block into. If the big
    // block_size divides the page exactly, big blocks tile the page with zero
    // remainder and the next (tiny) carve always starts a fresh page —
    // page_map == Layout, no mixed page.
    //
    // Task B1 added exact page-divisor classes (512/1024/2048/4096), so a
    // fixed `1024` seed (the pre-B1 choice) now tiles the 4096-byte page
    // perfectly and can never produce a big→tiny mixed page. Rather than pin
    // another magic size that a future table edit could again turn into a
    // divisor, probe a spread of "big" seed sizes and use the first that
    // actually yields a mixed page — the test self-adapts to the table
    // geometry while still asserting the real §13 invariant. The candidates
    // are deliberately non-power-of-two sizes likely to resolve to geometric
    // (non-divisor) classes; if the geometry ever changes so NONE of them
    // works, that is a real signal (fail loudly) rather than a silent pass.
    let big_seed_candidates = [
        700usize, 900, 1100, 1300, 1500, 1700, 2000, 2500, 3000, 3500,
    ];
    let tiny = Layout::from_size_align(16, 16).unwrap();

    let mut found = None;
    for &big in &big_seed_candidates {
        // Fresh substrate per candidate so a failed probe's allocations do
        // not perturb the next candidate's bump/page geometry.
        let mut a = AllocCore::new().unwrap();
        let big_layout = Layout::from_size_align(big, 16).unwrap();
        if let Some(x) = exhibit_mixed_class_page(&mut a, big_layout, tiny) {
            found = Some((a, x));
            break;
        }
    }
    let (mut a, (block, layout_class, page_map_class, block_layout)) = match found {
        Some(v) => v,
        None => {
            panic!(
                "test precondition failed: no big→tiny mixed-class page exhibited \
                 across any candidate big size {big_seed_candidates:?}. The \
                 bump/refill geometry changed and this test needs new seed \
                 candidates (pick sizes that resolve to a class whose block_size \
                 does NOT divide the page size)."
            );
        }
    };
    assert_ne!(layout_class, page_map_class);

    // The canary + probes (same logic as the first test, condensed).
    // Canary at offset 8 (past the free-list `next` word).
    let canary: u64 = 0xCA_FE_BA_BE_12_34_56_78;
    // SAFETY: block is valid for block_layout.size() bytes (>= 16).
    unsafe { ptr::write(block.add(8) as *mut u64, canary) };
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { a.dealloc(block, block_layout) };

    // Probe A: the block must not resurface on a page_map-class alloc.
    let mut pm_layout = None;
    for bs in 1..=4096 {
        let candidate = Layout::from_size_align(bs, 16).unwrap();
        if a.dbg_layout_class_for(candidate) == Some(page_map_class) {
            pm_layout = Some(candidate);
            break;
        }
    }
    let pm_layout = pm_layout.expect("a layout resolving to page_map_class exists");
    for _ in 0..256 {
        let p = a.alloc(pm_layout);
        assert!(!p.is_null());
        // SAFETY: p is valid for pm_layout.size() bytes (>= 16); read u64 at 8.
        let observed = unsafe { ptr::read(p.add(8) as *const u64) };
        assert_ne!(
            observed, canary,
            "page_map-class alloc returned the tiny block — dealloc used \
             page_map (§13 bug)"
        );
    }
    // Probe B: it must resurface on a Layout-class (tiny) alloc.
    let mut found = false;
    for _ in 0..256 {
        let p = a.alloc(block_layout);
        assert!(!p.is_null());
        // SAFETY: p is valid for block_layout.size() bytes (>= 16); read u64 at 8.
        if unsafe { ptr::read(p.add(8) as *const u64) } == canary {
            found = true;
            break;
        }
    }
    assert!(found, "freed block did not resurface (routing bug)");
}
