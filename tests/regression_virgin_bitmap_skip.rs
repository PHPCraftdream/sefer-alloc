//! PERF-PASS-2 (G5/C1, task #50) → B4 (variant в) — poison-then-assert
//! counterfactual for the fresh-segment STATE-BYTE virgin-init elision.
//!
//! `AllocCore::reserve_small_segment` and `bootstrap::primordial` SKIP the
//! explicit `StateByte::init_in_place` zero-write under `cfg(not(miri))`
//! (see the matching comments at both call sites in `reserve.rs` /
//! `bootstrap.rs`). The soundness argument is: the memory backing a
//! freshly-reserved segment is handed back by the OS already zeroed (Windows
//! `MEM_COMMIT` demand-zero; POSIX anonymous `mmap` zero-fill), and the state
//! byte's "never issued" encoding IS all-zeros — so the explicit write was
//! always a tautology on this exact code path (never on any other path: the
//! decommit-reuse full-reset performs its own SCOPED state-byte zeroing).
//! The same argument was first documented for the former
//! `AllocBitmap`/`MagazineBitmap` init elision (this file's original form,
//! `git show 897463e6:tests/regression_virgin_bitmap_skip.rs`); B4 removed
//! those dead bitmaps and routed their whole-footprint zero checks onto the
//! authoritative per-granule STATE-BYTE region.
//!
//! This is NOT the rejected P4(b) `alloc_zeroed` virgin-skip: that NO-GO was
//! about *user-visible payload* virginity, where a RECYCLED (not freshly-
//! reserved) mapping's "was it definitely never written since the OS handed
//! it out" question is unsound under macOS `MADV_DONTNEED` laziness. Here the
//! virgin signal is exact (this function runs immediately after
//! `Segment::reserve`, nothing else has touched the segment yet) and the
//! target is allocator metadata, not payload the user could have written.
//!
//! ## What "poison-then-assert" means here
//!
//! We cannot literally poison the bytes BEFORE the crate's own reserve call
//! (that would require intercepting between the OS `mmap`/`VirtualAlloc` and
//! the allocator's own init, which is not a seam this crate exposes — and
//! deliberately so: exposing it would be a bigger footgun than the test is
//! worth). Instead this test proves the soundness argument the elision relies
//! on ONE LEVEL UP, empirically, on THIS platform, via the crate's own
//! reserve path:
//!
//!   1. **T1 — raw empirical zero-read-back (primordial).** Reserve a fresh
//!      `AllocCore` (which reserves the PRIMORDIAL segment — one of the two
//!      call sites the skip applies to), alloc ONE block (the minimum needed
//!      to obtain a pointer into the segment for `dbg_state_bytes_for`'s
//!      (ptr -> base) contract), and read back its STATE-BYTE region
//!      EXCLUDING the narrow granule span the one alloc's own refill batch
//!      legitimately populates (`carve_block_with_refill` carves 31 extra
//!      same-class blocks and correctly frees them back to the BinTable). If
//!      the OS-zero guarantee did NOT hold on this platform (or the skip were
//!      wrongly applied to a non-virgin path), this read would observe
//!      non-zero garbage outside that span. It reads all zeros — this IS the
//!      empirical proof the skip's soundness argument depends on.
//!   2. **T2 — same proof for a SECOND fresh segment**, forcing
//!      `reserve_small_segment` (the non-primordial call site) by exhausting
//!      the primordial's small-class capacity, stopping at the FIRST pointer
//!      carved from the fresh segment, and reading back the same
//!      excluded-span logic as T1 — isolating "OS handed back zero" from "the
//!      allocator's own bookkeeping happens to read as zero".
//!   3. **T3 — behavioural counterfactual.** With the skip active, drive a
//!      full M2 double-free-guard exercise (alloc, double-free, re-alloc) on
//!      blocks carved from the just-reserved segment. If the skip had left
//!      SOME state bytes non-zero (garbage "already free / in-magazine"), the
//!      very first `alloc` from that class would either return null or a
//!      later double-free guard would misfire — this test's shape mirrors
//!      `tests/double_free_guard.rs` exactly, so a broken skip would fail it
//!      the same way a broken init has always failed that file's tests
//!      (proving THIS test is not vacuous).
//!
//! ## "No third path" audit (documented, not a runtime test)
//!
//! The state byte's only UNCONDITIONAL reset lives in the decommit-reset
//! full-reset branch (`decommit_empty_segment_impl`,
//! `release_follows = false`), which — since task #223 (R8-10) removed
//! decommit-on-pool-admission — has no production caller today. Historically
//! that was the "pool retain" path that re-initialised a non-virgin segment
//! before reuse. `decommit_empty_segment_for_release` (the
//! `release_follows = true` variant) performs no metadata reset at all — the
//! whole reservation is about to be released to the OS, so a future re-reserve
//! of that address range goes through one of the two (skipped-under-`not(miri)`)
//! virgin-reserve sites again. Since #223, pool admission performs NO decommit
//! call at all, so the release variant is the ONLY decommit path exercised
//! today.

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use core::alloc::Layout;
use std::collections::HashSet;

use sefer_alloc::alloc_core::AllocCore;

/// T1: a freshly-reserved `AllocCore` (primordial segment) reads back
/// all-zero across its STATE-BYTE region before any alloc/dealloc call
/// TOUCHES it (beyond the one alloc's own refill span). This is the empirical
/// anchor for the elision's soundness argument.
///
/// IMPORTANT: `ac.alloc(layout)` on a class miss does NOT carve just the ONE
/// requested block — the non-`fastbin` substrate path
/// (`carve_block_with_refill`) also carves `REFILL_BATCH = 31` EXTRA blocks of
/// the SAME class and pushes each through `dealloc_small`, which legitimately
/// `mark_free`s them (a real, correct free-list population — unrelated to
/// this task). So after one 16 B alloc, up to 32 consecutive 16 B-class
/// granules starting at the payload's first block legitimately read non-zero
/// (FREE). This test therefore checks the state bytes BEYOND that maximum
/// same-class refill span (32 blocks × 16 B = 32 granules) — the OS-zero
/// assumption is about to be exercised there just as much as anywhere else
/// in the segment, so this is still a meaningful, non-vacuous check of the
/// untouched majority of the region.
#[test]
fn t1_primordial_state_bytes_read_zero_before_any_traffic() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(16, 8).unwrap();
    let p = ac.alloc(layout);
    assert!(!p.is_null());

    // The requested block sits at `primordial_meta_end()` (the payload
    // start); its granule index is `off >> 4`. The refill batch
    // (`REFILL_BATCH = 31` extra same-class blocks, `carve_block_with_refill`)
    // covers at most the next 31 × `block_size` bytes of payload past that,
    // i.e. at most 32 total blocks (1 requested + 31 refilled).
    let off = (p as usize) - ((p as usize) & !((4 * 1024 * 1024) - 1));

    // B4 (в): STATE-BYTE region check. Everything outside the legitimately-refilled
    // granule span must read zero ("never issued"); the issued block itself
    // must read LIVE with a nonzero class field. An `AllocCore` alone never
    // parks blocks in a magazine, so no MAG state can legitimately appear.
    const STATE_FOOTPRINT: usize = 256 * 1024; // StateByte::FOOTPRINT = SEGMENT / MIN_BLOCK
    let mut state_buf = vec![0u8; STATE_FOOTPRINT];
    // SAFETY: `p` is a live allocation owned by `ac`.
    unsafe { ac.dbg_state_bytes_for(p, &mut state_buf) };
    const REFILL_BATCH_MARGIN_GRANULES: usize = 32; // 1 requested + 31 refilled
    let issued_granule = off >> 4;
    let state_skip_to = issued_granule + REFILL_BATCH_MARGIN_GRANULES;
    assert!(state_skip_to <= STATE_FOOTPRINT);
    assert!(
        state_buf[state_skip_to..].iter().all(|&b| b == 0),
        "freshly-reserved primordial segment's STATE-BYTE region did not \
         read back all-zero past the legitimately-refilled granule span — \
         the OS-zero assumption the virgin-init skip depends on does not \
         hold on this platform (or the skip fired on a non-virgin path). \
         First non-zero granule at {:?}.",
        state_buf[state_skip_to..]
            .iter()
            .position(|&b| b != 0)
            .map(|i| i + state_skip_to)
    );
    assert!(
        state_buf[..issued_granule].iter().all(|&b| b == 0),
        "STATE-BYTE region granules before the payload's first block did \
         not read back all-zero. First non-zero granule at {:?}.",
        state_buf[..issued_granule].iter().position(|&b| b != 0)
    );
    assert_ne!(
        state_buf[issued_granule], 0,
        "issued block's state byte is zero"
    );
    assert_eq!(
        state_buf[issued_granule] >> 6,
        0,
        "issued block's state byte is not LIVE"
    );

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p, layout) };
}

/// T2: force a SECOND, non-primordial small-segment reservation
/// (`reserve_small_segment`) by exhausting the primordial's small-class
/// capacity with one class, then read back the TAIL of that fresh segment's
/// STATE-BYTE region — far past any granule the handful of carve/refill
/// calls that crossed into it could have touched — isolating "OS handed back
/// zero" from "our own bookkeeping happens to read zero".
#[test]
fn t2_fresh_small_segment_state_bytes_read_zero_for_untouched_classes() {
    let mut ac = AllocCore::new().expect("primordial");

    // Drive 256 B allocations (a different class from T1's 16 B) until a
    // SECOND distinct 4 MiB-aligned region is observed, then STOP
    // immediately (do not keep allocating into the fresh segment) — the
    // point is to touch as little of the fresh segment's state region as
    // possible before reading it back, so the untouched tail is a meaningful
    // check.
    let layout = Layout::from_size_align(256, 8).unwrap();
    let mut ptrs: Vec<*mut u8> = Vec::new();
    let mut segment_bases: HashSet<usize> = HashSet::new();
    let mut fresh_ptr: Option<*mut u8> = None;
    for _ in 0..200_000 {
        let p = ac.alloc(layout);
        if p.is_null() {
            break;
        }
        let base = (p as usize) & !((4 * 1024 * 1024) - 1);
        let is_new_segment = segment_bases.insert(base);
        ptrs.push(p);
        if segment_bases.len() >= 2 && is_new_segment {
            // `p` is the FIRST pointer carved from the freshly-reserved
            // segment — record it and stop driving more allocations into it.
            fresh_ptr = Some(p);
            break;
        }
    }
    let fresh_ptr = fresh_ptr.unwrap_or_else(|| {
        panic!(
            "test setup failed to force a second small segment reservation \
             (only touched {} distinct 4 MiB-aligned regions) — increase the \
             allocation count",
            segment_bases.len()
        )
    });

    // B4 (в): the STATE-BYTE region of the SECOND (non-primordial)
    // virgin-reserve call site. The refill span covers 32 blocks of 256 B =
    // 512 granules (granule = MIN_BLOCK, independent of class size);
    // everything past it must read zero ("never issued"), the issued block
    // itself LIVE with a nonzero class field.
    const STATE_FOOTPRINT: usize = 256 * 1024; // StateByte::FOOTPRINT
    let mut state_buf = vec![0u8; STATE_FOOTPRINT];
    // SAFETY: `fresh_ptr` is a live allocation owned by `ac`.
    unsafe { ac.dbg_state_bytes_for(fresh_ptr, &mut state_buf) };
    let base = (fresh_ptr as usize) & !((4 * 1024 * 1024) - 1);
    let off = (fresh_ptr as usize) - base;
    let issued_granule = off >> 4;
    // Each 256 B block spans 16 granules; 1 requested + 31 refilled blocks
    // ⇒ 512 granules.
    const REFILL_BATCH_MARGIN_BLOCKS: usize = 32;
    let state_skip_to = issued_granule + REFILL_BATCH_MARGIN_BLOCKS * (layout.size() / 16);
    assert!(state_skip_to <= STATE_FOOTPRINT);
    assert!(
        state_buf[state_skip_to..].iter().all(|&b| b == 0),
        "freshly-reserved (non-primordial) small segment's STATE-BYTE \
         region did not read back all-zero past the refilled granule span — \
         the OS-zero assumption failed, or the skip fired on a non-virgin \
         segment. First non-zero granule at {:?}.",
        state_buf[state_skip_to..]
            .iter()
            .position(|&b| b != 0)
            .map(|i| i + state_skip_to)
    );
    assert!(
        state_buf[..issued_granule].iter().all(|&b| b == 0),
        "STATE-BYTE region granules before the fresh segment's first block \
         did not read back all-zero. First non-zero granule at {:?}.",
        state_buf[..issued_granule].iter().position(|&b| b != 0)
    );
    assert_ne!(
        state_buf[issued_granule], 0,
        "issued block's state byte is zero"
    );
    assert_eq!(
        state_buf[issued_granule] >> 6,
        0,
        "issued block's state byte is not LIVE"
    );
    for p in ptrs {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
}

/// T3: behavioural counterfactual — the M2 double-free guard must still work
/// correctly on blocks carved from a freshly-reserved (skip-elided-init)
/// segment. Mirrors `tests/double_free_guard.rs`'s shape exactly: if the
/// elided init left the state bytes in anything other than the correct
/// all-zero "never issued" starting state, either the first alloc from a
/// virgin block would misbehave, or a double-free on it would fail to no-op
/// (both
/// failure modes this test would catch, proving it is not vacuous the same
/// way `double_free_guard.rs` is not vacuous against ITS guard).
#[test]
fn t3_double_free_guard_still_correct_on_freshly_reserved_segment() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(16, 16).unwrap();

    let p = ac.alloc(layout);
    assert!(!p.is_null());

    // A virgin (never-freed) block's state byte must read "never issued"
    // (all-zero) — this is the exact state the elided init is supposed to
    // produce implicitly via the OS's zero pages, matching what the removed
    // explicit zero-write used to produce explicitly.
    assert!(
        !ac.dbg_is_free_for(p),
        "virgin block's state byte read FREE before any free() call — \
         the init-elision left the state region in the wrong starting state"
    );

    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p, layout) }; // legitimate free -> FREE state byte
                                      // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p, layout) }; // double-free -> must no-op (M2 guard)
                                      // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { ac.dealloc(p, layout) }; // triple-free -> still a no-op

    const K: usize = 32;
    let mut seen: HashSet<usize> = HashSet::new();
    let mut got = Vec::new();
    for _ in 0..K {
        let q = ac.alloc(layout);
        assert!(!q.is_null(), "alloc returned null after double-free");
        assert!(
            seen.insert(q as usize),
            "allocator handed out a DUPLICATE pointer {q:p} after a \
             double-free on a freshly-reserved (init-elided) segment — the \
             M2 guard failed under the virgin-init skip"
        );
        got.push(q);
    }
    for q in got {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { ac.dealloc(q, layout) };
    }
}
