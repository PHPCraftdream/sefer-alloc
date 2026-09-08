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
//!
//! ## Why these two tests are serialized, and why a stop-early is not a
//! failure (`docs/CORRECTNESS_OPEN_ITEMS.md` item 143)
//!
//! Both tests below drive an `AllocCore` to its full `MAX_SEGMENTS - 1`
//! ceiling. `AllocCore::table` is a per-INSTANCE field, not a process-wide
//! one, so two `AllocCore`s reach their ceilings independently — and libtest
//! runs the tests in one file concurrently by default. The two fills
//! therefore overlap, and the process's PEAK segment demand is twice what
//! either test alone needs. That doubling is self-inflicted and buys
//! nothing: neither test is about concurrency. `CEILING_LOCK` serializes
//! them so only one full-ceiling working set is live at a time.
//!
//! Serializing halves the peak but cannot make the demand unconditionally
//! satisfiable — the remaining requirement is still thousands of live
//! segment reservations, and whether the OS grants them depends on a
//! system-wide budget (on Windows, RAM + pagefile; a refusal surfaces as
//! `ERROR_COMMITMENT_LIMIT`/1455) shared with every other process on the
//! machine. A refused reservation and an exhausted `SegmentTable` both
//! surface identically to the caller: `alloc` returns null. Asserting the
//! achieved count alone therefore cannot distinguish the ceiling under test
//! from a busy machine, which is precisely how this test earned a flake
//! card.
//!
//! `AllocCore::dbg_segments_reserve_failed_total()` closes that gap: its
//! DELTA across the fill loop counts reservations the kernel refused. Zero
//! means every null came from the allocator's own bookkeeping and the count
//! is a valid assertion; non-zero means the environment cut the run short,
//! and the run reports that distinctly instead of blaming the allocator.

#![cfg(all(feature = "alloc-core", feature = "internals"))]

use std::alloc::Layout;
use std::sync::{Mutex, MutexGuard};

use sefer_alloc::{AllocCore, SegmentLayout};

/// Serializes the two full-ceiling fills in this file — see the module doc.
static CEILING_LOCK: Mutex<()> = Mutex::new(());

/// Takes [`CEILING_LOCK`], tolerating poisoning: if one test panics while
/// holding it, the other must still run serialized rather than fail with an
/// unrelated `PoisonError`.
fn ceiling_lock() -> MutexGuard<'static, ()> {
    CEILING_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// Reports whether a fill that stopped short did so because the OS refused
/// to back another mapping, given the reserve-failure counter delta observed
/// across that fill. Returns `true` when the caller should treat the run as
/// environment-limited rather than a regression.
///
/// Deliberately requires BOTH conditions: a short fill AND an observed
/// kernel refusal. A fill that reaches the expected ceiling is asserted
/// normally no matter what the counter did, so this can never turn the
/// test's real assertion off.
fn stopped_early_by_the_os(achieved: usize, expected_ceiling: usize, refused: u64) -> bool {
    if achieved >= expected_ceiling || refused == 0 {
        return false;
    }
    eprintln!(
        "r14_7 ceiling fill stopped at {achieved}/{expected_ceiling} live Large objects after \
         the OS refused {refused} segment reservation(s) — this machine could not back the \
         working set (system-wide commit budget), so the achieved count says nothing about \
         MAX_SEGMENTS and is NOT asserted. This is an environment limit, not an allocator \
         regression: a slot-bookkeeping regression stops short with ZERO refused reservations. \
         See docs/CORRECTNESS_OPEN_ITEMS.md item 143."
    );
    true
}

/// The usable ceiling for simultaneously-live Large objects is exactly
/// `MAX_SEGMENTS - 1` (slot 0 is the primordial segment's, permanently) —
/// reproduced under `production`'s actual shipping feature composition
/// (`alloc-decommit` included), confirming R13-8's finding that
/// `alloc-decommit`'s slot-recycle does NOT lift this ceiling for a live
/// (never-freed) working set: recycle only helps once an object is freed.
#[cfg_attr(miri, ignore)]
#[test]
fn live_large_objects_ceiling_is_exactly_max_segments_minus_one() {
    let _serialized = ceiling_lock();
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
    let refused_before = AllocCore::dbg_segments_reserve_failed_total();
    for _ in 0..attempt {
        let p = ac.alloc(layout);
        if p.is_null() {
            break;
        }
        achieved += 1;
        ptrs.push(p);
    }
    let refused = AllocCore::dbg_segments_reserve_failed_total() - refused_before;

    if stopped_early_by_the_os(achieved, expected_ceiling, refused) {
        for p in ptrs {
            // SAFETY: as the cleanup loop at the end of this test — each
            // pointer came from a matching alloc above and is freed once.
            unsafe { ac.dealloc(p, layout) };
        }
        return;
    }

    assert_eq!(
        achieved, expected_ceiling,
        "expected exactly MAX_SEGMENTS-1 ({expected_ceiling}) simultaneously-live \
         Large objects to succeed (MAX_SEGMENTS={max_segments}, primordial \
         segment permanently occupies slot 0) before the first null alloc; \
         got {achieved} with {refused} OS reservation(s) refused — either the \
         ceiling moved without this guard being updated, or a slot is being \
         lost/gained somewhere in the register/recycle bookkeeping. (A \
         non-zero refusal count here would mean the machine, not the \
         allocator, cut the fill short — see item 143.)"
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
    let _serialized = ceiling_lock();
    let mut ac = AllocCore::new().expect("primordial");

    let large_size = SegmentLayout::SMALL_MAX + SegmentLayout::PAGE;
    let layout = Layout::from_size_align(large_size, SegmentLayout::PAGE).unwrap();

    let max_segments = AllocCore::dbg_max_segments();
    let expected_ceiling = max_segments - 1;

    // First wave: fill to the ceiling. A null here is ambiguous exactly as
    // in the test above, so collect what we got and let the reserve-failure
    // delta decide whether it is a regression or this machine's limit.
    let mut ptrs = Vec::with_capacity(expected_ceiling);
    let refused_before = AllocCore::dbg_segments_reserve_failed_total();
    for _ in 0..expected_ceiling {
        let p = ac.alloc(layout);
        if p.is_null() {
            break;
        }
        ptrs.push(p);
    }
    let refused = AllocCore::dbg_segments_reserve_failed_total() - refused_before;
    if stopped_early_by_the_os(ptrs.len(), expected_ceiling, refused) {
        for p in ptrs {
            // SAFETY: each pointer came from a matching alloc above and is
            // freed exactly once here.
            unsafe { ac.dealloc(p, layout) };
        }
        return;
    }
    assert_eq!(
        ptrs.len(),
        expected_ceiling,
        "first wave must reach the ceiling without null; got {} with {refused} OS \
         reservation(s) refused",
        ptrs.len()
    );
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
    // Same OS-refusal caveat as the first wave — a null is only evidence
    // against the recycle path when the kernel refused nothing.
    let mut second_wave = Vec::with_capacity(expected_ceiling);
    let refused_before = AllocCore::dbg_segments_reserve_failed_total();
    for _ in 0..expected_ceiling {
        let p = ac.alloc(layout);
        if p.is_null() {
            break;
        }
        second_wave.push(p);
    }
    let refused = AllocCore::dbg_segments_reserve_failed_total() - refused_before;
    if !stopped_early_by_the_os(second_wave.len(), expected_ceiling, refused) {
        assert_eq!(
            second_wave.len(),
            expected_ceiling,
            "second wave stopped at {}/{expected_ceiling} with {refused} OS reservation(s) \
             refused — slots were not actually recycled after freeing the first wave",
            second_wave.len()
        );
    }

    for p in second_wave {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the
        // pointer was returned by a prior matching alloc in this test, is
        // live, and is freed exactly once here.
        unsafe { ac.dealloc(p, layout) };
    }
}
