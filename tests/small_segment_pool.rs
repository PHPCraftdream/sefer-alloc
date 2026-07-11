//! Mechanism 2 (task #51) — the empty-small-segment HYSTERESIS POOL.
//!
//! When a small segment's `live_count` reaches zero, instead of releasing it to
//! the OS the allocator MAY retain it (still registered + committed, free-lists
//! intact) in a bounded pool (`SmallSegmentPoolConfig`, default 4 segments /
//! 16 MiB, ON in `production`). A later `find_segment_with_free` reuses a pooled
//! segment's free blocks in place (un-pooling it), avoiding an OS round-trip.
//! This file verifies:
//!
//!   1. config resolution (`dbg_pool_cap`): default cap, disable via `0`, and
//!      the byte-cap clamp;
//!   2. admission (`dbg_pooled_count`): emptied segments are pooled up to the
//!      cap, and the cap is a HARD synchronous bound (never overfilled);
//!   3. reuse WITHOUT a fresh OS reservation — a pooled segment's free-listed
//!      blocks are re-served via `find_segment_with_free` (which un-pools the
//!      segment on reuse), so the pool shrinks and no matching OS reservation
//!      is made;
//!   4. eventual drain (`dbg_drain_small_pool`) — retention is temporary;
//!   5. the DISABLED pool (`pool_segments(0)`) reproduces the pre-Mechanism-2
//!      immediate-recycle behaviour (nothing ever pooled);
//!   6. a stale cross-thread free into a POOLED segment is a safe no-op
//!      (double-free of an already-free block), needing no special-casing.

#![cfg(all(
    feature = "alloc-core",
    feature = "alloc-xthread",
    feature = "alloc-decommit"
))]

use core::alloc::Layout;

use sefer_alloc::{AllocCore, LargeCacheConfig, SmallSegmentPoolConfig};

const SEGMENT: usize = 4 * 1024 * 1024;
const MIB: usize = 1024 * 1024;

/// Spread allocations across `target` distinct fresh small segments, recording
/// one survivor pointer per segment. Mirrors `regression_c3_unbounded_recycle`'s
/// spreading construction (keep every block alive until spreading is done, so
/// `small_cur` rolls forward instead of collapsing back onto one segment).
fn spread_across_segments(
    ac: &mut AllocCore,
    layout: Layout,
    target: usize,
) -> (std::collections::HashMap<usize, *mut u8>, Vec<*mut u8>) {
    const ROUND_BLOCKS: usize = 18_000; // > one fresh segment's ~16K capacity
    let mut survivors: std::collections::HashMap<usize, *mut u8> = std::collections::HashMap::new();
    let mut all_ptrs: Vec<*mut u8> = Vec::new();
    let mut round = 0usize;
    while survivors.len() < target && round < target * 2 {
        for _ in 0..ROUND_BLOCKS {
            let p = ac.alloc(layout);
            assert!(!p.is_null(), "alloc null in round={round}");
            let seg_base = (p as usize) & !(SEGMENT - 1);
            survivors.entry(seg_base).or_insert(p);
            all_ptrs.push(p);
        }
        round += 1;
    }
    assert!(
        survivors.len() >= target,
        "failed to spread across {target} segments (only {})",
        survivors.len()
    );
    (survivors, all_ptrs)
}

// ── 1. Config resolution ────────────────────────────────────────────────────

/// The default config (production) enables the pool at 4 segments.
#[test]
fn default_pool_cap_is_four() {
    let ac = AllocCore::new().expect("primordial");
    assert_eq!(ac.dbg_pool_cap(), 4, "default pool cap must be 4");
}

/// `pool_segments(0)` disables the pool (cap resolves to 0).
#[test]
fn pool_segments_zero_disables() {
    let cfg = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(0));
    let ac = AllocCore::new_with_config(cfg).expect("primordial");
    assert_eq!(
        ac.dbg_pool_cap(),
        0,
        "pool_segments(0) must disable the pool"
    );
}

/// `pool_byte_cap(0)` disables the pool (cap resolves to 0).
#[test]
fn pool_byte_cap_zero_disables() {
    let cfg = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_byte_cap(0));
    let ac = AllocCore::new_with_config(cfg).expect("primordial");
    assert_eq!(
        ac.dbg_pool_cap(),
        0,
        "pool_byte_cap(0) must disable the pool"
    );
}

/// The byte-cap clamps the effective segment count: 8 MiB / 4 MiB = 2 segments,
/// even though `pool_segments` requests 4.
#[test]
fn byte_cap_clamps_segment_count() {
    let cfg = LargeCacheConfig::new().pool(
        SmallSegmentPoolConfig::new()
            .pool_segments(4)
            .pool_byte_cap(8 * MIB),
    );
    let ac = AllocCore::new_with_config(cfg).expect("primordial");
    assert_eq!(
        ac.dbg_pool_cap(),
        2,
        "8 MiB byte-cap must clamp to 2 segments"
    );
}

/// RAD-3 (E2, task #56): `pool_segments` is HONOURED exactly, with no silent
/// compile-time clamp — the old `POOL_MAX_SLOTS = 4` fixed-array cap is gone
/// (the pool's storage is now an intrusive list threaded through the pooled
/// segments' own headers, which has no fixed capacity). A request well above
/// the old hard cap, with a byte budget generous enough not to bind, resolves
/// to EXACTLY the requested value — this is the load-bearing counterfactual
/// for the plan's "the public API advertises `.pool_segments(8)` but the
/// runtime silently reduces it to 4" defect (see
/// `docs/perf/PERF_PLAN_2026-07-10-radical-audit-implementation-plan.md` §E2):
/// before this task `pool_segments(99)` resolved to 4; after it, it resolves
/// to 99.
#[test]
fn pool_segments_above_old_hard_cap_is_honoured() {
    let cfg = LargeCacheConfig::new().pool(
        SmallSegmentPoolConfig::new()
            .pool_segments(99)
            .pool_byte_cap(usize::MAX),
    );
    let ac = AllocCore::new_with_config(cfg).expect("primordial");
    assert_eq!(
        ac.dbg_pool_cap(),
        99,
        "pool_segments must be honoured exactly — no silent clamp to the old \
         POOL_MAX_SLOTS=4 hard cap"
    );
}

// ── 2/3/4. Admission, reuse without OS reservation, drain ────────────────────

/// Emptying more segments than the cap pools EXACTLY `pool_cap` of them and
/// releases the rest — the synchronous budget cap is a hard bound.
#[cfg_attr(miri, ignore)] // large N; native soak, mirrors c3 sizing
#[test]
fn pool_fills_to_cap_and_no_more() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(256, 8).unwrap();
    let class_idx = ac.dbg_layout_class_for(layout).expect("256 B small class");
    let cap = ac.dbg_pool_cap();
    assert!(cap > 0);

    // Spread across cap + 10 segments so plenty empty out in one drain.
    let target = cap + 10;
    let (survivors, all_ptrs) = spread_across_segments(&mut ac, layout, target);

    // Free every non-survivor (own-thread), leaving one live block per segment.
    let survivor_set: std::collections::HashSet<usize> =
        survivors.values().map(|&p| p as usize).collect();
    for &p in &all_ptrs {
        if !survivor_set.contains(&(p as usize)) {
            ac.dealloc(p, layout);
        }
    }

    // Push each survivor into its own ring (cross-thread free of the last block).
    for &p in survivors.values() {
        assert!(ac.dbg_push_to_ring(p, class_idx));
    }

    assert_eq!(ac.dbg_pooled_count(), 0, "pool must start empty");
    ac.dbg_drain_all_rings();

    // The pool filled to EXACTLY the cap — never more (hard synchronous bound).
    assert_eq!(
        ac.dbg_pooled_count(),
        cap,
        "pool must fill to exactly pool_cap={cap}, got {}",
        ac.dbg_pooled_count()
    );

    // Force-drain: retention is temporary.
    let drained = ac.dbg_drain_small_pool();
    assert_eq!(drained, cap, "drain must release all {cap} pooled segments");
    assert_eq!(ac.dbg_pooled_count(), 0, "pool empty after drain");
}

/// Reusing a pooled segment does NOT go to the OS: after a pooled segment
/// exists, driving the allocator to reserve a fresh segment draws from the pool
/// FIRST, so `dbg_segments_reserved_total` does not advance for that reuse.
#[cfg_attr(miri, ignore)] // large N; native soak
#[test]
fn reuse_pooled_segment_skips_os_reservation() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(256, 8).unwrap();
    let class_idx = ac.dbg_layout_class_for(layout).expect("256 B small class");
    let cap = ac.dbg_pool_cap();
    assert!(cap > 0);

    let target = cap + 5;
    let (survivors, all_ptrs) = spread_across_segments(&mut ac, layout, target);
    let survivor_set: std::collections::HashSet<usize> =
        survivors.values().map(|&p| p as usize).collect();
    for &p in &all_ptrs {
        if !survivor_set.contains(&(p as usize)) {
            ac.dealloc(p, layout);
        }
    }
    for &p in survivors.values() {
        assert!(ac.dbg_push_to_ring(p, class_idx));
    }
    ac.dbg_drain_all_rings();
    let pooled = ac.dbg_pooled_count();
    assert!(pooled > 0, "expected at least one pooled segment");

    // Now allocate enough to consume the current segment's free blocks and reach
    // the cross-segment reuse path. `find_segment_with_free` scans registered
    // segments (POOLED ones INCLUDED), and when it reuses a pooled segment's
    // free blocks it UN-POOLS that segment (removing it from the pool) — reuse
    // without any OS reservation. We measure the process-wide reserved counter
    // and the pool shrink across the burst.
    let reserved_before = AllocCore::dbg_segments_reserved_total();
    // Allocate well beyond the non-pooled segments' free capacity so the reuse
    // path reaches the pooled segments and un-pools them.
    let mut keep = Vec::new();
    for _ in 0..64_000 {
        let p = ac.alloc(layout);
        assert!(!p.is_null());
        keep.push(p);
    }
    let reserved_after = AllocCore::dbg_segments_reserved_total();
    let os_reservations = (reserved_after - reserved_before) as usize;

    // Load-bearing: the pool SHRANK — at least one pooled segment was un-pooled
    // (reused via `find_segment_with_free`'s free-list path), serving
    // allocations WITHOUT an OS reservation. Each un-pooled segment is one the
    // allocator did NOT re-reserve.
    let pool_drawn = pooled - ac.dbg_pooled_count();
    assert!(
        pool_drawn > 0,
        "pool was not drawn from during reuse (pooled stayed {pooled}) — \
         reuse-without-OS-reservation did not happen"
    );
    // The total segment demand of the burst is `pool_drawn` (pool-served) +
    // `os_reservations` (OS-served). Since `pool_drawn > 0`, the OS reservations
    // are STRICTLY FEWER than the total demand — the pool genuinely displaced OS
    // work for `pool_drawn` segments.
    let total_switch_demand = pool_drawn + os_reservations;
    assert!(
        os_reservations < total_switch_demand,
        "expected the pool to displace OS reservations (pool_drawn={pool_drawn}, \
         os_reservations={os_reservations})"
    );

    // Cleanup.
    for &p in &keep {
        ac.dealloc(p, layout);
    }
}

// ── 5. Disabled pool reproduces immediate-recycle ───────────────────────────

/// With the pool disabled, no segment is ever pooled — every emptied segment is
/// released immediately (the pre-Mechanism-2 behaviour).
#[cfg_attr(miri, ignore)] // large N; native soak
#[test]
fn disabled_pool_never_retains() {
    let cfg = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(0));
    let mut ac = AllocCore::new_with_config(cfg).expect("primordial");
    assert_eq!(ac.dbg_pool_cap(), 0);
    let layout = Layout::from_size_align(256, 8).unwrap();
    let class_idx = ac.dbg_layout_class_for(layout).expect("256 B small class");

    let target = 12usize;
    let (survivors, all_ptrs) = spread_across_segments(&mut ac, layout, target);
    let survivor_set: std::collections::HashSet<usize> =
        survivors.values().map(|&p| p as usize).collect();
    for &p in &all_ptrs {
        if !survivor_set.contains(&(p as usize)) {
            ac.dealloc(p, layout);
        }
    }
    for &p in survivors.values() {
        assert!(ac.dbg_push_to_ring(p, class_idx));
    }
    ac.dbg_drain_all_rings();

    assert_eq!(
        ac.dbg_pooled_count(),
        0,
        "disabled pool must never retain a segment"
    );
    // All emptied segments (minus small_cur + primordial) must be recycled.
    let recycled = survivors
        .values()
        .filter(|&&p| ac.dbg_live_count_for(p).is_none())
        .count();
    assert!(
        recycled >= target - 2,
        "disabled pool must recycle all emptied segments immediately, \
         got {recycled} of {target}"
    );
}

// ── 6. Stale cross-thread free into a pooled segment is a no-op ──────────────

/// A cross-thread free (ring push + drain) targeting a block in a POOLED
/// segment — where every block is already free — is a double-free caught by the
/// existing bitmap `is_free` guard. It must be a safe no-op: the segment stays
/// pooled (live_count 0), the allocator stays healthy, and no crash/corruption.
#[cfg_attr(miri, ignore)] // large N; native soak
#[test]
fn stale_free_into_pooled_segment_is_noop() {
    let mut ac = AllocCore::new().expect("primordial");
    let layout = Layout::from_size_align(256, 8).unwrap();
    let class_idx = ac.dbg_layout_class_for(layout).expect("256 B small class");
    let cap = ac.dbg_pool_cap();
    assert!(cap > 0);

    let target = cap + 3;
    let (survivors, all_ptrs) = spread_across_segments(&mut ac, layout, target);
    let survivor_set: std::collections::HashSet<usize> =
        survivors.values().map(|&p| p as usize).collect();
    for &p in &all_ptrs {
        if !survivor_set.contains(&(p as usize)) {
            ac.dealloc(p, layout);
        }
    }
    for &p in survivors.values() {
        assert!(ac.dbg_push_to_ring(p, class_idx));
    }
    ac.dbg_drain_all_rings();
    assert!(ac.dbg_pooled_count() > 0);

    // Find a survivor pointer whose segment is POOLED (still registered,
    // live_count == 0). `dbg_live_count_for` returns Some(0) for a pooled
    // segment (still registered) and None for a recycled one.
    let pooled_ptr = survivors
        .values()
        .copied()
        .find(|&p| ac.dbg_live_count_for(p) == Some(0));
    let pooled_ptr = match pooled_ptr {
        Some(p) => p,
        None => return, // no pooled survivor (all were small_cur/primordial) — skip
    };

    // Push the SAME (already-free) block into its pooled segment's ring again —
    // a stale/duplicate cross-thread free. Then drain. It must be a no-op:
    // live_count stays 0, no crash.
    assert!(
        ac.dbg_push_to_ring(pooled_ptr, class_idx),
        "push into a registered pooled segment must succeed at the ring level"
    );
    ac.dbg_drain_all_rings();
    assert_eq!(
        ac.dbg_live_count_for(pooled_ptr),
        Some(0),
        "stale free into a pooled segment must leave live_count == 0 (no-op)"
    );

    // The allocator is still healthy: a fresh allocation burst succeeds.
    let mut keep = Vec::new();
    for _ in 0..5_000 {
        let p = ac.alloc(layout);
        assert!(
            !p.is_null(),
            "allocator unhealthy after stale free into pool"
        );
        keep.push(p);
    }
    for &p in &keep {
        ac.dealloc(p, layout);
    }
}

// ── 7. Reuse invariant under repeated pool churn (non-vacuous) ──────────────

/// The working-set-cycle shape the pool exists to accelerate: repeatedly fill a
/// working set that spills across several segments, free it all (emptying and
/// POOLING segments), then re-fill — many times. Every allocation must be
/// non-null, aligned, non-overlapping with live blocks, and writable/readable
/// (M1/M3). This is the pool's REUSE-INVARIANT test: with the pool ON, most of
/// the re-fills are served by un-pooling previously-emptied segments (no OS
/// reserve), so a pool bug (handing out a stale/overlapping/corrupt block on
/// reuse) would surface here as an M3 overlap or a readback mismatch. NON-VACUOUS:
/// it is counterfactually sensitive — a pool that returned a block still
/// considered live, or double-issued a free-listed block on un-pool, breaks the
/// no-overlap check; a pool that failed to preserve a reused segment's free list
/// breaks the readback.
#[cfg_attr(miri, ignore)] // large N; native soak
#[test]
fn reuse_invariant_under_pool_churn() {
    let mut ac = AllocCore::new().expect("primordial");
    assert!(ac.dbg_pool_cap() > 0, "pool must be ON for this test");
    // 1 KiB blocks: ~4K per segment. A working set of 12K spills ~3 segments.
    let layout = Layout::from_size_align(1024, 16).unwrap();
    const WORKING_SET: usize = 12_000;
    const CYCLES: usize = 8;

    let mut pool_reuse_seen = false;
    for cycle in 0..CYCLES {
        // Note the pool occupancy before the fill; if it shrinks during the
        // fill, the pool was drawn from (reuse without OS reservation).
        let pooled_before = ac.dbg_pooled_count();

        let mut ptrs: Vec<*mut u8> = Vec::with_capacity(WORKING_SET);
        for i in 0..WORKING_SET {
            let p = ac.alloc(layout);
            assert!(!p.is_null(), "alloc null cycle={cycle} i={i}");
            assert_eq!((p as usize) % 16, 0, "misaligned cycle={cycle} i={i}");
            // M3: no overlap with any live block (spot check against the last
            // few — a full O(n^2) scan would be too slow for 12K × 8; the last
            // window catches the most-likely pool-reuse aliasing).
            for &q in ptrs.iter().rev().take(64) {
                let a = p as usize;
                let b = q as usize;
                assert!(
                    a + 1024 <= b || b + 1024 <= a,
                    "overlap cycle={cycle} i={i}"
                );
            }
            // Write a per-block pattern.
            unsafe {
                let v = (i & 0xFF) as u8;
                core::ptr::write_bytes(p, v, 1024);
            }
            ptrs.push(p);
        }
        // Read every block back — a reused pooled segment's blocks must be ours
        // and intact.
        for (i, &p) in ptrs.iter().enumerate() {
            let v = (i & 0xFF) as u8;
            unsafe {
                assert_eq!(p.read(), v, "readback[0] cycle={cycle} i={i}");
                assert_eq!(p.add(1023).read(), v, "readback[N] cycle={cycle} i={i}");
            }
        }
        if ac.dbg_pooled_count() < pooled_before {
            pool_reuse_seen = true;
        }
        // Free everything → empties + pools segments for the next cycle.
        for &p in &ptrs {
            ac.dealloc(p, layout);
        }
    }

    // Non-vacuity anchor: across the cycles the pool WAS drawn from at least
    // once (a re-fill reused a previously-pooled segment without an OS
    // reservation) — otherwise this test would not actually exercise the pool
    // reuse path and would pass vacuously.
    assert!(
        pool_reuse_seen,
        "pool was never drawn from across {CYCLES} churn cycles — the reuse \
         path was not exercised (test would be vacuous)"
    );
}
