//! R2-1 regression (gap 2): `AllocCore::realloc` is a SAFE `pub fn`, yet its
//! move leg used to copy `old_layout.size().min(new_size)` bytes OUT of `ptr`
//! — trusting the caller-supplied `old_layout` exactly as the `unsafe`
//! `GlobalAlloc::realloc` does. A safe caller passing a bogus `old_layout`
//! (e.g. claiming 8 MiB for a 16-byte block) drove an out-of-bounds read: the
//! write side was bounded by `new_size` (`copy <= new_size <= the fresh
//! allocation`), but the READ of `copy` bytes escaped the 16-byte block's
//! 4 MiB segment.
//!
//! ## The fix
//!
//! The move leg now bounds the read by the block's ACTUAL committed span
//! (`AllocCore::safe_payload_read_span`, derived from the segment header
//! WITHOUT trusting `old_layout`): if `old_layout.size()` exceeds that span
//! the layout is inconsistent with the block and `realloc` returns null (`ptr`
//! untouched) instead of copying out of bounds.
//!
//! ## Counterfactual (non-vacuity)
//!
//! RED (fix reverted): `realloc(p_16b, 8 MiB layout, 8 MiB)` reaches the move
//! leg, allocates an 8 MiB destination, then `copy_nonoverlapping`s 8 MiB out
//! of a 16-byte block in a 4 MiB segment — a read that escapes the segment's
//! OS allocation. Under miri this is flagged as an out-of-bounds pointer
//! access; under native it segfaults or returns non-null (both fail the
//! `is_null()` assertion). GREEN (fix present): the size-consistency check
//! rejects the bogus layout before any allocation/copy → null.
//!
//! `legit_realloc_still_succeeds` is the control proving the bound does NOT
//! over-reject a correct layout. This file is also the strict-miri target for
//! the OOB scenario (see `scripts/miri.mjs` MATRIX).

#![cfg(feature = "alloc-core")]

use core::alloc::Layout;

use sefer_alloc::AllocCore;

/// 8 MiB — comfortably larger than one SEGMENT (4 MiB), so a block claiming
/// this size cannot possibly fit in a single small-segment block's span. The
/// move leg's `copy = min(8 MiB, 8 MiB) = 8 MiB` would read 8 MiB out of a
/// 4 MiB segment.
const BOGUS_OLD: usize = 8 * 1024 * 1024;

/// THE R2-1 gap-2 scenario: a safe caller claims a 16-byte block is 8 MiB and
/// asks to grow it to 8 MiB. The fix bounds the read to the segment's
/// committed span (~4 MiB) and rejects the inconsistent layout → null, with
/// no out-of-bounds read.
#[test]
fn realloc_with_oversized_old_layout_returns_null_not_oob() {
    let mut ac = AllocCore::new().expect("AllocCore::new");
    // A real 16-byte small block (class[0], block_size = MIN_BLOCK = 16).
    let small = Layout::from_size_align(16, 16).unwrap();
    let p = ac.alloc(small);
    assert!(!p.is_null(), "setup: 16-byte alloc failed");
    // SAFETY: `p` is valid for 16 bytes per the alloc contract (M1).
    unsafe { core::ptr::write_bytes(p, 0xA5, 16) };

    let bogus = Layout::from_size_align(BOGUS_OLD, 16).unwrap();
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
    let result = unsafe { ac.realloc(p, bogus, BOGUS_OLD) };
    assert!(
        result.is_null(),
        "realloc with a bogus oversized old_layout must return null (R2-1 bound), \
         not fall through to an 8 MiB out-of-segment read"
    );

    // The original block is intact: a null return is a realloc FAILURE, so per
    // the `GlobalAlloc` contract the old allocation is left untouched.
    // SAFETY: `p` is still valid for 16 bytes (the realloc returned null).
    unsafe {
        assert_eq!(
            core::ptr::read(p),
            0xA5,
            "original block disturbed by the rejected realloc"
        );
    }
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p, small) };
}

/// Control: a CORRECT layout is never rejected by the new bound. Grow a
/// 16-byte block to 64 bytes (both small; `copy = min(16, 64) = 16`, well
/// within the segment span). Must succeed (non-null), proving the bound is
/// not a blanket reject that would break legitimate reallocs.
#[test]
fn legit_realloc_still_succeeds_under_the_bound() {
    let mut ac = AllocCore::new().expect("AllocCore::new");
    let old = Layout::from_size_align(16, 16).unwrap();
    let p = ac.alloc(old);
    assert!(!p.is_null(), "setup: 16-byte alloc failed");
    // SAFETY: `p` is valid for 16 bytes.
    unsafe { core::ptr::write_bytes(p, 0x5A, 16) };

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
    let new_ptr = unsafe { ac.realloc(p, old, 64) };
    assert!(
        !new_ptr.is_null(),
        "a legit realloc (16 -> 64, correct layout) must succeed under the R2-1 bound"
    );

    // The min(old, new) = 16-byte prefix is preserved.
    // SAFETY: `new_ptr` is valid for 64 bytes; the first 16 are the copy.
    unsafe {
        for i in 0..16 {
            assert_eq!(
                core::ptr::read(new_ptr.add(i)),
                0x5A,
                "prefix byte {i} not preserved by the legit realloc"
            );
        }
    }
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(new_ptr, Layout::from_size_align(64, 16).unwrap()) };
}

/// A Large block that FILLS its segment's span is NOT rejected: the bound is
/// `span_usable - payload_offset`, and a legitimate `old_layout.size()` equals
/// the stored `large_size` which fits exactly. This guards against an
/// off-by-one in the `>` comparison that would break full-span large reallocs.
#[test]
fn realloc_large_block_filling_span_is_not_falsely_rejected() {
    let mut ac = AllocCore::new().expect("AllocCore::new");
    // 1 MiB large alloc — well above SMALL_MAX, routed to a dedicated Large
    // segment whose span_usable is at least one SEGMENT (4 MiB). old_layout
    // size (1 MiB) is far below the ~4 MiB read bound, so the grow must
    // succeed.
    let large = Layout::from_size_align(1024 * 1024, 16).unwrap();
    let p = ac.alloc(large);
    assert!(!p.is_null(), "setup: 1 MiB large alloc failed");
    // SAFETY: valid for 1 MiB.
    unsafe { core::ptr::write_bytes(p, 0x11, 1024 * 1024) };

    // Grow 1 MiB -> 2 MiB: both fit one 4 MiB Large segment; OPT-G may take the
    // in-place path or the move leg runs — either way the bound must NOT
    // reject (2 MiB <= ~4 MiB span).
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer is a live allocation made with the matching old_layout, freed exactly once; the old pointer is consumed on a non-null return.
    let new_ptr = unsafe { ac.realloc(p, large, 2 * 1024 * 1024) };
    assert!(
        !new_ptr.is_null(),
        "a legit large realloc (1 -> 2 MiB) must succeed under the R2-1 bound"
    );
    // SAFETY: first 1 MiB is the preserved prefix.
    unsafe {
        assert_eq!(
            core::ptr::read(new_ptr),
            0x11,
            "large realloc did not preserve the prefix"
        );
    }
    // SAFETY (R6-MS-1/2): `new_ptr` is the live result of the preceding
    // `realloc`, made with the matching 2 MiB layout, freed exactly once here.
    unsafe {
        ac.dealloc(
            new_ptr,
            Layout::from_size_align(2 * 1024 * 1024, 16).unwrap(),
        )
    };
}
