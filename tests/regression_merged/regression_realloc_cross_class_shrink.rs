//! Regression test for the `AllocCore::realloc` OPT-F cross-class in-place
//! shrink bug — a latent defect **exposed** (not introduced) by task B1's
//! page-aligned size classes.
//!
//! ## The bug
//!
//! `AllocCore::realloc`'s OPT-F fast path used to return the input pointer
//! unchanged whenever `new_class <= old_class` (the new size fit in the same
//! OR a smaller class). That is unsound for the *smaller*-class case:
//!
//! - A block is carved at an offset that is a multiple of ITS class's
//!   `block_size`. That offset is NOT necessarily a multiple of a smaller
//!   class's `block_size` — the class sizes are not divisors of one another
//!   (e.g. a 4560-byte geometric class is not a multiple of the 4096-byte
//!   page-aligned class).
//! - Per the `GlobalAlloc` contract, after `realloc(ptr, old, new_size)` the
//!   caller frees with the NEW layout (`new_size`, same align). `AllocCore`'s
//!   `dealloc` (post-#114) derives the size class from that layout ALONE —
//!   not from where the block physically sits.
//! - So a cross-class shrink that returned `ptr` unchanged would, on the
//!   eventual `dealloc`, push the block's (old-class-multiple) offset onto
//!   the SMALLER class's free list — where that offset is misaligned. A
//!   later `alloc` popping that free list then returns a mis-placed pointer
//!   that overlaps a neighbouring block (an M3 violation).
//!
//! ## Why it stayed latent until task B1
//!
//! Before B1, the size-class table (a pure 1.25× geometric progression) had
//! no class that was a multiple of 512/1024/2048/4096. A page-aligned shrink
//! target therefore classified to `None` (the Large path) and never reached
//! OPT-F's small-class branch — so the `<=` bug was unreachable for the
//! shapes that expose it. B1 added the page-aligned classes (512..16384) to
//! fix the #114 follow-up; that made `class_for` resolve those shrink targets
//! to a real small class, surfacing this pre-existing `realloc` defect. It
//! was first caught by the `alloc_core_differential` proptest once B1 landed;
//! the minimal shrink seed is saved in
//! `tests/alloc_core_differential.proptest-regressions` (`b2ad64d...`) and is
//! the primary end-to-end regression guard. THIS test is the focused,
//! human-readable companion that pins the exact fix directly.
//!
//! ## The fix
//!
//! OPT-F now takes the in-place fast path only when `new_class == old_class`.
//! A cross-class shrink (`new_class < old_class`) falls through to the slow
//! path (alloc a fresh block in the smaller class + copy + dealloc the old
//! block in its own class) — so it RELOCATES the block rather than aliasing
//! it into a foreign class's free list.
//!
//! ## Counterfactual (non-vacuity) — the load-bearing assertion
//!
//! This test asserts, for a shrink whose precondition (`old_class !=
//! new_class`, both `Some`) it checks explicitly, that the returned pointer
//! is DIFFERENT from the input — i.e. the block was relocated by the slow
//! path, not aliased in place. With the pre-fix `<=` OPT-F, a cross-class
//! shrink returned the SAME pointer (the bug), so `assert_ne!` fails.
//! Personally re-verified during review: reverting OPT-F to `<=` makes this
//! `assert_ne!` fail (and the `alloc_core_differential` seed fail too);
//! restoring `==` makes both pass. (The earlier attempt at a "hammer the
//! free list and detect overlap" test was discarded — it passed even with
//! the bug present, i.e. it was vacuous; this relocation assertion is the
//! deterministic signal.)

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

#[test]
fn realloc_cross_class_shrink_relocates_not_aliases() {
    let mut a = AllocCore::new().expect("AllocCore::new");

    // Grow a tiny block into a mid geometric class, then shrink across a
    // class boundary into the B1 page-aligned 4096 class.
    let l0 = Layout::from_size_align(1, 1).unwrap();
    let p0 = a.alloc(l0);
    assert!(!p0.is_null(), "initial alloc failed");

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
    let p1 = unsafe { a.realloc(p0, l0, 4097) }; // grow → some class covering 4097
    assert!(!p1.is_null(), "grow realloc failed");

    // Establish the precondition the bug needs: (4097, 1) and (3713, 1) must
    // classify to DIFFERENT small classes (a genuine cross-class shrink). If
    // the table geometry ever changes so these collapse into one class, this
    // test would be testing nothing — so assert the precondition explicitly
    // and fail loudly rather than pass vacuously.
    let old_class = SegmentLayout::class_for(4097, 1).expect("4097 must be a small class post-B1");
    let new_class = SegmentLayout::class_for(3713, 1).expect("3713 must be a small class post-B1");
    assert!(
        new_class < old_class,
        "test precondition broken: expected a cross-class shrink \
         (new_class {new_class} < old_class {old_class}); table geometry \
         may have changed — pick sizes that straddle a class boundary"
    );

    let l1 = Layout::from_size_align(4097, 1).unwrap();
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
    let p2 = unsafe { a.realloc(p1, l1, 3713) }; // cross-class shrink (the bug trigger)
    assert!(!p2.is_null(), "shrink realloc failed");

    // The load-bearing counterfactual assertion: a cross-class shrink MUST
    // relocate (slow path: new block in the smaller class), never alias the
    // old block. Pre-fix (`<=`) returned `p1` unchanged — the bug; post-fix
    // (`==`) returns a fresh pointer.
    assert_ne!(
        p2, p1,
        "cross-class shrink returned the SAME pointer — OPT-F aliased a \
         block into a smaller class's free list (the pre-fix `<=` bug). It \
         must relocate via the slow path so the block is placed at a valid \
         offset for its new class."
    );

    // The relocated block must be usable and correctly aligned.
    // SAFETY: p2 is valid for 3713 bytes.
    unsafe {
        std::ptr::write_bytes(p2, 0xAB, 3713);
        assert_eq!(p2.read(), 0xAB);
        assert_eq!(p2.add(3712).read(), 0xAB);
    }

    let l2 = Layout::from_size_align(3713, 1).unwrap();
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { a.dealloc(p2, l2) };
}
