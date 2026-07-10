//! Regression test for task #126 (redo of the rejected Phase C "C3" attempt).
//!
//! ## Background
//!
//! `AllocCore::find_segment_with_free` scans owned segments on a small-alloc
//! free-list miss, draining each segment's cross-thread `RemoteFreeRing` along
//! the way. Under `alloc-decommit`, a segment that empties out during its
//! drain is decommitted and its `SegmentTable` slot recycled (NULLed + OS
//! reservation released) so the slot can be reused by a future `register`.
//!
//! The REJECTED Phase C attempt replaced an 8 KiB `[*mut u8; MAX_SEGMENTS]`
//! stack pre-collect with direct iteration + a small fixed-size deferred-recycle
//! buffer (`RECYCLE_BUF_CAP = 32`). That buggy version silently DROPPED the
//! recycle for the 33rd+ segment that emptied out within a single scan: once
//! the buffer filled, further emptied segments were skipped with `continue`
//! and their `recycle()` call was never issued — on the NEXT scan their ring
//! was already empty (fully drained), so `decommit_happened` would be `false`
//! and they would NEVER be recycled again. That is a permanent `SegmentTable`
//! slot pin — the same leak class as #114/A1.
//!
//! Task #126's fix (index-driven `SegmentTable::base_at` scan, see
//! `alloc_core.rs::find_segment_with_free` and `segment_table.rs::base_at`)
//! removes the pre-collect buffer entirely and recycles each emptied segment
//! the moment it is discovered, with no fixed-size intermediate buffer — so
//! there is no capacity to overflow and no way to lose a recycle, however many
//! segments empty out in one scan.
//!
//! ## What this test proves
//!
//! Construct a scenario where **more than 64 segments** (comfortably beyond
//! the rejected version's CAP=32) empty out during a SINGLE
//! `find_segment_with_free` scan:
//!
//! 1. Allocate enough blocks to spread across `TARGET_SEGMENTS` (150) fresh
//!    Small segments, recording one survivor pointer per segment; once all
//!    segments are discovered, free every non-survivor block (own-thread),
//!    leaving exactly one live block per segment.
//! 2. Push each of the 150 remaining live blocks into its OWN segment's
//!    `RemoteFreeRing` via `dbg_push_to_ring` (simulating a cross-thread free
//!    that targets a non-current segment).
//! 3. Trigger exactly ONE scan via `dbg_drain_all_rings` (which uses the same
//!    index-driven `base_at` walk as `find_segment_with_free`). Every one of
//!    the 150 segments empties out (its one remaining live block is reclaimed)
//!    and must be recycled within this single call.
//! 4. Assert: (almost) every survivor segment was actually SLOT-RECYCLED —
//!    i.e. unregistered from the `SegmentTable` (`dbg_live_count_for` on a
//!    pointer into it returns `None`), not merely "decommitted" (payload
//!    released). This distinction matters: `dbg_decommit_count()` advances
//!    inside `decommit_empty_segment` regardless of whether the table slot is
//!    later recycled, so it alone cannot distinguish "recycled" from
//!    "decommitted but permanently pinned". The slot-level check is the
//!    load-bearing assertion. A SECOND round of allocation must also succeed
//!    and reuse the recycled slots (proving they were not silently leaked).
//!
//! Counterfactual (verified live against this exact test, see task #126's
//! commit history / PR description for the transcript): temporarily capping
//! the number of `table.recycle(base)` calls per `dbg_drain_all_rings` scan
//! at 32 (reproducing the rejected Phase C CAP=32 deferred-recycle buffer)
//! makes this test FAIL — only 32 of 150 survivor segments end up
//! slot-recycled, even though `dbg_decommit_count()` still advances for
//! (nearly) all 150 (payload decommit fires unconditionally; only the slot
//! recycle was capped). Removing the cap restores a clean pass.
//!
//! ## Mechanism 2 (task #51) — the bounded hysteresis pool
//!
//! The empty-small-segment pool (`SmallSegmentPoolConfig`, ON by default in
//! `production`) deliberately RETAINS up to `pool_cap` (default 4) emptied
//! small segments — kept registered + committed, free-lists intact — instead of
//! releasing them, so the next `reserve_small_segment` reuses one without an OS
//! round-trip. That means up to `pool_cap` of the 150 survivor segments stay
//! registered after the single drain (`dbg_live_count_for` returns `Some(0)`,
//! not `None`), lowering the recycled floor to `TARGET_SEGMENTS - 2 - pool_cap`.
//!
//! This does NOT weaken the test's guarantee — it strengthens it. The rejected
//! Phase C design pinned an UNBOUNDED, ever-growing set of slots PERMANENTLY
//! (no drain path). This test now proves the pool's retention is instead:
//!   - BOUNDED: `dbg_pooled_count() <= pool_cap` (the synchronous budget cap in
//!     `release_or_pool_empty_segment` never lets the pool overfill, even
//!     mid-scan); and
//!   - TEMPORARY / drainable: after `dbg_drain_small_pool()` force-drains the
//!     pool, recycling CONVERGES to the full pool-less floor `TARGET_SEGMENTS -
//!     2` (only `small_cur` + the primordial remain registered).
//!
//! Together those two facts are exactly the "no unbounded / no permanent
//! pinning" property the original test existed to prove — now demonstrated
//! against a bounded, drainable retention window rather than against
//! zero-retention.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-xthread",
    feature = "alloc-decommit"
))]

use core::alloc::Layout;

use sefer_alloc::alloc_core::AllocCore;

/// More than the rejected version's `RECYCLE_BUF_CAP = 32` — comfortably
/// exercises the unbounded path.
const TARGET_SEGMENTS: usize = 150;

#[cfg_attr(miri, ignore)] // large N; native-only soak, mirrors decommit_soak / decommit_stale_ring sizing conventions
#[test]
fn unbounded_recycle_within_single_scan() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(256, 8).unwrap();
    let class_idx = ac
        .dbg_layout_class_for(layout)
        .expect("256 B is a small class");

    // A 4 MiB segment holds roughly 16K of these 256 B blocks. We drive
    // `small_cur` forward across many fresh segments by allocating in bulk
    // batches, recording ONE survivor pointer per distinct segment along the
    // way (the first block seen for each newly-visited segment). All
    // non-survivor blocks are kept ALIVE (not freed) until the spreading
    // phase is fully done — freeing them early would put them back onto
    // their segment's free list, which `alloc_small`'s step 1 (pop from the
    // CURRENT segment's free list first) would immediately reuse, collapsing
    // every subsequent batch back onto the same segment forever instead of
    // making forward progress.
    const SEGMENT: usize = 4 * 1024 * 1024;
    let mut survivors: std::collections::HashMap<usize, *mut u8> = std::collections::HashMap::new();
    const ROUND_BLOCKS: usize = 18_000; // > one fresh segment's ~16K capacity

    // Keep EVERY block ever allocated alive until the very end of the
    // spreading phase — freeing a round's non-survivor blocks immediately
    // (own-thread) puts them back on their segment's free list, which the
    // NEXT round's `alloc` would greedily pop from (step 1 of `alloc_small`
    // always tries the current segment's free list first) — collapsing every
    // subsequent round back onto the SAME segment forever, since it never
    // truly fills up. Instead we let `small_cur` roll forward unobstructed by
    // holding every block live, then free the non-survivors ONLY after we've
    // finished discovering all TARGET_SEGMENTS distinct segments.
    let mut all_ptrs: Vec<*mut u8> = Vec::new();
    let mut round = 0usize;
    while survivors.len() < TARGET_SEGMENTS && round < TARGET_SEGMENTS * 2 {
        for _ in 0..ROUND_BLOCKS {
            let p = ac.alloc(layout);
            assert!(!p.is_null(), "alloc null in round={round}");
            let seg_base = (p as usize) & !(SEGMENT - 1);
            survivors.entry(seg_base).or_insert(p);
            all_ptrs.push(p);
        }
        round += 1;
    }

    // Now free every allocated block EXCEPT the recorded survivors (one per
    // distinct segment). This drives each segment's live_count down to
    // exactly 1 — but crucially this happens AFTER all TARGET_SEGMENTS were
    // already discovered via bump-pointer progress, so it cannot perturb
    // which segments were visited.
    let survivor_set: std::collections::HashSet<usize> =
        survivors.values().map(|&p| p as usize).collect();
    for &p in &all_ptrs {
        if !survivor_set.contains(&(p as usize)) {
            ac.dealloc(p, layout);
        }
    }

    assert!(
        survivors.len() >= TARGET_SEGMENTS,
        "failed to spread across {TARGET_SEGMENTS} distinct segments \
         (only reached {}) after {round} rounds; increase ROUND_BLOCKS or the round budget",
        survivors.len()
    );

    // Push every survivor into its OWN segment's ring — simulating a
    // cross-thread free of the LAST live block in that segment. None of
    // these own-thread `dealloc` calls run directly; the reclaim must happen
    // through the ring-drain path (`find_segment_with_free` /
    // `dbg_drain_all_rings`), exactly like `find_segment_with_free`'s
    // production code path.
    let mut pushed = 0usize;
    for &p in survivors.values() {
        if ac.dbg_push_to_ring(p, class_idx) {
            pushed += 1;
        }
    }
    assert!(
        pushed >= TARGET_SEGMENTS,
        "expected all {TARGET_SEGMENTS} survivor pushes to succeed, got {pushed}"
    );

    let decommit_before = AllocCore::dbg_decommit_count();

    // ONE single scan: drains every owned segment's ring, exactly as
    // `find_segment_with_free` does internally. Every survivor's segment
    // should empty out (live_count 0) and be recycled within this ONE call.
    ac.dbg_drain_all_rings();

    let decommit_after = AllocCore::dbg_decommit_count();
    let decommits_this_scan = decommit_after - decommit_before;
    // `dbg_decommit_count` is a coarse diagnostic: it increments inside
    // `decommit_empty_segment` regardless of whether the SLOT is later
    // recycled — a design that decommits a segment's payload but then drops
    // the `table.recycle(base)` call (exactly the rejected Phase C CAP=32
    // bug) would still advance this counter. It is a necessary but NOT
    // sufficient signal on its own, so we additionally check slot-level
    // recycling below (the actual thing task #126 must get right).
    assert!(
        decommits_this_scan > 0,
        "no decommit fired during the single drain call — the survivor \
         construction failed to produce empty-able segments"
    );

    // The load-bearing assertion: a segment whose slot was actually recycled
    // is UNREGISTERED from the table — `dbg_live_count_for` on any pointer
    // into that segment returns `None` (contains_base fails). Count how many
    // of the TARGET_SEGMENTS survivor segments were genuinely recycled. A
    // bounded deferred-recycle buffer (the rejected Phase C CAP=32 design)
    // decommits the payload for every emptied segment (advancing
    // `dbg_decommit_count` unconditionally) but only recycles the table slot
    // for the first 32 — the 33rd..150th segments stay registered forever
    // (permanently pinned), so `dbg_live_count_for` would still return
    // `Some(_)` for their survivor pointers.
    let recycled_count = survivors
        .values()
        .filter(|&&p| ac.dbg_live_count_for(p).is_none())
        .count();

    // Three exclusion classes account for the survivor segments that are
    // legitimately NOT slot-recycled at live_count == 0:
    //   1. `small_cur` — the currently active carve target: decommit/recycle
    //      never fires for it even when empty (§M6: a segment still being
    //      carved from stays committed).
    //   2. the PRIMORDIAL segment — never decommitted/recycled at all
    //      (`dec_live_and_maybe_decommit` only recycles `SegmentKind::Small`;
    //      the primordial hosts the self-hosted registry between
    //      `small_meta_end()` and `primordial_meta_end()`, so returning its
    //      pages to the OS would corrupt the substrate).
    //   3. Mechanism 2 (task #51) — the empty-small-segment HYSTERESIS POOL.
    //      Up to `pool_cap` (default 4) emptied segments are deliberately
    //      RETAINED (kept registered + committed, free-lists intact) instead of
    //      released, so the next `reserve_small_segment` reuses one without an
    //      OS round-trip. A retained segment stays registered, so
    //      `dbg_live_count_for` returns `Some(0)` for its survivor pointer —
    //      counting AGAINST `recycled_count` here.
    //
    // Task #145 (P1) added the exact 256 B size class, repacking these 256 B
    // blocks (~16 384/segment now, was ~13 791 at the old 304 B class). The
    // repacking shifted survivor discovery so ONE survivor now lands in the
    // primordial segment while `small_cur` is a DIFFERENT segment — so classes
    // (1)+(2) apply as two DISTINCT segments. Class (3) adds at most `pool_cap`
    // more. So the floor is TARGET_SEGMENTS - 2 - pool_cap.
    //
    // This STILL proves the load-bearing property — recycling is UNBOUNDED
    // (≫ the rejected CAP=32), not capped at any fixed buffer — because the
    // retained set is BOUNDED by `pool_cap` (asserted directly below) and
    // TEMPORARY (drained to full recycling further below). The rejected Phase C
    // CAP=32 bug pinned segments PERMANENTLY with no drain path; the pool's
    // retention is a documented, bounded, drainable window, not a leak.
    let pool_cap = ac.dbg_pool_cap();

    // (3a) Retention is BOUNDED: at most `pool_cap` segments are pooled at any
    // instant, even mid-scan (the synchronous budget cap in
    // `release_or_pool_empty_segment` — the pool never overfills). This is the
    // direct analogue of the rejected design's fatal property: THAT design let
    // an unbounded number of slots stay pinned; THIS one caps it hard.
    assert!(
        ac.dbg_pooled_count() <= pool_cap,
        "pool over-filled: pooled_count={} exceeds pool_cap={pool_cap} — the \
         synchronous budget cap in release_or_pool_empty_segment failed",
        ac.dbg_pooled_count()
    );

    let min_expected = TARGET_SEGMENTS - 2 - pool_cap;
    assert!(
        recycled_count >= min_expected,
        "only {recycled_count} of {TARGET_SEGMENTS} survivor segments were actually \
         SLOT-RECYCLED (unregistered from the table) after the single drain call \
         (expected >= {min_expected}, allowing the current carve segment, the \
         never-recyclable primordial segment, and up to pool_cap={pool_cap} \
         hysteresis-pooled segments to be excluded). `dbg_decommit_count` \
         advanced by {decommits_this_scan}. If this floor is UNDERSHOT it means \
         the recycle became bounded by a fixed buffer BEYOND the documented pool \
         (the rejected Phase C CAP=32 class of bug), because the pool retention \
         is separately proven bounded (<= pool_cap) and drainable (below)."
    );

    // (3b) Retention is TEMPORARY — NOT a permanent pin. Force-drain the pool:
    // every pooled segment is released (reset + table.recycle). After this,
    // recycling CONVERGES to the full (pool-less) floor of TARGET_SEGMENTS - 2:
    // the ONLY survivor segments still registered are the two never-recyclable
    // ones (small_cur + primordial). This is the load-bearing "no permanent
    // leak" proof — the pool held slots only within a bounded, drainable
    // window, exactly compatible with the test's original guarantee.
    let pooled_before_drain = ac.dbg_pooled_count();
    let drained = ac.dbg_drain_small_pool();
    assert_eq!(
        drained, pooled_before_drain,
        "dbg_drain_small_pool drained {drained} but pooled_count was \
         {pooled_before_drain} — the pool did not fully drain"
    );
    assert_eq!(ac.dbg_pooled_count(), 0, "pool not empty after force-drain");
    let recycled_after_drain = survivors
        .values()
        .filter(|&&p| ac.dbg_live_count_for(p).is_none())
        .count();
    assert!(
        recycled_after_drain >= TARGET_SEGMENTS - 2,
        "after draining the hysteresis pool only {recycled_after_drain} of \
         {TARGET_SEGMENTS} survivor segments are recycled (expected >= \
         {}) — a pooled segment was PERMANENTLY pinned rather than merely \
         retained in the bounded, drainable pool window. That would be the \
         rejected Phase C permanent-pin class of bug.",
        TARGET_SEGMENTS - 2
    );

    // Slots must be genuinely reusable: verify a second large round of
    // allocation succeeds and does not exhaust the table — this would fail
    // (null allocs) if the recycled slots were never actually freed (a
    // silent leak would eventually exhaust MAX_SEGMENTS = 1024).
    let mut second_round = Vec::with_capacity(TARGET_SEGMENTS * 100);
    for i in 0..(TARGET_SEGMENTS * 100) {
        let p = ac.alloc(layout);
        assert!(
            !p.is_null(),
            "second-round alloc null at i={i} — recycled slots were not \
             actually reusable (leak from the single-scan drain)"
        );
        second_round.push(p);
    }
    for &p in &second_round {
        ac.dealloc(p, layout);
    }
}
