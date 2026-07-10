//! Phase 35 (M6) — bounded miri decommit/recommit cycle (`alloc-decommit`).
//!
//! A deliberately SMALL test sized for miri (~1000× slower than native): it
//! allocates just enough large-ish small blocks to spill past the primordial
//! segment into ONE fresh `Small` segment, frees everything so that fresh
//! segment empties and decommits, then re-allocates and writes/reads back to
//! prove the recommit path is sound. Under miri `os::decommit_pages` /
//! `recommit_pages` are no-ops, so this verifies the BOOKKEEPING (live_count
//! zero-crossing → decommit hook, the reset, and reuse correctness), not RSS —
//! exactly what the design (§5) asks miri to cover. No UAF, no OOB write during
//! the reset, no access to "freed" pages on reuse.
//!
//! Kept separate from `decommit_soak` (whose N is large for a native RSS soak)
//! so the miri target is fast. Block size is chosen so the primordial fills in
//! a few thousand blocks rather than tens of thousands.

#![cfg(all(feature = "alloc-core", feature = "alloc-decommit"))]

use core::alloc::Layout;

use sefer_alloc::alloc_core::AllocCore;
use sefer_alloc::{LargeCacheConfig, SmallSegmentPoolConfig};

#[test]
fn decommit_recommit_cycle_bookkeeping() {
    let before = AllocCore::dbg_decommit_count();
    // Mechanism 2 (task #51): DISABLE the empty-small-segment pool for this
    // test. With the pool ON (the production default), the handful of segments
    // this small miri-sized workload empties are ABSORBED by the pool (retained
    // committed, no decommit) instead of decommitted+recommitted — so the
    // decommit hook would never fire and the reuse would go through the pool,
    // not the recommit path. Disabling the pool restores the deterministic
    // decommit→recommit cycle this test was written to cover (that path is still
    // fully live under `production`: it fires whenever the pool is full or
    // disabled). The pool's OWN reuse path is covered by
    // `tests/small_segment_pool.rs`.
    let cfg = LargeCacheConfig::new().pool(SmallSegmentPoolConfig::new().pool_segments(0));
    let mut ac = AllocCore::new_with_config(cfg).expect("primordial");
    // 2 KiB blocks: ~2K per 4 MiB payload, so a few thousand spills one fresh
    // segment. Small enough for miri to finish quickly.
    let layout = Layout::from_size_align(2048, 8).unwrap();

    // Two rounds: round 1 fills + empties (→ decommit); round 2 reuses.
    for round in 0..2 {
        let mut ptrs = Vec::new();
        // Enough to overflow the primordial into >= 2 fresh Small segments, so
        // at least one fresh `Small` segment is NON-current when emptied (the
        // current carve target is never decommitted). ~2K blocks/segment at 2 KiB,
        // so ~6000 spans the primordial + 2 fresh segments.
        for i in 0..6000usize {
            let p = ac.alloc(layout);
            assert!(!p.is_null(), "alloc null i={i} round={round}");
            // Touch a few bytes (recommitted pages on reuse must read back).
            unsafe {
                let b = (i & 0xFF) as u8;
                p.write(b);
                p.add(2047).write(b);
            }
            ptrs.push((p, (i & 0xFF) as u8));
        }
        for &(p, b) in &ptrs {
            unsafe {
                assert_eq!(p.read(), b, "byte0 readback round={round}");
                assert_eq!(p.add(2047).read(), b, "byteN readback round={round}");
            }
        }
        // Free all → empties the fresh non-current Small segment → decommit.
        for &(p, _) in &ptrs {
            ac.dealloc(p, layout);
        }
    }

    let after = AllocCore::dbg_decommit_count();
    assert!(
        after > before,
        "decommit hook never fired (before={before}, after={after})"
    );
}
