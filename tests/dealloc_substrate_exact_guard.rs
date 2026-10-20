//! F15 (review, B4 state-byte) — the substrate `dealloc_small` EXACT guard.
//!
//! Every internal caller of `AllocCore::dealloc_small` passes a currently-
//! issued block START of exactly `class_idx` (the user free face via
//! `mem_impl::dealloc_at_base`, and the freshly-carved refill blocks in
//! `find_segment/scan.rs::carve_block_with_refill`), so the guard requires
//! the exact issued byte `encode(class_idx, LIVE)` plus a 16 B START
//! alignment — not merely "not FREE". This file pins the strengthened guard:
//! an interior (16-aligned INSIDE a 64 B block), a never-issued (zero-byte)
//! and a wrong-class address are each a benign no-op — no byte changes, no
//! bin membership, no reissue.
//!
//! Mutants (each RED here):
//!  * restore the state-only-FREE decode (`byte != 0 && byte >> 6 == FREE`)
//!    → the never-issued probe (zero byte) is ACCEPTED and pushed onto the
//!    16 B bin → the storm reissues it (`f15_never_issued_free_is_a_noop`).
//!  * drop the alignment term → the `p+1` probe reads the start granule's
//!    LIVE byte and is accepted → the storm reissues `p+1`
//!    (`f15_interior_free_is_a_noop`).
//!  * make the compare state-only (drop the class field) → the wrong-class
//!    probe pushes `p` onto the 32 B bin → the 32 B storm reissues it
//!    (`f15_wrong_class_free_is_a_noop`).

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use core::alloc::Layout;

use sefer_alloc::AllocCore;

fn l16() -> Layout {
    Layout::from_size_align(16, 16).unwrap()
}
fn l32() -> Layout {
    Layout::from_size_align(32, 16).unwrap()
}
fn l64() -> Layout {
    Layout::from_size_align(64, 16).unwrap()
}

/// Interior free: a 16-ALIGNED address strictly inside a live 64 B block.
/// Passes the old alignment-only intuition (it IS 16-aligned) but its byte
/// is the interior granule's И1-zero — and without the START-alignment term
/// even `p+1..p+15` would index the start's LIVE byte.
#[test]
fn f15_interior_free_is_a_noop() {
    let mut ac = AllocCore::new().expect("primordial");
    let p = ac.alloc(l64());
    assert!(!p.is_null());
    let anchor = ac.alloc(l16());
    assert!(!anchor.is_null());

    let interior = unsafe { p.add(16) };
    let mut buf = [0u8; 1];
    // SAFETY: `p`'s segment is owned by `ac`.
    unsafe { ac.dbg_state_bytes_for(p, &mut buf) };
    let byte_before = buf[0];

    // SAFETY (guard probe): `interior` is inside the live block `p`; the
    // strengthened guard must reject it as a no-op.
    unsafe { ac.dealloc(interior, l16()) };

    // SAFETY: `p`'s segment is owned by `ac`.
    unsafe { ac.dbg_state_bytes_for(p, &mut buf) };
    assert_eq!(
        buf[0], byte_before,
        "interior free changed the block's state byte"
    );
    // The interior address is never issued, and `p` (still live) is issued
    // at most once across a 16 B storm.
    let mut seen_interior = 0;
    let mut seen_p = 0;
    let mut storm = Vec::new();
    for _ in 0..64 {
        let q = ac.alloc(l16());
        assert!(!q.is_null());
        if q == interior {
            seen_interior += 1;
        }
        if q == p {
            seen_p += 1;
        }
        storm.push(q);
    }
    assert_eq!(seen_interior, 0, "interior address was issued");
    assert!(seen_p <= 1, "live block reissued {seen_p}x");
    for q in storm {
        // SAFETY: live, matching layout, freed exactly once here.
        unsafe { ac.dealloc(q, l16()) };
    }
    // The genuine free still works, exactly once.
    // SAFETY: live, matching layout, freed exactly once here.
    unsafe { ac.dealloc(p, l64()) };
    let r = ac.alloc(l64());
    assert_eq!(r, p, "the block was lost to the rejected interior free");
    // SAFETY: live, matching layout, freed exactly once here.
    unsafe { ac.dealloc(r, l64()) };
    // SAFETY: live anchor, matching layout, freed exactly once here.
    unsafe { ac.dealloc(anchor, l16()) };
}

/// Never-issued address: far beyond the fresh segment's bump, its state byte
/// is zero (invariant И1). The former state-only-FREE guard ACCEPTED it
/// (zero is "not FREE") and pushed it onto the bin — free-list corruption.
#[test]
fn f15_never_issued_free_is_a_noop() {
    let mut ac = AllocCore::new().expect("primordial");
    let p = ac.alloc(l16());
    assert!(!p.is_null());
    let anchor = ac.alloc(l16());
    assert!(!anchor.is_null());

    let ghost = unsafe { p.add(4096) };
    // SAFETY (guard probe): `ghost` is a never-issued address inside this
    // core's fresh segment payload (bump is far below); the guard must
    // reject the free as a no-op.
    unsafe { ac.dealloc(ghost, l16()) };

    let mut storm = Vec::new();
    for _ in 0..64 {
        let q = ac.alloc(l16());
        assert!(!q.is_null());
        assert_ne!(q, ghost, "never-issued address entered the free list");
        storm.push(q);
    }
    for q in storm {
        // SAFETY: live, matching layout, freed exactly once here.
        unsafe { ac.dealloc(q, l16()) };
    }
    // The genuine free still works.
    // SAFETY: live, matching layout, freed exactly once here.
    unsafe { ac.dealloc(p, l16()) };
    let r = ac.alloc(l16());
    assert_eq!(r, p);
    // SAFETY: live, matching layout, freed exactly once here.
    unsafe { ac.dealloc(r, l16()) };
    // SAFETY: live anchor, matching layout, freed exactly once here.
    unsafe { ac.dealloc(anchor, l16()) };
}

/// Wrong-class free of a LIVE block at the substrate: `encode(c16, LIVE) !=
/// encode(c32, LIVE)` → no-op; the block stays issued and the 32 B storm
/// never reissues it; the correct free afterwards still works.
#[test]
fn f15_wrong_class_free_is_a_noop() {
    let mut ac = AllocCore::new().expect("primordial");
    let p = ac.alloc(l16());
    assert!(!p.is_null());
    let anchor = ac.alloc(l32());
    assert!(!anchor.is_null());

    let mut buf = [0u8; 1];
    // SAFETY: `p`'s segment is owned by `ac`.
    unsafe { ac.dbg_state_bytes_for(p, &mut buf) };
    let byte_before = buf[0];

    // SAFETY (guard probe): deliberate wrong-class no-op probe.
    unsafe { ac.dealloc(p, l32()) };

    // SAFETY: `p`'s segment is owned by `ac`.
    unsafe { ac.dbg_state_bytes_for(p, &mut buf) };
    assert_eq!(
        buf[0], byte_before,
        "wrong-class free changed the state byte"
    );
    let mut storm = Vec::new();
    for _ in 0..64 {
        let q = ac.alloc(l32());
        assert!(!q.is_null());
        assert_ne!(
            q, p,
            "wrong-class free pushed the 16 B block onto the 32 B bin"
        );
        storm.push(q);
    }
    for q in storm {
        // SAFETY: live, matching layout, freed exactly once here.
        unsafe { ac.dealloc(q, l32()) };
    }
    // The correct-class free works.
    // SAFETY: live, matching layout, freed exactly once here.
    unsafe { ac.dealloc(p, l16()) };
    let r = ac.alloc(l16());
    assert_eq!(r, p);
    // SAFETY: live, matching layout, freed exactly once here.
    unsafe { ac.dealloc(r, l16()) };
    // SAFETY: live anchor, matching layout, freed exactly once here.
    unsafe { ac.dealloc(anchor, l32()) };
}
