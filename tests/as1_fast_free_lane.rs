//! AS-1: the own-thread small free fast lane accepts exactly what the chain's
//! guard accepts. Oracle: the magazine is LIFO, so an accepted free hands the
//! same block to the next allocation of its layout, and a rejected free must
//! not. Each case below is a free the lane has to hand back to the chain;
//! skipping the matching check in the lane would push the block and make it
//! re-issuable, which the oracle catches.
//! `exact-object-proto` aborts on the deliberately invalid frees below, so it
//! is excluded.
#![cfg(all(
    feature = "alloc-global",
    feature = "fastbin",
    not(feature = "exact-object-proto")
))]

use std::alloc::{GlobalAlloc, Layout};

use sefer_alloc::{SeferAlloc, SegmentLayout};

static A: SeferAlloc = SeferAlloc::new();

const SEGMENT: usize = 4 * 1024 * 1024;

fn layout(size: usize) -> Layout {
    Layout::from_size_align(size, 8).unwrap()
}

/// Allocate a block of `size` and a neighbour, so the block is not the
/// magazine's only entry; returns the block.
fn fresh(size: usize) -> *mut u8 {
    // SAFETY: nonzero size.
    let p = unsafe { A.alloc(layout(size)) };
    assert!(!p.is_null());
    p
}

/// The next allocation of `size` returns `p` iff `p` is on top of the magazine.
fn reissued(p: *mut u8, size: usize) -> bool {
    // SAFETY: nonzero size; the block is freed again before return.
    let q = unsafe { A.alloc(layout(size)) };
    let hit = q == p;
    // SAFETY: q was just allocated with this layout.
    unsafe { A.dealloc(q, layout(size)) };
    hit
}

#[test]
fn accepted_free_is_reissued() {
    let p = fresh(32);
    // SAFETY: p is live with this layout.
    unsafe { A.dealloc(p, layout(32)) };
    assert!(
        reissued(p, 32),
        "an accepted free must reach the magazine top"
    );
}

#[test]
fn double_free_is_rejected() {
    let p = fresh(48);
    let keep = fresh(48);
    // SAFETY: first free is valid; the second is the double free under test.
    unsafe {
        A.dealloc(p, layout(48));
        A.dealloc(p, layout(48));
    }
    // SAFETY: nonzero size.
    let (a, b) = unsafe { (A.alloc(layout(48)), A.alloc(layout(48))) };
    assert_ne!(
        a, b,
        "a double free must not put the block in the magazine twice"
    );
    // SAFETY: all live with this layout.
    unsafe {
        A.dealloc(a, layout(48));
        A.dealloc(b, layout(48));
        A.dealloc(keep, layout(48));
    }
}

#[test]
fn interior_and_misaligned_frees_are_rejected() {
    let p = fresh(64);
    // SAFETY: deliberately invalid frees of an interior granule and a
    // misaligned address inside a live block; the allocator must ignore them.
    unsafe {
        A.dealloc(p.add(16), layout(16));
        A.dealloc(p.add(8), layout(64));
    }
    assert!(
        !reissued(p.wrapping_add(16), 16),
        "interior granule must not be issued"
    );
    assert!(
        !reissued(p.wrapping_add(8), 64),
        "misaligned pointer must not be issued"
    );
    // SAFETY: p is still live.
    unsafe { A.dealloc(p, layout(64)) };
    assert!(reissued(p, 64));
}

#[test]
fn wrong_layout_free_is_rejected() {
    let p = fresh(16);
    // SAFETY: deliberate wrong-layout free (class of 256 B for a 16 B block).
    unsafe { A.dealloc(p, layout(256)) };
    assert!(
        !reissued(p, 256),
        "wrong-class free must not reach the 256 B magazine"
    );
    assert!(!reissued(p, 16), "the block is still live in its own class");
    // SAFETY: p is still live with its real layout.
    unsafe { A.dealloc(p, layout(16)) };
    assert!(reissued(p, 16));
}

#[test]
fn free_just_past_small_max_is_rejected() {
    // `size` in (SMALL_MAX, SMALL_MAX + MIN_BLOCK] maps to the last SIZE2CLASS
    // bucket, which `class_for` treats as out of the small range.
    // The last class parks one block, so its magazine must start empty: the
    // memo is primed by a 16 B free in the same segment, not by a neighbour of
    // the same class.
    let small = SegmentLayout::SMALL_MAX;
    let p = fresh(small);
    let s = fresh(16);
    assert_eq!(p as usize & !(SEGMENT - 1), s as usize & !(SEGMENT - 1));
    // SAFETY: s is live with this layout.
    unsafe { A.dealloc(s, layout(16)) };
    // SAFETY: deliberate wrong-layout free of a live block.
    unsafe { A.dealloc(p, layout(small + 1)) };
    assert!(
        !reissued(p, small),
        "a free larger than SMALL_MAX must not reach the small magazine"
    );
    // SAFETY: p is still live with its real layout.
    unsafe { A.dealloc(p, layout(small)) };
}

#[test]
fn metadata_and_uncarved_frees_are_rejected() {
    let p = fresh(16);
    let base = (p as usize & !(SEGMENT - 1)) as *mut u8;
    // SAFETY: deliberately invalid frees of a metadata offset and of an
    // offset far past the carved region in the same (memo-hit) segment.
    unsafe {
        A.dealloc(base.wrapping_add(0x1000), layout(16));
        A.dealloc(base.wrapping_add(SEGMENT - 4096), layout(16));
    }
    assert!(!reissued(base.wrapping_add(0x1000), 16));
    assert!(!reissued(base.wrapping_add(SEGMENT - 4096), 16));
    // SAFETY: p is still live.
    unsafe { A.dealloc(p, layout(16)) };
    assert!(reissued(p, 16));
}

#[test]
fn large_and_overaligned_frees_take_the_chain() {
    let big = Layout::from_size_align(1 << 20, 8).unwrap();
    let wide = Layout::from_size_align(64, 64).unwrap();
    // SAFETY: nonzero sizes; each block is freed once with its layout.
    unsafe {
        let l = A.alloc(big);
        let w = A.alloc(wide);
        assert!(!l.is_null() && !w.is_null());
        A.dealloc(l, big);
        A.dealloc(w, wide);
        let w2 = A.alloc(wide);
        assert!(!w2.is_null());
        A.dealloc(w2, wide);
    }
}
