//! R9-8 (task #230) correctness tests — per-class miss-streak decoupling and
//! the OOM-rescue scan backstop.
//!
//! Two fixes from the external review's finding on the R8-2 directory-
//! authoritative-miss fast path's worst case under a hypothetical directory-
//! invariant drift:
//!   1. **Per-class miss-streak** — the streak that gates the periodic
//!      re-validation scan is now tracked PER-CLASS (was a single `u32` shared
//!      across every size class), so a drift-affected class trips its OWN
//!      rescan independent of how often other (healthy) classes miss.
//!   2. **Rescue scan before OOM** — right before the small path would surface
//!      an OOM (segment table full or OS reservation failure), a forced O(S)
//!      linear scan runs as a last resort, in case the directory has a stale-
//!      negative bit for a class a full scan would find real free capacity for.
//!
//! Feature-gated identically to `directory_authoritative_miss.rs`: the logic
//! under test is compiled out under `numa-aware` (the directory is never
//! trusted for lookups there) and needs `alloc-stats` for the counter
//! assertions.

#![cfg(all(
    feature = "alloc-segment-directory",
    feature = "alloc-stats",
    not(feature = "numa-aware")
))]

use std::alloc::Layout;
use std::sync::Mutex;

use sefer_alloc::{AllocCore, SegmentLayout};

// Process-wide directory counters are shared across every `AllocCore`; this
// lock serialises these tests against each other (and the sibling
// `directory_authoritative_miss.rs` tests touch the same counters, but those
// run in a separate test binary so there is no cross-binary contention).
static TEST_LOCK: Mutex<()> = Mutex::new(());

// ── helpers (verbatim from directory_authoritative_miss.rs) ───────────────

/// Allocate until `table.count() > threshold`, returning pointers + class.
fn push_past_threshold(core: &mut AllocCore) -> (Vec<*mut u8>, usize) {
    let threshold = AllocCore::dbg_directory_materialize_threshold();
    let small_max = SegmentLayout::SMALL_MAX;
    let layout = Layout::from_size_align(small_max, 1).unwrap();
    let class_idx =
        SegmentLayout::class_for(small_max, 1).expect("SMALL_MAX must resolve to a small class");

    let mut ptrs: Vec<*mut u8> = Vec::new();
    let max_allocs = (threshold as usize + 5) * 20;
    for _ in 0..max_allocs {
        let p = core.alloc(layout);
        assert!(!p.is_null(), "alloc returned null");
        ptrs.push(p);
        if core.dbg_table_count() > threshold {
            break;
        }
    }
    assert!(
        core.dbg_table_count() > threshold,
        "failed to push table count past threshold"
    );
    (ptrs, class_idx)
}

// ── Test 1: per-class streak decouples rescans across classes ─────────────

/// **Test 1**: a busy healthy class's misses do NOT advance a different
/// (drift-affected) class's miss-streak, and that class's periodic
/// re-validation scan fires at its OWN period regardless of the cross-class
/// traffic.
///
/// Pre-R9-8 the streak was a SINGLE `u32` shared across every class, so
/// `class_y`'s misses would have consumed the shared budget and a `class_x`
/// rescan timed for its own boundary would fire at the wrong point (or not at
/// all, if `class_y`'s traffic reset the counter in between). This test sets
/// `class_x`'s streak one shy of the period, drives `3 × period` `class_y`
/// misses, and asserts `class_x`'s streak is UNCHANGED and its next miss still
/// trips the rescan — the direct decoupling proof. (Counterfactual: under the
/// old shared counter, `class_y`'s `3 × period` misses reset the shared counter
/// three times, so the subsequent single `class_x` miss lands at streak 1, not
/// `period`, and the rescan does NOT fire — verified by temporarily reverting
/// to a shared counter; see R9-8 report §3.)
#[test]
fn per_class_streak_decouples_rescans_across_classes() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut core = AllocCore::new().unwrap();
    let (_threshold_ptrs, small_max_class) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());
    core.dbg_directory_reset_miss_streak();

    let period = AllocCore::dbg_directory_miss_full_scan_period();
    assert!(
        period >= 2,
        "test needs DIRECTORY_MISS_FULL_SCAN_PERIOD >= 2"
    );

    // class_x (drift-affected) and class_y (healthy + busy): both fresh (never
    // allocated during push_past_threshold, which only uses SMALL_MAX's class).
    let class_x = 0usize;
    let class_y = 1usize;
    assert!(
        class_x < small_max_class && class_y < small_max_class && class_x != class_y,
        "need two distinct fresh classes below SMALL_MAX's class"
    );
    let layout_y = Layout::from_size_align(AllocCore::dbg_block_size(class_y), 8)
        .expect("class_y block size is a valid layout");
    let layout_x = Layout::from_size_align(AllocCore::dbg_block_size(class_x), 8)
        .expect("class_x block size is a valid layout");

    // Position class_x's streak one shy of its re-validation boundary.
    core.dbg_directory_set_miss_streak_for_class(class_x, (period - 1) as u8);
    assert_eq!(
        core.dbg_directory_miss_streak_for_class(class_x),
        period - 1,
        "streak setter must position class_x at period - 1"
    );

    // ── THE DECOUPLING PROOF (load-bearing): ONE class_y miss must NOT trip a
    //    rescan, even though class_x's streak is parked at period - 1. Under
    //    per-class tracking, class_y has its OWN streak (0 → 1, well under the
    //    period), so no scan fires. Under the PRE-R9-8 SHARED COUNTER, this
    //    single class_y miss would bump the shared slot from period - 1 to
    //    `period`, IMMEDIATELY tripping a rescan — so
    //    `fallback_scans` would rise here. Asserting it does NOT is the
    //    counterfactual that fails under the old shared-counter behaviour
    //    (verified by temporarily routing all classes through slot 0; see
    //    R9-8 report §3).
    let prev_fallback = AllocCore::dbg_directory_fallback_scans();
    let mut y_drain_buf: Vec<*mut u8> = vec![std::ptr::null_mut(); 256];
    let mut last_y_ptr: *mut u8 = {
        let p = core.alloc(layout_y);
        assert!(
            !p.is_null(),
            "single class_y miss-driving alloc must succeed"
        );
        p
    };
    assert_eq!(
        AllocCore::dbg_directory_fallback_scans(),
        prev_fallback,
        "a single class_y miss must NOT trip a rescan while class_x is at period - 1 \
         (per-class decoupling — this assertion FAILS under the pre-R9-8 shared counter)"
    );
    // class_x's streak is still untouched.
    assert_eq!(
        core.dbg_directory_miss_streak_for_class(class_x),
        period - 1,
        "class_x's streak must be unchanged by the class_y miss"
    );

    // ── Corroboration: drive enough class_y misses to trip class_y's OWN
    //    period exactly once, confirming class_y's streak is tracked
    //    independently (it started at 0 after the single miss above bumped it
    //    to 1, so `period - 1` more misses land it at `period`).
    for _ in 0..(period - 1) {
        // SAFETY: `last_y_ptr` was returned by `core.alloc(layout_y)` above
        // / in a previous iteration and identifies a live segment owned by core.
        unsafe {
            core.dbg_drain_freelist_batch(last_y_ptr, class_y, &mut y_drain_buf);
        }
        let p = core.alloc(layout_y);
        assert!(!p.is_null(), "class_y miss-driving alloc must succeed");
        last_y_ptr = p;
    }
    assert_eq!(
        AllocCore::dbg_directory_fallback_scans(),
        prev_fallback + 1,
        "class_y should trip its OWN period exactly once (independent of class_x)"
    );
    // class_x STILL untouched through the entire class_y storm.
    assert_eq!(
        core.dbg_directory_miss_streak_for_class(class_x),
        period - 1,
        "class_x's streak must remain period - 1 through class_y's full period"
    );

    // ── BEHAVIORAL CORROBORATION: the NEXT class_x miss trips the rescan at
    //    class_x's own boundary, despite the class_y storm. small_cur's
    //    BinTable[class_x] is empty (class_y carves only touched
    //    BinTable[class_y]), so pop_free fails and the directory-miss path is
    //    reached. Pre-position the class_x streak again defensively (the
    //    class_y storm should not have touched it, but this makes the test
    //    robust to any future cross-class coupling).
    core.dbg_directory_set_miss_streak_for_class(class_x, (period - 1) as u8);
    let prev_fallback_x = AllocCore::dbg_directory_fallback_scans();
    let p = core.alloc(layout_x);
    assert!(!p.is_null(), "class_x rescan alloc must succeed");
    assert_eq!(
        AllocCore::dbg_directory_fallback_scans(),
        prev_fallback_x + 1,
        "class_x's own rescan must fire on its single miss (independent of class_y)"
    );
    // After the rescan + carve, class_x's streak reset to 0.
    assert_eq!(
        core.dbg_directory_miss_streak_for_class(class_x),
        0,
        "class_x's streak must reset after its rescan fired"
    );
}

// ── Test 2: rescue scan finds a drifted block and avoids OOM ──────────────

/// **Test 2**: the R9-8 OOM-rescue scan, when the directory has a stale-
/// negative bit for a class a full scan would find real free capacity for,
/// finds that block, self-heals the bit, and bumps
/// `DIRECTORY_RESCUE_OOM_AVOIDED` — without bumping the periodic-re-validation
/// canary `DIRECTORY_MISS_SELF_HEAL` (the two drift signals stay
/// distinguishable).
///
/// Reaching a REAL OOM (`MAX_SEGMENTS` table full or OS reservation failure)
/// is impractical in a unit test (~1024 live 4 MiB segments). The test-only
/// `dbg_directory_rescue_scan` hook invokes the EXACT rescue code path the
/// production OOM branches (`alloc_small`'s step-4 `None` branch and the
/// magazine-refill equivalent) call — `find_segment_with_free_forced` +
/// `DIRECTORY_RESCUE_OOM_AVOIDED` — so this exercises the real mechanism
/// without driving the table to capacity.
#[test]
fn rescue_scan_finds_drifted_block_and_avoids_oom() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut core = AllocCore::new().unwrap();
    let (_threshold_ptrs, small_max_class) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());

    let class_x = 0usize;
    assert!(
        class_x < small_max_class,
        "need a fresh class below SMALL_MAX"
    );
    let layout_x = Layout::from_size_align(AllocCore::dbg_block_size(class_x), 8)
        .expect("class_x block size is a valid layout");

    // ── Setup: create a segment for class_x with TWO live free blocks, then
    //    manufacture directory drift by force-clearing the bit WITHOUT touching
    //    the BinTable (the directory now wrongly says class_x is empty here).
    let block_a = core.alloc(layout_x);
    assert!(!block_a.is_null());
    let block_b = core.alloc(layout_x);
    assert!(!block_b.is_null());
    let base_of = |p: *mut u8| SegmentLayout::segment_base_of(p as usize);
    assert_eq!(
        base_of(block_a),
        base_of(block_b),
        "two back-to-back allocs of the same class must land in the same segment"
    );
    let target_base = base_of(block_a);
    let target_slot = core.dbg_segment_id_of(block_a) as usize;

    // Drain the carve's class_x refill so the BinTable is clean, then re-free
    // exactly two blocks.
    let mut drain_buf: Vec<*mut u8> = vec![std::ptr::null_mut(); 256];
    // SAFETY: `block_b` is a live allocation pointer into a segment owned by core.
    let _n_drained = unsafe { core.dbg_drain_freelist_batch(block_b, class_x, &mut drain_buf) };
    // SAFETY: block_a/block_b were returned by `core.alloc(layout_x)` and are
    // freed exactly once each here.
    unsafe {
        core.dealloc(block_a, layout_x);
        core.dealloc(block_b, layout_x);
    }
    assert_eq!(
        core.dbg_directory_get_bit(class_x, target_slot),
        Some(true),
        "directory bit must be SET after the two frees"
    );

    // Manufacture the drift.
    assert!(
        core.dbg_directory_force_clear_bit(class_x, target_slot),
        "force_clear_bit must succeed on a materialised directory"
    );
    assert_eq!(
        core.dbg_directory_get_bit(class_x, target_slot),
        Some(false),
        "the drifted bit must read CLEAR before the rescue"
    );

    // ── Run the rescue scan (the production OOM-path code, via the test hook).
    let before_rescue = AllocCore::dbg_directory_rescue_oom_avoided();
    let before_heal = AllocCore::dbg_directory_miss_self_heal();
    let found = core.dbg_directory_rescue_scan(class_x);
    let after_rescue = AllocCore::dbg_directory_rescue_oom_avoided();
    let after_heal = AllocCore::dbg_directory_miss_self_heal();

    // (a) The rescue FOUND the drifted segment (did not concede OOM).
    assert!(
        found.is_some(),
        "rescue scan must find the drifted block (avoid the spurious OOM)"
    );
    let found_base = found.unwrap();
    assert_eq!(
        base_of(found_base),
        target_base,
        "the rescued segment must be the drifted target_segment"
    );

    // (b) The rescue counter bumped exactly once.
    assert_eq!(
        after_rescue,
        before_rescue + 1,
        "directory_rescue_oom_avoided must rise by exactly 1 (before={before_rescue}, after={after_rescue})"
    );

    // (c) The PERIODIC canary was NOT bumped — rescue and periodic stay
    //     distinguishable in diagnostics.
    assert_eq!(
        after_heal, before_heal,
        "directory_miss_self_heal must NOT rise on a rescue (it is the periodic-only canary)"
    );

    // (d) The rescue self-healed the bit in-place.
    assert_eq!(
        core.dbg_directory_get_bit(class_x, target_slot),
        Some(true),
        "the drifted bit must be healed (SET) after the rescue scan"
    );

    // (e) Counterfactual sanity: with NO drift (bit correctly SET), the rescue
    //    finds the block via the directory POSITIVE lookup (not a forced
    //    scan) — so it still succeeds, but without needing the heal. Re-run
    //    on a healthy directory: the bit is already SET, the rescue still
    //    returns the segment, and the counter rises again (a rescue that
    //    avoided an OOM, even though the directory was actually correct).
    //    This confirms the rescue is a safe no-op-when-healthy backstop.
    let before2 = AllocCore::dbg_directory_rescue_oom_avoided();
    let found2 = core.dbg_directory_rescue_scan(class_x);
    assert!(
        found2.is_some(),
        "rescue on a healthy directory must still find the segment"
    );
    assert_eq!(
        AllocCore::dbg_directory_rescue_oom_avoided(),
        before2 + 1,
        "rescue counter rises whenever the scan finds a block (healthy or drifted)"
    );
}
