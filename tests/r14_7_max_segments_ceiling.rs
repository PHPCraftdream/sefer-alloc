//! R14-7 (task #292) — regression guard for the raised `MAX_SEGMENTS`
//! ceiling.
//!
//! ## Background
//!
//! R13-8 (`docs/perf/R13_8_MEDIUM_WORKING_SET_JUDGE.md`) precisely located a
//! 100%-reproducible capacity cliff: every Large allocation consumes exactly
//! one `SegmentTable` slot, independent of feature combination and
//! independent of `alloc-decommit` (which only recycles a slot once its
//! object is *freed* — a live, never-freed working set never benefits from
//! it). With `MAX_SEGMENTS = 1024` the usable ceiling for
//! simultaneously-live Large objects was exactly 1023 (slot 0 is
//! permanently reserved for the primordial segment). R14-7 raised
//! `MAX_SEGMENTS` to 4096 after measuring the raise is cheap on every axis
//! that matters (idle-process RSS unchanged; primordial-segment metadata
//! footprint grows by ~84 KiB inside a ~3.9 MiB fixed budget; no
//! non-linear scan-path degradation, since the `production`-default
//! `alloc-segment-directory` bounds the hot lookup path independent of
//! table size) — see
//! `docs/perf/R14_7_EXPANDABLE_SEGMENT_TABLE_DESIGN.md` for the follow-on
//! design if a workload ever needs more than `MAX_SEGMENTS - 1`
//! simultaneously-live Large objects.
//!
//! ## What this test guards against
//!
//! A future accidental revert (or retune) of `MAX_SEGMENTS` back toward a
//! low value without updating this guard, OR a regression that makes the
//! ceiling something other than the expected `MAX_SEGMENTS - 1`.
//!
//! ## Density-agnostic by construction (R12-14/task #265 convention)
//!
//! The expected ceiling is read at runtime via
//! [`sefer_alloc::AllocCore::dbg_max_segments`] rather than hardcoded as a
//! literal — this test must keep passing unchanged if `MAX_SEGMENTS` is
//! retuned again in a future round, and must keep passing under
//! `--all-features` (which does not change `MAX_SEGMENTS` itself, but this
//! test avoids assuming so).
//!
//! `#[cfg_attr(miri, ignore)]` — reserves up to `MAX_SEGMENTS` real OS
//! segments (multi-GiB of VA); too slow under miri's interpreter overhead.
//! Correctness of the underlying slot bookkeeping is covered by
//! `tests/segment_table_recycle.rs` and other lighter-weight tests that DO
//! run under miri.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;

use sefer_alloc::{AllocCore, SegmentLayout};

/// The usable ceiling for simultaneously-live Large objects is exactly
/// `MAX_SEGMENTS - 1` (slot 0 is the primordial segment's, permanently) —
/// reproduced under `production`'s actual shipping feature composition
/// (`alloc-decommit` included), confirming R13-8's finding that
/// `alloc-decommit`'s slot-recycle does NOT lift this ceiling for a live
/// (never-freed) working set: recycle only helps once an object is freed.
#[cfg_attr(miri, ignore)]
#[test]
fn live_large_objects_ceiling_is_exactly_max_segments_minus_one() {
    let mut ac = AllocCore::new().expect("primordial");

    let large_size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    let layout = Layout::from_size_align(large_size, SegmentLayout::PAGE).unwrap();

    let max_segments = AllocCore::dbg_max_segments();
    let expected_ceiling = max_segments - 1;

    // Push well past the expected ceiling; count exactly how many succeed
    // before the first null.
    let attempt = max_segments + 64;
    let mut ptrs = Vec::with_capacity(attempt);
    let mut achieved = 0usize;
    for _ in 0..attempt {
        let p = ac.alloc(layout);
        if p.is_null() {
            break;
        }
        achieved += 1;
        ptrs.push(p);
    }

    assert_eq!(
        achieved, expected_ceiling,
        "expected exactly MAX_SEGMENTS-1 ({expected_ceiling}) simultaneously-live \
         Large objects to succeed (MAX_SEGMENTS={max_segments}, primordial \
         segment permanently occupies slot 0) before the first null alloc; \
         got {achieved} — either the ceiling moved without this guard being \
         updated, or a slot is being lost/gained somewhere in the register/\
         recycle bookkeeping"
    );

    // The very next alloc must ALSO be null (the wall is total, not a single
    // transient miss) — matches R13-8's "binary and total" characterisation.
    let one_more = ac.alloc(layout);
    assert!(
        one_more.is_null(),
        "alloc past the ceiling must keep returning null (graceful OOM), not \
         intermittently succeed"
    );

    // Cleanup: free everything we got (Drop would also release these, but
    // freeing explicitly exercises the Large dealloc path one more time and
    // keeps this test's resource footprint tidy under repeated local runs).
    for p in ptrs {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the
        // pointer was returned by a prior matching alloc in this test, is
        // live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
}

/// After freeing every live object at the ceiling, a fresh alloc must
/// succeed again (the slot-recycle path is not itself broken by the raised
/// `MAX_SEGMENTS` — this is a sanity companion to the ceiling test above,
/// isolating "we can still get UP TO the ceiling again after a full
/// free-all" from "we cannot exceed the ceiling while everything stays
/// live").
#[cfg_attr(miri, ignore)]
#[test]
fn ceiling_is_not_permanent_after_freeing_everything() {
    let mut ac = AllocCore::new().expect("primordial");

    let large_size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    let layout = Layout::from_size_align(large_size, SegmentLayout::PAGE).unwrap();

    let max_segments = AllocCore::dbg_max_segments();
    let expected_ceiling = max_segments - 1;

    // First wave: fill to the ceiling.
    let mut ptrs = Vec::with_capacity(expected_ceiling);
    for _ in 0..expected_ceiling {
        let p = ac.alloc(layout);
        assert!(
            !p.is_null(),
            "first wave must reach the ceiling without null"
        );
        ptrs.push(p);
    }
    assert!(
        ac.alloc(layout).is_null(),
        "table must be full after reaching the ceiling"
    );

    // Free everything.
    for p in ptrs {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the
        // pointer was returned by a prior matching alloc in this test, is
        // live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }

    // Second wave: must be able to reach the ceiling again (slots recycled).
    let mut second_wave = Vec::with_capacity(expected_ceiling);
    for i in 0..expected_ceiling {
        let p = ac.alloc(layout);
        assert!(
            !p.is_null(),
            "second wave alloc null at i={i}/{expected_ceiling} — slots were \
             not actually recycled after freeing the first wave"
        );
        second_wave.push(p);
    }

    for p in second_wave {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the
        // pointer was returned by a prior matching alloc in this test, is
        // live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
}
