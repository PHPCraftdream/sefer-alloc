//! Regression tests for Э8 (task #162) — same-segment run batching in
//! `AllocCore::flush_class`. Each test is a COUNTERFACTUAL: it goes RED if the
//! batch flush drops one of the two per-block guards (`is_free`, `off >= bump`)
//! or mis-computes the per-run segment base.
//!
//! The three load-bearing properties:
//!
//! (a) RING-DF SKIP — a magazine-resident block whose cross-thread free is
//!     already in-flight in its segment ring, once the ring is drained, has its
//!     BinTable bitmap marked FREE while STILL sitting in the magazine. A
//!     following flush of that magazine MUST skip that block (`is_free == true`)
//!     or the freelist gets a duplicate → double-issue. Counterfactual: hoist
//!     `is_free` out of the per-block loop (check once per run) → RED.
//!
//! (b) MULTI-SEGMENT — a "magazine" holding blocks from ≥2 segments, interleaved
//!     so runs are length 1, must route each block to ITS OWN segment's BinTable.
//!     Realloc-all then yields globally distinct pointers. Counterfactual: use
//!     one base for the whole batch → RED (blocks land on the wrong segment's
//!     freelist / bitmap → corruption / double-issue).
//!
//! (c) DECOMMIT equivalence — a same-segment run that empties a non-current
//!     Small segment decommits exactly once and recycles; live_count is exact.
//!     Counterfactual: a stray extra `dec_live` (over-decrement) would decommit
//!     the WRONG (still-live) segment or an extra time → RED.
//!
//! These run at the `AllocCore` level (where `flush_class` lives) via the
//! `dbg_*` hooks, giving deterministic control over segments, rings and decommit
//! without real cross-thread races.

#![cfg(feature = "alloc-core")]

use std::alloc::Layout;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};

use sefer_alloc::{AllocCore, SegmentLayout};

/// L-4 (UBFIX-11): `AllocCore::dbg_decommit_count()` backs a process-WIDE
/// relaxed atomic (`DECOMMIT_CALLS`), shared across every test in this
/// binary. `cargo test`'s default parallel test-thread execution can
/// interleave two tests that both read a before/after delta around it,
/// making either delta observe the OTHER test's decommit events too —
/// exactly the interference observed between
/// `c_decommit_fires_once_per_emptied_segment` and the L-4 counterfactual
/// added below during this task's development. Serialize the (small) subset
/// of tests in this file that consult that counter, mirroring the
/// `SerialGuard` pattern already established elsewhere in the test suite
/// (see `tests/regression_hardened_large_kind_own_free.rs`).
static DECOMMIT_COUNTER_SERIAL: AtomicBool = AtomicBool::new(false);

struct SerialGuard;
impl SerialGuard {
    fn acquire() -> Self {
        while DECOMMIT_COUNTER_SERIAL
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        SerialGuard
    }
}
impl Drop for SerialGuard {
    fn drop(&mut self) {
        DECOMMIT_COUNTER_SERIAL.store(false, Ordering::Release);
    }
}

fn class_for(core: &AllocCore, size: usize, align: usize) -> usize {
    let layout = Layout::from_size_align(size, align).unwrap();
    core.dbg_layout_class_for(layout)
        .expect("expected a small class")
}

fn seg_base(ptr: *mut u8) -> usize {
    SegmentLayout::segment_base_of(ptr as usize)
}

// ---------------------------------------------------------------------------
// (a) RING-DF SKIP — the load-bearing per-block `is_free` guard.
//
// Plant a block P into a "magazine" (a local Vec). Simulate its cross-thread
// free by pushing it to the ring and draining: the drain's `reclaim_offset`
// marks P FREE on the BinTable and links it onto the freelist, WHILE P is still
// in our magazine. Now flush the magazine (which contains P). The batch flush
// must observe `is_free(P) == true` and SKIP P — not link it a SECOND time.
//
// A re-refill afterwards must never hand out P more than once (no double-issue).
//
// COUNTERFACTUAL: hoist `is_free` out of the per-block loop (check it once for
// the whole run) → P is accepted and spliced onto the freelist a second time →
// P appears TWICE in the freelist → re-refill issues it twice → RED.
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-xthread")]
#[test]
fn a_ring_df_block_is_skipped_by_flush() {
    let mut core = AllocCore::new().unwrap();
    let c = class_for(&core, 16, 8);
    let layout = Layout::from_size_align(16, 8).unwrap();

    // A batch of same-segment blocks acting as the "magazine".
    const N: usize = 8;
    let mut mag = vec![core::ptr::null_mut::<u8>(); N];
    let got = core.refill_class(c, N, &mut mag);
    assert_eq!(got, N);
    // All must be the SAME segment for this to be a single run (16 B blocks all
    // carve from the primordial segment first).
    let base0 = seg_base(mag[0]);
    assert!(
        mag.iter().all(|&p| seg_base(p) == base0),
        "test precondition: all N blocks share one segment"
    );

    // Pick P = mag[3]. Simulate its cross-thread free: push to ring + drain.
    // After the drain, P is on the BinTable freelist AND bitmap-free, but STILL
    // resident in `mag`.
    let p = mag[3];
    assert!(
        core.dbg_push_to_ring(p, c),
        "ring push failed (ring full or P not owned)"
    );
    core.dbg_drain_all_rings();

    // Sanity: P is now free on the BinTable (the ring drain reclaimed it).
    // (No direct getter for is_free at the core level, but the flush's skip is
    // what we ultimately assert via no-double-issue below.)

    // Flush the whole magazine (P included). The batch flush must SKIP P
    // (is_free == true) and link the other N-1 blocks normally. P must NOT be
    // linked a second time.
    // SAFETY (R6-MS-3): blocks are prior matching allocs of class `c`, live, owned by this core, freed exactly once here.
    unsafe { core.flush_class(c, &mag) };

    // Re-refill: pull every free block of this class back out. P must appear at
    // most ONCE across the returned set (no double-issue). We pull generously.
    let mut out = vec![core::ptr::null_mut::<u8>(); N + 8];
    let n2 = core.refill_class(c, N + 8, &mut out);
    let p_count = out[..n2].iter().filter(|&&q| q == p).count();
    assert!(
        p_count <= 1,
        "RING-DF block P was double-issued ({p_count}×) — the batch flush must \
         skip a block whose bitmap is already free (per-block is_free guard). \
         Hoisting is_free out of the run loop makes this RED."
    );

    // Also: no duplicates at all in the re-refill (conservation).
    let unique: HashSet<usize> = out[..n2].iter().map(|p| *p as usize).collect();
    assert_eq!(
        unique.len(),
        n2,
        "re-refill returned duplicate pointers after flush (freelist corrupted)"
    );

    for &q in &out[..n2] {
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(q, layout) };
    }
}

// ---------------------------------------------------------------------------
// (b) MULTI-SEGMENT flush — per-run base must be each block's OWN segment.
//
// Build a magazine holding blocks from >= 2 distinct segments, INTERLEAVED so
// consecutive entries alternate segments (runs of length 1 — the worst case for
// run detection, and the exact case a "one base for the whole batch" bug gets
// wrong). Flush all, then realloc-all: every returned pointer must be globally
// distinct (each block returned to ITS OWN segment's freelist, none lost/dup'd).
//
// COUNTERFACTUAL: compute one base (e.g. base of blocks[0]) for the whole batch
// → blocks from the OTHER segment get their `next`/bitmap written into the wrong
// segment's metadata → freelist corruption / double-issue / lost blocks → RED.
// ---------------------------------------------------------------------------

#[test]
fn b_multi_segment_flush_routes_per_segment() {
    let mut core = AllocCore::new().unwrap();
    // 1024 B blocks span segments quickly (~4000/segment).
    let size = 1024usize;
    let align = 8usize;
    let c = class_for(&core, size, align);
    let layout = Layout::from_size_align(size, align).unwrap();

    let n = 5000usize; // forces >= 2 segments
    let mut buf = vec![core::ptr::null_mut::<u8>(); n];
    let got = core.refill_class(c, n, &mut buf);
    assert_eq!(got, n);

    // Partition into two segments.
    let mut by_base: std::collections::HashMap<usize, Vec<*mut u8>> =
        std::collections::HashMap::new();
    for &p in &buf {
        by_base.entry(seg_base(p)).or_default().push(p);
    }
    assert!(
        by_base.len() >= 2,
        "test precondition: need >= 2 segments, got {}",
        by_base.len()
    );

    // Take exactly two segments A and B. To make the routing bug observable
    // INDEPENDENTLY of the decommit `off >= bump` filter, we flush the FULL
    // contents of both, interleaved (runs of length 1). Correct routing drives
    // BOTH A's and B's live_count to exactly 0 (each block decremented on its
    // OWN segment). A single-base bug attributes every decrement to A, so B's
    // live_count never drops → the per-segment live_count assertion catches it
    // even when the mis-offset blocks are silently filtered.
    let mut segs: Vec<Vec<*mut u8>> = by_base.into_values().collect();
    segs.sort_by_key(|v| v[0] as usize);
    let a = segs[0].clone();
    let b = segs[1].clone();
    let base_a = seg_base(a[0]);
    let base_b = seg_base(b[0]);
    #[cfg(feature = "alloc-decommit")]
    {
        assert_eq!(
            core.dbg_live_count_for(a[0]),
            Some(a.len() as u32),
            "A live == its block count"
        );
        assert_eq!(
            core.dbg_live_count_for(b[0]),
            Some(b.len() as u32),
            "B live == its block count"
        );
    }

    // Interleave the FULL lists (runs of length 1 where they overlap; a tail of
    // whichever is longer). Every adjacency across the overlap alternates
    // segments — the worst case for run detection.
    let mut mag: Vec<*mut u8> = Vec::with_capacity(a.len() + b.len());
    let k = a.len().min(b.len());
    for i in 0..k {
        mag.push(a[i]);
        mag.push(b[i]);
    }
    mag.extend_from_slice(&a[k..]);
    mag.extend_from_slice(&b[k..]);

    // Free everything NOT in A or B first (other segments incl. small_cur /
    // primordial) so the only live blocks left are exactly A's and B's.
    let ab: HashSet<usize> = mag.iter().map(|p| *p as usize).collect();
    for &p in &buf {
        if !ab.contains(&(p as usize)) {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { core.dealloc(p, layout) };
        }
    }

    // Flush the interleaved A+B magazine.
    // SAFETY (R6-MS-3): blocks are prior matching allocs of class `c`, live, owned by this core, freed exactly once here.
    unsafe { core.flush_class(c, &mag) };

    // Under decommit: correct routing → each of A and B is fully emptied:
    // live_count is either Some(0) (segment stayed committed — it was small_cur)
    // or None (it decommitted and its slot was recycled). A single-base bug
    // leaves B (or A) with a POSITIVE live_count → RED.
    #[cfg(feature = "alloc-decommit")]
    for (name, base, blk) in [("A", base_a, a[0]), ("B", base_b, b[0])] {
        let live = core.dbg_live_count_for(blk);
        assert!(
            matches!(live, None | Some(0)),
            "segment {name} ({base:#x}) not fully emptied after flush: \
             live_count = {live:?} (expected 0 or None). A block was routed to \
             the WRONG segment (per-run base bug)."
        );
    }

    // Portable check (runs in EVERY combo, incl. no-decommit): re-refill the
    // exact number of blocks we flushed and require them all distinct and
    // usable. Under no-decommit there is no `off >= bump` filter, so a
    // wrong-base block would corrupt segment A's freelist/bitmap at a bogus
    // offset → a subsequent refill returns a duplicate or crashes → RED. Under
    // decommit an emptied segment may decommit+recycle, so we do NOT assert the
    // returned SET equals the flushed set (fresh carves are legal), only that
    // the returned pointers are non-null and mutually distinct.
    let total = a.len() + b.len();
    let mut out = vec![core::ptr::null_mut::<u8>(); total];
    let n2 = core.refill_class(c, total, &mut out);
    assert_eq!(n2, total, "re-refill short after multi-segment flush");
    let unique: HashSet<usize> = out.iter().map(|p| *p as usize).collect();
    assert_eq!(
        unique.len(),
        total,
        "multi-segment flush produced duplicate pointers — a block was routed \
         to the WRONG segment's BinTable (per-run base bug)."
    );
    for &p in &out {
        // touch the whole block to surface any freelist corruption
        unsafe { core::ptr::write_bytes(p, 0x7E, size) };
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
    }
    let _ = (base_a, base_b);
}

// ---------------------------------------------------------------------------
// (c) DECOMMIT equivalence — a same-segment run that empties a non-current
// Small segment decommits EXACTLY ONCE and recycles; live_count is exact.
//
// COUNTERFACTUAL: a stray extra `dec_live` (over-decrement) in the run would
// drive live_count below the true value, decommitting a segment that still has
// live blocks (or an extra decommit) → the assertion on decommit-count delta /
// on the surviving live blocks being usable goes RED.
// ---------------------------------------------------------------------------

/// (c') PARTIAL-run live_count exactness — a same-segment run that does NOT
/// empty its segment must decrement `live_count` by EXACTLY the number of
/// accepted blocks, no more. This is the leg that catches a stray `dec_live`
/// which the full-empty case masks (there `dec_live` saturates at 0 and the
/// `is_decommitted` idempotency guard swallows a second decommit). Here the
/// segment stays live, so an off-by-one under-count is directly observable —
/// and, worse in production, it would fire a premature decommit later while a
/// block is still live.
///
/// COUNTERFACTUAL: `for _ in 0..accepted_count + 1` (stray extra dec_live) →
/// live_count reads one LESS than expected → RED.
#[cfg(feature = "alloc-decommit")]
#[test]
fn c_partial_flush_live_count_exact() {
    let mut core = AllocCore::new().unwrap();
    let size = 1024usize;
    let align = 8usize;
    let c = class_for(&core, size, align);
    let layout = Layout::from_size_align(size, align).unwrap();

    let n = 5000usize; // >= 2 segments
    let mut buf = vec![core::ptr::null_mut::<u8>(); n];
    assert_eq!(core.refill_class(c, n, &mut buf), n);

    let mut by_base: std::collections::HashMap<usize, Vec<*mut u8>> =
        std::collections::HashMap::new();
    for &p in &buf {
        by_base.entry(seg_base(p)).or_default().push(p);
    }
    // Pick a segment with many blocks so a strict subset still leaves it live.
    let (&base, blocks) = by_base
        .iter()
        .max_by_key(|(_, v)| v.len())
        .expect("at least one segment");
    let total = blocks.len();
    assert!(total >= 4, "need a fat segment; got {total}");
    let live0 = core.dbg_live_count_for(blocks[0]).unwrap();
    assert_eq!(
        live0 as usize, total,
        "live_count == block count precondition"
    );

    // Flush a STRICT subset (half) as a single same-segment run.
    let half = total / 2;
    let run: Vec<*mut u8> = blocks[..half].to_vec();
    // SAFETY (R6-MS-3): blocks are prior matching allocs of class `c`, live, owned by this core, freed exactly once here.
    unsafe { core.flush_class(c, &run) };

    // The segment is still live; live_count must be EXACTLY total - half.
    let expected = (total - half) as u32;
    let got = core.dbg_live_count_for(base as *mut u8);
    assert_eq!(
        got,
        Some(expected),
        "partial flush of {half} blocks must leave live_count = {expected} \
         (a stray extra dec_live under-counts by one → premature decommit \
         hazard). Segment must still be committed (not decommitted)."
    );
    assert_eq!(
        core.dbg_is_decommitted_for(base as *mut u8),
        Some(false),
        "a partially-flushed segment must NOT decommit"
    );

    // Cleanup: free the rest.
    for &p in &buf {
        if !run.contains(&p) {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { core.dealloc(p, layout) };
        }
    }
}

#[cfg(feature = "alloc-decommit")]
#[test]
fn c_decommit_fires_once_per_emptied_segment() {
    // L-4 (UBFIX-11): serialize against the other test in this file that
    // reads `AllocCore::dbg_decommit_count()`'s process-wide counter — see
    // `SerialGuard`'s doc comment above.
    let _guard = SerialGuard::acquire();
    // Mechanism 2 (task #51): DISABLE the empty-small-segment pool — this test
    // asserts decommit fires once per emptied segment. With the pool ON
    // (production default) the emptied segments are absorbed by the pool (no
    // decommit). Disabling it exercises the batch-flush→decommit path this test
    // covers. Pool behaviour is covered by `tests/small_segment_pool.rs`.
    let mut core = AllocCore::new_with_config(
        sefer_alloc::LargeCacheConfig::new()
            .pool(sefer_alloc::SmallSegmentPoolConfig::new().pool_segments(0)),
    )
    .unwrap();
    let size = 1024usize;
    let align = 8usize;
    let c = class_for(&core, size, align);
    let layout = Layout::from_size_align(size, align).unwrap();

    // Span >= 3 segments: primordial (never decommits) + >=2 Small.
    let n = 12_000usize;
    let mut buf = vec![core::ptr::null_mut::<u8>(); n];
    let got = core.refill_class(c, n, &mut buf);
    assert_eq!(got, n);

    let mut by_base: std::collections::HashMap<usize, Vec<*mut u8>> =
        std::collections::HashMap::new();
    for &p in &buf {
        by_base.entry(seg_base(p)).or_default().push(p);
    }
    assert!(
        by_base.len() >= 3,
        "need >= 3 segments, got {}",
        by_base.len()
    );

    // Flush each segment's blocks as its OWN same-segment run, one segment at a
    // time, tracking the per-run decommit delta and the segment's live_count.
    // We do NOT know which segment is `small_cur` (its base is not necessarily
    // extremal), so we assert per-run invariants that hold for BOTH cases:
    //
    //   * Each run empties its segment: live_count must reach EXACTLY 0.
    //   * The decommit delta for a run is 0 (segment is small_cur / primordial —
    //     stays committed) or 1 (non-current Small — decommits). NEVER >= 2:
    //     a stray extra dec_live in the run would over-decrement and either
    //     drive a second decommit or fire on a still-live segment.
    //   * A decommitted segment reads back `is_decommitted == true`; a
    //     still-committed emptied segment reads `false` and live_count 0.
    //   * At least one non-current Small segment MUST decommit (so the batch
    //     path's decommit branch is genuinely exercised, not vacuously skipped).
    let mut bases: Vec<usize> = by_base.keys().copied().collect();
    bases.sort();
    let mut total_decommits = 0u64;
    for &b in &bases {
        let run: Vec<*mut u8> = by_base[&b].clone();
        let live_before = core
            .dbg_live_count_for(run[0])
            .expect("segment must be small/primordial");
        assert_eq!(
            live_before as usize,
            run.len(),
            "segment {b:#x}: live_count must equal the number of blocks we hold"
        );
        assert_eq!(
            core.dbg_is_decommitted_for(run[0]),
            Some(false),
            "segment {b:#x} must start committed"
        );

        let before = AllocCore::dbg_decommit_count();
        // SAFETY (R6-MS-3): blocks are prior matching allocs of class `c`, live, owned by this core, freed exactly once here.
        unsafe { core.flush_class(c, &run) };
        let delta = AllocCore::dbg_decommit_count() - before;
        total_decommits += delta;

        assert!(
            delta <= 1,
            "segment {b:#x}: decommit fired {delta}× for a single emptied \
             segment — a stray extra dec_live over-decremented (must be 0 or 1)."
        );
        if delta == 1 {
            // Decommit fired → the slot was recycled (table.recycle NULLs it),
            // so the dbg getters now return None. That None is itself proof the
            // recycle ran exactly once for this emptied segment.
            assert_eq!(
                core.dbg_live_count_for(run[0]),
                None,
                "segment {b:#x}: after decommit the slot must be recycled (None)"
            );
            assert_eq!(core.dbg_is_decommitted_for(run[0]), None);
        } else {
            // No decommit (small_cur / primordial): the slot survives, and the
            // emptied segment reads live_count exactly 0 and stays committed.
            // A stray extra dec_live would make this negative-saturate or fire a
            // decommit — either way this leg would not hold.
            assert_eq!(
                core.dbg_live_count_for(run[0]),
                Some(0),
                "segment {b:#x}: live_count must be exactly 0 after full flush"
            );
            assert_eq!(
                core.dbg_is_decommitted_for(run[0]),
                Some(false),
                "segment {b:#x}: emptied-but-current segment stays committed"
            );
        }
    }
    assert!(
        total_decommits >= 1,
        "no segment decommitted — the batch flush decommit branch was never \
         exercised (test is vacuous)"
    );

    // buf is fully flushed now; nothing more to free.
    let _ = layout;
}

// ---------------------------------------------------------------------------
// (d) L-4 (UBFIX-11) — a SECOND same-base run after a mid-batch
// decommit-recycle must NOT re-touch the (now-unmapped) segment's metadata.
//
// `flush_class` groups a magazine batch into same-segment RUNS (Э8). The
// grouping assumes each segment appears in at most one run — true for a
// legitimate batch, but an upstream double-free reaching the magazine can
// hand `flush_class` the SAME segment's blocks in two separate runs
// (separated by a block from a different segment, breaking contiguous-base
// detection). If the FIRST run empties its segment, `flush_run` decommits +
// recycles it (releases the OS reservation, NULLs the table slot — pool
// disabled here to force the REAL release leg, not the pool leg). Without
// the L-4 fix, the SECOND run for that same base would still call
// `flush_run`, which unconditionally reads/writes the segment's metadata
// (`bump_of`, `bin_table()`, `alloc_bitmap()`, `kind_at`) — a metadata-level
// use-after-free on unmapped memory.
//
// COUNTERFACTUAL: skip the `already_recycled` guard in `flush_class` (revert
// to the pre-fix unconditional `self.flush_run(...)` call) → the second run
// dereferences the unmapped segment → the process crashes with an access
// violation (proven manually during this task's implementation; see the
// task summary). With the fix, the second run is silently skipped — no
// crash, and the surviving (non-recycled) segment's blocks flush normally.
// ---------------------------------------------------------------------------

#[cfg(feature = "alloc-decommit")]
#[test]
fn d_second_run_after_mid_batch_recycle_is_skipped_not_uaf() {
    // L-4 (UBFIX-11): this test triggers real decommit events (it does not
    // itself read `dbg_decommit_count()`, but its `flush_class` call bumps
    // that process-wide counter) — serialize against
    // `c_decommit_fires_once_per_emptied_segment`, which DOES read a
    // before/after delta of that counter and would otherwise observe THIS
    // test's decommit events if the two interleave. See `SerialGuard`'s doc
    // comment above.
    let _guard = SerialGuard::acquire();
    // Pool disabled: force the REAL release leg (OS unmap + slot NULL) on
    // the first run's decommit, not the pool leg (which would leave the
    // segment mapped and make the hazard this test targets unreachable).
    let mut core = AllocCore::new_with_config(
        sefer_alloc::LargeCacheConfig::new()
            .pool(sefer_alloc::SmallSegmentPoolConfig::new().pool_segments(0)),
    )
    .unwrap();
    let size = 1024usize;
    let align = 8usize;
    let c = class_for(&core, size, align);
    let layout = Layout::from_size_align(size, align).unwrap();

    // Span >= 2 Small segments (plus the primordial, which never decommits).
    let n = 12_000usize;
    let mut buf = vec![core::ptr::null_mut::<u8>(); n];
    let got = core.refill_class(c, n, &mut buf);
    assert_eq!(got, n);

    let mut by_base: std::collections::HashMap<usize, Vec<*mut u8>> =
        std::collections::HashMap::new();
    for &p in &buf {
        by_base.entry(seg_base(p)).or_default().push(p);
    }
    assert!(
        by_base.len() >= 2,
        "need >= 2 segments, got {}",
        by_base.len()
    );

    // Identify `small_cur`'s base indirectly: one more alloc always lands on
    // the CURRENT carve target, which `dec_live_and_maybe_decommit` excludes
    // from decommit (`base == small_cur` guard) — so A must NOT be this
    // segment, or the first run would never empty+recycle it (test would be
    // vacuous, as observed before this fix). Free this probe block right
    // back so it does not perturb the block sets collected above.
    let probe = core.alloc(layout);
    assert!(!probe.is_null());
    let current_base = seg_base(probe);
    // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
    unsafe { core.dealloc(probe, layout) };

    // Pick segment A: a NON-primordial, NON-current segment with >= 2 blocks
    // (the primordial NEVER decommits — `dec_live_and_maybe_decommit`'s
    // `matches!(kind_at(base), SegmentKind::Small)` guard explicitly excludes
    // it — and neither does the current carve target — `base == small_cur`
    // guard — so neither is eligible to be A, or the first run would never
    // empty+recycle it, making the test vacuous). B is any OTHER segment with
    // >= 2 blocks (current or primordial is fine for B — it is never asked
    // to decommit).
    let mut bases: Vec<usize> = by_base.keys().copied().collect();
    bases.sort();
    let candidates: Vec<usize> = bases
        .iter()
        .copied()
        .filter(|&b| {
            by_base[&b].len() >= 2 && b != current_base && core.dbg_kind_at_tag(by_base[&b][0]) == 1
            // 1 == SegmentKind::Small
        })
        .collect();
    assert!(
        !candidates.is_empty(),
        "need >= 1 non-current segment with >= 2 blocks for A, got {} \
         (current_base={current_base:#x})",
        candidates.len()
    );
    let base_a = candidates[0];
    let base_b = bases
        .iter()
        .copied()
        .find(|&b| b != base_a && by_base[&b].len() >= 2)
        .expect("need a second segment with >= 2 blocks for B");
    let a_blocks = by_base[&base_a].clone();
    let b_blocks = by_base[&base_b].clone();

    // Free every OTHER segment's blocks up front, so A and B are the only
    // live non-primordial segments left (isolates the decommit accounting).
    let ab: HashSet<usize> = a_blocks
        .iter()
        .chain(b_blocks.iter())
        .map(|&p| p as usize)
        .collect();
    for &p in &buf {
        if !ab.contains(&(p as usize)) {
            // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
            unsafe { core.dealloc(p, layout) };
        }
    }

    let live_a = core.dbg_live_count_for(a_blocks[0]);
    let live_b = core.dbg_live_count_for(b_blocks[0]);
    assert_eq!(live_a, Some(a_blocks.len() as u32), "A precondition");
    assert_eq!(live_b, Some(b_blocks.len() as u32), "B precondition");

    // Build the batch: ALL of A's blocks (run 1, will empty + recycle A),
    // then ONE block of B (breaks run contiguity — forces a NEW run to
    // start), then ALL of A's blocks AGAIN (run 2 — the "double-free
    // reaching the magazine" scenario: the same pointers appear a second
    // time, simulating a stale/duplicate free that slipped past the
    // magazine's own bookkeeping). Run 2 targets `base_a`, which by then has
    // been decommitted-and-released by run 1.
    let mut mag: Vec<*mut u8> = Vec::with_capacity(a_blocks.len() * 2 + 1);
    mag.extend_from_slice(&a_blocks);
    mag.push(b_blocks[0]);
    mag.extend_from_slice(&a_blocks); // duplicate run for the SAME base

    // Must not crash (access violation / segfault). If L-4 regresses, this
    // process aborts here instead of returning — the strongest possible
    // counterfactual signal (proven during this task's implementation: the
    // unguarded second run reliably crashes with STATUS_ACCESS_VIOLATION /
    // SIGSEGV on the unmapped segment's metadata read).
    //
    // NOTE: deliberately NOT using `AllocCore::dbg_decommit_count()` here —
    // it is a process-WIDE relaxed atomic (`DECOMMIT_CALLS`), shared across
    // every test in this binary; reading a before/after delta around this
    // call would race against sibling tests' own decommit activity under
    // `cargo test`'s default parallel test-thread execution (observed: this
    // exact interference against `c_decommit_fires_once_per_emptied_segment`
    // during this test's development). `dbg_live_count_for`/
    // `dbg_is_decommitted_for` below are PER-SEGMENT state, not shared
    // globals, so they give the same non-vacuousness proof without the race.
    // SAFETY (R6-MS-3): blocks are prior matching allocs of class `c`, live, owned by this core, freed exactly once here.
    unsafe { core.flush_class(c, &mag) };

    // A must be gone (slot recycled) — the dbg getters return None. This is
    // simultaneously the non-vacuousness proof (A really did decommit+recycle
    // from run 1) and the "no crash occurred" proof (we reached this line at
    // all, having survived whatever run 2 did or didn't do to A's now-unmapped
    // metadata).
    assert_eq!(
        core.dbg_live_count_for(a_blocks[0]),
        None,
        "segment A must be fully recycled (unmapped) after the first run — \
         if this reads Some(_) instead, A never actually decommitted and \
         this test is vacuous (did not exercise the L-4 hazard at all)"
    );
    assert_eq!(
        core.dbg_is_decommitted_for(a_blocks[0]),
        None,
        "segment A's slot must be fully recycled (None), not merely \
         decommitted-but-still-registered"
    );

    // B must be UNAFFECTED by A's recycle and by the (skipped) second run:
    // exactly one block (b_blocks[0]) was flushed, so live_count drops by
    // exactly 1 — not corrupted by a stray write from a mis-routed second
    // run for A's base.
    let expected_b_live = b_blocks.len() as u32 - 1;
    assert_eq!(
        core.dbg_live_count_for(b_blocks[0]),
        Some(expected_b_live),
        "segment B's live_count corrupted — the second (should-be-skipped) \
         run for A may have bled into B's accounting"
    );

    // B's remaining blocks must still be valid and usable — the strongest
    // proof that no stray metadata write escaped into B's segment.
    for &p in &b_blocks[1..] {
        unsafe {
            core::ptr::write_bytes(p, 0x5A, size);
            assert_eq!(
                *p, 0x5A,
                "B block became unwritable/corrupted after A's mid-batch recycle"
            );
        }
        // SAFETY (R6-MS-1/2): honoring the `unsafe fn` contract — the pointer was returned by a prior matching alloc in this test, is live, and is freed exactly once here.
        unsafe { core.dealloc(p, layout) };
    }
}
