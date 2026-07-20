//! R8-2 (task #215) correctness test — authoritative-miss fast path and the
//! periodic re-validation self-heal safety net.
//!
//! Before R8-2, a directory MISS in `find_segment_with_free_impl`
//! UNCONDITIONALLY fell through to the O(S) guarded linear-scan fallback —
//! so a miss cost the same as if the directory didn't exist. R8-2 makes a
//! genuine miss AUTHORITATIVE in the common case (the O(S) scan is skipped
//! and the caller carves a fresh segment as if the directory's "empty for
//! this class" were the truth), with a periodic safety net: every
//! `DIRECTORY_MISS_FULL_SCAN_PERIOD` consecutive misses runs the full scan
//! anyway as a re-validation pass. If that scan finds a segment the
//! directory missed, it both USES it (correctness) and REPAIRS the bit
//! in-place (self-heal) and bumps `DIRECTORY_MISS_SELF_HEAL` as a canary
//! counter (a nonzero value in real testing/CI indicates a genuine
//! directory-tracking bug, NOT a normal event).
//!
//! These three tests prove the three load-bearing behaviours:
//!   1. The authoritative-miss fast path actually skips the O(S) scan
//!      (counter `directory_authoritative_miss` rises while
//!      `full_scan_slots_examined` stays flat).
//!   2. The periodic re-validation pass fires exactly at the period boundary
//!      — neither never (streak doesn't trigger) nor every time (streak
//!      doesn't reset) — directly observable via `directory_fallback_scans`.
//!   3. The periodic re-validation pass, when it finds a directory-missed
//!      segment, self-heals the bit and bumps `directory_miss_self_heal` —
//!      using a new test-only `dbg_directory_force_clear_bit` hook to
//!      manufacture the (otherwise unreachable through the invariant-preserving
//!      API) directory drift.
//!
//! Non-vacuousness proofs (personally verified by the implementing agent):
//! each assertion is shown to FAIL on the pre-R8-2 behaviour by temporarily
//! commenting out the new early-return branch in `find_segment_with_free_impl`
//! — see the report accompanying this task for the concrete failing output.
//!
//! Feature-gated behind `alloc-segment-directory` (the directory under test)
//! PLUS `alloc-stats` (the new counters) AND `not(numa-aware)`: under
//! `numa-aware` the ENTIRE directory-driven lookup block in
//! `find_segment_with_free_impl` (including the R8-2 authoritative-miss /
//! periodic-self-heal logic this file tests) is compiled out — the directory
//! bitmap is still maintained (A1/A2), but nothing queries it for lookups, so
//! `directory_authoritative_miss`/`directory_fallback_scans`/
//! `directory_miss_self_heal` never fire under that feature. Excluding
//! `numa-aware` here matches the production gate exactly (caught via a
//! `--all-features` `npm run check` failure: this file originally lacked the
//! exclusion, so under `--all-features` every genuine miss silently fell
//! through the (compiled-out) authoritative path with the counters staying
//! at 0, failing the very first assertion).

#![cfg(all(
    feature = "alloc-segment-directory",
    feature = "alloc-stats",
    not(feature = "numa-aware")
))]

use std::alloc::Layout;
use std::sync::Mutex;

use sefer_alloc::{AllocCore, SegmentLayout};

// The directory diagnostic counters (`DIRECTORY_AUTHORITATIVE_MISS`,
// `DIRECTORY_FALLBACK_SCANS`, `DIRECTORY_MISS_SELF_HEAL`,
// `FULL_SCAN_SLOTS_EXAMINED`) are PROCESS-WIDE static atomics shared across
// every `AllocCore` in the process. `cargo test` runs the three tests in this
// file in parallel by default, and several assertions compare deltas on those
// counters across multi-step sequences — a parallel sibling test incrementing
// the same counter mid-sequence would break the delta arithmetic. This
// process-wide lock serialises the three tests so each one observes a quiescent
// counter state. (Each test still creates its own `AllocCore`; the
// `directory_miss_streak` field that gates the periodic re-validation is
// per-instance and so not actually shared — only the counter reads are.)
static TEST_LOCK: Mutex<()> = Mutex::new(());

// ── helpers (copied verbatim from segment_directory_a2.rs /
//    dirty_directory_incremental_sync.rs — the established correctness-oracle
//    helpers in this repo) ─────────────────────────────────────────────────

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

/// Assert that the incremental directory bitmap equals a fresh rebuild for
/// ALL (class, slot) pairs. Panics with a detailed message on mismatch.
fn assert_directory_equals_rebuild(core: &mut AllocCore) {
    // Take a snapshot of the current incremental directory.
    let class_count = AllocCore::dbg_small_class_count();
    let mut incremental = vec![vec![false; 1024]; class_count];
    for (c, row) in incremental.iter_mut().enumerate() {
        for (s, cell) in row.iter_mut().enumerate() {
            *cell = core.dbg_directory_get_bit(c, s).unwrap_or(false);
        }
    }

    // Rebuild from scratch.
    let rebuilt = core.dbg_rebuild_directory();
    assert!(
        rebuilt,
        "directory should be materialised for this assertion"
    );

    // Compare.
    for (c, row) in incremental.iter().enumerate() {
        for (s, &inc_val) in row.iter().enumerate() {
            let fresh = core.dbg_directory_get_bit(c, s).unwrap_or(false);
            assert_eq!(
                inc_val, fresh,
                "directory mismatch at class={c} slot={s}: \
                 incremental={inc_val}, rebuild={fresh}",
            );
        }
    }
}

// ── tests ────────────────────────────────────────────────────────────────

/// **Test 1**: a genuine directory miss for a class with NO live free blocks
/// anywhere is AUTHORITATIVE — the O(S) linear scan is SKIPPED.
///
/// Drives ~20 consecutive genuine misses for distinct fresh classes (a class
/// that has never been allocated has zero free blocks in every segment, so
/// the directory lookup for it must miss). Each miss must (a) bump
/// `directory_authoritative_miss` and (b) leave `full_scan_slots_examined`
/// unchanged — the direct proof the O(S) work is actually skipped.
///
/// (Each `alloc` for a fresh class does CARVE a segment for that class —
/// `find_segment_with_free` returns `None`, then `alloc_small` carves. That
/// carve does not enter the linear scan, so `full_scan_slots_examined` is
/// untouched by it.)
#[test]
fn authoritative_miss_skips_full_scan() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut core = AllocCore::new().unwrap();
    let (_threshold_ptrs, small_max_class) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());
    // `push_past_threshold` leaves `directory_miss_streak` in an unknown
    // residual state (it drives its own misses while crossing the threshold).
    // Reset it so the per-alloc assertions below see a known starting point.
    core.dbg_directory_reset_miss_streak();

    let n_misses: usize = 20;
    // Classes `[0, n_misses)` — all smaller than `small_max_class` (which is
    // the top small class), so all are genuinely fresh (no carve for any of
    // them happened during `push_past_threshold`).
    assert!(
        n_misses < small_max_class,
        "test needs n_misses distinct fresh classes below SMALL_MAX's class"
    );

    for c in 0..n_misses {
        let size = AllocCore::dbg_block_size(c);
        let layout = Layout::from_size_align(size, 8).expect("class block size is a valid layout");
        let before_authoritative = AllocCore::dbg_directory_authoritative_miss();
        let before_slots = AllocCore::dbg_full_scan_slots_examined();

        let p = core.alloc(layout);
        assert!(!p.is_null(), "alloc for class {c} returned null");

        let after_authoritative = AllocCore::dbg_directory_authoritative_miss();
        let after_slots = AllocCore::dbg_full_scan_slots_examined();
        // (a) This alloc caused an authoritative miss.
        assert!(
            after_authoritative > before_authoritative,
            "class {c}: expected directory_authoritative_miss to rise \
             (before={before_authoritative}, after={after_authoritative})"
        );
        // (b) The O(S) linear scan was NOT entered.
        assert_eq!(
            after_slots, before_slots,
            "class {c}: full_scan_slots_examined rose across an authoritative \
             miss (before={before_slots}, after={after_slots}) — the O(S) scan \
             should have been skipped"
        );
    }
}

/// **Test 2**: the periodic re-validation pass fires EXACTLY at the period
/// boundary — neither never (streak doesn't trigger) nor every time (streak
/// doesn't reset).
///
/// Drives `DIRECTORY_MISS_FULL_SCAN_PERIOD` consecutive genuine misses for ONE
/// class (draining the carve's refill between iterations so each `alloc`
/// re-misses), and confirms `directory_fallback_scans`'s delta is exactly `1`
/// at the period boundary. Then drives `PERIOD - 1` more misses (delta still
/// `1` — streak reset, no second scan yet) and one more (delta `2` — second
/// period hit) — proving the counter both fires and resets.
#[test]
fn periodic_revalidation_runs_every_period_misses() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut core = AllocCore::new().unwrap();
    let (_threshold_ptrs, _) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());
    // `push_past_threshold` leaves `directory_miss_streak` in an unknown
    // residual state; reset so the period boundary assertions see streak==0
    // at the start of phase A.
    core.dbg_directory_reset_miss_streak();

    let period = AllocCore::dbg_directory_miss_full_scan_period();
    assert!(
        period >= 2,
        "test needs DIRECTORY_MISS_FULL_SCAN_PERIOD >= 2"
    );

    // Pick a class other than SMALL_MAX (so its BinTable is empty in every
    // `push_past_threshold` segment). class 0 is the smallest class — its
    // size is `dbg_block_size(0)`, which is always < SMALL_MAX.
    let class_idx = 0usize;
    let size = AllocCore::dbg_block_size(class_idx);
    let layout = Layout::from_size_align(size, 8).expect("class block size is a valid layout");

    let mut drain_buf: Vec<*mut u8> = vec![std::ptr::null_mut(); 256];
    let mut last_ptr: *mut u8 = std::ptr::null_mut();

    // Helper: drive exactly one genuine miss for `class_idx`.
    let mut drive_one_miss = |core: &mut AllocCore, last_ptr: &mut *mut u8| {
        if !last_ptr.is_null() {
            // Drain the previous carve's class_idx refill so `pop_free(small_cur)`
            // fails and `find_segment_with_free` is reached on this iter's alloc.
            // SAFETY: `*last_ptr` was returned by `core.alloc(layout)` in the
            // previous iteration and identifies a live segment owned by `core`.
            unsafe {
                core.dbg_drain_freelist_batch(*last_ptr, class_idx, &mut drain_buf);
            }
        }
        let p = core.alloc(layout);
        assert!(!p.is_null(), "alloc returned null during miss-driving");
        *last_ptr = p;
    };

    let mut prev_fallback = AllocCore::dbg_directory_fallback_scans();

    // Phase A: 0..period-1 misses — all authoritative, fallback_scans unchanged.
    for _ in 0..(period - 1) {
        drive_one_miss(&mut core, &mut last_ptr);
        let now = AllocCore::dbg_directory_fallback_scans();
        assert_eq!(
            now, prev_fallback,
            "fallback_scans rose BEFORE the period boundary (expected no scan yet)"
        );
    }

    // Phase B: the period-th miss — periodic re-validation fires, delta becomes 1.
    drive_one_miss(&mut core, &mut last_ptr);
    let after_first_period = AllocCore::dbg_directory_fallback_scans();
    assert_eq!(
        after_first_period,
        prev_fallback + 1,
        "fallback_scans should rise by exactly 1 at the period boundary \
         (before={}, after={})",
        prev_fallback,
        after_first_period
    );
    prev_fallback = after_first_period;

    // Phase C: period-1 more misses — streak was reset, no second scan yet.
    for _ in 0..(period - 1) {
        drive_one_miss(&mut core, &mut last_ptr);
        let now = AllocCore::dbg_directory_fallback_scans();
        assert_eq!(
            now, prev_fallback,
            "fallback_scans rose inside the second period before its boundary \
             (streak should have reset to 0 and not re-triggered yet)"
        );
    }

    // Phase D: the second period's final miss — delta becomes 2 total.
    drive_one_miss(&mut core, &mut last_ptr);
    let after_second_period = AllocCore::dbg_directory_fallback_scans();
    assert_eq!(
        after_second_period,
        prev_fallback + 1,
        "fallback_scans should rise by exactly 1 at the SECOND period boundary \
         (proving the streak resets and re-fires, not one-shot)"
    );
}

/// **Test 3**: the periodic re-validation scan, when it finds a segment the
/// directory missed, SELF-HEALS the bit in-place and bumps the canary counter
/// `directory_miss_self_heal`.
///
/// Directory drift (a directory bit stale-CLEAR while the underlying BinTable
/// is non-empty) is impossible to manufacture through the invariant-preserving
/// API — every `publish_empty` call site is gated on a real non-empty→empty
/// BinTable transition. This test uses the new test-only
/// `dbg_directory_force_clear_bit` hook to manufacture drift directly, then
/// proves the self-heal (a) returns the missed block to the caller
/// (correctness preserved despite the drift), (b) bumps
/// `directory_miss_self_heal` exactly once, and (c) leaves the directory
/// consistent with a fresh rebuild afterwards.
#[test]
fn self_heal_repairs_and_finds_segment() {
    let _guard = TEST_LOCK.lock().unwrap();
    let mut core = AllocCore::new().unwrap();
    let (_threshold_ptrs, small_max_class) = push_past_threshold(&mut core);
    assert!(core.dbg_directory_is_materialised());
    // `push_past_threshold` leaves `directory_miss_streak` in an unknown
    // residual state; reset so the heal-triggering alloc is at a known streak.
    core.dbg_directory_reset_miss_streak();

    // Two distinct classes other than `small_max_class`: class_x will host the
    // drifted bit; class_y is used to drive the streak up to the period. Both
    // must be fresh (no carve during `push_past_threshold`).
    let class_x = 0usize;
    let class_y = 1usize;
    assert!(
        class_x < small_max_class && class_y < small_max_class && class_x != class_y,
        "need two distinct fresh classes below SMALL_MAX's class"
    );
    let layout_x = Layout::from_size_align(AllocCore::dbg_block_size(class_x), 8)
        .expect("class_x block size is a valid layout");
    let layout_y = Layout::from_size_align(AllocCore::dbg_block_size(class_y), 8)
        .expect("class_y block size is a valid layout");

    let period = AllocCore::dbg_directory_miss_full_scan_period();

    // ── Setup: create a segment for class_x with TWO live free blocks in its
    //    BinTable (so pop_free consuming one leaves the other, keeping the
    //    directory bit SET after the heal — observable in the final state
    //    rather than immediately re-cleared by pop_free's publish_empty).
    let block_a = core.alloc(layout_x);
    assert!(!block_a.is_null());
    let block_b = core.alloc(layout_x);
    assert!(!block_b.is_null());
    // Both blocks came from the same carve → same segment.
    let base_of = |p: *mut u8| SegmentLayout::segment_base_of(p as usize);
    assert_eq!(
        base_of(block_a),
        base_of(block_b),
        "two back-to-back allocs of the same class must land in the same segment"
    );
    let target_base = base_of(block_a);
    let target_slot = core.dbg_segment_id_of(block_a) as usize;

    // Drain the carve's class_x refill leftover so the BinTable is clean
    // before we re-insert exactly two free blocks.
    let mut drain_buf: Vec<*mut u8> = vec![std::ptr::null_mut(); 256];
    // SAFETY: `block_b` is a live allocation pointer into a segment owned by
    // `core`; the callee derives base from it and mutates only that segment's
    // free list.
    let _n_drained = unsafe { core.dbg_drain_freelist_batch(block_b, class_x, &mut drain_buf) };

    // Free exactly two class_x blocks back into the target segment.
    // SAFETY: block_a/block_b were returned by `core.alloc(layout_x)` above,
    // are still live, and are freed exactly once each here.
    unsafe {
        core.dealloc(block_a, layout_x);
        core.dealloc(block_b, layout_x);
    }

    // Confirm the directory bit is correctly set (the invariant-preserving
    // path published non-empty for both frees; the second is idempotent).
    assert_eq!(
        core.dbg_directory_get_bit(class_x, target_slot),
        Some(true),
        "directory bit for class_x/target_slot must be SET after two frees"
    );

    // ── Manufacture directory drift: force-clear the bit WITHOUT touching
    //    BinTable. The directory now (wrongly) says class_x is empty in
    //    target_slot, while BinTable still has two live free blocks.
    let cleared = core.dbg_directory_force_clear_bit(class_x, target_slot);
    assert!(
        cleared,
        "dbg_directory_force_clear_bit must succeed on a materialised directory"
    );
    assert_eq!(
        core.dbg_directory_get_bit(class_x, target_slot),
        Some(false),
        "dbg_directory_force_clear_bit must have cleared the bit"
    );

    // ── Move `small_cur` AWAY from target_segment so `pop_free(small_cur,
    //    class_x)` returns None on the next alloc, forcing `alloc_small` to
    //    reach `find_segment_with_free(class_x)` (the directory-miss path).
    //    One alloc of a different fresh class carves a new segment for that
    //    class and points `small_cur` at it. This first alloc is ALSO a
    //    directory miss (streak becomes 1).
    let _y_ptr = core.alloc(layout_y);
    // small_cur is now a class_y segment (its BinTable[class_x] is empty).

    // ── Drive the streak up so the NEXT directory miss (the heal alloc for
    //    class_x below) is the one that crosses the period boundary and runs
    //    the periodic re-validation scan. After the class_x carve at the
    //    start (streak 0 → 1) and the class_y switch alloc above (streak 1 →
    //    2), this loop must drive exactly `period - 3` more misses (iter 0
    //    reuses seg34's class_y refill leftover so pop_free succeeds and
    //    find_segment_with_free is NOT reached — only iters 1..N each drive
    //    one miss; N - 1 misses total). With N = period - 2, that is
    //    `period - 3` misses, landing streak at `2 + (period - 3) = period - 1`
    //    so the heal alloc's miss bumps it to `period`, triggering the
    //    periodic re-validation scan. (Driving `period - 1` or more would
    //    trip the re-validation INSIDE the loop and leave the heal alloc as
    //    an authoritative miss, breaking the test — verified during
    //    development.)
    let mut last_y_ptr: *mut u8 = std::ptr::null_mut();
    let mut y_drain_buf: Vec<*mut u8> = vec![std::ptr::null_mut(); 256];
    for _ in 0..(period - 2) {
        if !last_y_ptr.is_null() {
            // SAFETY: `last_y_ptr` was returned by `core.alloc(layout_y)` in
            // the previous iteration and identifies a live segment owned by `core`.
            unsafe {
                core.dbg_drain_freelist_batch(last_y_ptr, class_y, &mut y_drain_buf);
            }
        }
        let p = core.alloc(layout_y);
        assert!(!p.is_null());
        last_y_ptr = p;
    }
    // streak is now == period - 1: 2 from the class_x carve + class_y switch,
    // then period - 3 from this loop (iter 0 reuses seg34's refill so it does
    // not miss). The heal alloc below is the one that crosses the period.

    // ── Trigger the heal. This alloc for class_x:
    //    1. pop_free(small_cur=class_y_seg, class_x) → None.
    //    2. find_segment_with_free(class_x) → directory lookup, bit for
    //       target_slot was force-cleared, no other class_x bits set → MISS.
    //       streak was == period → periodic re-validation runs the full scan.
    //    3. The scan walks the table, finds target_segment (BinTable[class_x]
    //       non-empty) → returns Some(target_base).
    //    4. Self-heal: publish_nonempty(class_x, target_slot) → bit SET,
    //       DIRECTORY_MISS_SELF_HEAL += 1.
    //    5. alloc_small does pop_free(target_base, class_x) → returns one of
    //       block_a/block_b (the previously-freed block). BinTable now has
    //       one block left, so pop_free does NOT publish_empty — the healed
    //       bit stays SET and is observable in the final state.
    let before_heal = AllocCore::dbg_directory_miss_self_heal();
    let found = core.alloc(layout_x);
    let after_heal = AllocCore::dbg_directory_miss_self_heal();

    // (a) Correctness: the alloc succeeded and returned one of the two
    //     previously-freed class_x blocks from the drifted segment (NOT a
    //     freshly-carved block from a new segment).
    assert!(
        !found.is_null(),
        "alloc for class_x must succeed (correctness preserved despite drift)"
    );
    assert_eq!(
        base_of(found),
        target_base,
        "the healed alloc must return a block from the drifted segment, \
         not a freshly-carved one"
    );
    assert!(
        found == block_a || found == block_b,
        "the healed alloc must return one of the two previously-freed blocks"
    );

    // (b) The self-heal counter bumped exactly once.
    assert_eq!(
        after_heal,
        before_heal + 1,
        "directory_miss_self_heal must rise by exactly 1 across the heal-triggering alloc \
         (before={before_heal}, after={after_heal})"
    );

    // (c) After the heal, the directory is consistent with a fresh rebuild.
    //    The healed bit (class_x, target_slot) is SET (one block remains in
    //    BinTable after pop_free consumed the other); the rebuild agrees.
    assert_eq!(
        core.dbg_directory_get_bit(class_x, target_slot),
        Some(true),
        "the healed bit must be SET in the final state (one free block remains)"
    );
    assert_directory_equals_rebuild(&mut core);

    // Silence unused-warning for target_base in non-debug builds where the
    // assert_eq! may be elided (it isn't, but be defensive).
    let _ = target_base;
}
